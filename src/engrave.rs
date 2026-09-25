//! Rhythmic engraving: beat grouping, beaming decisions and proportional spacing.
//!
//! This module is pure computation over the document model. It knows about time,
//! never about pitch or geometry beyond horizontal millimetres: stem direction and
//! staff positions depend on pitch and live in `notation`.
//!
//! The spacing computed here is shared by all three rows of a block (tablature,
//! strum row, notation staff), which is what keeps them vertically aligned.

use crate::model::{Bar, Document, Dur, Event, NoteValue, Technique};

/// Air between a barline and the bar's first event.
///
/// Nothing is added behind the last event: its own duration slot is already the
/// air before the closing barline (see [`natural_bar_width`]), the way the next
/// note would stand there if there were one. Adding barline air on top of that
/// slot left twice as much paper on the right of a bar as on its left. The slot
/// is floored at this value, so a bar ending on a sixteenth still breathes.
pub const BAR_LEAD_MM: f32 = 4.375;
/// Extra room after the event that closes a rhythmic group: one unit between the
/// notes of a group of quavers, 1.3 between two groups, so the beats read apart
/// on every row. See [`group_span`] for where groups end, and [`slots`] for why
/// a group closing on a shorter note still gets a quaver's worth.
pub const GROUP_GAP: f32 = 0.3;
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

/// Ticks between the group breaks spacing leaves [`GROUP_GAP`] at: one beat once
/// the bar moves in quavers or faster, two in a metre of four-plus beats that
/// moves no faster than crotchets -- `q q | q q` rather than four evenly spread
/// quarters. Where a two-beat span has no midpoint to show (2/4, 3/4, compound
/// metres) it falls back to the beat, which spaces every crotchet alike and so
/// changes nothing once the line is justified.
///
/// Beaming keeps its own window ([`beam_span`]): in 4/4 of running quavers the
/// gap falls at each beat, inside the beam of four.
fn group_span(time_sig: (u8, u8), bar: &Bar) -> u32 {
    let beat = beat_ticks(time_sig).max(1);
    let shortest = fastest_note(bar).unwrap_or(beat);
    if shortest >= NoteValue::Quarter.ticks() && has_wide_midpoint(time_sig) {
        beat * 2
    } else {
        beat
    }
}

/// Paper each event of `bar` moves the cursor on by, its lead-in apart: the
/// duration slot, and where the next event opens a group, `1 + GROUP_GAP` times
/// that slot floored at a quaver's. The last event's slot is the air before the
/// barline, floored at [`BAR_LEAD_MM`].
///
/// The floor makes the space between two beats one constant wherever a group
/// closes on a quaver or anything shorter: without it a beat ending on a
/// sixteenth (a syncopation `for_export` cut at the beat) stood closer to the
/// next than a beat ending on a quaver, and the tie across it came out shorter
/// than the one across the next beat.
///
/// The one place slots are decided: [`natural_bar_width`] sums them and
/// [`system_spacing`] walks them, so a bar's width and its columns cannot drift.
fn slots(bar: &Bar, time_sig: (u8, u8), h: f32) -> Vec<f32> {
    let span = group_span(time_sig, bar);
    let onsets = onsets(bar);
    let quaver = natural_event_width(
        &Dur {
            base: NoteValue::Eighth,
            dots: 0,
        },
        h,
    );
    let last = bar.events.len().saturating_sub(1);
    bar.events
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let w = natural_event_width(&e.dur, h);
            if i == last {
                w.max(BAR_LEAD_MM * h)
            } else if onsets[i + 1].is_multiple_of(span) {
                w.max(quaver) * (1.0 + GROUP_GAP)
            } else {
                w
            }
        })
        .collect()
}

