//! The live player: the score passing under a fixed playhead at its own tempo,
//! the beat being played highlighted at the centre of the screen — karaoke for
//! guitar.
//!
//! A module of the *binary*, like `canvas.rs`, because it drives egui. It
//! engraves nothing of its own: the timing comes from `engrave::timeline`, the
//! geometry from `layout::strip` (one endless line) or `layout::paginate` (the
//! page exactly as it prints), and both are painted through `canvas`'s `Prim`
//! painter. The live view is the same engraving as the page — only bigger, and
//! moving.

use eframe::egui;
use strungin::engrave::Cue;
use strungin::layout::{Page, Strip};
use strungin::model::Document;
use strungin::tablature::Hit;
use strungin::{engrave, i18n::t, layout, P, PAGE_H_MM, PAGE_W_MM};

use crate::canvas::{draw_prim, page_rect, to_screen, PAGE_GAP_MM};

/// Highlighter yellow. Laid on the paper *under* the ink, which is what makes it
/// read as a marker stroke rather than a coloured box on top of the notes.
const MARKER: egui::Color32 = egui::Color32::from_rgba_premultiplied(0xFF, 0xD8, 0x2A, 0xC0);
/// The fixed "now" line of the single-line view.
const PLAYHEAD: egui::Color32 = egui::Color32::from_rgb(0x5E, 0x5C, 0xE6);
/// Air above and below a highlighted column, so the marker covers the note and
/// its stem instead of stopping at the outer string lines.
const MARKER_PAD_MM: f32 = 1.6;
/// Millimetres to pixels. Far beyond the editor's range: this is a score read at
/// arm's length, with a guitar in the way.
const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 3.0..=24.0;
/// Playing at a quarter speed is how a hard bar gets learnt; past double, the
/// scrolling is faster than anyone reads.
const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.25..=2.0;
/// A frame that took longer than this (a window drag, a hidden window) advances
/// the music by this much and no more, rather than skipping a bar.
const MAX_FRAME_SECS: f32 = 0.1;

/// The player's own state. None of it belongs to the document: a rehearsal speed
/// is temporary by definition, and so is where the playhead happens to be.
pub struct LiveState {
    /// Whether the player has taken the window over from the editor.
    pub open: bool,
    /// One endless line, rather than the printed page's stacked systems.
    pub linear: bool,
    pub playing: bool,
    /// Multiplier on the document's tempo.
    pub speed: f32,
    pub zoom: f32,
    /// The playhead, in seconds from the start of the piece.
    pub t: f32,
    show: Option<Show>,
}

/// Everything the player draws, built once when live mode opens and dropped when
/// it closes.
///
/// From `engrave::for_export(doc)`, not from the document the editor holds: that
/// is what drops the blank bars the editor keeps ready under the music — silence
/// the player would otherwise sit through — and writes its per-beat rests the way
/// a score writes them. It renumbers events, so the cues, the strip and the pages
/// must all come from that one normalised document; building them together here
/// is what guarantees they agree.
struct Show {
    /// The document as the editor holds it, kept only to notice that it has been
    /// replaced underneath the player.
    source: Document,
    cues: Vec<Cue>,
    strip: Strip,
    pages: Vec<Page>,
    /// Seconds of music.
    duration: f32,
    tempo: u16,
    bars: usize,
}

impl Default for LiveState {
    fn default() -> Self {
        LiveState {
            open: false,
            linear: false,
            playing: false,
            speed: 1.0,
            zoom: 9.0,
            t: 0.0,
            show: None,
        }
    }
}

impl LiveState {
    /// Take the window over, with `doc` frozen as it is now.
    pub fn enter(&mut self, doc: &Document) {
        let source = doc.clone();
        let doc = engrave::for_export(doc);
        let cues = engrave::timeline(&doc);
        self.open = true;
        self.playing = false;
        self.t = 0.0;
        self.show = Some(Show {
            source,
            duration: cues.last().map(|c| c.end).unwrap_or(0.0),
            tempo: doc.tempo,
            bars: doc.bars.len(),
            cues,
            strip: layout::strip(&doc),
            pages: layout::paginate(&doc),
        });
    }

