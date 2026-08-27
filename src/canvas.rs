//! egui rendering of the paginated `Prim`s, and mouse/keyboard editing on top of
//! them: hit-testing, fret entry, the tool palette, and a cheap undo stack.
//!
//! A module of the *binary*, not the library: `lib.rs` stays free of eframe/egui
//! so it keeps building and testing without a window (see its doc comment). This
//! file is the only place in the app that touches egui widgets directly.

use eframe::egui;
use strungin::layout::Page;
use strungin::model::{Document, NoteValue, Strum, Technique};
use strungin::{i18n::t, model, staff, tablature, Align, Prim, PAGE_H_MM, PAGE_W_MM};

/// Visual gap between stacked pages on screen. Screen-only: has no equivalent in
/// the printed layout, where every page is its own sheet of paper.
const PAGE_GAP_MM: f32 = 10.0;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x5E, 0x5C, 0xE6);

/// A selected or hovered cell: one string at one event of one bar. Mirrors
/// `tablature::Hit`'s addressing, minus the geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    pub bar: usize,
    pub event: usize,
    pub string: u8,
}

/// Everything the editor remembers between frames: view state (zoom), selection,
/// the armed tool, and the undo stack. Owned by the app, threaded through every
/// call here.
pub struct EditorState {
    pub zoom: f32,
    pub selected: Option<Sel>,
    /// Technique applied to the selected note when clicked, or armed for the next
    /// note placed.
    pub tool_tech: Technique,
    /// Note value (with dots) applied to the selected event when clicked.
    pub tool_value: model::Dur,
    /// Armed value for Bend / Bend-and-release / Pre-bend.
    pub bend_quarters: u8,
    /// Armed value for Trill.
    pub trill_to_fret: u8,
    digit_buffer: String,
    digit_deadline: Option<f64>,
    undo_stack: Vec<Document>,
}

const DIGIT_WINDOW_SECS: f64 = 0.6;
const UNDO_DEPTH: usize = 20;

impl Default for EditorState {
    fn default() -> Self {
        EditorState {
            zoom: 3.5,
            selected: None,
            tool_tech: Technique::Plain,
            tool_value: model::Dur {
                base: NoteValue::Quarter,
                dots: 0,
            },
            bend_quarters: 2,
            trill_to_fret: 0,
            digit_buffer: String::new(),
            digit_deadline: None,
            undo_stack: Vec::new(),
        }
    }
}

/// What happened this frame that the caller (the app) needs to react to.
pub enum Action {
    /// The document was mutated: mark the file dirty. Re-pagination is the
    /// caller's business (it already recomputes `pages` every frame).
    Changed,
}

/// Push an undo snapshot, then run the mutation. "Push before each mutation,
/// twenty entries is plenty" — the cheap version on purpose, no command pattern.
fn mutate(state: &mut EditorState, doc: &mut Document, f: impl FnOnce(&mut Document)) {
    state.undo_stack.push(doc.clone());
    if state.undo_stack.len() > UNDO_DEPTH {
        state.undo_stack.remove(0);
    }
    f(doc);
}

/// Pop the undo stack onto `doc`. Returns whether it did anything, so the caller
/// knows whether to mark the file dirty.
pub fn undo(state: &mut EditorState, doc: &mut Document) -> bool {
    match state.undo_stack.pop() {
        Some(prev) => {
            *doc = prev;
            true
        }
        None => false,
    }
}

pub(crate) fn rgb(c: strungin::Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c.0, c.1, c.2)
}

fn to_screen(page_rect: egui::Rect, zoom: f32, p: strungin::P) -> egui::Pos2 {
    egui::pos2(
        page_rect.min.x + p.x * zoom,
        page_rect.min.y + (PAGE_H_MM - p.y) * zoom,
    )
}