/// Width a bar wants when nothing constrains it, at note spacing `h`. Every
/// millimetre scales with `h` — [`BAR_LEAD_MM`] too — so
/// `natural_bar_width(.., h) == h * natural_bar_width(.., 1.0)` at any density.
///
/// **Every** event claims a duration slot, the last one included. A slot is how
/// long the event lasts, and the eye reads duration as horizontal distance: a bar
/// ending on a quarter rest must show a quarter's worth of paper before the
/// barline, or that whole beat reads as an instant. Leaving the last event out --
/// which is what put the music optically in the middle of the bar -- made the
/// final beat of every bar roughly a third of its rightful width.
pub fn natural_bar_width(bar: &Bar, time_sig: (u8, u8), h: f32) -> f32 {
    let events: f32 = if bar.events.is_empty() {
        // An empty bar still needs to be visible and clickable.
        natural_event_width(
            &Dur {
                base: NoteValue::Whole,
                dots: 0,
            },
            h,
        )
    } else {
        slots(bar, time_sig, h).iter().sum()
    };
    let lead_in: f32 = bar.events.iter().map(|e| event_lead_in(e, h)).sum();
    BAR_LEAD_MM * h + events + lead_in
}

/// Width of one time-signature digit, the widest the rows print (the tablature's).
const TIME_SIG_DIGIT_MM: f32 = 3.9;

/// Digits of the wider half of a time signature: 12/8 is two wide.
fn time_sig_digits(sig: (u8, u8)) -> f32 {
    sig.0.max(sig.1).to_string().len() as f32
}

/// The signature bar `index` has to print: the piece's first, or a change.
fn time_sig_mark(doc: &Document, index: usize) -> Option<(u8, u8)> {
    let sig = doc.time_sig_at(index.min(doc.bars.len().checked_sub(1)?));
    match index {
        0 => Some(sig),
        i if i < doc.bars.len() && doc.time_sig_at(i - 1) != sig => Some(sig),
        _ => None,
    }
}

/// Whether a signature fits the staff head, left of a system's first barline.
/// A two-digit one (12/8) would run into the clef, so it stands after the
/// barline instead, like a change inside the system.
fn fits_margin(sig: (u8, u8)) -> bool {
    time_sig_digits(sig) <= 1.0
}

/// Room a time signature claims right after bar `index`'s opening barline, or 0
/// where none prints there. The rows draw the digits in it, centred
/// [`time_sig_offset`] past the barline. When the bar opens a system its
/// signature usually sits in the margin instead and claims nothing.
pub fn time_sig_lead(doc: &Document, index: usize, opens_system: bool) -> f32 {
    match time_sig_mark(doc, index) {
        Some(sig) if !(opens_system && fits_margin(sig)) => {
            // The air after the digits keeps an accidental hanging left of the
            // first head off them.
            3.0 + time_sig_digits(sig) * TIME_SIG_DIGIT_MM
        }
        _ => 0.0,
    }
}

/// Centre of a time signature standing after its barline, measured from it.
pub fn time_sig_offset(sig: (u8, u8)) -> f32 {
    1.0 + time_sig_digits(sig) * TIME_SIG_DIGIT_MM * 0.5
}