    /// Hand the window back to the editor. The speed, size and view the player
    /// was left on survive to the next entry; the frozen score does not.
    pub fn exit(&mut self) {
        self.open = false;
        self.playing = false;
        self.show = None;
    }
}

/// Index of the cue sounding at `t`. Cue starts only ever increase, which is what
/// makes the binary search legitimate.
fn cue_index(cues: &[Cue], t: f32) -> usize {
    cues.partition_point(|c| c.start <= t).saturating_sub(1)
}

/// Move the playhead one bar back or forward, to that bar's first event.
fn seek_bar(state: &mut LiveState, show: &Show, delta: i32) {
    let bar = show
        .cues
        .get(cue_index(&show.cues, state.t))
        .map_or(0, |c| c.bar);
    let target = (bar as i32 + delta).max(0) as usize;
    state.t = show
        .cues
        .iter()
        .find(|c| c.bar >= target)
        .map_or(show.duration, |c| c.start);
}

/// `m:ss`, for the position readout.
fn clock(secs: f32) -> String {
    let s = secs.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The union of one event's six string cells: the column a marker covers.
fn column(hits: &[Hit], bar: usize, event: usize) -> Option<(P, P)> {
    hits.iter()
        .filter(|h| h.bar == bar && h.event == event)
        .fold(None, |acc, h| {
            Some(match acc {
                None => (h.min, h.max),
                Some((lo, hi)) => (
                    P::new(lo.x.min(h.min.x), lo.y.min(h.min.y)),
                    P::new(hi.x.max(h.max.x), hi.y.max(h.max.y)),
                ),
            })
        })
}

/// The same lookup across a paginated score, plus the page it landed on.
fn find_column(pages: &[Page], bar: usize, event: usize) -> Option<(usize, (P, P))> {
    pages
        .iter()
        .enumerate()
        .find_map(|(i, p)| column(&p.hits, bar, event).map(|c| (i, c)))
}

/// Paint the highlighter over a column, `frame` being the page (or virtual page)
/// the millimetres are relative to.
fn marker(painter: &egui::Painter, frame: egui::Rect, zoom: f32, lo: P, hi: P) {
    let a = to_screen(frame, zoom, P::new(lo.x, hi.y + MARKER_PAD_MM));
    let b = to_screen(frame, zoom, P::new(hi.x, lo.y - MARKER_PAD_MM));
    painter.rect_filled(egui::Rect::from_two_pos(a, b), 2.0, MARKER);
}

/// Draw the player. Everything it needs was frozen by [`LiveState::enter`].
pub fn show(ui: &mut egui::Ui, state: &mut LiveState, doc: &Document) {
    // The player works from a score frozen at the moment it opened. The menu bar
    // is still there, so that score can be replaced underneath it -- by File >
    // Open, or by an undo -- and when it is, freeze the new one rather than play
    // music that is no longer in the document.
    if state.show.as_ref().is_some_and(|s| s.source != *doc) {
        state.enter(doc);
    }

    // Taken out of the state for the frame, so the drawing code can borrow the
    // frozen score while the transport writes to the rest. Put back on the one
    // exit path at the bottom — which is why nothing in between returns early.
    let Some(show) = state.show.take() else {
        state.open = false;
        return;
    };
    let mut leave = false;

    ui.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
            state.playing = !state.playing;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            leave = true;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
            seek_bar(state, &show, -1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
            seek_bar(state, &show, 1);
        }
    });

    if state.playing {
        let dt = ui.input(|i| i.stable_dt).min(MAX_FRAME_SECS);
        state.t += dt * state.speed;
        if state.t >= show.duration {
            state.t = show.duration;
            state.playing = false;
        }
        // egui repaints on input alone; music is the one thing here that moves
        // without any.
        ui.ctx().request_repaint();
    }

    egui::Panel::top("live_transport").show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let play = if state.playing {
                t("live.pause")
            } else {
                t("live.play")
            };
            if ui.button(play).clicked() {
                state.playing = !state.playing;
            }
            if ui.button(t("live.restart")).clicked() {
                state.t = 0.0;
            }
            ui.separator();
            ui.label(t("live.speed"));
            ui.add(
                egui::DragValue::new(&mut state.speed)
                    .range(SPEED_RANGE)
                    .speed(0.01)
                    .max_decimals(2),
            );
            ui.weak(format!(
                "{:.0} {}",
                show.tempo as f32 * state.speed,
                t("live.bpm")
            ));
            ui.separator();
            ui.label(t("live.size"));
            ui.add(
                egui::DragValue::new(&mut state.zoom)
                    .range(ZOOM_RANGE)
                    .speed(0.1)
                    .max_decimals(1),
            );
            ui.separator();
            if ui
                .selectable_label(!state.linear, t("live.view_pages"))
                .clicked()
            {
                state.linear = false;
            }
            if ui
                .selectable_label(state.linear, t("live.view_linear"))
                .clicked()
            {
                state.linear = true;
            }
            ui.separator();
            if ui.button(t("live.exit")).clicked() {
                leave = true;
            }
        });
        ui.add_space(4.0);
    });

    egui::Panel::bottom("live_position").show(ui, |ui| {
        ui.horizontal(|ui| {
            let bar = show
                .cues
                .get(cue_index(&show.cues, state.t))
                .map_or(0, |c| c.bar + 1);
            ui.label(format!("{} {} / {}", t("status.bar"), bar, show.bars));
            ui.separator();
            ui.label(format!("{} / {}", clock(state.t), clock(show.duration)));
            ui.separator();
            let width = ui.available_width().max(80.0);
            ui.add_sized(
                [width, ui.spacing().interact_size.y],
                egui::Slider::new(&mut state.t, 0.0..=show.duration.max(0.1)).show_value(false),
            );
        });
    });

    egui::CentralPanel::default()
        .frame(egui::Frame::default())
        .show(ui, |ui| {
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if zoom_delta != 1.0 {
                state.zoom =
                    (state.zoom * zoom_delta).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
            }
            let rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            let i = cue_index(&show.cues, state.t);
            if state.linear {
                draw_linear(ui.ctx(), &painter, rect, state, &show, i);
            } else {
                draw_pages(ui.ctx(), &painter, rect, state, &show, i);
            }
        });

    state.show = Some(show);
    if leave {
        state.exit();
    }
}