/// Millimetres-per-point times zoom converts a `pt_for_cap`-style font size
/// straight into screen pixels, matching how the mm rows are scaled.
#[allow(clippy::too_many_arguments)]
fn draw_text(
    ctx: &egui::Context,
    painter: &egui::Painter,
    pos: egui::Pos2,
    pt: f32,
    text: &str,
    color: egui::Color32,
    align: Align,
    zoom: f32,
) {
    if text.is_empty() {
        return;
    }
    let px = pt * staff::MM_PER_PT * zoom;
    let font = egui::FontId::proportional(px);
    // ponytail: egui's public Fonts API exposes row_height (ascent + descent +
    // line gap) but not the ascent alone, so this is an approximation rather than
    // the font's exact ascent. Good enough on screen -- the PDF backend (phase 6)
    // is the byte-exact target for print, and `Prim::Text::pos` is documented as
    // the baseline exactly so both backends can agree on it.
    let ascent = ctx.fonts_mut(|f| f.row_height(&font)) * 0.8;
    let anchor = match align {
        Align::Left => egui::Align2::LEFT_TOP,
        Align::Center => egui::Align2::CENTER_TOP,
        Align::Right => egui::Align2::RIGHT_TOP,
    };
    painter.text(egui::pos2(pos.x, pos.y - ascent), anchor, text, font, color);
}

fn draw_prim(
    ctx: &egui::Context,
    painter: &egui::Painter,
    page_rect: egui::Rect,
    zoom: f32,
    prim: &Prim,
) {
    match prim {
        Prim::Line { a, b, w, color } => {
            painter.line_segment(
                [
                    to_screen(page_rect, zoom, *a),
                    to_screen(page_rect, zoom, *b),
                ],
                egui::Stroke::new((*w * zoom).max(0.5), rgb(*color)),
            );
        }
        Prim::Poly { pts, color } => {
            let points: Vec<egui::Pos2> =
                pts.iter().map(|p| to_screen(page_rect, zoom, *p)).collect();
            painter.add(egui::Shape::convex_polygon(
                points,
                rgb(*color),
                egui::Stroke::NONE,
            ));
        }
        Prim::Curve {
            a,
            c1,
            c2,
            b,
            w,
            color,
        } => {
            let bez = egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    to_screen(page_rect, zoom, *a),
                    to_screen(page_rect, zoom, *c1),
                    to_screen(page_rect, zoom, *c2),
                    to_screen(page_rect, zoom, *b),
                ],
                false,
                egui::Color32::TRANSPARENT,
                egui::Stroke::new((*w * zoom).max(0.5), rgb(*color)),
            );
            painter.add(egui::Shape::CubicBezier(bez));
        }
        Prim::Text {
            pos,
            s,
            pt,
            color,
            align,
        } => {
            draw_text(
                ctx,
                painter,
                to_screen(page_rect, zoom, *pos),
                *pt,
                s,
                rgb(*color),
                *align,
                zoom,
            );
        }
    }
}

/// Screen rect of the page at `index`, given the scroll content's top-left.
fn page_rect(content_min: egui::Pos2, index: usize, zoom: f32) -> egui::Rect {
    let stride = (PAGE_H_MM + PAGE_GAP_MM) * zoom;
    egui::Rect::from_min_size(
        egui::pos2(content_min.x, content_min.y + index as f32 * stride),
        egui::vec2(PAGE_W_MM * zoom, PAGE_H_MM * zoom),
    )
}

/// Turn a screen position (relative to the scroll content's origin) into a
/// selection, by finding which page it falls on and which `Hit` it lands in.
fn pick(pages: &[Page], content_min: egui::Pos2, zoom: f32, screen_pos: egui::Pos2) -> Option<Sel> {
    let stride = (PAGE_H_MM + PAGE_GAP_MM) * zoom;
    if stride <= 0.0 {
        return None;
    }
    let local_y = screen_pos.y - content_min.y;
    if local_y < 0.0 {
        return None;
    }
    let index = (local_y / stride).floor() as usize;
    let page = pages.get(index)?;
    let within = local_y - index as f32 * stride;
    if within > PAGE_H_MM * zoom {
        return None; // in the gap between pages
    }
    let mm_x = (screen_pos.x - content_min.x) / zoom;
    let mm_y = PAGE_H_MM - within / zoom;
    page.hits
        .iter()
        .find(|h| mm_x >= h.min.x && mm_x <= h.max.x && mm_y >= h.min.y && mm_y <= h.max.y)
        .map(|h| Sel {
            bar: h.bar,
            event: h.event,
            string: h.string,
        })
}

/// Find the page and `Hit` a selection currently corresponds to, for drawing a
/// highlight. A plain linear search: cheap next to actually rendering the page.
fn find_hit(pages: &[Page], sel: Sel) -> Option<(usize, &tablature::Hit)> {
    pages.iter().enumerate().find_map(|(i, p)| {
        p.hits
            .iter()
            .find(|h| h.bar == sel.bar && h.event == sel.event && h.string == sel.string)
            .map(|h| (i, h))
    })
}

