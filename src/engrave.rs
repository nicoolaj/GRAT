//! Rhythmic engraving: beat grouping, beaming decisions and proportional spacing.
//!
//! This module is pure computation over the document model. It knows about time,
//! never about pitch or geometry beyond horizontal millimetres: stem direction and
//! staff positions depend on pitch and live in `notation`.
//!
//! The spacing computed here is shared by all three rows of a block (tablature,
//! strum row, notation staff), which is what keeps them vertically aligned.

use crate::model::{Bar, Document, Dur, NoteValue};

/// Horizontal room reserved after a barline before the first event.
pub const BAR_LEAD_IN_MM: f32 = 2.5;
/// Horizontal room reserved after the last event before the next barline.
pub const BAR_TRAIL_MM: f32 = 2.0;
/// A system is never squeezed below this fraction of its natural width; past that
/// point the music overflows rather than becoming unreadable.
pub const MIN_SQUEEZE: f32 = 0.6;

/// Natural width in millimetres allotted to one event before justification.
///
/// A duration table rather than a formula: it is trivially tunable by eye and the
/// values below already read well at A4 print size.
///
/// ponytail: linear table; switch to `w = k * ticks^0.6` if the visual density of
/// mixed-duration bars ever needs finer control.
pub fn natural_event_width(dur: &Dur) -> f32 {
    let base = match dur.base {
        NoteValue::Whole => 26.0,
        NoteValue::Half => 18.0,
        NoteValue::Quarter => 12.0,
        NoteValue::Eighth => 8.0,
        NoteValue::Sixteenth => 5.5,
        NoteValue::ThirtySecond => 4.0,
    };
    base * (1.0 + 0.25 * dur.dots as f32)
}

/// Width a bar wants when nothing constrains it.
pub fn natural_bar_width(bar: &Bar) -> f32 {
    let events: f32 = bar.events.iter().map(|e| natural_event_width(&e.dur)).sum();
    // An empty bar still needs to be visible and clickable.
    let events = events.max(natural_event_width(&Dur {
        base: NoteValue::Whole,
        dots: 0,
    }));
    BAR_LEAD_IN_MM + events + BAR_TRAIL_MM
}

/// Horizontal placement of one bar inside a system. All values are millimetres
/// relative to the system origin (its left edge).
#[derive(Clone, Debug, PartialEq)]
pub struct BarSpacing {
    /// Index of this bar in `Document::bars`.
    pub index: usize,
    /// x of the bar's opening barline.
    pub x: f32,
    pub width: f32,
    /// x of each event, in order.
    pub events: Vec<f32>,
}

/// Horizontal placement of a whole system (one block's worth of bars).
#[derive(Clone, Debug, PartialEq)]
pub struct Spacing {
    pub bars: Vec<BarSpacing>,
    /// Total width in millimetres, after justification.
    pub width: f32,
}

impl Spacing {
    /// x of an event, addressed by its position within the system.
    pub fn event_x(&self, bar_in_system: usize, event: usize) -> Option<f32> {
        self.bars
            .get(bar_in_system)
            .and_then(|b| b.events.get(event))
            .copied()
    }

    /// x of the closing barline of the system.
    pub fn end_x(&self) -> f32 {
        self.width
    }
}

/// Lay out `bars` (indices into `doc.bars`) across `target_width_mm`.
///
/// Passing `None` yields the natural, unjustified width — that is what the line
/// breaker uses to decide how many bars fit. Passing `Some(w)` stretches (or, down
/// to [`MIN_SQUEEZE`], compresses) every event uniformly so the system ends exactly
/// on the right margin.
pub fn system_spacing(
    doc: &Document,
    bars: std::ops::Range<usize>,
    target_width_mm: Option<f32>,
) -> Spacing {
    let natural: f32 = bars
        .clone()
        .filter_map(|i| doc.bars.get(i))
        .map(natural_bar_width)
        .sum();

    let scale = match target_width_mm {
        Some(target) if natural > 0.0 => (target / natural).max(MIN_SQUEEZE),
        _ => 1.0,
    };

    let mut out = Vec::new();
    let mut x = 0.0_f32;
    for index in bars {
        let Some(bar) = doc.bars.get(index) else {
            continue;
        };
        let width = natural_bar_width(bar) * scale;
        let mut cursor = x + BAR_LEAD_IN_MM * scale;
        let mut events = Vec::with_capacity(bar.events.len());
        for event in &bar.events {
            events.push(cursor);
            cursor += natural_event_width(&event.dur) * scale;
        }
        out.push(BarSpacing {
            index,
            x,
            width,
            events,
        });
        x += width;
    }

    Spacing {
        bars: out,
        width: x,
    }
}

