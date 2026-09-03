//! Page layout and pagination: turns a [`Document`] into a sequence of A4 pages,
//! each a flat list of [`Prim`]s plus the clickable [`tablature::Hit`] boxes the
//! mouse editor hit-tests against.
//!
//! Line breaking (bars onto a system) and page breaking (blocks onto a page) are
//! both greedy, and both always take at least one more than a strict fit would
//! allow: a lone over-wide bar still gets its own system, a lone over-tall block
//! still gets its own page, rather than looping forever or producing nothing.

use std::ops::Range;

use crate::engrave::{self, Spacing};
use crate::i18n;
use crate::model::{Bar, BlockModel, Document, StaffOrder};
use crate::notation;
use crate::staff::{self, FAINT, INK};
use crate::tablature;
use crate::{Align, Prim, Rgb, MARGIN_MM, P, PAGE_H_MM, PAGE_W_MM};

/// One finished page: primitives in page millimetres, plus the clickable cells on it.
pub struct Page {
    pub prims: Vec<Prim>,
    pub hits: Vec<tablature::Hit>,
}

// Title block (page 1 only): title, a gap, the author, then a gap before the
// first system. Reduces the usable content height on page 1 only.
const TITLE_CAP_MM: f32 = 6.0;
const AUTHOR_CAP_MM: f32 = 3.0;
const TITLE_GAP_MM: f32 = 3.0;
const TITLE_BLOCK_GAP_MM: f32 = 6.0;
const TITLE_BLOCK_MM: f32 = TITLE_CAP_MM + TITLE_GAP_MM + AUTHOR_CAP_MM + TITLE_BLOCK_GAP_MM;

// The running header (odd pages from 3 on) and the footer (every page) are both
// drawn inside the page's existing top/bottom margins, so neither one takes any
// extra bite out of the content height the way the title block does.
const RUNNING_HEADER_CAP_MM: f32 = 2.2;
const FOOTER_CAP_MM: f32 = 3.0;

/// Air between two blocks stacked on the same page.
const BLOCK_GAP_MM: f32 = 6.0;

/// Warning rule under an incomplete bar's tablature staff.
const WARN_COLOR: Rgb = Rgb(0xFF, 0x95, 0x00);
const WARN_OFFSET_MM: f32 = 0.8;
const WARN_W_MM: f32 = 0.35;

/// One justification scale for the whole document, rather than one per system.
///
/// A bar's width has to depend only on what the bar holds: four quarters must
/// occupy the same room wherever they land, so that two systems carrying the same
/// music have their barlines in the same columns down the page. Stretching each
/// system to the right margin on its own breaks precisely that — a system that
/// happens to end on a short bar is stretched harder than its neighbour, and bars
/// that hold identical music drift apart by the difference.
///
/// So the widest system is the one that fills the line, and every other system
/// ends short of it. The ragged right margin is the cost of homogeneous bars;
/// there is no way to have both.
///
/// ponytail: a bar too wide for the line (only reachable with an extreme time
/// signature) drags the whole document's scale down with it, where per-system
/// justification only squeezed its own line. Clamped at [`engrave::MIN_SQUEEZE`]
/// like before; give that one bar its own scale if it ever actually happens.
fn justification_scale(naturals: &[f32], music_width: f32) -> f32 {
    let widest = naturals.iter().copied().fold(0.0_f32, f32::max);
    if widest > 0.0 {
        (music_width / widest).max(engrave::MIN_SQUEEZE)
    } else {
        1.0
    }
}