/// Where a system prints its time signatures, as `(x from the system origin,
/// signature)`: the piece's first metre, each change at its barline (in the
/// margin when it opens the system and fits there), and a courtesy signature
/// after the closing barline when the next system opens with a change. Never
/// restated merely because a new system starts.
pub fn time_sig_marks(doc: &Document, spacing: &Spacing) -> Vec<(f32, (u8, u8))> {
    /// Centre of a signature standing in the staff head, left of the first barline.
    const IN_MARGIN: f32 = -3.5;
    let mut marks = Vec::new();
    for (i, bar) in spacing.bars.iter().enumerate() {
        if let Some(sig) = time_sig_mark(doc, bar.index) {
            let x = if i == 0 && fits_margin(sig) {
                IN_MARGIN
            } else {
                bar.x + time_sig_offset(sig)
            };
            marks.push((x, sig));
        }
    }
    if let Some(next) = spacing.bars.last().map(|b| b.index + 1) {
        if let Some(sig) = time_sig_mark(doc, next) {
            marks.push((spacing.width + time_sig_offset(sig), sig));
        }
    }
    marks
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
    let first = bars.start;
    let lead = |i: usize| time_sig_lead(doc, i, i == first);
    let natural: f32 = bars
        .clone()
        .filter_map(|i| {
            doc.bars
                .get(i)
                .map(|bar| natural_bar_width(bar, doc.time_sig_at(i), h) + lead(i))
        })
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
        let sig = doc.time_sig_at(index);
        let width = (natural_bar_width(bar, sig, h) + lead(index)) * scale;
        // The barline air in front; behind the last event, its own slot is the air.
        let mut cursor = x + (lead(index) + BAR_LEAD_MM * h) * scale;
        let mut events = Vec::with_capacity(bar.events.len());
        for (event, slot) in bar.events.iter().zip(slots(bar, sig, h)) {
            // A lead-in pushes this event's own column right, carving out the room
            // its glyph hangs into on the left. `natural_bar_width` counts the same
            // term, so the air behind the bar's last note is unchanged.
            cursor += event_lead_in(event, h) * scale;
            events.push(cursor);
            cursor += slot * scale;
        }
        // A whole-bar rest is centred between its barlines: the one place notation
        // puts a symbol at the middle of the bar rather than at the instant it
        // falls on. `for_export` collapses every silent bar to exactly this shape,
        // and without the special case each one would hug its opening barline.
        if bar.events.len() == 1 && bar.events[0].is_rest() {
            events[0] = x + width * 0.5;
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
    let unit = NoteValue::for_denominator(time_sig.1).ticks();
    if is_compound(time_sig) {
        unit * 3
    } else {
        unit
    }
}

/// Total ticks a complete bar of this metre holds.
pub fn bar_ticks(time_sig: (u8, u8)) -> u32 {
    NoteValue::for_denominator(time_sig.1).ticks() * time_sig.0 as u32
}

/// A metre that counts in dotted beats: 6/8, 9/8, 12/8, never 3/8 or 6/4.
fn is_compound((num, den): (u8, u8)) -> bool {
    den >= 8 && num % 3 == 0 && num > 3
}

/// Whether `time_sig` is a simple metre of four beats or more -- wide enough
/// that its own midpoint (half the bar) has to stay visible in print. Never
/// true for a compound metre (6/8, 9/8, 12/8), which already groups by its
/// dotted beat and has no such midpoint. Shared by [`beam_span`] (wider
/// beaming) and `rest_span` (wider rest grouping) -- same threshold, same
/// reason, applied to two different kinds of run.
fn has_wide_midpoint(time_sig: (u8, u8)) -> bool {
    !is_compound(time_sig) && time_sig.0 >= 4
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

/// Seconds one tick lasts at `tempo` beats per minute, where a beat is
/// [`beat_ticks`] -- a quarter note in a simple metre, a dotted quarter in a
/// compound one, which is the convention a tempo marking is read by in either
/// case. Shared by [`timeline`] and [`metronome_beats`] so the note clock and
/// the click clock can never drift apart.
fn seconds_per_tick(time_sig: (u8, u8), tempo: u16) -> f32 {
    60.0 / (tempo.max(1) as f32 * beat_ticks(time_sig) as f32)
}

/// Seconds one bar of `time_sig` lasts at `tempo`.
pub fn bar_duration_secs(time_sig: (u8, u8), tempo: u16) -> f32 {
    bar_ticks(time_sig) as f32 * seconds_per_tick(time_sig, tempo)
}

/// One event's moment in playback: when it starts sounding, and when the next
/// event does.
///
/// Deliberately geometry-free — the live player maps `(bar, event)` onto the
/// screen itself, and does it differently in each of its two views.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub bar: usize,
    /// Where `bar` stands in the order played: a repeated bar is one bar but
    /// two steps, which is how a caller tells its two passes apart.
    pub step: usize,
    pub event: usize,
    /// Seconds from the start of the piece.
    pub start: f32,
    /// Seconds: where the next event starts, or where the piece ends.
    pub end: f32,
}

/// The bars in the order they are played, repeats unrolled: at a closing
/// repeat marked to sound n times, back to its passage's start until the
/// passage has sounded n times. A passage starts at the last opening repeat
/// before it, or else just after the previous closing repeat, or else at the
/// top of the piece.
///
/// Every closing repeat sends the player back at most n - 1 times, so this
/// ends, and `Document::from_json` keeps n small.
pub fn play_order(doc: &Document) -> Vec<usize> {
    let mut order = Vec::with_capacity(doc.bars.len());
    let mut sounded = vec![1u8; doc.bars.len()];
    let (mut i, mut start) = (0, 0);
    while let Some(bar) = doc.bars.get(i) {
        if bar.repeat_start {
            start = i;
        }
        order.push(i);
        match bar.repeat_end {
            Some(n) if sounded[i] < n => {
                sounded[i] += 1;
                i = start;
            }
            Some(_) => {
                start = i + 1;
                i += 1;
            }
            None => i += 1,
        }
    }
    order
}

/// Every event of the bars in `order` -- [`play_order`] for the piece as
/// written, a plain range for a looped passage -- timed at `doc.tempo` beats
/// per minute (see [`seconds_per_tick`]).
pub fn timeline(doc: &Document, order: &[usize]) -> Vec<Cue> {
    let mut t = 0.0;
    let mut out = Vec::new();
    for (step, &bar) in order.iter().enumerate() {
        let Some(b) = doc.bars.get(bar) else {
            continue;
        };
        let per_tick = seconds_per_tick(doc.time_sig_at(bar), doc.tempo);
        for (event, e) in b.events.iter().enumerate() {
            let start = t;
            t += e.dur.ticks() as f32 * per_tick;
            out.push(Cue {
                bar,
                step,
                event,
                start,
                end: t,
            });
        }
    }
    out
}

/// One metronome pulse. Independent of `Bar::events` -- a metronome ticks
/// through a bar's rests exactly like it ticks through its notes, which is why
/// this is not just read off [`Cue`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub bar: usize,
    /// Seconds from the start of the piece (or, for a count-in's own grid, from
    /// the start of the count-in).
    pub time: f32,
    /// First beat of its bar: the metronome's accented click.
    pub downbeat: bool,
    /// Position within the bar, 0-based -- 0 is the downbeat itself.
    pub index_in_bar: u32,
    /// How many beats this bar's metre has in total, so a caller can place
    /// `index_in_bar` on a scale (a colour ramp, a printed beat number) without
    /// re-deriving it from the time signature.
    pub beats_in_bar: u32,
}

