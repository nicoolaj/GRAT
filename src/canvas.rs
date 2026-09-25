//! egui rendering of the paginated `Prim`s, and mouse/keyboard editing on top of
//! them: hit-testing, fret entry, the tool palette, and a cheap undo stack.
//!
//! A module of the *binary*, not the library: `lib.rs` stays free of eframe/egui
//! so it keeps building and testing without a window (see its doc comment). This
//! file is the only place in the app that touches egui widgets directly.

use eframe::egui;
use grat::layout::Page;
use grat::model::{Document, Instrument, NoteValue, Strum, Technique, MAX_DOTS, MAX_REPEAT_PLAYS};
use grat::{engrave, i18n::t, model, staff, tablature, Align, Prim, PAGE_H_MM, PAGE_W_MM};

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

impl Sel {
    /// The event this cell sits on, while the document still has it.
    fn event(self, doc: &Document) -> Option<&model::Event> {
        doc.bars.get(self.bar)?.events.get(self.event)
    }

    fn event_mut(self, doc: &mut Document) -> Option<&mut model::Event> {
        doc.bars.get_mut(self.bar)?.events.get_mut(self.event)
    }

    /// The note this cell holds, if any.
    fn note(self, doc: &Document) -> Option<&model::Note> {
        self.event(doc)?
            .notes
            .iter()
            .find(|n| n.string == self.string)
    }

    fn note_mut(self, doc: &mut Document) -> Option<&mut model::Note> {
        self.event_mut(doc)?
            .notes
            .iter_mut()
            .find(|n| n.string == self.string)
    }
}

