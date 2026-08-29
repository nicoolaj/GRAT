//! Rhythmic engraving: beat grouping, beaming decisions and proportional spacing.
//!
//! This module is pure computation over the document model. It knows about time,
//! never about pitch or geometry beyond horizontal millimetres: stem direction and
//! staff positions depend on pitch and live in `notation`.
//!
//! The spacing computed here is shared by all three rows of a block (tablature,
//! strum row, notation staff), which is what keeps them vertically aligned.

use crate::model::{Bar, Document, Dur, Event, NoteValue, Technique};

/// Air around a barline: half before the bar's first event, half after its last.
///
/// One constant split in two, rather than a lead-in and a trail that can drift
/// apart — that equality is exactly what makes a bar's music sit centred between
/// its own barlines instead of hugging the left one. The value is what the old
/// asymmetric pair added up to, so line breaking packs the same bars per line.
///
/// ponytail: fixed, so a bar ending on a whole note gets no more air before the
/// barline than one ending on a sixteenth. Scale the trailing half by the last
/// event's duration if that ever reads as cramped — at the cost of the symmetry.
pub const BAR_GAP_MM: f32 = 8.75;
/// A system is never squeezed below this fraction of its natural width; past that
/// point the music overflows rather than becoming unreadable.
pub const MIN_SQUEEZE: f32 = 0.6;

/// Natural width in millimetres allotted to one event before justification,
/// scaled by `h` (the document's `note_spacing`: below 1.0 is denser).
///
/// A duration table rather than a formula: it is trivially tunable by eye and the
/// values below already read well at A4 print size. They are the millimetres the
/// default density actually prints — the table was retuned when the standard
/// moved to what used to be `note_spacing = 0.7`, rather than hiding that move
/// behind a multiplier.
///
/// ponytail: linear table; switch to `w = k * ticks^0.6` if the visual density of
/// mixed-duration bars ever needs finer control.
pub fn natural_event_width(dur: &Dur, h: f32) -> f32 {
    let base = match dur.base {
        NoteValue::Whole => 18.2,
        NoteValue::Half => 12.6,
        NoteValue::Quarter => 8.4,
        NoteValue::Eighth => 5.6,
        NoteValue::Sixteenth => 3.85,
        NoteValue::ThirtySecond => 2.8,
    };
    base * (1.0 + 0.25 * dur.dots as f32) * h
}

/// Extra room in front of an event, for a glyph drawn to the LEFT of its own
/// column. Only the approach slide has one: its departure digit sits before the
/// note, and `natural_event_width` — a function of duration alone — reserves
/// nothing for it, so without this the digit lands on the barline or on the
/// previous note.
///
/// ponytail: one flat value, not the digit's measured width, and it does not
/// follow `tab_scale` — same simplification as `tablature::BAND_MM`. Measure with
/// `staff::label_width` here if a two-digit departure fret at a large `tab_scale`
/// ever crowds again.
pub const SLIDE_IN_LEAD_MM: f32 = 4.0;

pub fn event_lead_in(event: &Event, h: f32) -> f32 {
    if event
        .notes
        .iter()
        .any(|n| matches!(n.tech, Technique::SlideIn { .. }))
    {
        SLIDE_IN_LEAD_MM * h
    } else {
        0.0
    }
}

