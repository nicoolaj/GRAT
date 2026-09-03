//! egui rendering of the paginated `Prim`s, and mouse/keyboard editing on top of
//! them: hit-testing, fret entry, the tool palette, and a cheap undo stack.
//!
//! A module of the *binary*, not the library: `lib.rs` stays free of eframe/egui
//! so it keeps building and testing without a window (see its doc comment). This
//! file is the only place in the app that touches egui widgets directly.

use eframe::egui;
use strungin::layout::Page;
use strungin::model::{Document, NoteValue, Strum, Technique, MAX_DOTS};
use strungin::{engrave, i18n::t, model, staff, tablature, Align, Prim, PAGE_H_MM, PAGE_W_MM};

/// Visual gap between stacked pages on screen. Screen-only: has no equivalent in
/// the printed layout, where every page is its own sheet of paper.
pub(crate) const PAGE_GAP_MM: f32 = 10.0;
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
    /// Whether an explicit duration change (the palette's value and dot widgets)
    /// pushes the rest of the piece forward instead of absorbing locally. Digit
    /// entry ignores this -- see the comment in `handle_digit`.
    pub shift_following: bool,
    /// Armed value for SlideIn's departure fret.
    pub slide_from_fret: u8,
    /// Armed value for Bend.
    pub bend_quarters: u8,
    /// Armed value for Bend-and-release.
    pub bend_release_quarters: u8,
    /// Armed value for Pre-bend.
    pub pre_bend_quarters: u8,
    /// Armed value for Trill.
    pub trill_to_fret: u8,
    /// One bit per `TECH_LEGEND` entry: the technique buttons the palette shows.
    /// Edited from Edit > note styles, remembered across sessions.
    pub tech_shown: u32,
    digit_buffer: String,
    digit_deadline: Option<f64>,
    /// Set by `set_sel` whenever the selection changes; consumed (and cleared) the
    /// next time the selection highlight is drawn, so the view scrolls to follow
    /// the cursor without fighting the user's own mouse-wheel scrolling.
    scroll_to_sel: bool,
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
            shift_following: false,
            slide_from_fret: 2,
            bend_quarters: 2,
            bend_release_quarters: 2,
            pre_bend_quarters: 2,
            trill_to_fret: 0,
            tech_shown: u32::MAX,
            digit_buffer: String::new(),
            digit_deadline: None,
            scroll_to_sel: false,
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

/// Push an undo snapshot. "Push before each mutation, twenty entries is plenty" --
/// the cheap version on purpose, no command pattern. Split out from `mutate` so a
/// multi-frame drag can snapshot once at drag-start and then write straight through
/// every following frame, instead of flooding the 20-deep stack in 20 frames.
fn snapshot(state: &mut EditorState, doc: &Document) {
    state.undo_stack.push(doc.clone());
    if state.undo_stack.len() > UNDO_DEPTH {
        state.undo_stack.remove(0);
    }
}

/// Push an undo snapshot, then run the mutation.
fn mutate(state: &mut EditorState, doc: &mut Document, f: impl FnOnce(&mut Document)) {
    snapshot(state, doc);
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

/// Whether `tech` keeps a button in the palette. The bit is the technique's position
/// in `TECH_LEGEND`; scanning twenty entries per button is cheaper than a second
/// table to keep in sync. A technique missing from the table stays visible.
pub(crate) fn tech_visible(state: &EditorState, tech: &Technique) -> bool {
    crate::TECH_LEGEND
        .iter()
        .position(|(t, _, _)| std::mem::discriminant(t) == std::mem::discriminant(tech))
        .is_none_or(|i| state.tech_shown & (1 << i) != 0)
}

/// The armed-value field this technique reads and writes, or `None` when it carries no number.
fn param_slot<'a>(state: &'a mut EditorState, tech: &Technique) -> Option<&'a mut u8> {
    Some(match tech {
        Technique::SlideIn { .. } => &mut state.slide_from_fret,
        Technique::Bend { .. } => &mut state.bend_quarters,
        Technique::BendRelease { .. } => &mut state.bend_release_quarters,
        Technique::PreBend { .. } => &mut state.pre_bend_quarters,
        Technique::Trill { .. } => &mut state.trill_to_fret,
        _ => return None,
    })
}