/// Everything the editor remembers between frames: view state (zoom), selection,
/// the armed tool, and the undo stack. Owned by the app, threaded through every
/// call here.
pub struct EditorState {
    pub zoom: f32,
    pub selected: Option<Sel>,
    /// The other end of a multi-event selection, set by a Shift-click or Shift-arrow
    /// and cleared by any plain click or arrow move. `None` means `selected` is a
    /// single cell, as it always was before copy/paste existed.
    pub range_anchor: Option<Sel>,
    /// Copied/cut events, in document order. Session-only, like `undo_stack`.
    pub clipboard: Vec<model::Event>,
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
            range_anchor: None,
            clipboard: Vec::new(),
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
    /// caller's business (it repaginates once its `layout_dirty` is set).
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
/// The time signatures offered, per bar here and for the whole piece in the
/// Tools menu.
/// Grouped by denominator so a signature is found where a musician looks for it.
/// 6/4 counts in quarters (`engrave::beat_ticks` treats only `/8` and finer as
/// compound).
/// ponytail: fixed list; a num/den pair if someone asks for 13/16.
pub const TIME_SIGS: [(u8, u8); 14] = [
    (2, 2),
    (3, 2),
    (2, 4),
    (3, 4),
    (4, 4),
    (5, 4),
    (6, 4),
    (7, 4),
    (3, 8),
    (5, 8),
    (6, 8),
    (7, 8),
    (9, 8),
    (12, 8),
];

/// Rebar the whole piece in `sig`, undoably. The selection is dropped: the bar
/// count changes, so the indices it held may no longer exist.
pub fn set_time_sig_everywhere(state: &mut EditorState, doc: &mut Document, sig: (u8, u8)) {
    mutate(state, doc, |doc| engrave::set_time_sig_everywhere(doc, sig));
    state.selected = None;
    state.range_anchor = None;
}

/// Change instrument and tuning, undoably (see [`Document::retune`]). A selection
/// on a string that no longer exists is dropped.
pub fn retune(
    state: &mut EditorState,
    doc: &mut Document,
    instrument: Instrument,
    tuning: Vec<u8>,
    keep_pitches: bool,
) {
    mutate(state, doc, |doc| {
        doc.retune(instrument, tuning, keep_pitches)
    });
    let strings = doc.tuning.len();
    if [state.selected, state.range_anchor]
        .iter()
        .flatten()
        .any(|s| s.string as usize >= strings)
    {
        state.selected = None;
        state.range_anchor = None;
    }
}

fn mutate(state: &mut EditorState, doc: &mut Document, f: impl FnOnce(&mut Document)) {
    snapshot(state, doc);
    f(doc);
}

/// Push an undo snapshot, then change the event under `sel`: what most editing
/// keys and palette buttons do.
fn edit_event(
    state: &mut EditorState,
    doc: &mut Document,
    sel: Sel,
    f: impl FnOnce(&mut model::Event),
) {
    mutate(state, doc, |doc| {
        if let Some(event) = sel.event_mut(doc) {
            f(event);
        }
    });
}

/// Swap in another document (New, Open). The selection, the fret being typed
/// and the undo history all belonged to the old one: an undo reaching across
/// would put the old piece under the new file's name, for the next save to write
/// over it. The clipboard stays, so music can be carried from one piece to the next.
pub fn replace_document(state: &mut EditorState, doc: &mut Document, new: Document) {
    *doc = new;
    state.selected = None;
    state.range_anchor = None;
    state.digit_buffer.clear();
    state.digit_deadline = None;
    state.undo_stack.clear();
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

pub(crate) fn rgb(c: grat::Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(c.0, c.1, c.2)
}

/// Paints a calendar decoration's shapes centered on `center` with `radius` pixels --
/// the same rect the logo image itself occupies (splash, About). This is the live,
/// on-screen counterpart of `decorations::bake`, which stamps the same `Decoration`
/// into the dock/taskbar icon's raw pixels once, at startup; this file is the only
/// place in the app that touches egui widgets directly, which is why the renderer
/// lives here rather than alongside the GUI-free shape data in the library.
pub(crate) fn paint_decoration(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    deco: &grat::decorations::Decoration,
) {
    for shape in &deco.shapes {
        match shape {
            grat::decorations::Shape::Circle { c, r, color } => {
                painter.circle_filled(
                    center + egui::vec2(c.0, c.1) * radius,
                    r * radius,
                    rgb(*color),
                );
            }
            grat::decorations::Shape::Poly { pts, color } => {
                let points: Vec<egui::Pos2> = pts
                    .iter()
                    .map(|&(x, y)| center + egui::vec2(x, y) * radius)
                    .collect();
                fill_polygon(painter, points, rgb(*color));
            }
            grat::decorations::Shape::Line { a, b, w, color } => {
                painter.line_segment(
                    [
                        center + egui::vec2(a.0, a.1) * radius,
                        center + egui::vec2(b.0, b.1) * radius,
                    ],
                    egui::Stroke::new(w * radius, rgb(*color)),
                );
            }
        }
    }
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

pub(crate) fn to_screen(page_rect: egui::Rect, zoom: f32, p: grat::P) -> egui::Pos2 {
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
            fill_polygon(painter, points, rgb(*color));
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

/// Fill an outline the way the PDF fills a `Prim::Poly`, whatever its shape.
/// epaint fills any polygon as a fan from its first point, which assumes it is
/// convex: right for a note head or a beam, but it filled a flag's curl into a
/// solid sail. A concave outline is cut into triangles here instead, and edged
/// with a hairline for the anti-aliasing a bare mesh does not get.
fn fill_polygon(painter: &egui::Painter, points: Vec<egui::Pos2>, color: egui::Color32) {
    if is_convex(&points) {
        painter.add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::NONE,
        ));
        return;
    }
    let mut mesh = egui::Mesh::default();
    for &p in &points {
        mesh.colored_vertex(p, color);
    }
    for [a, b, c] in triangulate(&points) {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.add(egui::Shape::closed_line(
        points,
        egui::Stroke::new(0.5, color),
    ));
}

/// z of the cross product of `a -> b` and `a -> c`: positive when `c` lies to
/// the left of `a -> b` (y down on screen, so a counter-clockwise turn).
fn turn(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (b - a).x * (c - a).y - (b - a).y * (c - a).x
}

/// Whether every corner of the outline turns the same way.
fn is_convex(pts: &[egui::Pos2]) -> bool {
    let n = pts.len();
    let turns = (0..n).map(|i| turn(pts[i], pts[(i + 1) % n], pts[(i + 2) % n]));
    let (mut left, mut right) = (false, false);
    for t in turns {
        left |= t > 1e-6;
        right |= t < -1e-6;
    }
    !(left && right)
}

/// Triangles (indices into `pts`) exactly covering a simple outline -- one no
/// edge of which crosses another -- by ear clipping: O(n²), and a flag has 34
/// points.
fn triangulate(pts: &[egui::Pos2]) -> Vec<[usize; 3]> {
    // Twice the signed area: which way the outline winds, so that a corner
    // turning with it is convex whichever way that is.
    let n = pts.len();
    let wind: f32 = (0..n)
        .map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y)
        .sum();
    let mut left: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n.saturating_sub(2));
    while left.len() > 3 {
        let m = left.len();
        let corner = |i: usize| [left[(i + m - 1) % m], left[i], left[(i + 1) % m]];
        let is_ear = |i: usize| {
            let [a, b, c] = corner(i);
            let (pa, pb, pc) = (pts[a], pts[b], pts[c]);
            turn(pa, pb, pc) * wind > 0.0
                && left.iter().all(|&j| {
                    let p = pts[j];
                    j == a
                        || j == b
                        || j == c
                        || turn(pa, pb, p) * wind < 0.0
                        || turn(pb, pc, p) * wind < 0.0
                        || turn(pc, pa, p) * wind < 0.0
                })
        };
        // An outline folding back on itself can leave no clean ear; clipping
        // a corner anyway still ends the loop, where waiting for one would not.
        let i = (0..m).find(|&i| is_ear(i)).unwrap_or(0);
        out.push(corner(i));
        left.remove(i);
    }
    if let [a, b, c] = left[..] {
        out.push([a, b, c]);
    }
    out
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
    let min = to_screen(rect, zoom, grat::P::new(hit.min.x, hit.max.y));
    let max = to_screen(rect, zoom, grat::P::new(hit.max.x, hit.min.y));
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

/// Like `extend_selection`, but for the Shift-arrow path: `move_selection` already
/// knows how to step `selected`, so this only has to plant the anchor first.
fn ensure_anchor(state: &mut EditorState) {
    if state.range_anchor.is_none() {
        state.range_anchor = state.selected;
    }
}

/// Extend the selection from the current cell to `sel` (a Shift-click). Plants the
/// anchor on the first extension, then just moves the live end.
fn extend_selection(state: &mut EditorState, sel: Option<Sel>) {
    let Some(sel) = sel else { return };
    ensure_anchor(state);
    set_sel(state, Some(sel));
}

/// Every `(bar, event)` pair in the document, in reading order -- the same order
/// `next_cell`/`prev_cell` walk by hand. Backs both the range highlight and the
/// paste cursor, which both need to step across bar boundaries.
fn flat_positions(doc: &Document) -> Vec<(usize, usize)> {
    doc.bars
        .iter()
        .enumerate()
        .flat_map(|(b, bar)| (0..bar.events.len()).map(move |e| (b, e)))
        .collect()
}

/// The `(bar, event)` span from `a` to `b`, in document order regardless of which
/// came first or which string either carries -- copy/cut/paste work on whole events.
fn event_span(a: Sel, b: Sel) -> ((usize, usize), (usize, usize)) {
    let ka = (a.bar, a.event);
    let kb = (b.bar, b.event);
    if ka <= kb {
        (ka, kb)
    } else {
        (kb, ka)
    }
}

/// Copy the selected event, or the whole range when `range_anchor` is set, into the
/// clipboard. Returns whether there was anything to copy.
pub fn copy(state: &mut EditorState, doc: &Document) -> bool {
    let Some(sel) = state.selected else {
        return false;
    };
    let anchor = state.range_anchor.unwrap_or(sel);
    let (lo, hi) = event_span(anchor, sel);
    let positions = flat_positions(doc);
    let (Some(i0), Some(i1)) = (
        positions.iter().position(|&p| p == lo),
        positions.iter().position(|&p| p == hi),
    ) else {
        return false;
    };
    state.clipboard = positions[i0..=i1]
        .iter()
        .filter_map(|&(b, e)| doc.bars.get(b)?.events.get(e).cloned())
        .collect();
    !state.clipboard.is_empty()
}

/// Copy the selection, then clear the notes of every event in it -- what Space
/// already does to one cell, extended to the whole range.
pub fn cut(state: &mut EditorState, doc: &mut Document) -> bool {
    if !copy(state, doc) {
        return false;
    }
    let sel = state.selected.expect("copy() returned true");
    let anchor = state.range_anchor.unwrap_or(sel);
    let (lo, hi) = event_span(anchor, sel);
    mutate(state, doc, move |doc| {
        let positions = flat_positions(doc);
        let (Some(i0), Some(i1)) = (
            positions.iter().position(|&p| p == lo),
            positions.iter().position(|&p| p == hi),
        ) else {
            return;
        };
        for &(b, e) in &positions[i0..=i1] {
            if let Some(event) = doc.bars.get_mut(b).and_then(|bar| bar.events.get_mut(e)) {
                event.notes.clear();
            }
        }
    });
    true
}

/// Paste the clipboard starting at the selected cell (or the start of the selected
/// range), one clipboard event per step in document order. Each target event first takes the clipboard event's duration
/// (via `engrave::set_event_dur`, which backfills with rests or swallows what follows
/// so the bar total never changes), then its content -- so eight sixteenths pasted
/// over an empty bar of quarter rests come out as eight sixteenths, not four
/// quarters. Stops at the end of the document rather than inserting bars.
///
/// ponytail: techniques that find their partner by position in the bar (slide,
/// hammer-on/pull-off, trill -- via `Bar::next_on_string`) are copied as-is; at the
/// new position their partner can change or vanish. Reproducing the technique glyph
/// is the goal here, not re-deriving its partner -- upgrade path is a second pass
/// that re-links partners after paste, if that ever bites in practice.
pub fn paste(state: &mut EditorState, doc: &mut Document) -> bool {
    if state.clipboard.is_empty() {
        return false;
    }
    let Some(sel) = state.selected else {
        return false;
    };
    // With a range selected, paste from its start, whichever end is the live one.
    let ((b0, e0), _) = event_span(state.range_anchor.unwrap_or(sel), sel);
    let clip = state.clipboard.clone();
    let mut pasted = false;
    mutate(state, doc, |doc| {
        let (mut b, mut e) = (b0, e0);
        for ev in clip {
            if doc.bars.get(b).is_some_and(|bar| e >= bar.events.len()) {
                (b, e) = (b + 1, 0);
            }
            if b >= doc.bars.len() {
                break;
            }
            let sig = doc.time_sig_at(b);
            let bar = &mut doc.bars[b];
            if e >= bar.events.len() {
                break;
            }
            // Clamped at the bar line when the clipboard event doesn't fit.
            engrave::set_event_dur(bar, sig, e, ev.dur);
            let event = &mut bar.events[e];
            *event = model::Event {
                dur: event.dur,
                ..ev
            };
            // Copied before a retune to fewer strings: those notes have nowhere to go.
            event
                .notes
                .retain(|n| (n.string as usize) < doc.tuning.len());
            pasted = true;
            e += 1;
        }
    });
    pasted
}

fn move_selection(state: &mut EditorState, doc: &Document, key: egui::Key) {
    let Some(sel) = state.selected else { return };
    let next = match key {
        egui::Key::ArrowUp => Sel {
            string: sel.string.saturating_sub(1),
            ..sel
        },
        egui::Key::ArrowDown => Sel {
            string: (sel.string + 1).min(doc.tuning.len().saturating_sub(1) as u8),
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
    // An undo can bring back fewer strings than the selection sits on, and a
    // note written there would index past the tuning the moment it is drawn.
    let Some(sel) = state
        .selected
        .filter(|s| (s.string as usize) < doc.tuning.len())
    else {
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

/// One step shorter (`shorter: true`) or longer than `v`, clamped at both ends:
/// there is no wraparound, so `-` on a thirty-second note (or `+` on a whole
/// note) does nothing.
fn step_value(v: NoteValue, shorter: bool) -> NoteValue {
    let i = NoteValue::ALL.iter().position(|&x| x == v).unwrap_or(0);
    let j = if shorter { i + 1 } else { i.wrapping_sub(1) };
    NoteValue::ALL.get(j).copied().unwrap_or(v)
}

/// The selected event's current duration, or the default if there's no event
/// there. Both the duration keys and the two palette widgets read the half of
/// `Dur` they are *not* currently changing from here (never from
/// `state.tool_value`), so pressing `-` or dragging the dot count doesn't
/// clobber the other half.
fn selected_dur(doc: &Document, sel: Sel) -> model::Dur {
    sel.event(doc).map(|e| e.dur).unwrap_or_default()
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

            // Only the pages in view get built: all of them were, every frame --
            // 140 000 shapes for a 400-bar piece, to show one or two pages. The
            // margin keeps the drop shadow of a page just out of view.
            let clip = painter.clip_rect();
            let zoom = state.zoom;
            let visible = |index: usize| {
                let prect = page_rect(content_min, index, zoom);
                prect.expand(4.0).intersects(clip).then_some(prect)
            };
            for (index, page) in pages.iter().enumerate() {
                let Some(prect) = visible(index) else {
                    continue;
                };
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
            // A multi-event selection: every cell (every string) between the anchor
            // and the live end, lightly shaded -- the live end itself is redrawn
            // below at full strength. One pass over the cells of the pages in view:
            // looking each cell up from the top of the score cost 40 ms a frame for
            // a range near the end of a 400-bar piece.
            if let (Some(anchor), Some(sel)) = (state.range_anchor, state.selected) {
                let (lo, hi) = event_span(anchor, sel);
                for (index, page) in pages.iter().enumerate() {
                    if visible(index).is_none() {
                        continue;
                    }
                    for hit in page
                        .hits
                        .iter()
                        .filter(|h| (lo..=hi).contains(&(h.bar, h.event)))
                    {
                        draw_highlight(
                            &painter,
                            content_min,
                            zoom,
                            index,
                            hit,
                            ACCENT.gamma_multiply(0.3),
                        );
                    }
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
                    if ui.input(|i| i.modifiers.shift) {
                        extend_selection(state, sel);
                    } else {
                        state.range_anchor = None;
                        set_sel(state, sel);
                    }
                }
            }

            // A right click inside a range keeps it, so the menu acts on all of
            // it; anywhere else it selects the one cell under the pointer.
            if response.secondary_clicked() {
                if let Some(p) = response.interact_pointer_pos() {
                    let sel = pick(pages, content_min, state.zoom, p);
                    let inside = match (sel, state.selected, state.range_anchor) {
                        (Some(s), Some(cur), Some(anchor)) => {
                            let (lo, hi) = event_span(anchor, cur);
                            (lo..=hi).contains(&(s.bar, s.event))
                        }
                        _ => false,
                    };
                    if !inside {
                        state.range_anchor = None;
                        set_sel(state, sel);
                    }
                }
            }
            // The right-click menu: the note's own number when its technique
            // carries one (a `Hit` exists even for an empty cell), then the Bar
            // menu for the selected bars. context_menu() has to be called every
            // frame (not just on the click) for the popup to stay open -- it does
            // its own secondary-click detection, so this runs unconditionally and
            // is a no-op most frames.
            if let Some(sel) = state.selected {
                let param = sel
                    .note(doc)
                    .and_then(|n| n.tech.param().map(|p| (n.tech, p)));
                response.context_menu(|ui| {
                    if let Some((tech, (v0, range))) = param {
                        let mut v = v0;
                        ui.label(t(param_key(&tech)));
                        let dv = ui.add(egui::DragValue::new(&mut v).range(range));
                        if dv.drag_started() || (dv.changed() && !dv.dragged()) {
                            snapshot(state, doc);
                        }
                        if dv.changed() {
                            if let Some(note) = sel.note_mut(doc) {
                                note.tech = note.tech.with_param(v);
                            }
                            action = Some(Action::Changed);
                        }
                        ui.separator();
                    }
                    if bar_menu(ui, state, doc) {
                        action = Some(Action::Changed);
                    }
                });
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
                                edit_event(state, doc, sel, |e| {
                                    e.notes.retain(|n| n.string != sel.string)
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
                                edit_event(state, doc, sel, |e| e.notes.clear());
                                action = Some(Action::Changed);
                            }
                        }
                        egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } if matches!(
                            key,
                            egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::ArrowLeft
                                | egui::Key::ArrowRight
                        ) =>
                        {
                            if modifiers.shift {
                                ensure_anchor(state);
                            } else {
                                state.range_anchor = None;
                            }
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
        if let Some(note) = state.selected.and_then(|sel| sel.note_mut(doc)) {
            note.tech = note.tech.with_param(v);
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
            let touches_doc = state
                .selected
                .and_then(|sel| sel.note(doc))
                .is_some_and(|n| std::mem::discriminant(&n.tech) == std::mem::discriminant(&tech));

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
        if let Some(sel) = state.selected.filter(|sel| sel.note(doc).is_some()) {
            mutate(state, doc, |doc| {
                if let Some(note) = sel.note_mut(doc) {
                    note.tech = tech;
                }
            });
            *action = Some(Action::Changed);
        }
    }
}

/// The bars the selection covers, first and last, while the document has them.
fn selected_bars(state: &EditorState, doc: &Document) -> Option<(usize, usize)> {
    let sel = state.selected?;
    let anchor = state.range_anchor.unwrap_or(sel);
    let (first, last) = (sel.bar.min(anchor.bar), sel.bar.max(anchor.bar));
    (last < doc.bars.len()).then_some((first, last))
}

/// The bars a range selection (Shift + click) covers, which the live player
/// loops; `None` for a lone cell.
pub fn range_bars(state: &EditorState, doc: &Document) -> Option<(usize, usize)> {
    state.range_anchor?;
    selected_bars(state, doc)
}

/// Repeat the selected bars: an opening repeat on the first, a closing one on
/// the last -- or, where they carry exactly that already, take both off.
pub fn repeat_selection(state: &mut EditorState, doc: &mut Document) -> bool {
    let Some((first, last)) = selected_bars(state, doc) else {
        return false;
    };
    let on = doc.bars[first].repeat_start && doc.bars[last].repeat_end.is_some();
    mutate(state, doc, |doc| {
        doc.bars[first].repeat_start = !on;
        let end = &mut doc.bars[last].repeat_end;
        *end = if on { None } else { Some(end.unwrap_or(2)) };
    });
    true
}

/// The selected bars' repeat marks: an opening repeat on the first bar, a
/// closing one on the last, and how many times the passage plays. The palette,
/// the Bar menu and the right-click menu all show these same controls. True
/// when the document changed.
pub fn repeat_controls(ui: &mut egui::Ui, state: &mut EditorState, doc: &mut Document) -> bool {
    let bars = selected_bars(state, doc);
    let mut changed = false;
    let mut start = bars.is_some_and(|(first, _)| doc.bars[first].repeat_start);
    if ui
        .add_enabled(
            bars.is_some(),
            egui::Checkbox::new(&mut start, t("bar.repeat_start")),
        )
        .changed()
    {
        if let Some((first, _)) = bars {
            mutate(state, doc, |doc| doc.bars[first].repeat_start = start);
            changed = true;
        }
    }
    let plays = bars.and_then(|(_, last)| doc.bars[last].repeat_end);
    ui.horizontal(|ui| {
        let mut end = plays.is_some();
        if ui
            .add_enabled(
                bars.is_some(),
                egui::Checkbox::new(&mut end, t("bar.repeat_end")),
            )
            .changed()
        {
            if let Some((_, last)) = bars {
                mutate(state, doc, |doc| {
                    doc.bars[last].repeat_end = end.then_some(2)
                });
                changed = true;
            }
        }
        if let (Some(mut n), Some((_, last))) = (plays, bars) {
            let dv = ui
                .add(
                    egui::DragValue::new(&mut n)
                        .range(2..=MAX_REPEAT_PLAYS)
                        .prefix("×"),
                )
                .on_hover_text(t("bar.repeat_plays"));
            if dv.drag_started() || (dv.changed() && !dv.dragged()) {
                snapshot(state, doc);
            }
            if dv.changed() {
                doc.bars[last].repeat_end = Some(n);
                changed = true;
            }
        }
    });
    changed
}

/// What the Bar menu does to the bars themselves: each takes the selection's
/// first and last bar, changes the document, and names the bar to select after.
type BarOp = fn(&mut Document, usize, usize) -> usize;
const BAR_OPS: [(&str, BarOp); 4] = [
    ("menu.insert_bar_before", |doc, first, _| {
        doc.insert_bar(first);
        first
    }),
    ("menu.insert_bar_after", |doc, _, last| {
        doc.insert_bar(last + 1);
        last + 1
    }),
    ("menu.duplicate_bars", |doc, first, last| {
        doc.duplicate_bars(first, last);
        last + 1
    }),
    ("menu.delete_bars", |doc, first, last| {
        doc.delete_bars(first, last);
        first
    }),
];

/// Run `op` on the selected bars as one undo step, then select the first cell of
/// the bar it names, on the same string: the new bar, the copy, or whatever
/// took the deleted bars' place.
fn bar_op(state: &mut EditorState, doc: &mut Document, op: BarOp) -> bool {
    let Some((first, last)) = selected_bars(state, doc) else {
        return false;
    };
    let string = state.selected.map_or(0, |s| s.string);
    let mut bar = first;
    mutate(state, doc, |doc| bar = op(doc, first, last));
    state.range_anchor = None;
    set_sel(
        state,
        Some(Sel {
            bar: bar.min(doc.bars.len().saturating_sub(1)),
            event: 0,
            string,
        }),
    );
    true
}

/// The Bar menu, shown both in the menu bar and at a right click on the score:
/// everything done to the selected bars. True when the document changed.
pub fn bar_menu(ui: &mut egui::Ui, state: &mut EditorState, doc: &mut Document) -> bool {
    let enabled = selected_bars(state, doc).is_some();
    let mut changed = false;
    let shortcut = ui.ctx().format_shortcut(&crate::SHORTCUT_REPEAT);
    if ui
        .add_enabled(
            enabled,
            egui::Button::new(t("menu.repeat_selection")).shortcut_text(shortcut),
        )
        .clicked()
    {
        changed |= repeat_selection(state, doc);
        ui.close();
    }
    changed |= repeat_controls(ui, state, doc);
    ui.separator();
    for (key, op) in BAR_OPS {
        if ui.add_enabled(enabled, egui::Button::new(t(key))).clicked() {
            changed |= bar_op(state, doc, op);
            ui.close();
        }
    }
    changed
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
        for v in NoteValue::ALL {
            let armed = state.tool_value.base == v;
            if ui
                .selectable_label(armed, grat::i18n::value_name(v))
                .clicked()
            {
                state.tool_value.base = v;
                let dots = sel.and_then(|s| s.event(doc)).map_or(0, |e| e.dur.dots);
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
                .and_then(|s| s.event(doc))
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
            edit_event(state, doc, sel, |e| e.notes.clear());
            action = Some(Action::Changed);
        }
    }

    ui.separator();
    // One button per technique, in `TECH_LEGEND`'s order: the Help legend's and
    // the note-styles menu's, grouped by kind.
    for &(tech, key, _) in crate::TECH_LEGEND {
        tech_button(ui, state, doc, tech, key, &mut action);
    }

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
                edit_event(state, doc, sel, |e| {
                    e.strum = if e.strum == Some(strum) {
                        None
                    } else {
                        Some(strum)
                    };
                });
                action = Some(Action::Changed);
            }
        }
    }

    ui.separator();
    let (mut palm_mute, mut let_ring) = sel
        .and_then(|s| s.event(doc))
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
            edit_event(state, doc, sel, |e| e.palm_mute = palm_mute);
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
            edit_event(state, doc, sel, |e| e.let_ring = let_ring);
            action = Some(Action::Changed);
        }
    }

    ui.separator();
    let bar_idx = sel.map(|s| s.bar);
    if repeat_controls(ui, state, doc) {
        action = Some(Action::Changed);
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
            // Uncapped: egui's default 200 px scrolled the tail of the list,
            // 2/2 included, out of sight.
            egui::ComboBox::from_id_salt("bar_time_sig")
                .selected_text(label)
                .height(f32::INFINITY)
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
                    for sig in TIME_SIGS {
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
    fn every_offered_time_sig_rebars_and_prints() {
        for sig in TIME_SIGS {
            let mut doc = Document::new_empty();
            doc.bars[0].events[0].notes.push(model::Note {
                string: 0,
                fret: 3,
                tech: Technique::Plain,
                tie_next: false,
            });
            engrave::set_time_sig_everywhere(&mut doc, sig);
            assert_eq!(doc.time_sig_at(0), sig);
            for bar in &doc.bars {
                assert!(engrave::is_complete(bar, sig), "{sig:?}: {bar:?}");
            }
            assert!(!grat::layout::paginate(&doc).is_empty(), "{sig:?}");
        }
    }

    #[test]
    fn retune_to_fewer_strings_keeps_the_selection_on_the_staff() {
        let mut state = EditorState::default();
        let mut doc = Document::new_empty();
        let bass = Instrument::Bass.default_tuning().to_vec();

        state.selected = Some(Sel {
            bar: 0,
            event: 0,
            string: 5,
        });
        retune(&mut state, &mut doc, Instrument::Bass, bass.clone(), false);
        assert_eq!(state.selected, None, "string 6 no longer exists");

        let low = Sel {
            bar: 0,
            event: 0,
            string: 3,
        };
        state.selected = Some(low);
        move_selection(&mut state, &doc, egui::Key::ArrowDown);
        assert_eq!(state.selected, Some(low), "the bottom string is the floor");
        assert!(undo(&mut state, &mut doc));
        assert_eq!(doc.tuning.len(), 6, "a retune is one undo step");
    }

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
    fn a_digit_on_a_string_the_tuning_lacks_writes_nothing() {
        let mut doc = Document::new_empty();
        doc.retune(
            Instrument::Bass,
            Instrument::Bass.default_tuning().to_vec(),
            false,
        );
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 0,
                string: 5,
            }),
            ..Default::default()
        };
        assert!(!handle_digit(&mut state, &mut doc, 3, 0.0));
        assert!(doc.bars[0].events[0].is_rest());
        // The note it used to write panicked here, indexing the tuning.
        assert!(!grat::layout::paginate(&doc).is_empty());
    }

    #[test]
    fn replacing_the_document_forgets_its_history_and_selection() {
        let mut doc = Document::new_empty();
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 0,
                string: 5,
            }),
            ..Default::default()
        };
        assert!(handle_digit(&mut state, &mut doc, 7, 0.0));
        assert!(copy(&mut state, &doc));