/// Metronome pulses for the bars in `order` (as for [`timeline`]), at
/// `doc.tempo`. One pulse per beat of each bar's own metre -- the same grouping
/// [`beam_groups`] beams by, so a compound bar like 6/8 clicks in two, not six.
pub fn metronome_beats(doc: &Document, order: &[usize]) -> Vec<Beat> {
    let mut t = 0.0;
    let mut out = Vec::new();
    for &bar in order.iter().filter(|&&b| b < doc.bars.len()) {
        let sig = doc.time_sig_at(bar);
        let per_tick = seconds_per_tick(sig, doc.tempo);
        let beat = beat_ticks(sig).max(1);
        let total = bar_ticks(sig);
        let beats_in_bar = total.div_ceil(beat);
        let mut consumed = 0;
        let mut index_in_bar = 0;
        while consumed < total {
            out.push(Beat {
                bar,
                time: t,
                downbeat: consumed == 0,
                index_in_bar,
                beats_in_bar,
            });
            let step = beat.min(total - consumed);
            t += step as f32 * per_tick;
            consumed += step;
            index_in_bar += 1;
        }
    }
    out
}

/// Whether the bar's contents add up to its metre. A `false` here is shown as a
/// warning in the editor, never as an error: half-written bars are normal while typing.
pub fn is_complete(bar: &Bar, time_sig: (u8, u8)) -> bool {
    bar.events.iter().map(|e| e.dur.ticks()).sum::<u32>() == bar_ticks(time_sig)
}

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
    while let Some(base) = NoteValue::ALL.into_iter().find(|v| v.ticks() <= remaining) {
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
/// whatever came before) and reflow the section it governs -- up to the next bar
/// that names its own signature -- into bars of the new length, keeping every
/// event at the same moment of the piece. Going from 4/4 to 2/4 doubles the bars
/// of that section instead of cutting each one in half.
///
/// A note that ends up straddling a new barline is shortened to what fits and
/// the rest of its time becomes rests, so everything after it keeps its onset.
/// ponytail: the cut-off part is silence, not a tied continuation -- ties across
/// a barline are out of scope for v1; carry it as a tied note once they exist.
pub fn set_time_sig(doc: &mut Document, bar_index: usize, sig: Option<(u8, u8)>) {
    if bar_index >= doc.bars.len() {
        return;
    }
    let end = (bar_index + 1..doc.bars.len())
        .find(|&i| doc.bars[i].time_sig.is_some())
        .unwrap_or(doc.bars.len());
    reflow(doc, bar_index, end, sig);
}

/// Put the whole piece in `sig`: every signature change inside it goes, and all
/// of it is reflowed as [`set_time_sig`] reflows one section.
pub fn set_time_sig_everywhere(doc: &mut Document, sig: (u8, u8)) {
    let len = doc.bars.len();
    reflow(doc, 0, len, Some(sig));
}

/// Replace `doc.bars[bar_index..end]` by the same music barred in `sig`, the first
/// new bar carrying `sig`. The old bars' own signature overrides go with them.
fn reflow(doc: &mut Document, bar_index: usize, end: usize, sig: Option<(u8, u8)>) {
    let old_caps: Vec<u32> = (bar_index..end)
        .map(|i| bar_ticks(doc.time_sig_at(i)))
        .collect();
    let old: Vec<Bar> = doc.bars.drain(bar_index..end).collect();
    let new_sig = sig.unwrap_or_else(|| match bar_index {
        0 => (4, 4),
        i => doc.time_sig_at(i - 1),
    });
    let cap = bar_ticks(new_sig).max(1);

    let rests = |ticks: u32| {
        split_ticks(ticks).into_iter().map(|dur| Event {
            dur,
            ..Default::default()
        })
    };
    // The section as one stream, a short old bar padded so it still spans its
    // full length -- every event keeps its onset from the start of the section.
    let stream = old.iter().zip(&old_caps).flat_map(|(bar, &old_cap)| {
        let played: u32 = bar.events.iter().map(|e| e.dur.ticks()).sum();
        bar.events
            .iter()
            .cloned()
            .chain(rests(old_cap.saturating_sub(played)))
    });

    let mut out: Vec<Bar> = Vec::new();
    let mut t = 0u32; // ticks from the start of the section
    for e in stream {
        let mut ticks = e.dur.ticks();
        let mut first = true;
        while ticks > 0 {
            if t / cap >= out.len() as u32 {
                out.push(Bar::default());
            }
            let take = ticks.min(cap - t % cap);
            let events = &mut out.last_mut().expect("pushed above").events;
            match split_ticks(take).first() {
                _ if first && take == ticks => events.push(e.clone()),
                Some(&dur) if first && !e.is_rest() => {
                    events.push(Event { dur, ..e.clone() });
                    events.extend(rests(take - dur.ticks()));
                }
                _ => events.extend(rests(take)),
            }
            first = false;
            ticks -= take;
            t += take;
        }
    }
    if out.is_empty() {
        out.push(Bar::default());
    }

    // Repeat signs follow the moment they marked.
    let mut start = 0u32;
    for (bar, &old_cap) in old.iter().zip(&old_caps) {
        let last = ((start + old_cap).saturating_sub(1) / cap) as usize;
        if let Some(b) = out.get_mut((start / cap) as usize) {
            b.repeat_start |= bar.repeat_start;
        }
        if let (Some(b), Some(n)) = (out.get_mut(last), bar.repeat_end) {
            b.repeat_end = Some(n);
        }
        start += old_cap;
    }

    out[0].time_sig = sig;
    for bar in &mut out {
        refit(bar, new_sig); // pads the last bar, and any tick `split_ticks` couldn't name
    }
    doc.bars.splice(bar_index..bar_index, out);
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
        split_notes_at_beats(&mut doc.bars[i], sig);
        merge_bar_rests(&mut doc.bars[i], sig);
    }

    doc
}