fn draw_highlight(
    painter: &egui::Painter,
    content_min: egui::Pos2,
    zoom: f32,
    index: usize,
    hit: &tablature::Hit,
    color: egui::Color32,
) {
    let rect = page_rect(content_min, index, zoom);
    let min = to_screen(rect, zoom, strungin::P::new(hit.min.x, hit.max.y));
    let max = to_screen(rect, zoom, strungin::P::new(hit.max.x, hit.min.y));
    painter.rect_filled(egui::Rect::from_min_max(min, max), 2.0, color);
}

fn move_selection(state: &mut EditorState, doc: &Document, key: egui::Key) {
    let Some(sel) = state.selected else { return };
    state.selected = Some(match key {
        egui::Key::ArrowUp => Sel {
            string: sel.string.saturating_sub(1),
            ..sel
        },
        egui::Key::ArrowDown => Sel {
            string: (sel.string + 1).min(5),
            ..sel
        },
        egui::Key::ArrowLeft => prev_cell(doc, sel),
        egui::Key::ArrowRight => next_cell(doc, sel),
        _ => sel,
    });
}

fn prev_cell(doc: &Document, sel: Sel) -> Sel {
    if sel.event > 0 {
        return Sel {
            event: sel.event - 1,
            ..sel
        };
    }
    for b in (0..sel.bar).rev() {
        if let Some(bar) = doc.bars.get(b) {
            if !bar.events.is_empty() {
                return Sel {
                    bar: b,
                    event: bar.events.len() - 1,
                    string: sel.string,
                };
            }
        }
    }
    sel
}

fn next_cell(doc: &Document, sel: Sel) -> Sel {
    if let Some(bar) = doc.bars.get(sel.bar) {
        if sel.event + 1 < bar.events.len() {
            return Sel {
                event: sel.event + 1,
                ..sel
            };
        }
    }
    for b in (sel.bar + 1)..doc.bars.len() {
        if let Some(bar) = doc.bars.get(b) {
            if !bar.events.is_empty() {
                return Sel {
                    bar: b,
                    event: 0,
                    string: sel.string,
                };
            }
        }
    }
    sel
}

/// A typed digit: two within the window combine into one two-digit fret (e.g.
/// "1" then "2" within [`DIGIT_WINDOW_SECS`] writes fret 12).
fn handle_digit(state: &mut EditorState, doc: &mut Document, digit: u32, now: f64) -> bool {
    let Some(sel) = state.selected else {
        return false;
    };
    if state.digit_deadline.is_none_or(|t| now > t) || state.digit_buffer.len() >= 2 {
        state.digit_buffer.clear();
    }
    state.digit_buffer.push_str(&digit.to_string());
    state.digit_deadline = Some(now + DIGIT_WINDOW_SECS);
    let Ok(fret) = state.digit_buffer.parse::<u8>() else {
        return false;
    };

    let tech = state.tool_tech;
    mutate(state, doc, move |doc| {
        let Some(event) = doc
            .bars
            .get_mut(sel.bar)
            .and_then(|b| b.events.get_mut(sel.event))
        else {
            return;
        };
        match event.notes.iter_mut().find(|n| n.string == sel.string) {
            Some(note) => note.fret = fret,
            None => {
                event.notes.push(model::Note {
                    string: sel.string,
                    fret,
                    tech,
                    tie_next: false,
                });
                event.notes.sort_by_key(|n| n.string);
            }
        }
    });
    true
}