/// Where the playhead sits on the strip at `t`, in strip millimetres: the current
/// event's column, glided towards the next one so the ribbon moves continuously
/// instead of hopping from note to note.
fn head_x(show: &Show, t: f32) -> f32 {
    let strip = &show.strip;
    let Some(cue) = show.cues.get(cue_index(&show.cues, t)) else {
        return strip.x;
    };
    let x0 = strip.event_x(cue.bar, cue.event).unwrap_or(strip.x);
    let x1 = show
        .cues
        .get(cue_index(&show.cues, t) + 1)
        .and_then(|n| strip.event_x(n.bar, n.event))
        .unwrap_or_else(|| strip.end_x());
    let k = ((t - cue.start) / (cue.end - cue.start).max(f32::EPSILON)).clamp(0.0, 1.0);
    x0 + (x1 - x0) * k
}

/// The single-line view: the whole piece as one ribbon, sliding right to left
/// under a fixed playhead.
fn draw_linear(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &LiveState,
    show: &Show,
    i: usize,
) {
    let strip = &show.strip;
    let zoom = state.zoom;
    let centre = rect.center();
    // A virtual page frame, chosen so that `canvas::to_screen` lands the playhead
    // on the centre of the screen and the strip's own middle on the centre line.
    // The strip is then painted by the very same primitive painter as a page,
    // with no second transform to keep in step with the first.
    let frame = egui::Rect::from_min_size(
        egui::pos2(
            centre.x - head_x(show, state.t) * zoom,
            centre.y - (PAGE_H_MM - strip.height * 0.5) * zoom,
        ),
        egui::vec2(PAGE_W_MM * zoom, PAGE_H_MM * zoom),
    );

    // Paper: a ribbon across the whole window, so the music never floats on the
    // desk background.
    let top = to_screen(frame, zoom, P::new(0.0, strip.height + MARKER_PAD_MM)).y;
    let bottom = to_screen(frame, zoom, P::new(0.0, -MARKER_PAD_MM)).y;
    painter.rect_filled(
        egui::Rect::from_x_y_ranges(rect.x_range(), top..=bottom),
        0.0,
        egui::Color32::WHITE,
    );

    if let Some((lo, hi)) = show
        .cues
        .get(i)
        .and_then(|c| column(&strip.hits, c.bar, c.event))
    {
        marker(painter, frame, zoom, lo, hi);
    }
    for prim in &strip.prims {
        draw_prim(ctx, painter, frame, zoom, prim);
    }
    painter.line_segment(
        [egui::pos2(centre.x, top), egui::pos2(centre.x, bottom)],
        egui::Stroke::new(1.5, PLAYHEAD.gamma_multiply(0.6)),
    );
}