/// Lay out the whole document onto A4 pages.
pub fn paginate(doc: &Document) -> Vec<Page> {
    let usable_width = PAGE_W_MM - 2.0 * MARGIN_MM;
    let music_width = usable_width - tablature::HEAD_MM;
    let systems = break_lines(doc, music_width);

    // Each system's target width: its own natural width taken through the one
    // document-wide scale, so `system_spacing` re-derives that same scale for
    // every system instead of a different one per line.
    let naturals: Vec<f32> = systems
        .iter()
        .map(|r| engrave::system_spacing(doc, r.clone(), None).width)
        .collect();
    let scale = justification_scale(&naturals, music_width);
    let systems: Vec<(Range<usize>, f32)> = systems
        .into_iter()
        .zip(naturals.iter().map(|n| n * scale))
        .collect();

    let block_h = block_height(doc);
    let capacity = |content_top: f32| -> usize {
        let content_height = content_top - MARGIN_MM;
        (((content_height + BLOCK_GAP_MM) / (block_h + BLOCK_GAP_MM)).floor()).max(1.0) as usize
    };
    let cap_first = capacity(PAGE_H_MM - MARGIN_MM - TITLE_BLOCK_MM);
    let cap_rest = capacity(PAGE_H_MM - MARGIN_MM);

    let mut chunks: Vec<&[(Range<usize>, f32)]> = Vec::new();
    if systems.is_empty() {
        // Still one page, furniture only — never zero pages.
        chunks.push(&[]);
    } else {
        let mut idx = 0;
        while idx < systems.len() {
            let cap = if chunks.is_empty() {
                cap_first
            } else {
                cap_rest
            };
            let take = cap.min(systems.len() - idx).max(1);
            chunks.push(&systems[idx..idx + take]);
            idx += take;
        }
    }

    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, sys)| render_page(doc, sys, i + 1, total, block_h))
        .collect()
}

/// The whole document laid out on one endless line, for the live player.
///
/// The same block, the same shared [`Spacing`], the same row renderers as a page
/// — only the line breaking is gone. Every bar keeps its natural width, so
/// nothing is stretched to a margin and the music passes the playhead at a
/// steady speed.
pub struct Strip {
    pub prims: Vec<Prim>,
    /// One box per string per event, exactly as on a page: the live player
    /// highlights an event's column by taking the union of its six.
    pub hits: Vec<tablature::Hit>,
    /// Event positions, relative to the first barline at [`Strip::x`].
    spacing: Spacing,
    /// x of the first barline; the staff head is drawn in `[0, x]`.
    pub x: f32,
    /// The block occupies y in `[0, height]`.
    pub height: f32,
}

impl Strip {
    /// x of one event on the line, in strip millimetres. `bar` indexes
    /// `Document::bars` directly: a strip holds every bar, so a bar's position in
    /// the line and its index in the document are the same number.
    pub fn event_x(&self, bar: usize, event: usize) -> Option<f32> {
        self.spacing.event_x(bar, event).map(|x| self.x + x)
    }

    /// x where the music ends.
    pub fn end_x(&self) -> f32 {
        self.x + self.spacing.width
    }
}

/// Lay `doc` out as a single [`Strip`].
pub fn strip(doc: &Document) -> Strip {
    let height = block_height(doc);
    let spacing = engrave::system_spacing(doc, 0..doc.bars.len(), None);
    let mut prims = Vec::new();
    let mut hits = Vec::new();
    place_block(
        doc,
        &spacing,
        tablature::HEAD_MM,
        height,
        &mut prims,
        &mut hits,
    );
    Strip {
        prims,
        hits,
        spacing,
        x: tablature::HEAD_MM,
        height,
    }
}

/// Greedy line breaking: bars pile onto a system while their natural width still
/// fits, and every system gets at least one bar even if that one alone overflows.
fn break_lines(doc: &Document, music_width: f32) -> Vec<Range<usize>> {
    let h = doc.note_spacing;
    let mut systems = Vec::new();
    let mut i = 0;
    while i < doc.bars.len() {
        let mut width = engrave::natural_bar_width(&doc.bars[i], h);
        let mut j = i + 1;
        while j < doc.bars.len() {
            let w = engrave::natural_bar_width(&doc.bars[j], h);
            if width + w > music_width {
                break;
            }
            width += w;
            j += 1;
        }
        systems.push(i..j);
        i = j;
    }
    systems
}