/// Ticks in one beat, i.e. the span over which notes are beamed together.
///
/// Compound metres (6/8, 9/8, 12/8) group by dotted quarter, which is what makes
/// 6/8 read as two groups of three quavers rather than three groups of two.
pub fn beat_ticks(time_sig: (u8, u8)) -> u32 {
    let (num, den) = time_sig;
    let unit = match den {
        1 => crate::model::TICKS_WHOLE,
        2 => crate::model::TICKS_WHOLE / 2,
        4 => crate::model::TICKS_WHOLE / 4,
        8 => crate::model::TICKS_WHOLE / 8,
        16 => crate::model::TICKS_WHOLE / 16,
        32 => crate::model::TICKS_WHOLE / 32,
        _ => crate::model::TICKS_WHOLE / 4,
    };
    if den >= 8 && num % 3 == 0 && num > 3 {
        unit * 3
    } else {
        unit
    }
}

/// Total ticks a complete bar of this metre holds.
pub fn bar_ticks(time_sig: (u8, u8)) -> u32 {
    let (num, den) = time_sig;
    let unit = match den {
        1 => crate::model::TICKS_WHOLE,
        2 => crate::model::TICKS_WHOLE / 2,
        4 => crate::model::TICKS_WHOLE / 4,
        8 => crate::model::TICKS_WHOLE / 8,
        16 => crate::model::TICKS_WHOLE / 16,
        32 => crate::model::TICKS_WHOLE / 32,
        _ => crate::model::TICKS_WHOLE / 4,
    };
    unit * num as u32
}

/// Onset of every event in the bar, in ticks from the barline.
pub fn onsets(bar: &Bar) -> Vec<u32> {
    let mut t = 0;
    bar.events
        .iter()
        .map(|e| {
            let at = t;
            t += e.dur.ticks();
            at
        })
        .collect()
}

/// Whether the bar's contents add up to its metre. A `false` here is shown as a
/// warning in the editor, never as an error: half-written bars are normal while typing.
pub fn is_complete(bar: &Bar, time_sig: (u8, u8)) -> bool {
    bar.events.iter().map(|e| e.dur.ticks()).sum::<u32>() == bar_ticks(time_sig)
}

/// A run of events joined by beams.
#[derive(Clone, Debug, PartialEq)]
pub struct BeamGroup {
    /// Indices into `Bar::events`, consecutive and in order. Always at least 2.
    pub events: Vec<usize>,
    /// How many beams each of those events needs: 1 for an eighth, 2 for a
    /// sixteenth, 3 for a thirty-second. Parallel to `events`.
    pub beams: Vec<u8>,
}

/// Group the bar's events into beams, one beat at a time.
///
/// Rules applied, which are the conventional ones: beams never cross a beat
/// boundary; rests and notes of a quarter or longer break a run; a run of a single
/// beamable event is left to be drawn with a flag instead.
pub fn beam_groups(doc: &Document, bar_index: usize) -> Vec<BeamGroup> {
    let Some(bar) = doc.bars.get(bar_index) else {
        return Vec::new();
    };
    let beat = beat_ticks(doc.time_sig_at(bar_index)).max(1);
    let onsets = onsets(bar);

    let mut groups = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    let mut run_beat = 0_u32;

    for (i, event) in bar.events.iter().enumerate() {
        let beamable = !event.is_rest() && event.dur.base.flags() >= 1;
        let this_beat = onsets[i] / beat;

        if beamable && (run.is_empty() || this_beat == run_beat) {
            if run.is_empty() {
                run_beat = this_beat;
            }
            run.push(i);
            continue;
        }

        flush(&mut run, bar, &mut groups);
        if beamable {
            run_beat = this_beat;
            run.push(i);
        }
    }
    flush(&mut run, bar, &mut groups);
    groups
}

fn flush(run: &mut Vec<usize>, bar: &Bar, out: &mut Vec<BeamGroup>) {
    if run.len() >= 2 {
        let beams = run
            .iter()
            .map(|&i| bar.events[i].dur.base.flags())
            .collect();
        out.push(BeamGroup {
            events: std::mem::take(run),
            beams,
        });
    } else {
        run.clear();
    }
}

/// Maximal runs within a beam group that carry at least `level` beams.
///
/// Level 1 is the group itself; levels 2 and 3 are the secondary beams, which stop
/// and start with the shorter values. A run of length 1 is a beamlet: a stub drawn
/// on one side of the stem only.
pub fn beam_runs(group: &BeamGroup, level: u8) -> Vec<std::ops::RangeInclusive<usize>> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    for (pos, &count) in group.beams.iter().enumerate() {
        if count >= level {
            start.get_or_insert(pos);
        } else if let Some(s) = start.take() {
            runs.push(s..=pos - 1);
        }
    }
    if let Some(s) = start {
        runs.push(s..=group.beams.len() - 1);
    }
    runs
}