/// The canvas viewport: a scrollable, zoomable stack of pages, mouse hit-testing,
/// and the keyboard editing that happens while looking at it (fret digits,
/// backspace, space, arrow-key navigation). Returns `Some(Action::Changed)` the
/// frame something in `doc` actually changed.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    doc: &mut Document,
    pages: &[Page],
) -> Option<Action> {
    let mut action = None;
    let ctx = ui.ctx().clone();

    // Ctrl+scroll / pinch to zoom, centred roughly on the pointer by just scaling
    // in place -- simplest thing that still feels right at typical zoom steps.
    if ui.rect_contains_pointer(ui.max_rect()) {
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if zoom_delta != 1.0 {
            state.zoom = (state.zoom * zoom_delta).clamp(1.0, 12.0);
        }
    }

    let content_size = egui::vec2(
        PAGE_W_MM * state.zoom,
        pages.len() as f32 * (PAGE_H_MM + PAGE_GAP_MM) * state.zoom,
    );

    egui::ScrollArea::both()
        .id_salt("canvas_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let bg = ui.visuals().extreme_bg_color;
            let (rect, response) =
                ui.allocate_exact_size(content_size.max(ui.available_size()), egui::Sense::click());
            ui.painter().rect_filled(rect, 0.0, bg);
            let painter = ui.painter_at(rect);
            let content_min = rect.min;

            for (index, page) in pages.iter().enumerate() {
                let prect = page_rect(content_min, index, state.zoom);
                painter.rect_filled(
                    prect.translate(egui::vec2(2.0, 3.0)),
                    2.0,
                    egui::Color32::from_black_alpha(50),
                );
                painter.rect_filled(prect, 2.0, egui::Color32::WHITE);
                for prim in &page.prims {
                    draw_prim(&ctx, &painter, prect, state.zoom, prim);
                }
            }

            let hover = response
                .hover_pos()
                .and_then(|p| pick(pages, content_min, state.zoom, p));
            if let Some(sel) = hover {
                if let Some((index, hit)) = find_hit(pages, sel) {
                    draw_highlight(
                        &painter,
                        content_min,
                        state.zoom,
                        index,
                        hit,
                        egui::Color32::from_black_alpha(25),
                    );
                }
            }
            if let Some(sel) = state.selected {
                if let Some((index, hit)) = find_hit(pages, sel) {
                    draw_highlight(
                        &painter,
                        content_min,
                        state.zoom,
                        index,
                        hit,
                        ACCENT.gamma_multiply(0.55),
                    );
                }
            }

            if response.clicked() {
                response.request_focus();
                if let Some(p) = response.interact_pointer_pos() {
                    state.selected = pick(pages, content_min, state.zoom, p);
                }
            }

            if response.has_focus() {
                let now = ui.input(|i| i.time);
                let events = ui.ctx().input(|i| i.events.clone());
                for ev in events {
                    match ev {
                        egui::Event::Text(txt) => {
                            for ch in txt.chars() {
                                if let Some(d) = ch.to_digit(10) {
                                    if handle_digit(state, doc, d, now) {
                                        action = Some(Action::Changed);
                                    }
                                }
                            }
                        }
                        egui::Event::Key {
                            key: egui::Key::Backspace,
                            pressed: true,
                            ..
                        } => {
                            if let Some(sel) = state.selected {
                                mutate(state, doc, move |doc| {
                                    if let Some(event) = doc
                                        .bars
                                        .get_mut(sel.bar)
                                        .and_then(|b| b.events.get_mut(sel.event))
                                    {
                                        event.notes.retain(|n| n.string != sel.string);
                                    }
                                });
                                action = Some(Action::Changed);
                            }
                        }
                        egui::Event::Key {
                            key: egui::Key::Space,
                            pressed: true,
                            ..
                        } => {
                            if let Some(sel) = state.selected {
                                mutate(state, doc, move |doc| {
                                    if let Some(event) = doc
                                        .bars
                                        .get_mut(sel.bar)
                                        .and_then(|b| b.events.get_mut(sel.event))
                                    {
                                        event.notes.clear();
                                    }
                                });
                                action = Some(Action::Changed);
                            }
                        }
                        egui::Event::Key {
                            key, pressed: true, ..
                        } if matches!(
                            key,
                            egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::ArrowLeft
                                | egui::Key::ArrowRight
                        ) =>
                        {
                            move_selection(state, doc, key);
                        }
                        _ => {}
                    }
                }
            }
        });

    action
}

/// Bottom status bar additions: page X of T, current bar, and the zoom level.
pub fn status(ui: &mut egui::Ui, state: &mut EditorState, pages: &[Page]) {
    let total = pages.len().max(1);
    let current_page = state
        .selected
        .and_then(|sel| find_hit(pages, sel))
        .map(|(index, _)| index + 1)
        .unwrap_or(1);
    ui.label(format!("{} {} / {}", t("status.page"), current_page, total));
    ui.separator();
    let bar_label = state
        .selected
        .map(|sel| (sel.bar + 1).to_string())
        .unwrap_or_else(|| "-".to_string());
    ui.label(format!("{} {}", t("status.bar"), bar_label));
    ui.separator();
    ui.label(t("tool.zoom"));
    ui.add(
        egui::DragValue::new(&mut state.zoom)
            .range(1.0..=12.0)
            .speed(0.05),
    );
}