/// i18n key for a parameterised technique's number label -- a UI concern (which
/// string names the field), so it stays out of model.rs alongside `param()`'s data.
fn param_key(tech: &Technique) -> &'static str {
    match tech {
        Technique::SlideIn { .. } => "param.from_fret",
        Technique::Bend { .. } | Technique::BendRelease { .. } | Technique::PreBend { .. } => {
            "param.bend"
        }
        Technique::Trill { .. } => "param.to_fret",
        _ => unreachable!("only called on a technique with a parameter"),
    }
}

pub(crate) fn to_screen(page_rect: egui::Rect, zoom: f32, p: strungin::P) -> egui::Pos2 {
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

pub(crate) fn draw_prim(
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
pub(crate) fn page_rect(content_min: egui::Pos2, index: usize, zoom: f32) -> egui::Rect {
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
) -> egui::Rect {
    let rect = page_rect(content_min, index, zoom);
    let min = to_screen(rect, zoom, strungin::P::new(hit.min.x, hit.max.y));
    let max = to_screen(rect, zoom, strungin::P::new(hit.max.x, hit.min.y));
    let highlight = egui::Rect::from_min_max(min, max);
    painter.rect_filled(highlight, 2.0, color);
    highlight
}

/// The one gate every write to `state.selected` must go through (`move_selection`
/// and the click handler in `show`): a fret typed for one cell must never leak
/// into the next, so any change of cell drops the in-progress digit buffer, and
/// any change of cell asks the view to scroll the new cell into sight.
fn set_sel(state: &mut EditorState, sel: Option<Sel>) {
    if state.selected != sel {
        state.digit_buffer.clear();
        state.digit_deadline = None;
        state.scroll_to_sel = true;
    }
    state.selected = sel;
}

fn move_selection(state: &mut EditorState, doc: &Document, key: egui::Key) {
    let Some(sel) = state.selected else { return };
    let next = match key {
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
    };
    set_sel(state, Some(next));
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
    let value = state.tool_value;
    mutate(state, doc, move |doc| {
        let sig = doc.time_sig_at(sel.bar); // read before the &mut below -- borrowck
        let Some(bar) = doc.bars.get_mut(sel.bar) else {
            return;
        };
        // The armed value only takes hold on an empty cell: correcting a fret or
        // stacking a string onto an existing chord must not shorten what is
        // already written. Entry always absorbs locally here (never
        // `shift_event_dur`, regardless of `state.shift_following`): placing a
        // note in a rest isn't "changing an existing note's timing", and
        // shifting the whole piece on every typed fret would make entry
        // unusable.
        if bar.events.get(sel.event).is_some_and(|e| e.is_rest()) {
            engrave::set_event_dur(bar, sig, sel.event, value);
        }
        let Some(event) = bar.events.get_mut(sel.event) else {
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

/// `NoteValue` variants ordered longest to shortest, matching both the palette
/// row and `NoteValue`'s own declaration order.
const NOTE_VALUES: [NoteValue; 6] = [
    NoteValue::Whole,
    NoteValue::Half,
    NoteValue::Quarter,
    NoteValue::Eighth,
    NoteValue::Sixteenth,
    NoteValue::ThirtySecond,
];

/// One step shorter (`shorter: true`) or longer than `v`, clamped at both ends:
/// there is no wraparound, so `-` on a thirty-second note (or `+` on a whole
/// note) does nothing.
fn step_value(v: NoteValue, shorter: bool) -> NoteValue {
    let i = NOTE_VALUES.iter().position(|&x| x == v).unwrap_or(0);
    let j = if shorter { i + 1 } else { i.wrapping_sub(1) };
    NOTE_VALUES.get(j).copied().unwrap_or(v)
}

/// The selected event's current duration, or the default if there's no event
/// there. Both the duration keys and the two palette widgets read the half of
/// `Dur` they are *not* currently changing from here (never from
/// `state.tool_value`), so pressing `-` or dragging the dot count doesn't
/// clobber the other half.
fn selected_dur(doc: &Document, sel: Sel) -> model::Dur {
    doc.bars
        .get(sel.bar)
        .and_then(|b| b.events.get(sel.event))
        .map(|e| e.dur)
        .unwrap_or_default()
}

/// Apply `dur` to the selected event exactly as the value and dot widgets in
/// the palette do: absorb the change locally, or push the rest of the piece
/// forward when `state.shift_following` is set. Shared by those two widgets and
/// the `-`/`+`/`.` keyboard shortcuts. Returns whether it did anything, so
/// callers know whether to mark the document changed.
fn apply_dur(state: &mut EditorState, doc: &mut Document, dur: model::Dur) -> bool {
    let Some(sel) = state.selected else {
        return false;
    };
    let sig = doc.time_sig_at(sel.bar);
    let shift_following = state.shift_following;
    mutate(state, doc, move |doc| {
        if shift_following {
            engrave::shift_event_dur(doc, sel.bar, sel.event, dur);
        } else if let Some(bar) = doc.bars.get_mut(sel.bar) {
            engrave::set_event_dur(bar, sig, sel.event, dur);
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

            // egui treats an unmodified arrow key as directional focus navigation
            // (jumping focus to the nearest widget in that direction) unless the
            // focused widget opts out via an EventFilter -- without this, the first
            // arrow press hands focus to a palette button or DragValue and keyboard
            // entry stops dead. Reclaim focus the moment nothing else holds it, and
            // lock out that navigation for as long as we have it. `tab`/`escape`
            // stay false: Tab still reaches the toolbar's text fields, and Escape
            // still surrenders focus back to us.
            //
            // ponytail: `set_focus_lock_filter` only takes hold once this widget
            // already had focus on the *previous* frame (egui's own guard against
            // locking a filter in before focus is confirmed), so the one frame
            // where we reclaim focus from nobody is briefly unprotected. Only
            // reachable if a cold app's very first-ever input is an arrow key with
            // no prior click; upgrade path is to stop relying on public egui API
            // and reach into `Memory`'s private focus state directly, if that ever
            // becomes necessary.
            if ui.ctx().memory(|m| m.focused().is_none()) {
                response.request_focus();
            }
            if response.has_focus() {
                ui.ctx().memory_mut(|m| {
                    m.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            tab: false,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: false,
                        },
                    )
                });
            }

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
                    let rect = draw_highlight(
                        &painter,
                        content_min,
                        state.zoom,
                        index,
                        hit,
                        ACCENT.gamma_multiply(0.55),
                    );
                    if state.scroll_to_sel {
                        ui.scroll_to_rect(rect, None);
                        state.scroll_to_sel = false;
                    }
                }
            }

            if response.clicked() {
                response.request_focus();
                if let Some(p) = response.interact_pointer_pos() {
                    let sel = pick(pages, content_min, state.zoom, p);
                    set_sel(state, sel);
                }
            }

            if response.secondary_clicked() {
                if let Some(p) = response.interact_pointer_pos() {
                    let sel = pick(pages, content_min, state.zoom, p);
                    set_sel(state, sel);
                }
            }
            // A `Hit` exists even for an empty cell, so only offer a menu once a
            // real note is there and its technique actually carries a number.
            // context_menu() has to be called every frame (not just on the click)
            // for the popup to stay open -- it does its own secondary-click
            // detection, so this runs unconditionally and is a no-op most frames.
            if let Some(sel) = state.selected {
                let param = doc
                    .bars
                    .get(sel.bar)
                    .and_then(|b| b.events.get(sel.event))
                    .and_then(|e| e.notes.iter().find(|n| n.string == sel.string))
                    .and_then(|n| n.tech.param().map(|p| (n.tech, p)));
                if let Some((tech, (v0, range))) = param {
                    let mut v = v0;
                    response.context_menu(|ui| {
                        ui.label(t(param_key(&tech)));
                        let dv = ui.add(egui::DragValue::new(&mut v).range(range));
                        if dv.drag_started() || (dv.changed() && !dv.dragged()) {
                            snapshot(state, doc);
                        }
                        if dv.changed() {
                            if let Some(note) = doc
                                .bars
                                .get_mut(sel.bar)
                                .and_then(|b| b.events.get_mut(sel.event))
                                .and_then(|e| e.notes.iter_mut().find(|n| n.string == sel.string))
                            {
                                note.tech = note.tech.with_param(v);
                            }
                            action = Some(Action::Changed);
                        }
                    });
                }
            }

            if response.has_focus() {
                let now = ui.input(|i| i.time);
                let events = ui.ctx().input(|i| i.events.clone());
                for ev in events {
                    // A fresh document starts with no selection, so the very first
                    // editing keystroke needs somewhere to land.
                    if state.selected.is_none()
                        && matches!(
                            ev,
                            egui::Event::Text(_) | egui::Event::Key { pressed: true, .. }
                        )
                    {
                        set_sel(
                            state,
                            Some(Sel {
                                bar: 0,
                                event: 0,
                                string: 0,
                            }),
                        );
                    }
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
                        egui::Event::Key {
                            key, pressed: true, ..
                        } if matches!(
                            key,
                            egui::Key::Minus | egui::Key::Plus | egui::Key::Equals
                        ) =>
                        {
                            if let Some(sel) = state.selected {
                                let current = selected_dur(doc, sel);
                                let base = step_value(current.base, key == egui::Key::Minus);
                                state.tool_value.base = base;
                                if apply_dur(
                                    state,
                                    doc,
                                    model::Dur {
                                        base,
                                        dots: current.dots,
                                    },
                                ) {
                                    action = Some(Action::Changed);
                                }
                            }
                        }
                        egui::Event::Key {
                            key: egui::Key::Period,
                            pressed: true,
                            ..
                        } => {
                            if let Some(sel) = state.selected {
                                let current = selected_dur(doc, sel);
                                let dots = (current.dots + 1) % (MAX_DOTS + 1);
                                state.tool_value.dots = dots;
                                if apply_dur(
                                    state,
                                    doc,
                                    model::Dur {
                                        base: current.base,
                                        dots,
                                    },
                                ) {
                                    action = Some(Action::Changed);
                                }
                            }
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
    ui.separator();
    // Insert-mode light: bright when a duration change shifts the following
    // music, dim when it absorbs locally. Toggled from the Edit menu.
    if state.shift_following {
        ui.strong(t("status.insert_mode"));
    } else {
        ui.weak(t("status.insert_mode"));
    }
}

/// Write `v` into `tech`'s armed slot and, when `touches_doc`, into the selected
/// note too. `touches_doc` is decided by the caller -- it also gates whether a
/// snapshot is worth taking, which must happen before any mutation.
fn apply_param(
    state: &mut EditorState,
    doc: &mut Document,
    action: &mut Option<Action>,
    tech: Technique,
    v: u8,
    touches_doc: bool,
) {
    if let Some(slot) = param_slot(state, &tech) {
        *slot = v;
    }
    if std::mem::discriminant(&state.tool_tech) == std::mem::discriminant(&tech) {
        state.tool_tech = tech.with_param(v);
    }
    if touches_doc {
        if let Some(sel) = state.selected {
            if let Some(note) = doc
                .bars
                .get_mut(sel.bar)
                .and_then(|b| b.events.get_mut(sel.event))
                .and_then(|e| e.notes.iter_mut().find(|n| n.string == sel.string))
            {
                note.tech = note.tech.with_param(v);
            }
        }
        *action = Some(Action::Changed);
    }
}

fn tech_button(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    doc: &mut Document,
    mut tech: Technique,
    key: &str,
    action: &mut Option<Action>,
) {
    if !tech_visible(state, &tech) {
        return;
    }
    // The call site only ever passes a placeholder number (or none) -- stamp in the
    // current armed value here so the button and any click always agree with
    // param_slot, and state.tool_tech can never hold a stale number.
    if let Some(slot) = param_slot(state, &tech) {
        tech = tech.with_param(*slot);
    }
    let color = rgb(model::technique_color(&tech));
    let armed = std::mem::discriminant(&state.tool_tech) == std::mem::discriminant(&tech);

    let resp = match tech.param() {
        None => ui.add(egui::Button::new(egui::RichText::new(t(key)).color(color)).selected(armed)),
        Some((v0, range)) => {
            let mut v = v0;
            // A drag may only reach the selected note when that note already
            // carries this same technique (by discriminant) -- otherwise it would
            // silently retag the note. Decided once per frame and reused by both
            // widgets below: only one of them can be interacted with per frame.
            let touches_doc = state.selected.is_some_and(|sel| {
                doc.bars
                    .get(sel.bar)
                    .and_then(|b| b.events.get(sel.event))
                    .and_then(|e| e.notes.iter().find(|n| n.string == sel.string))
                    .is_some_and(|n| {
                        std::mem::discriminant(&n.tech) == std::mem::discriminant(&tech)
                    })
            });

            let btn = egui::Frame::new()
                .fill(color.gamma_multiply(0.18))
                .corner_radius(8)
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let btn = ui.add(
                            egui::Button::new(egui::RichText::new(t(key)).color(color))
                                .selected(armed),
                        );
                        let dv = ui
                            .add(egui::DragValue::new(&mut v).range(range.clone()))
                            .on_hover_text(t(param_key(&tech)));
                        if touches_doc && (dv.drag_started() || (dv.changed() && !dv.dragged())) {
                            snapshot(state, doc);
                        }
                        if dv.changed() {
                            apply_param(state, doc, action, tech, v, touches_doc);
                        }
                        btn
                    })
                    .inner
                })
                .inner;

            btn.context_menu(|ui| {
                ui.label(t(param_key(&tech)));
                let dv = ui.add(egui::DragValue::new(&mut v).range(range));
                if touches_doc && (dv.drag_started() || (dv.changed() && !dv.dragged())) {
                    snapshot(state, doc);
                }
                if dv.changed() {
                    apply_param(state, doc, action, tech, v, touches_doc);
                }
            });
            btn
        }
    };

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

    // How the two duration widgets below behave when they change an existing
    // event -- absorb locally (default) or push the rest of the piece forward --
    // lives in the Edit menu now, mirrored by the status bar's "INS" light.
    ui.label(t("tool.value"));
    ui.horizontal_wrapped(|ui| {
        for v in NOTE_VALUES {
            let armed = state.tool_value.base == v;
            if ui
                .selectable_label(armed, strungin::i18n::value_name(v))
                .clicked()
            {
                state.tool_value.base = v;
                let dots = sel
                    .and_then(|s| doc.bars.get(s.bar).and_then(|b| b.events.get(s.event)))
                    .map_or(0, |e| e.dur.dots);
                if apply_dur(state, doc, model::Dur { base: v, dots }) {
                    action = Some(Action::Changed);
                }
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(t("tool.dot"));
        if ui
            .add(egui::DragValue::new(&mut state.tool_value.dots).range(0..=MAX_DOTS))
            .changed()
        {
            let dots = state.tool_value.dots;
            let base = sel
                .and_then(|s| doc.bars.get(s.bar).and_then(|b| b.events.get(s.event)))
                .map_or(NoteValue::default(), |e| e.dur.base);
            if apply_dur(state, doc, model::Dur { base, dots }) {
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
    tech_button(
        ui,
        state,
        doc,
        Technique::SlideIn { from_fret: 0 },
        "tech.slide_in",
        &mut action,
    );
    tech_button(ui, state, doc, Technique::Grace, "tech.grace", &mut action);

    tech_button(
        ui,
        state,
        doc,
        Technique::Bend { quarters: 0 },
        "tech.bend",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::BendRelease { quarters: 0 },
        "tech.bend_release",
        &mut action,
    );
    tech_button(
        ui,
        state,
        doc,
        Technique::PreBend { quarters: 0 },
        "tech.pre_bend",
        &mut action,
    );

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
    tech_button(ui, state, doc, Technique::Slap, "tech.slap", &mut action);
    tech_button(ui, state, doc, Technique::Pop, "tech.pop", &mut action);
    tech_button(ui, state, doc, Technique::Dead, "tech.dead", &mut action);
    tech_button(ui, state, doc, Technique::Ghost, "tech.ghost", &mut action);

    tech_button(
        ui,
        state,
        doc,
        Technique::Trill { to_fret: 0 },
        "tech.trill",
        &mut action,
    );

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
    ui.horizontal(|ui| {
        ui.label(t("bar.time_sig"));
        let valid_bar = bar_idx.filter(|&b| b < doc.bars.len());
        let current = valid_bar.and_then(|b| doc.bars[b].time_sig);
        let label = match (valid_bar, current) {
            (_, Some(sig)) => format!("{}/{}", sig.0, sig.1),
            (Some(b), None) => {
                let (num, den) = doc.time_sig_at(b);
                format!("{} ({num}/{den})", t("bar.time_sig_inherit"))
            }
            (None, None) => t("bar.time_sig_inherit"),
        };
        ui.add_enabled_ui(valid_bar.is_some(), |ui| {
            egui::ComboBox::from_id_salt("bar_time_sig")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    let b = valid_bar.unwrap_or(0);
                    if ui
                        .add_enabled(
                            b != 0,
                            egui::Button::selectable(current.is_none(), t("bar.time_sig_inherit")),
                        )
                        .clicked()
                    {
                        mutate(state, doc, move |doc| engrave::set_time_sig(doc, b, None));
                        action = Some(Action::Changed);
                    }
                    // ponytail: fixed list; a num/den pair if someone asks for 13/16.
                    for sig in [
                        (2u8, 4u8),
                        (3, 4),
                        (4, 4),
                        (5, 4),
                        (6, 8),
                        (7, 8),
                        (9, 8),
                        (12, 8),
                        (2, 2),
                    ] {
                        if ui
                            .selectable_label(current == Some(sig), format!("{}/{}", sig.0, sig.1))
                            .clicked()
                        {
                            mutate(state, doc, move |doc| {
                                engrave::set_time_sig(doc, b, Some(sig))
                            });
                            action = Some(Action::Changed);
                        }
                    }
                });
        });
    });

    ui.separator();
    if ui.button(t("tool.undo")).clicked() && undo(state, doc) {
        action = Some(Action::Changed);
    }

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_technique_has_its_own_visibility_bit() {
        let mut state = EditorState::default();
        for (i, (tech, _, _)) in crate::TECH_LEGEND.iter().enumerate() {
            assert!(tech_visible(&state, tech), "all styles show by default");
            state.tech_shown = !(1 << i);
            assert!(!tech_visible(&state, tech), "bit {i} hides its own style");
            assert!(
                crate::TECH_LEGEND
                    .iter()
                    .filter(|(t, _, _)| std::mem::discriminant(t) != std::mem::discriminant(tech))
                    .all(|(t, _, _)| tech_visible(&state, t)),
                "and hides nothing else"
            );
            state.tech_shown = u32::MAX;
        }
    }

    #[test]
    fn every_parameterised_technique_has_an_armed_slot() {
        let mut state = EditorState::default();
        for (tech, _, _) in crate::TECH_LEGEND.iter() {
            let slot = param_slot(&mut state, tech);
            assert_eq!(
                tech.param().is_some(),
                slot.is_some(),
                "{tech:?}: param() and param_slot() disagree on whether it carries a number"
            );
            if let Some((_, range)) = tech.param() {
                for v in [*range.start(), *range.end()] {
                    assert_eq!(tech.with_param(v).param(), Some((v, range.clone())));
                }
            }
        }
    }

    #[test]
    fn handle_digit_arms_the_tool_value_on_an_empty_cell() {
        let mut doc = Document::new_empty();
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
            tool_value: model::Dur {
                base: NoteValue::Eighth,
                dots: 0,
            },
            ..Default::default()
        };
        assert!(handle_digit(&mut state, &mut doc, 5, 0.0));
        let bar = &doc.bars[0];
        assert_eq!(bar.events[0].dur.base, NoteValue::Eighth);
        assert_eq!(bar.events[0].notes[0].fret, 5);
        assert!(
            bar.events[1].is_rest() && bar.events[1].dur.base == NoteValue::Eighth,
            "the freed time becomes its own eighth rest"
        );
        assert!(engrave::is_complete(bar, (4, 4)));
    }

    #[test]
    fn handle_digit_leaves_duration_alone_on_an_occupied_cell() {
        let mut doc = Document::new_empty();
        doc.bars[0].events[0].notes.push(model::Note {
            string: 0,
            fret: 2,
            tech: Technique::Plain,
            tie_next: false,
        });
        let original_dur = doc.bars[0].events[0].dur;
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
            tool_value: model::Dur {
                base: NoteValue::Eighth,
                dots: 0,
            },
            ..Default::default()
        };
        assert!(handle_digit(&mut state, &mut doc, 7, 0.0));
        let bar = &doc.bars[0];
        assert_eq!(
            bar.events[0].dur, original_dur,
            "correcting a fret must not touch duration"
        );
        assert_eq!(bar.events[0].notes[0].fret, 7);
        assert_eq!(bar.events.len(), 4, "no rest was inserted");
    }

    #[test]
    fn digit_buffer_does_not_leak_across_cells() {
        let mut doc = Document::new_empty();
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
            ..Default::default()
        };
        assert!(handle_digit(&mut state, &mut doc, 1, 0.0));
        set_sel(
            &mut state,
            Some(Sel {
                bar: 0,
                event: 1,
                string: 0,
            }),
        );
        // Still inside DIGIT_WINDOW_SECS: without the fix this combines with the
        // leftover "1" into fret 12 instead of landing fret 2 on its own cell.
        assert!(handle_digit(&mut state, &mut doc, 2, 0.1));

        let bar = &doc.bars[0];
        assert_eq!(bar.events[0].notes[0].fret, 1);
        assert_eq!(bar.events[1].notes[0].fret, 2);
    }

    #[test]
    fn duration_keys_step_through_the_values() {
        assert_eq!(step_value(NoteValue::Quarter, true), NoteValue::Eighth);
        assert_eq!(step_value(NoteValue::Eighth, false), NoteValue::Quarter);
        assert_eq!(
            step_value(NoteValue::Whole, false),
            NoteValue::Whole,
            "longer than Whole clamps instead of wrapping"
        );
        assert_eq!(
            step_value(NoteValue::ThirtySecond, true),
            NoteValue::ThirtySecond,
            "shorter than ThirtySecond clamps instead of wrapping"
        );
    }
}