/// Keep at least one completely empty system at the end of the document, so the
/// editor always has a fresh line to write on: the moment the last line takes a
/// note, the next one is already there. The editor calls this every frame.
///
/// Only ever appends empty bars, never removes, and leaves a document with no
/// bars at all untouched. It settles after a few pushes — an empty bar is a
/// little narrower than a full system — but the loop is capped regardless.
pub fn ensure_trailing_blank_system(doc: &mut Document) {
    let music_width = PAGE_W_MM - 2.0 * MARGIN_MM - tablature::HEAD_MM;
    for _ in 0..64 {
        let systems = break_lines(doc, music_width);
        let Some(last) = systems.last() else { return };
        let has_notes = last.clone().any(|bi| {
            doc.bars
                .get(bi)
                .is_some_and(|b| b.events.iter().any(|e| !e.is_rest()))
        });
        if !has_notes {
            return;
        }
        let sig = doc.time_sig_at(doc.bars.len() - 1);
        doc.bars.push(Bar {
            events: Bar::new_empty(Some(sig)).events,
            ..Bar::default()
        });
    }
}

fn render_page(
    doc: &Document,
    systems: &[(Range<usize>, f32)],
    page_no: usize,
    total: usize,
    block_h: f32,
) -> Page {
    let mut prims = Vec::new();
    let mut hits = Vec::new();
    let left_x = MARGIN_MM + tablature::HEAD_MM;

    let mut top = PAGE_H_MM - MARGIN_MM;
    if page_no == 1 {
        top = title_block(doc, top, &mut prims);
    }
    if page_no >= 3 && page_no % 2 == 1 {
        running_header(doc, &mut prims);
    }

    for (range, target) in systems {
        let spacing = engrave::system_spacing(doc, range.clone(), Some(*target));
        place_block(doc, &spacing, left_x, top, &mut prims, &mut hits);
        top -= block_h + BLOCK_GAP_MM;
    }

    footer(page_no, total, &mut prims);
    Page { prims, hits }
}

/// Draw one line of centred text with `cap_mm` cap height and return the baseline
/// it used, so callers can chain lines top to bottom.
fn text_line(out: &mut Vec<Prim>, top: f32, cap_mm: f32, s: String, color: Rgb, x: f32) -> f32 {
    let baseline = top - cap_mm;
    out.push(Prim::Text {
        pos: P::new(x, baseline),
        s,
        pt: staff::pt_for_cap(cap_mm),
        color,
        align: Align::Center,
    });
    baseline
}

/// Title and author, centred under the top margin. Returns the new cursor: the y
/// where the first block's footprint should start.
fn title_block(doc: &Document, top: f32, out: &mut Vec<Prim>) -> f32 {
    let cx = PAGE_W_MM / 2.0;
    let mut cursor = text_line(out, top, TITLE_CAP_MM, doc.title.clone(), INK, cx);
    cursor -= TITLE_GAP_MM;
    cursor = text_line(out, cursor, AUTHOR_CAP_MM, doc.author.clone(), FAINT, cx);
    cursor - TITLE_BLOCK_GAP_MM
}

/// A faint "Title — Author" reminder at the top of odd pages from 3 on, drawn
/// inside the existing top margin so it never touches the content height.
fn running_header(doc: &Document, out: &mut Vec<Prim>) {
    out.push(Prim::Text {
        pos: P::new(PAGE_W_MM / 2.0, PAGE_H_MM - MARGIN_MM + 4.0),
        s: format!("{} — {}", doc.title, doc.author),
        pt: staff::pt_for_cap(RUNNING_HEADER_CAP_MM),
        color: FAINT,
        align: Align::Center,
    });
}

/// "Page N / T", centred in the bottom margin.
fn footer(page_no: usize, total: usize, out: &mut Vec<Prim>) {
    out.push(Prim::Text {
        pos: P::new(PAGE_W_MM / 2.0, MARGIN_MM * 0.5),
        s: format!("{} {} / {}", i18n::t("status.page"), page_no, total),
        pt: staff::pt_for_cap(FOOTER_CAP_MM),
        color: INK,
        align: Align::Center,
    });
}