fn tech_button(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    doc: &mut Document,
    tech: Technique,
    key: &str,
    action: &mut Option<Action>,
) {
    let color = rgb(model::technique_color(&tech));
    let armed = std::mem::discriminant(&state.tool_tech) == std::mem::discriminant(&tech);
    let resp = ui.add(egui::Button::new(egui::RichText::new(t(key)).color(color)).selected(armed));
    if resp.clicked() {
        state.tool_tech = tech;
        if let Some(sel) = state.selected {
            let had_note = doc
                .bars
                .get(sel.bar)
                .and_then(|b| b.events.get(sel.event))
                .is_some_and(|e| e.notes.iter().any(|n| n.string == sel.string));
            if had_note {
                mutate(state, doc, move |doc| {
                    if let Some(note) = doc
                        .bars
                        .get_mut(sel.bar)
                        .and_then(|b| b.events.get_mut(sel.event))
                        .and_then(|e| e.notes.iter_mut().find(|n| n.string == sel.string))
                    {
                        note.tech = tech;
                    }
                });
                *action = Some(Action::Changed);
            }
        }
    }
}

/// The left tool palette: every technique, note values with dots, a rest button,
/// the strum/tap marks, palm-mute/let-ring toggles for the selected event, and
/// repeat toggles for the selected bar.
pub fn palette(ui: &mut egui::Ui, state: &mut EditorState, doc: &mut Document) -> Option<Action> {
    let mut action = None;
    let sel = state.selected;

    ui.label(t("tool.value"));
    ui.horizontal_wrapped(|ui| {
        for v in [
            NoteValue::Whole,
            NoteValue::Half,
            NoteValue::Quarter,
            NoteValue::Eighth,
            NoteValue::Sixteenth,
            NoteValue::ThirtySecond,
        ] {
            let armed = state.tool_value.base == v;
            if ui
                .selectable_label(armed, strungin::i18n::value_name(v))
                .clicked()
            {
                state.tool_value.base = v;
                if let Some(sel) = sel {
                    mutate(state, doc, move |doc| {
                        if let Some(event) = doc
                            .bars
                            .get_mut(sel.bar)
                            .and_then(|b| b.events.get_mut(sel.event))
                        {
                            event.dur.base = v;
                        }
                    });
                    action = Some(Action::Changed);
                }
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(t("tool.dot"));
        if ui
            .add(egui::DragValue::new(&mut state.tool_value.dots).range(0..=3))
            .changed()
        {
            let dots = state.tool_value.dots;
            if let Some(sel) = sel {
                mutate(state, doc, move |doc| {
                    if let Some(event) = doc
                        .bars
                        .get_mut(sel.bar)
                        .and_then(|b| b.events.get_mut(sel.event))
                    {
                        event.dur.dots = dots;
                    }
                });
                action = Some(Action::Changed);
            }
        }
    });

    if ui
        .add_enabled(sel.is_some(), egui::Button::new(t("tool.rest")))
        .clicked()
    {
        if let Some(sel) = sel {
            mutate(state, doc, move |doc| {
                if let Some(event) = doc
                    .bars
                    .get_mut(sel.bar)
                    .and_then(|b| b.events.get_mut(sel.event))
                {
                    event.notes.clear();
                }
            });
            action = Some(Action::Changed);
        }
    }

    ui.separator();
    tech_button(ui, state, doc, Technique::Plain, "tech.plain", &mut action);
    tech_button(
        ui,
        state,
        doc,
        Technique::HammerOn,
        "tech.hammer_on",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::PullOff,
        "tech.pull_off",
        &mut action,
    );
    tech_button(ui, state, doc, Technique::Slide, "tech.slide", &mut action);
    tech_button(
        ui,
        state,
        doc,
        Technique::SlideShift,
        "tech.slide_shift",
        &mut action,
    );
    tech_button(ui, state, doc, Technique::Grace, "tech.grace", &mut action);

    let bq = state.bend_quarters;
    tech_button(
        ui,
        state,
        doc,
        Technique::Bend { quarters: bq },
        "tech.bend",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::BendRelease { quarters: bq },
        "tech.bend_release",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::PreBend { quarters: bq },
        "tech.pre_bend",
        &mut action,
    );
    // Bend amount in quarters of a tone (1 = quarter, 2 = half, 4 = full) shared
    // by all three bend techniques above -- adjustable, never hardcoded.
    ui.add(egui::DragValue::new(&mut state.bend_quarters).range(1..=8));

    tech_button(
        ui,
        state,
        doc,
        Technique::Vibrato,
        "tech.vibrato",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::WideVibrato,
        "tech.wide_vibrato",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::Harmonic,
        "tech.harmonic",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::PinchHarmonic,
        "tech.pinch_harmonic",
        &mut action,
    );
    tech_button(ui, state, doc, Technique::Tap, "tech.tap", &mut action);
    tech_button(ui, state, doc, Technique::Dead, "tech.dead", &mut action);
    tech_button(ui, state, doc, Technique::Ghost, "tech.ghost", &mut action);

    let tf = state.trill_to_fret;
    tech_button(
        ui,
        state,
        doc,
        Technique::Trill { to_fret: tf },
        "tech.trill",
        &mut action,
    );
    // Target fret the trill alternates up to -- likewise adjustable.
    ui.add(egui::DragValue::new(&mut state.trill_to_fret).range(0..=24));

    ui.separator();
    for (strum, key) in [
        (Strum::Down, "strum.down"),
        (Strum::Up, "strum.up"),
        (Strum::TapRight, "strum.tap_right"),
        (Strum::TapLeft, "strum.tap_left"),
    ] {
        if ui
            .add_enabled(sel.is_some(), egui::Button::new(t(key)))
            .clicked()
        {
            if let Some(sel) = sel {
                mutate(state, doc, move |doc| {
                    if let Some(event) = doc
                        .bars
                        .get_mut(sel.bar)
                        .and_then(|b| b.events.get_mut(sel.event))
                    {
                        event.strum = if event.strum == Some(strum) {
                            None
                        } else {
                            Some(strum)
                        };
                    }
                });
                action = Some(Action::Changed);
            }
        }
    }

    ui.separator();
    let (mut palm_mute, mut let_ring) = sel
        .and_then(|s| doc.bars.get(s.bar).and_then(|b| b.events.get(s.event)))
        .map(|e| (e.palm_mute, e.let_ring))
        .unwrap_or((false, false));
    if ui
        .add_enabled(
            sel.is_some(),
            egui::Checkbox::new(&mut palm_mute, t("tech.palm_mute")),
        )
        .changed()
    {
        if let Some(sel) = sel {
            mutate(state, doc, move |doc| {
                if let Some(event) = doc
                    .bars
                    .get_mut(sel.bar)
                    .and_then(|b| b.events.get_mut(sel.event))
                {
                    event.palm_mute = palm_mute;
                }
            });
            action = Some(Action::Changed);
        }
    }
    if ui
        .add_enabled(
            sel.is_some(),
            egui::Checkbox::new(&mut let_ring, t("tech.let_ring")),
        )
        .changed()
    {
        if let Some(sel) = sel {
            mutate(state, doc, move |doc| {
                if let Some(event) = doc
                    .bars
                    .get_mut(sel.bar)
                    .and_then(|b| b.events.get_mut(sel.event))
                {
                    event.let_ring = let_ring;
                }
            });
            action = Some(Action::Changed);
        }
    }

    ui.separator();
    let bar_idx = sel.map(|s| s.bar);
    let mut repeat_start = bar_idx
        .and_then(|b| doc.bars.get(b))
        .is_some_and(|b| b.repeat_start);
    if ui
        .add_enabled(
            bar_idx.is_some(),
            egui::Checkbox::new(&mut repeat_start, t("bar.repeat_start")),
        )
        .changed()
    {
        if let Some(b) = bar_idx {
            mutate(state, doc, move |doc| {
                if let Some(bar) = doc.bars.get_mut(b) {
                    bar.repeat_start = repeat_start;
                }
            });
            action = Some(Action::Changed);
        }
    }
    let mut repeat_end = bar_idx
        .and_then(|b| doc.bars.get(b))
        .is_some_and(|b| b.repeat_end.is_some());
    if ui
        .add_enabled(
            bar_idx.is_some(),
            egui::Checkbox::new(&mut repeat_end, t("bar.repeat_end")),
        )
        .changed()
    {
        if let Some(b) = bar_idx {
            mutate(state, doc, move |doc| {
                if let Some(bar) = doc.bars.get_mut(b) {
                    bar.repeat_end = if repeat_end { Some(2) } else { None };
                }
            });
            action = Some(Action::Changed);
        }
    }

    ui.separator();
    if ui.button(t("tool.undo")).clicked() && undo(state, doc) {
        action = Some(Action::Changed);
    }

    action
}