/// Re-spell notes so none hides a beat: a note that begins **off** the beat and
/// runs **past** the next beat boundary is cut there, and the pieces rejoined by a
/// tie.
///
/// This is the note-side twin of [`emit_rests`], and it is what makes a
/// syncopation print the way a player reads it. `8th. 8th. 8th` over beats 1-2 of
/// 4/4 -- the middle quaver straddling the beat -- becomes
/// `8th. | 16th ~ 8th | 8th`: two beamed groups that show both beats, with a tie
/// carrying the sound across. Writing the straddling note as one dotted value
/// instead hides where beat 2 falls, which is the thing notation exists to show.
///
/// A note that *starts* on a beat is left whole however long it is: a half note on
/// beat 1 is a half note, not two tied quarters.
///
/// Only the first piece is struck. The continuations drop the strum and the
/// technique glyph, or the page would show a second pick and a second bend arrow
/// for one sounded note.
///
/// ponytail: cuts at beats only. Gould also asks a long off-beat note not to hide
/// the middle of a 4/4 bar; beats are what the eye reads first, so that rule waits
/// until a real piece needs it.
fn split_notes_at_beats(bar: &mut Bar, time_sig: (u8, u8)) {
    let beat = beat_ticks(time_sig).max(1);
    // A tie is there to clarify the beat, and a sixteenth is as fine a grain as
    // that ever needs -- unless the music is already quicker, in which case its
    // own fastest note is the floor. Below it lies nothing but new rhythmic levels
    // the reader was not asked to parse: an off-grid bar (which the
    // incomplete-bar warning already flags) would shatter into tied
    // thirty-seconds rather than admit it is off-grid.
    let floor = NoteValue::Sixteenth
        .ticks()
        .min(fastest_note(bar).unwrap_or(u32::MAX));
    let mut out: Vec<Event> = Vec::with_capacity(bar.events.len());
    let mut t = 0u32;

    for event in &bar.events {
        let ticks = event.dur.ticks();
        let boundary = (t / beat + 1) * beat;
        if event.is_rest() || t.is_multiple_of(beat) || t + ticks <= boundary {
            out.push(event.clone());
            t += ticks;
            continue;
        }

        // One piece per beat segment, each named by the fewest values `split_ticks`
        // allows -- the same walk `emit_rests` does over a run of silence.
        let mut pieces: Vec<Dur> = Vec::new();
        let (mut a, end) = (t, t + ticks);
        while a < end {
            let b = ((a / beat + 1) * beat).min(end);
            pieces.extend(split_ticks(b - a));
            a = b;
        }

        if pieces.iter().any(|d| d.ticks() < floor) {
            out.push(event.clone());
            t += ticks;
            continue;
        }

        let last = pieces.len().saturating_sub(1);
        for (i, dur) in pieces.into_iter().enumerate() {
            let mut piece = event.clone();
            piece.dur = dur;
            if i > 0 {
                piece.strum = None;
                for note in &mut piece.notes {
                    note.tech = Technique::Plain;
                }
            }
            for note in &mut piece.notes {
                note.tie_next |= i < last;
            }
            out.push(piece);
        }
        t += ticks;
    }

    bar.events = out;
}