/// Which rows make up one block, top to bottom, in [`StaffOrder::TabFirst`] order.
/// [`StaffOrder::NotationFirst`] is the very same list reversed: the strum row
/// (when present) is symmetric in the middle, so reversing is all the swap needs.
#[derive(Clone, Copy)]
enum RowKind {
    Tab,
    Strum,
    Notation,
}

fn row_kinds(model: BlockModel) -> Vec<RowKind> {
    match model {
        BlockModel::OneLine => vec![RowKind::Tab],
        BlockModel::TwoLine => vec![RowKind::Tab, RowKind::Notation],
        BlockModel::ThreeLine => vec![RowKind::Tab, RowKind::Strum, RowKind::Notation],
    }
}

/// Footprint of one row as (height above its own origin, height below it) — this
/// mirrors the origin-semantics doc comments on `tablature::render`,
/// `tablature::render_strum` and `notation::render` exactly, so the block-height
/// used for pagination and the per-row origins used for rendering can never drift
/// apart: both are computed from this one function.
fn row_extent(kind: RowKind, show_rhythm: bool) -> (f32, f32) {
    match kind {
        RowKind::Tab => (
            tablature::STAFF_MM + tablature::BAND_MM,
            if show_rhythm {
                tablature::RHYTHM_MM
            } else {
                0.0
            },
        ),
        RowKind::Strum => (tablature::STRUM_MM, 0.0),
        RowKind::Notation => (
            notation::ROW_MM - notation::BASELINE_OFFSET_MM,
            notation::BASELINE_OFFSET_MM,
        ),
    }
}

fn block_height(doc: &Document) -> f32 {
    let show_rhythm = tablature::shows_rhythm(doc);
    row_kinds(doc.model)
        .iter()
        .map(|&k| {
            let (above, below) = row_extent(k, show_rhythm);
            above + below
        })
        .sum()
}

/// Render one block (tablature, optionally strum, optionally notation, in the
/// document's [`StaffOrder`]) from a single shared [`Spacing`] — the whole point
/// being that a fret number, its strum arrow and its note head land in the same
/// column — stacked with no gap between rows: each row's own band/ledger padding
/// already reads as the gap.
fn place_block(
    doc: &Document,
    spacing: &Spacing,
    x: f32,
    top: f32,
    out: &mut Vec<Prim>,
    hits: &mut Vec<tablature::Hit>,
) {
    let show_rhythm = tablature::shows_rhythm(doc);
    let mut order = row_kinds(doc.model);
    if matches!(doc.staff_order, StaffOrder::NotationFirst) {
        order.reverse();
    }

    let mut cursor = top;
    let mut tab_origin_y = None;
    for kind in order {
        let (above, below) = row_extent(kind, show_rhythm);
        let origin_y = cursor - above;
        cursor = origin_y - below;
        let origin = P::new(x, origin_y);
        match kind {
            RowKind::Tab => {
                tablature::render(doc, spacing, origin, show_rhythm, out, hits);
                tab_origin_y = Some(origin_y);
            }
            RowKind::Strum => tablature::render_strum(doc, spacing, origin, out),
            RowKind::Notation => notation::render(doc, spacing, origin, out),
        }
    }

    if let Some(y) = tab_origin_y {
        warn_incomplete_bars(doc, spacing, x, y, out);
    }
}

/// A thin orange rule under an incomplete bar's tablature staff. Never blocks,
/// never panics: a half-typed bar is normal while the user is still typing.
fn warn_incomplete_bars(
    doc: &Document,
    spacing: &Spacing,
    x: f32,
    tab_origin_y: f32,
    out: &mut Vec<Prim>,
) {
    let y = tab_origin_y - WARN_OFFSET_MM;
    for bs in &spacing.bars {
        let Some(bar) = doc.bars.get(bs.index) else {
            continue;
        };
        if engrave::is_complete(bar, doc.time_sig_at(bs.index)) {
            continue;
        }
        out.push(Prim::Line {
            a: P::new(x + bs.x, y),
            b: P::new(x + bs.x + bs.width, y),
            w: WARN_W_MM,
            color: WARN_COLOR,
        });
    }
}