/// Width a bar wants when nothing constrains it, at note spacing `h`. Every
/// millimetre scales with `h` — [`BAR_GAP_MM`] too — so
/// `natural_bar_width(.., h) == h * natural_bar_width(.., 1.0)` at any density.
///
/// Only the events that have a successor *inside the bar* claim a duration slot:
/// a slot is the room to the next note, and the last note has none. Its width is
/// [`BAR_GAP_MM`] / 2 of air, the same as the first note's, which is what puts the
/// music in the middle of the bar. A lead-in is different: it reserves room in
/// front of its own note regardless of what follows, so every event counts here,
/// the last one included.
pub fn natural_bar_width(bar: &Bar, h: f32) -> f32 {
    let events: f32 = match bar.events.split_last() {
        // An empty bar still needs to be visible and clickable.
        None => natural_event_width(
            &Dur {
                base: NoteValue::Whole,
                dots: 0,
            },
            h,
        ),
        Some((_last, head)) => head.iter().map(|e| natural_event_width(&e.dur, h)).sum(),
    };
    let lead_in: f32 = bar.events.iter().map(|e| event_lead_in(e, h)).sum();
    BAR_GAP_MM * h + events + lead_in
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
    let h = doc.note_spacing;
    let natural: f32 = bars
        .clone()
        .filter_map(|i| doc.bars.get(i))
        .map(|bar| natural_bar_width(bar, h))
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
        let width = natural_bar_width(bar, h) * scale;
        // Half the barline gap in front; the other half falls out behind the last
        // event, which claims no slot of its own.
        let mut cursor = x + BAR_GAP_MM * 0.5 * h * scale;
        let mut events = Vec::with_capacity(bar.events.len());
        for event in &bar.events {
            // A lead-in pushes this event's own column right, carving out the room
            // its glyph hangs into on the left. `natural_bar_width` counts the same
            // term, so the air behind the bar's last note is unchanged -- except
            // when the *first* event itself claims a lead-in, which deliberately
            // unbalances the bar: it eats into the front gap instead of the back.
            cursor += event_lead_in(event, h) * scale;
            events.push(cursor);
            cursor += natural_event_width(&event.dur, h) * scale;
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

/// Six undotted note values, longest first — the basis `split_ticks` is greedy over.
const SPLITTABLE: [NoteValue; 6] = [
    NoteValue::Whole,
    NoteValue::Half,
    NoteValue::Quarter,
    NoteValue::Eighth,
    NoteValue::Sixteenth,
    NoteValue::ThirtySecond,
];

/// Break `ticks` into the fewest printable (undotted) durations, longest first.
///
/// Stops as soon as no value fits what remains, rather than looping forever: a
/// remainder under one thirty-second (120 ticks -- e.g. 225 left over, which only
/// a triple-dotted value could name) isn't representable at all, so it is dropped.
/// The caller ends up a few ticks short, which the existing incomplete-bar warning
/// already surfaces.
///
/// ponytail: greedy and not beat-aware, so a rest can straddle a beat; split at
/// beat boundaries too if a misplaced rest is ever visible enough in print.
pub fn split_ticks(ticks: u32) -> Vec<Dur> {
    let mut out = Vec::new();
    let mut remaining = ticks;
    while let Some(base) = SPLITTABLE.iter().copied().find(|v| v.ticks() <= remaining) {
        remaining -= base.ticks();
        out.push(Dur { base, dots: 0 });
    }
    out
}

/// Change event `index`'s duration to `dur`, keeping the bar's total exactly at
/// `bar_ticks(time_sig)`. Shortening backfills the freed time with rests right
/// after the event; lengthening consumes the events that follow, dropping any
/// whose notes are fully swallowed and trimming the one that only partly is.
///
/// Nothing at or before `index` is ever inserted or removed, so a caller's
/// selection by index stays valid across the call.
pub fn set_event_dur(bar: &mut Bar, time_sig: (u8, u8), index: usize, dur: Dur) {
    let Some(&start) = onsets(bar).get(index) else {
        return;
    };
    let room = bar_ticks(time_sig).saturating_sub(start);
    let target = if dur.ticks() <= room {
        dur
    } else {
        // ponytail: bounded at the bar line; a tie across it (out of scope for v1)
        // would let this overflow instead of clamping.
        match split_ticks(room).first() {
            Some(&fit) => fit,
            None => return,
        }
    };

    let old_ticks = bar.events[index].dur.ticks();
    bar.events[index].dur = target;
    let new_ticks = target.ticks();

    if new_ticks < old_ticks {
        let rests = split_ticks(old_ticks - new_ticks)
            .into_iter()
            .map(|dur| Event {
                dur,
                ..Default::default()
            });
        bar.events.splice(index + 1..index + 1, rests);
    } else if new_ticks > old_ticks {
        let mut spare = new_ticks - old_ticks;
        let i = index + 1; // stays put: a removal shifts the next event into i itself
        while spare > 0 && i < bar.events.len() {
            let covered = bar.events[i].dur.ticks();
            if covered <= spare {
                bar.events.remove(i); // fully swallowed, including whatever notes it had
                spare -= covered;
            } else {
                let rests: Vec<Event> = split_ticks(covered - spare)
                    .into_iter()
                    .map(|dur| Event {
                        dur,
                        ..Default::default()
                    })
                    .collect();
                bar.events.splice(i..i + 1, rests);
                spare = 0;
            }
        }
    }
}

/// Change event `index`'s duration, then push everything that follows -- the rest
/// of the bar, then later bars -- forward without losing anything: the document
/// gains a bar rather than dropping an event. Unlike [`set_event_dur`], nothing
/// after `index` is trimmed or swallowed: the whole tail from `bar_index` onward
/// is re-split from scratch around the new duration.
///
/// `index` itself never moves (nothing before it is touched), so a caller's
/// selection by `(bar_index, index)` stays valid across the call.
pub fn shift_event_dur(doc: &mut Document, bar_index: usize, index: usize, dur: Dur) {
    let Some(event) = doc
        .bars
        .get_mut(bar_index)
        .and_then(|b| b.events.get_mut(index))
    else {
        return;
    };
    event.dur = dur;

    // Pull every event from `bar_index` on into an owned queue: draining (not
    // cloning) releases the borrow on `doc.bars` before it's rebuilt below, and
    // owning the events, not just their durations, carries fretting, technique
    // and strum along for free.
    let original_len = doc.bars.len();
    let mut stream: std::collections::VecDeque<Event> = doc.bars[bar_index..]
        .iter_mut()
        .flat_map(|b| std::mem::take(&mut b.events))
        .collect();

    let mut i = bar_index;
    while !stream.is_empty() {
        if i == doc.bars.len() {
            // The stream outgrew the document -- grow it rather than lose a note.
            doc.bars.push(Bar::default());
        }
        let cap = bar_ticks(doc.time_sig_at(i)); // read before the &mut below -- borrowck
        let mut events = Vec::new();
        let mut total = 0u32;
        while let Some(mut e) = stream.pop_front() {
            let room = cap.saturating_sub(total);
            let ticks = e.dur.ticks();
            if ticks <= room {
                total += ticks;
                events.push(e);
            } else if total == 0 {
                // An event longer than a whole bar (a whole note pushed into a
                // 3/4 bar) would never fit anywhere, so without clamping it the
                // loop above would never terminate. Bound it exactly as
                // `set_event_dur` bounds an overlong duration at a bar line.
                // ponytail: the excess is dropped rather than tied into the next
                // bar -- see `notation::ties`'s ponytail, which only connects
                // two events within the same bar. Splitting this into two tied
                // notes needs ties across a barline (out of scope for v1);
                // printing two repeated notes without a tie would be a
                // rhythmic lie worse than the drop.
                e.dur = split_ticks(cap).first().copied().unwrap_or(e.dur);
                total += e.dur.ticks();
                events.push(e);
            } else {
                // Never split an event across a bar line: it starts the next
                // bar instead, and the room left here becomes a rest.
                stream.push_front(e);
                break;
            }
        }
        if total < cap {
            events.extend(split_ticks(cap - total).into_iter().map(|dur| Event {
                dur,
                ..Default::default()
            }));
        }
        doc.bars[i].events = events;
        i += 1;
    }

    // Bars the stream never reached were drained empty above; give them back one
    // rest per beat (clickable cells) instead of leaving them silent.
    for j in i..original_len {
        let sig = doc.time_sig_at(j);
        doc.bars[j].events = Bar::new_empty(Some(sig)).events;
    }
}

/// Bring a bar's total back to exactly `bar_ticks(time_sig)`: the first event that
/// would overrun the bar is shrunk to what still fits (or dropped if nothing
/// does), everything after it is replaced by rests, and a bar left short is
/// padded the same way.
pub fn refit(bar: &mut Bar, time_sig: (u8, u8)) {
    let cap = bar_ticks(time_sig);
    let mut total = 0u32;
    for i in 0..bar.events.len() {
        let ticks = bar.events[i].dur.ticks();
        if total + ticks <= cap {
            total += ticks;
            continue;
        }
        let room = cap - total;
        match split_ticks(room).into_iter().next() {
            Some(fit) => {
                bar.events[i].dur = fit;
                total += fit.ticks();
                bar.events.truncate(i + 1);
            }
            None => bar.events.truncate(i),
        }
        break;
    }
    if total < cap {
        bar.events
            .extend(split_ticks(cap - total).into_iter().map(|dur| Event {
                dur,
                ..Default::default()
            }));
    }
}

/// Set bar `bar_index`'s time signature (`None` reverts to inheriting from
/// whatever came before) and refit it and every bar it governs, stopping at the
/// next bar that names its own signature.
pub fn set_time_sig(doc: &mut Document, bar_index: usize, sig: Option<(u8, u8)>) {
    let Some(bar) = doc.bars.get_mut(bar_index) else {
        return;
    };
    bar.time_sig = sig;

    for i in bar_index..doc.bars.len() {
        if i > bar_index && doc.bars[i].time_sig.is_some() {
            break; // that bar starts its own section
        }
        let effective = doc.time_sig_at(i); // read before the &mut below -- borrowck
        refit(&mut doc.bars[i], effective);
    }
}

/// Normalise a document for printing: the editor's convenient one-rest-per-beat
/// "blank beat" becomes the rests a finished score is actually written with, and
/// whole empty bars trailing the music are dropped.
///
/// Pure — the returned document is what `pdf::export` lays out; the editor keeps
/// rendering the original, where scattered single-beat rests are the point.
pub fn for_export(doc: &Document) -> Document {
    let mut doc = doc.clone();

    // Right-trim: drop whole bars past the last one that sounds a note. Keep at
    // least one so an all-rest piece still prints a staff, and leave mid-piece
    // blank bars alone -- only the tail goes.
    //
    // ponytail: a trimmed bar's `repeat_end` / `time_sig` override goes with it.
    // In practice the only blank bars at the end are the ones the editor grows
    // (`layout::ensure_trailing_blank_system`), which carry neither.
    while doc.bars.len() > 1
        && doc
            .bars
            .last()
            .is_some_and(|b| b.events.iter().all(Event::is_rest))
    {
        doc.bars.pop();
    }

    for i in 0..doc.bars.len() {
        let sig = doc.time_sig_at(i); // read before the &mut below -- borrowck
        merge_bar_rests(&mut doc.bars[i], sig);
    }

    doc
}

/// Rewrite `bar.events` so runs of consecutive rests read the way a score is
/// written: a single whole rest for a wholly silent bar, otherwise the rest run
/// broken at beat boundaries. Notes pass through untouched.
fn merge_bar_rests(bar: &mut Bar, time_sig: (u8, u8)) {
    let cap = bar_ticks(time_sig);
    let beat = beat_ticks(time_sig).max(1);

    // A bar that is nothing but rests adding up to a full measure is one whole
    // rest, whatever the metre -- exactly how notation shows an untouched bar.
    if !bar.events.is_empty()
        && bar.events.iter().all(Event::is_rest)
        && bar.events.iter().map(|e| e.dur.ticks()).sum::<u32>() == cap
    {
        bar.events = vec![Event {
            dur: Dur {
                base: NoteValue::Whole,
                dots: 0,
            },
            ..Default::default()
        }];
        return;
    }

    let mut out: Vec<Event> = Vec::with_capacity(bar.events.len());
    let mut t = 0u32; // onset of the event under the cursor
    let mut run_start: Option<u32> = None; // onset of the current rest run, if any
    for event in &bar.events {
        if event.is_rest() {
            run_start.get_or_insert(t);
        } else {
            if let Some(start) = run_start.take() {
                emit_rests(start, t, beat, &mut out);
            }
            out.push(event.clone());
        }
        t += event.dur.ticks();
    }
    if let Some(start) = run_start.take() {
        emit_rests(start, t, beat, &mut out);
    }
    bar.events = out;
}

/// Fill `[from, to)` with rests, split at every beat boundary so none straddles a
/// beat, each beat-aligned piece named by the fewest values `split_ticks` allows.
///
/// ponytail: a multi-beat run that is not a whole bar comes out as one rest per
/// beat -- no half / dotted-half grouping. Switch to beat-unit grouping (Gould,
/// "Behind Bars") if that ever reads as too fussy in print.
fn emit_rests(from: u32, to: u32, beat: u32, out: &mut Vec<Event>) {
    let mut a = from;
    while a < to {
        let b = ((a / beat + 1) * beat).min(to); // end of a's beat, clamped to `to`
        out.extend(split_ticks(b - a).into_iter().map(|dur| Event {
            dur,
            ..Default::default()
        }));
        a = b;
    }
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