/// Rewrite `bar.events` so runs of consecutive rests read the way a score is
/// written: a single whole rest for a wholly silent bar, otherwise the rest run
/// broken at span boundaries, widened past a beat only where that doesn't hide
/// the bar's midpoint. Notes pass through untouched.
fn merge_bar_rests(bar: &mut Bar, time_sig: (u8, u8)) {
    let cap = bar_ticks(time_sig);
    let span = rest_span(time_sig);

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
                emit_rests(start, t, span, &mut out);
            }
            out.push(event.clone());
        }
        t += event.dur.ticks();
    }
    if let Some(start) = run_start.take() {
        emit_rests(start, t, span, &mut out);
    }
    bar.events = out;
}

/// Ticks over which a run of silence is grouped into one rest -- one beat, or
/// two in a simple metre of four-plus beats ([`has_wide_midpoint`]). A run that
/// exactly fills the first or second half of such a bar prints as one half
/// rest; a run crossing the midpoint (e.g. beats 2-3 of 4/4) still breaks
/// there, one rest per beat, because that onset is exactly what must stay
/// visible (Gould, *Behind Bars*). Every other metre (2/4, 3/4, every compound
/// metre) keeps the one-rest-per-beat grid it already had.
fn rest_span(time_sig: (u8, u8)) -> u32 {
    let beat = beat_ticks(time_sig).max(1);
    if has_wide_midpoint(time_sig) {
        beat * 2
    } else {
        beat
    }
}