        let mut bass = Document::new_empty();
        bass.retune(
            Instrument::Bass,
            Instrument::Bass.default_tuning().to_vec(),
            false,
        );
        replace_document(&mut state, &mut doc, bass.clone());
        assert_eq!(doc, bass);
        assert_eq!(state.selected, None);
        assert!(
            !undo(&mut state, &mut doc),
            "no way back into the old piece"
        );
        assert_eq!(doc, bass);
        assert!(!state.clipboard.is_empty(), "the clipboard carries over");
    }

    /// How many shapes one frame of the page view paints in a 1000x720 window,
    /// run headless. The first pass loads the fonts and sizes the scroll area.
    fn shapes_painted(state: &mut EditorState, doc: &mut Document, pages: &[Page]) -> usize {
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 720.0),
            )),
            ..Default::default()
        };
        let mut frame = || {
            let mut out = ctx.run_ui(input(), |ui| {
                show(ui, state, doc, pages);
            });
            // No renderer to hand the font atlas to; egui asserts it was taken.
            out.textures_delta.clear();
            out.shapes.len()
        };
        frame();
        frame()
    }

    #[test]
    fn only_the_pages_in_view_are_painted() {
        let mut doc = Document::new_empty();
        doc.bars = (0..300).map(|_| model::Bar::new_empty(None)).collect();
        let pages = grat::layout::paginate(&doc);
        assert!(pages.len() >= 5, "{} pages", pages.len());
        let painted = shapes_painted(&mut EditorState::default(), &mut doc, &pages);
        assert!(
            painted < pages[0].prims.len() + pages[1].prims.len(),
            "{painted} shapes for a window that shows page 1"
        );
    }

    #[test]
    fn a_range_shades_every_string_of_every_event_in_it() {
        let mut doc = Document::new_empty(); // bars of four quarter rests
        let pages = grat::layout::paginate(&doc);
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 1,
                event: 3,
                string: 2,
            }),
            ..Default::default()
        };
        let one_cell = shapes_painted(&mut state, &mut doc, &pages);
        state.range_anchor = Some(Sel {
            bar: 0,
            event: 2,
            string: 0,
        });
        let range = shapes_painted(&mut state, &mut doc, &pages);
        // Bar 1's events 3 and 4, then all four of bar 2: six events, six strings.
        assert_eq!(range - one_cell, 6 * 6);
    }

    fn area(pts: &[egui::Pos2]) -> f32 {
        let n = pts.len();
        (0..n)
            .map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y)
            .sum::<f32>()
            .abs()
            / 2.0
    }

    /// Every outline the screen fills that is not a plain quad or a head: the
    /// staff's flags, and each calendar decoration's shapes.
    fn outlines() -> Vec<Vec<egui::Pos2>> {
        let mut doc = Document::new_empty();
        doc.rows = vec![model::Row::Tab, model::Row::Notation];
        let bar = &mut doc.bars[0];
        for (i, base) in [(0, NoteValue::Eighth), (2, NoteValue::Sixteenth)] {
            engrave::set_event_dur(bar, (4, 4), i, model::Dur { base, dots: 0 });
            bar.events[i].notes.push(model::Note {
                string: 0,
                fret: 3,
                tech: Technique::Plain,
                tie_next: false,
            });
        }
        let pos = |x: f32, y: f32| egui::pos2(x, -y);
        let flags = grat::layout::paginate(&doc).remove(0).prims.into_iter();
        let flags = flags.filter_map(|p| match p {
            Prim::Poly { pts, .. } if pts.len() > 20 => {
                Some(pts.iter().map(|p| pos(p.x, p.y)).collect())
            }
            _ => None,
        });
        let decorations = grat::decorations::DECORATED_DATES
            .iter()
            .filter_map(|&(m, d)| grat::decorations::decoration_for(m, d))
            .flat_map(|deco| deco.shapes)
            .filter_map(|s| match s {
                grat::decorations::Shape::Poly { pts, .. } => {
                    Some(pts.iter().map(|&(x, y)| pos(x, y)).collect())
                }
                _ => None,
            });
        flags.chain(decorations).collect()
    }

    #[test]
    fn a_concave_outline_is_filled_exactly_not_fanned() {
        let outlines = outlines();
        let concave: Vec<_> = outlines.iter().filter(|o| !is_convex(o)).collect();
        assert!(concave.len() >= 3, "the flags, at least, are concave");
        for pts in &outlines {
            let covered: f32 = triangulate(pts)
                .iter()
                .map(|&[a, b, c]| area(&[pts[a], pts[b], pts[c]]))
                .sum();
            let want = area(pts);
            assert!(
                (covered - want).abs() <= want * 1e-3,
                "{} points: triangles cover {covered}, the outline {want}",
                pts.len()
            );
        }
        // What epaint's fan from the first point covered instead.
        let fan: f32 = (1..concave[0].len() - 1)
            .map(|i| area(&[concave[0][0], concave[0][i], concave[0][i + 1]]))
            .sum();
        assert!(
            fan > area(concave[0]) * 1.2,
            "{fan} vs {}",
            area(concave[0])
        );
    }

    #[test]
    fn note_heads_and_beams_keep_the_convex_fill() {
        let head = staff::ellipse(grat::P::new(10.0, 5.0), 1.6, 1.2, -21.0);
        let head: Vec<_> = head.iter().map(|p| egui::pos2(p.x, -p.y)).collect();
        assert!(is_convex(&head));
        let beam = [(0.0, 0.0), (8.0, 1.0), (8.0, 2.2), (0.0, 1.2)].map(|(x, y)| egui::pos2(x, y));
        assert!(is_convex(&beam));
    }

    #[test]
    fn a_bar_operation_selects_the_bar_it_made() {
        let mut doc = Document::new_empty();
        let len = doc.bars.len();
        let cell = |bar, string| Sel {
            bar,
            event: 2,
            string,
        };
        let mut state = EditorState {
            selected: Some(cell(3, 4)),
            range_anchor: Some(cell(2, 0)),
            ..Default::default()
        };
        let [before, after, duplicate, delete] = BAR_OPS.map(|(_, op)| op);
        assert!(bar_op(&mut state, &mut doc, duplicate));
        assert_eq!(doc.bars.len(), len + 2);
        assert_eq!(
            state.selected,
            Some(Sel {
                event: 0,
                ..cell(4, 4)
            }),
            "the copy"
        );
        assert_eq!(state.range_anchor, None);
        assert!(bar_op(&mut state, &mut doc, after));
        assert_eq!(state.selected.map(|s| s.bar), Some(5));
        assert!(bar_op(&mut state, &mut doc, before));
        assert_eq!(state.selected.map(|s| s.bar), Some(5));
        assert_eq!(doc.bars.len(), len + 4);
        assert!(bar_op(&mut state, &mut doc, delete));
        assert_eq!(doc.bars.len(), len + 3);
        for _ in 0..4 {
            assert!(undo(&mut state, &mut doc), "one undo step each");
        }
        assert_eq!(doc.bars.len(), len);
    }

    #[test]
    fn repeating_the_selection_marks_its_first_and_last_bars() {
        let mut doc = Document::new_empty();
        let cell = |bar| Sel {
            bar,
            event: 0,
            string: 0,
        };
        let mut state = EditorState {
            selected: Some(cell(3)),
            range_anchor: Some(cell(1)),
            ..Default::default()
        };
        assert!(repeat_selection(&mut state, &mut doc));
        assert!(doc.bars[1].repeat_start && doc.bars[3].repeat_end == Some(2));
        assert!(!doc.bars[2].repeat_start && doc.bars[2].repeat_end.is_none());

        // A count already chosen survives taking the marks off and on again.
        doc.bars[3].repeat_end = Some(4);
        assert!(repeat_selection(&mut state, &mut doc));
        assert!(!doc.bars[1].repeat_start && doc.bars[3].repeat_end.is_none());
        assert!(undo(&mut state, &mut doc), "one undo step");
        assert_eq!(doc.bars[3].repeat_end, Some(4));

        state.range_anchor = None;
        state.selected = Some(cell(99));
        assert!(!repeat_selection(&mut state, &mut doc), "no such bar");
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

    #[test]
    fn extend_selection_plants_anchor_once() {
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
            ..Default::default()
        };
        extend_selection(
            &mut state,
            Some(Sel {
                bar: 0,
                event: 2,
                string: 0,
            }),
        );
        assert_eq!(
            state.range_anchor,
            Some(Sel {
                bar: 0,
                event: 0,
                string: 0
            })
        );

        extend_selection(
            &mut state,
            Some(Sel {
                bar: 1,
                event: 1,
                string: 0,
            }),
        );
        assert_eq!(
            state.range_anchor,
            Some(Sel {
                bar: 0,
                event: 0,
                string: 0
            }),
            "anchor stays put on a second extension"
        );
        assert_eq!(
            state.selected,
            Some(Sel {
                bar: 1,
                event: 1,
                string: 0
            })
        );
    }

    #[test]
    fn copy_then_paste_a_range_of_events() {
        let mut doc = Document::new_empty();
        doc.bars[0].events[0].notes.push(model::Note {
            string: 0,
            fret: 3,
            tech: Technique::Plain,
            tie_next: false,
        });
        doc.bars[0].events[1].notes.push(model::Note {
            string: 1,
            fret: 5,
            tech: Technique::Plain,
            tie_next: false,
        });
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 1,
                string: 0,
            }),
            range_anchor: Some(Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
            ..Default::default()
        };
        assert!(copy(&mut state, &doc));
        assert_eq!(state.clipboard.len(), 2);

        state.range_anchor = None;
        state.selected = Some(Sel {
            bar: 1,
            event: 0,
            string: 0,
        });
        assert!(paste(&mut state, &mut doc));

        assert_eq!(doc.bars[1].events[0].notes[0].fret, 3);
        assert_eq!(doc.bars[1].events[1].notes[0].fret, 5);
        assert_eq!(doc.bars[1].events[1].notes[0].string, 1);
    }

    #[test]
    fn paste_carries_the_clipboard_rhythm() {
        let mut doc = Document::new_empty(); // 4/4, bars of 4 quarter rests
        let sixteenth = model::Dur {
            base: NoteValue::Sixteenth,
            dots: 0,
        };
        let mut state = EditorState {
            clipboard: (0..8)
                .map(|fret| model::Event {
                    dur: sixteenth,
                    notes: vec![model::Note {
                        string: 1,
                        fret,
                        tech: Technique::Plain,
                        tie_next: false,
                    }],
                    ..Default::default()
                })
                .collect(),
            selected: Some(Sel {
                bar: 1,
                event: 0,
                string: 0,
            }),
            ..Default::default()
        };
        assert!(paste(&mut state, &mut doc));
        let bar = &doc.bars[1];
        assert!(bar.events[..8].iter().all(|e| e.dur == sixteenth));
        assert_eq!(bar.events[7].notes[0].fret, 7);
        assert!(engrave::is_complete(bar, doc.time_sig_at(1)));
        assert!(doc.bars[2].events.iter().all(|e| e.is_rest()));
    }

    #[test]
    fn cut_clears_notes_across_the_range() {
        let mut doc = Document::new_empty();
        doc.bars[0].events[0].notes.push(model::Note {
            string: 0,
            fret: 3,
            tech: Technique::Plain,
            tie_next: false,
        });
        doc.bars[0].events[1].notes.push(model::Note {
            string: 1,
            fret: 5,
            tech: Technique::Plain,
            tie_next: false,
        });
        let mut state = EditorState {
            selected: Some(Sel {
                bar: 0,
                event: 1,
                string: 0,
            }),
            range_anchor: Some(Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
            ..Default::default()
        };
        assert!(cut(&mut state, &mut doc));
        assert_eq!(state.clipboard.len(), 2);
        assert_eq!(state.clipboard[0].notes[0].fret, 3);
        assert!(doc.bars[0].events[0].is_rest());
        assert!(doc.bars[0].events[1].is_rest());
    }

    #[test]
    fn paste_near_the_end_of_the_document_truncates() {
        let mut doc = Document::new_empty(); // 8 bars x 4 quarter rests each
        let note_event = |fret: u8| model::Event {
            dur: model::Dur {
                base: NoteValue::Quarter,
                dots: 0,
            },
            notes: vec![model::Note {
                string: 0,
                fret,
                tech: Technique::Plain,
                tie_next: false,
            }],
            strum: None,
            palm_mute: false,
            let_ring: false,
        };
        let mut state = EditorState {
            clipboard: vec![note_event(1), note_event(2), note_event(3)],
            selected: Some(Sel {
                bar: 7,
                event: 3,
                string: 0,
            }),
            ..Default::default()
        };
        assert!(paste(&mut state, &mut doc));
        assert_eq!(
            doc.bars[7].events[3].notes[0].fret, 1,
            "only the first clipboard event fit before the document ran out"
        );
    }

    #[test]
    fn paste_keeps_the_target_duration() {
        let mut doc = Document::new_empty(); // quarter rests
        let mut state = EditorState {
            clipboard: vec![model::Event {
                dur: model::Dur {
                    base: NoteValue::Sixteenth,
                    dots: 1,
                },
                notes: vec![model::Note {
                    string: 0,
                    fret: 8,
                    tech: Technique::Plain,
                    tie_next: false,
                }],
                ..Default::default()
            }],
            selected: Some(Sel {
                bar: 0,
                event: 0,
                string: 0,
            }),
            ..Default::default()
        };
        assert!(paste(&mut state, &mut doc));
        assert_eq!(doc.bars[0].events[0].notes[0].fret, 8);
        assert!(engrave::is_complete(&doc.bars[0], doc.time_sig_at(0)));
    }
}