/// Where the page view centres at `t`, and the column the marker covers: the
/// current event's column, glided towards the next one while both sit on the same
/// system, snapped otherwise — a line break is a jump, not a slide.
fn page_focus(show: &Show, i: usize, t: f32) -> Option<(usize, egui::Vec2, P, P)> {
    let cue = show.cues.get(i)?;
    let (page, (lo, hi)) = find_column(&show.pages, cue.bar, cue.event)?;
    let mid = |lo: P, hi: P| egui::vec2((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
    let here = mid(lo, hi);
    let next = show
        .cues
        .get(i + 1)
        .and_then(|n| find_column(&show.pages, n.bar, n.event))
        .filter(|&(p, (l, h))| p == page && (mid(l, h).y - here.y).abs() < 0.01)
        .map(|(_, (l, h))| mid(l, h));
    let centre = match next {
        Some(n) => {
            let k = ((t - cue.start) / (cue.end - cue.start).max(f32::EPSILON)).clamp(0.0, 1.0);
            here + (n - here) * k
        }
        None => here,
    };
    Some((page, centre, lo, hi))
}

/// The scrolling-page view: the score exactly as it prints, moved so the beat
/// being played sits in the middle of the screen.
fn draw_pages(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &LiveState,
    show: &Show,
    i: usize,
) {
    let Some((page, focus, lo, hi)) = page_focus(show, i, state.t) else {
        return;
    };
    let zoom = state.zoom;
    let centre = rect.center();
    let content_min = egui::pos2(
        centre.x - focus.x * zoom,
        centre.y - page as f32 * (PAGE_H_MM + PAGE_GAP_MM) * zoom - (PAGE_H_MM - focus.y) * zoom,
    );

    // Two passes over the pages: every sheet of paper first, then the marker,
    // then the ink — a highlighter goes under the notes, not over them.
    let visible = |idx: usize| {
        let prect = page_rect(content_min, idx, zoom);
        prect.intersects(rect).then_some(prect)
    };
    for idx in 0..show.pages.len() {
        if let Some(prect) = visible(idx) {
            painter.rect_filled(prect, 2.0, egui::Color32::WHITE);
        }
    }
    marker(painter, page_rect(content_min, page, zoom), zoom, lo, hi);
    for (idx, p) in show.pages.iter().enumerate() {
        let Some(prect) = visible(idx) else { continue };
        for prim in &p.prims {
            draw_prim(ctx, painter, prect, zoom, prim);
        }
    }
}