/// Fill `[from, to)` with rests, split at every `span` boundary so none
/// straddles one, each span-aligned piece named by the fewest values
/// `split_ticks` allows. `span` is one beat, or two in a simple metre of
/// four-plus beats ([`rest_span`]).
///
/// ponytail: still per-beat in 2/4, 3/4 and every compound metre, and never
/// wider than two beats even in 5/4 or 7/4 -- widen further if a user asks.
fn emit_rests(from: u32, to: u32, span: u32, out: &mut Vec<Event>) {
    let mut a = from;
    while a < to {
        let b = ((a / span + 1) * span).min(to); // end of a's span, clamped to `to`
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

/// Ticks of the shortest **note** in the bar -- the fastest rhythmic level the
/// reader is already being asked to parse. `None` for a bar of nothing but rests.
///
/// Rests are excluded on purpose: the editor backfills freed time with sixteenth
/// rests, and those must not pass for what the music is actually made of.
fn fastest_note(bar: &Bar) -> Option<u32> {
    bar.events
        .iter()
        .filter(|e| !e.is_rest())
        .map(|e| e.dur.ticks())
        .min()
}

/// Ticks spanned by one beam group: the window inside which consecutive beamable
/// events are joined under a beam.
///
/// Normally one beat. But in a simple metre of four or more beats whose fastest
/// note is a quaver, the convention (Gould, *Behind Bars*; Read, *Music Notation*)
/// is to beam in wider groups that still expose the bar's midpoint -- 4/4 of
/// running eighths reads as two groups of four, not four pairs. A sixteenth (or
/// faster) anywhere in the bar pulls the window back to one beat, so the beat
/// stays legible under the faster notes. Compound metres already group by their
/// dotted beat and are left alone.
///
/// ponytail: only the 4/4 case is tuned. 2/4 and 3/4 fall through to the per-beat
/// window (conventional enough); widen them here if a user asks.
fn beam_span(time_sig: (u8, u8), bar: &Bar) -> u32 {
    let beat = beat_ticks(time_sig).max(1);
    let shortest = fastest_note(bar).unwrap_or(beat);
    let quaver_is_fastest =
        (NoteValue::Eighth.ticks()..NoteValue::Quarter.ticks()).contains(&shortest);
    if has_wide_midpoint(time_sig) && quaver_is_fastest {
        (beat * 2).min(bar_ticks(time_sig).max(beat))
    } else {
        beat
    }
}

/// Group the bar's events into beams, breaking at each [`beam_span`] boundary.
///
/// Rules applied, which are the conventional ones: rests and notes of a quarter or
/// longer break a run; a run of a single beamable event is left to be drawn with a
/// flag instead; and a new beam starts at a span boundary (a beat, or the bar's
/// midpoint in running-quaver common time) *only when a note actually begins on
/// it*. A note that merely sustains across the boundary -- a syncopation, like the
/// middle quaver of `8th. 8th. 8th` filling beats 1-2 -- stays in the beam, its
/// offset shown by the note values, not by a broken beam.
///
/// ponytail: a note longer than the span that straddles a boundary without landing
/// on it (e.g. `8th.` x4 across 4/4) will still carry the beam across -- rare, and
/// arguably right for that cross-rhythm. Add a hard midpoint break if it bites.
pub fn beam_groups(doc: &Document, bar_index: usize) -> Vec<BeamGroup> {
    let Some(bar) = doc.bars.get(bar_index) else {
        return Vec::new();
    };
    let span = beam_span(doc.time_sig_at(bar_index), bar);
    let onsets = onsets(bar);

    let mut groups = Vec::new();
    let mut run: Vec<usize> = Vec::new();

    for (i, event) in bar.events.iter().enumerate() {
        let beamable = !event.is_rest() && event.dur.base.flags() >= 1;
        if !beamable {
            flush(&mut run, bar, &mut groups);
            continue;
        }
        if !run.is_empty() && onsets[i].is_multiple_of(span) {
            flush(&mut run, bar, &mut groups);
        }
        run.push(i);
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

/// The x-span one [`beam_runs`] range covers, in the caller's own column
/// coordinates (`xs`, one per event in the group). A run of two or more spans
/// from its first column to its last; a lone beamlet has no partner to span
/// to, so it gets a `beamlet`-long stub pointing back into the group -- toward
/// the following note if it's the group's first event, toward the preceding
/// one otherwise. Shared by the notation staff and the tablature-only rhythm
/// row, which draw beams very differently (slanted vs. flat, up or down vs.
/// always stacking upward) but agree on this.
pub fn beam_run_span(
    run: &std::ops::RangeInclusive<usize>,
    xs: &[f32],
    beamlet: f32,
) -> (f32, f32) {
    let (s, e) = (*run.start(), *run.end());
    if s == e {
        let x = xs[s];
        (x, if s == 0 { x + beamlet } else { x - beamlet })
    } else {
        (xs[s], xs[e])
    }
}
