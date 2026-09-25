//! Data model for a tablature document: notes, bars, events, and the document itself.
//!
//! Time is measured in integer ticks so that dotted values (and, later, tuplets) never
//! need floating point: a whole note is [`TICKS_WHOLE`] ticks, chosen to be evenly
//! divisible by 2, 3, 4, and 5.

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::Rgb;

/// Ticks per whole note. 3840 = 2^8 * 3 * 5: divisible by 2, 3, 4, and 5, so a quarter
/// note is exactly 960 ticks and dotted/future-tuplet values stay integers.
pub const TICKS_WHOLE: u32 = 3840;

/// On-disk format version, written into every `.gtab` as `format_version`.
///
/// Bump this only when the shape of the saved JSON changes in a way this build's
/// `Document` can't deserialise straight (a field renamed or removed, a type
/// changed, an enum reworked); adding a field with `#[serde(default)]` does not
/// need a bump. A file with no `format_version` key predates versioning and is
/// treated as version 1. See [`Document::from_json`] for the read path and the
/// migration seam.
///
/// v2 changed `Dur`'s wire shape from `{"base": ..., "dots": ...}` to a plain
/// tick count (see its hand-written `Serialize`/`Deserialize` below) and stopped
/// writing fields that equal their default. A pre-v2 build cannot read a bare
/// tick count as the old two-field shape, so a v2 file must be refused by an
/// older build rather than mis-parsed — the `TooNew` path below already does
/// that, unchanged.
///
/// v3 replaced the `model` (one/two/three lines) and `staff_order` fields with
/// `rows`, an ordered list of [`Row`]s; [`Document::from_json`] rewrites the old
/// pair on load.
///
/// v4 turned `tuning` from exactly six open strings into any number of them (a
/// bass has four, a banjo five) and added `instrument` and `tuning_label`. The
/// array reads the same either way, so nothing is migrated; the bump is so a
/// pre-v4 build calls a four-string file too new rather than corrupt.
pub const FORMAT_VERSION: u32 = 4;

/// Most strings a document may carry. Guards `from_json` against a hand-edited
/// tuning; the presets top out at eight.
pub const MAX_STRINGS: usize = 10;

/// Highest fret a re-fretted note may land on.
const MAX_FRET: i32 = 24;

fn default_format_version() -> u32 {
    1
}

/// Why a `.gtab` failed to load.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    /// Not valid document JSON (truncated, hand-mangled, not a `.gtab` at all).
    Parse,
    /// `format_version` is newer than [`FORMAT_VERSION`] — a file from a later
    /// build of GRAT. The number is the version the file claims.
    TooNew(u32),
}

/// The base note value, before augmentation dots are applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteValue {
    Whole,
    Half,
    #[default]
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl NoteValue {
    /// Every value, longest first: the palette's order, and the one `split_ticks`
    /// is greedy in.
    pub const ALL: [NoteValue; 6] = [
        NoteValue::Whole,
        NoteValue::Half,
        NoteValue::Quarter,
        NoteValue::Eighth,
        NoteValue::Sixteenth,
        NoteValue::ThirtySecond,
    ];

    /// The value a time signature's denominator counts in: 4 is a quarter. One
    /// that names no value counts in quarters.
    pub fn for_denominator(den: u8) -> NoteValue {
        match den {
            1 => NoteValue::Whole,
            2 => NoteValue::Half,
            8 => NoteValue::Eighth,
            16 => NoteValue::Sixteenth,
            32 => NoteValue::ThirtySecond,
            _ => NoteValue::Quarter,
        }
    }

    /// Duration of this note value alone (no dots), in ticks.
    pub fn ticks(self) -> u32 {
        match self {
            NoteValue::Whole => 3840,
            NoteValue::Half => 1920,
            NoteValue::Quarter => 960,
            NoteValue::Eighth => 480,
            NoteValue::Sixteenth => 240,
            NoteValue::ThirtySecond => 120,
        }
    }

    /// Number of beams/flags drawn on the stem.
    pub fn flags(self) -> u8 {
        match self {
            NoteValue::Whole | NoteValue::Half | NoteValue::Quarter => 0,
            NoteValue::Eighth => 1,
            NoteValue::Sixteenth => 2,
            NoteValue::ThirtySecond => 3,
        }
    }
}

/// Augmentation dots the interface allows.
pub const MAX_DOTS: u8 = 3;

/// A note value plus augmentation dots.
///
/// On the wire (`.gtab` format v2+) this is a plain tick count, not
/// `{"base": ..., "dots": ...}` — see the hand-written `Serialize`/`Deserialize`
/// below, and [`Dur::ticks`] / [`Dur::from_ticks`] for the conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dur {
    pub base: NoteValue,
    pub dots: u8,
}

impl Dur {
    /// Total duration in ticks: `base * (2^(dots+1) - 1) / 2^dots`.
    pub fn ticks(self) -> u32 {
        let base = u64::from(self.base.ticks());
        let num = (1u64 << (self.dots + 1)) - 1;
        let den = 1u64 << self.dots;
        (base * num / den) as u32
    }

    /// Inverse of [`Dur::ticks`] — a duration's on-disk form is its tick count.
    // ponytail: linear scan of the 24 combinations (6 values x 0..=MAX_DOTS) at load
    // time. If tuplets ever arrive, this is where a real decode table goes.
    pub fn from_ticks(ticks: u32) -> Option<Dur> {
        NoteValue::ALL.into_iter().find_map(|base| {
            (0..=MAX_DOTS)
                .map(|dots| Dur { base, dots })
                .find(|d| d.ticks() == ticks)
        })
    }
}

impl Serialize for Dur {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.ticks())
    }
}

impl<'de> Deserialize<'de> for Dur {
    fn deserialize<D>(deserializer: D) -> Result<Dur, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // The entire v1 compatibility story for durations: an integer is the new
        // tick-count shape, an object is the old `{base, dots}` shape. No JSON
        // tree rewriting anywhere else is needed because of this.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum DurWire {
            Ticks(u32),
            V1 {
                base: NoteValue,
                #[serde(default)]
                dots: u8,
            },
        }
        match DurWire::deserialize(deserializer)? {
            DurWire::Ticks(ticks) => Dur::from_ticks(ticks).ok_or_else(|| {
                serde::de::Error::custom(format!("not a valid duration tick count: {ticks}"))
            }),
            // `ticks` shifts by the dot count: an unbounded one overflows it.
            DurWire::V1 { base, dots } if dots <= MAX_DOTS => Ok(Dur { base, dots }),
            DurWire::V1 { dots, .. } => Err(serde::de::Error::custom(format!(
                "too many augmentation dots: {dots}"
            ))),
        }
    }
}

/// A playing technique applied to a fretted note.
///
/// The variants that name a partner (hammer-on, pull-off, slide, trill) connect
/// this note to the next event that plays the same string; the renderer finds that
/// partner itself, so nothing has to be kept in sync by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Technique {
    #[default]
    Plain,
    HammerOn,
    PullOff,
    /// Legato slide: a line between the frets plus a slur.
    Slide,
    /// Picked slide: the line without the slur.
    SlideShift,
    /// Approach slide: a small departure fret slid into the note, on one beat.
    /// Unlike `Slide`, it needs no partner event — both frets live on this note.
    SlideIn {
        from_fret: u8,
    },
    /// Appoggiatura.
    Grace,
    /// Quarters of a tone: 1 = quarter, 2 = half, 4 = full.
    Bend {
        quarters: u8,
    },
    BendRelease {
        quarters: u8,
    },
    PreBend {
        quarters: u8,
    },
    Vibrato,
    WideVibrato,
    /// Natural harmonic, printed as `<12>`.
    Harmonic,
    PinchHarmonic,
    Tap,
    /// Slap: a low string struck with the thumb, printed as `S`.
    Slap,
    /// Pop: a string plucked and snapped back against the fretboard, printed as `P`.
    Pop,
    /// Muted note, printed as `x`.
    Dead,
    /// Optional note, printed as `(5)`.
    Ghost,
    Trill {
        to_fret: u8,
    },
}

// Technique colours, shared verbatim by the on-screen and PDF backends.
pub const COLOR_PLAIN: Rgb = Rgb(0x1D, 0x1D, 0x1F);
pub const COLOR_HAMMER_ON: Rgb = Rgb(0x34, 0xC7, 0x59);
pub const COLOR_PULL_OFF: Rgb = Rgb(0x30, 0xB0, 0xC7);
pub const COLOR_SLIDE: Rgb = Rgb(0x0A, 0x84, 0xFF);
pub const COLOR_SLIDE_SHIFT: Rgb = Rgb(0x5A, 0xC8, 0xFA);
pub const COLOR_SLIDE_IN: Rgb = Rgb(0x00, 0x64, 0xD2);
pub const COLOR_GRACE: Rgb = Rgb(0x8E, 0x8E, 0x93);
pub const COLOR_BEND: Rgb = Rgb(0xFF, 0x3B, 0x30);
pub const COLOR_BEND_RELEASE: Rgb = Rgb(0xFF, 0x69, 0x61);
pub const COLOR_PREBEND: Rgb = Rgb(0xFF, 0x95, 0x00);
pub const COLOR_VIBRATO: Rgb = Rgb(0xAF, 0x52, 0xDE);
pub const COLOR_WIDE_VIBRATO: Rgb = Rgb(0xC7, 0x7D, 0xFF);
pub const COLOR_HARMONIC: Rgb = Rgb(0x00, 0xC7, 0xBE);
pub const COLOR_PINCH_HARMONIC: Rgb = Rgb(0xA2, 0x84, 0x5E);
pub const COLOR_TAP: Rgb = Rgb(0x5E, 0x5C, 0xE6);
pub const COLOR_SLAP: Rgb = Rgb(0x9B, 0x1B, 0x8F);
pub const COLOR_POP: Rgb = Rgb(0xC7, 0x4A, 0xBD);
pub const COLOR_DEAD: Rgb = Rgb(0x8E, 0x8E, 0x93);
pub const COLOR_GHOST: Rgb = Rgb(0xAE, 0xAE, 0xB2);
pub const COLOR_TRILL: Rgb = Rgb(0xFF, 0x2D, 0x55);

pub fn technique_color(t: &Technique) -> Rgb {
    match t {
        Technique::Plain => COLOR_PLAIN,
        Technique::HammerOn => COLOR_HAMMER_ON,
        Technique::PullOff => COLOR_PULL_OFF,
        Technique::Slide => COLOR_SLIDE,
        Technique::SlideShift => COLOR_SLIDE_SHIFT,
        Technique::SlideIn { .. } => COLOR_SLIDE_IN,
        Technique::Grace => COLOR_GRACE,
        Technique::Bend { .. } => COLOR_BEND,
        Technique::BendRelease { .. } => COLOR_BEND_RELEASE,
        Technique::PreBend { .. } => COLOR_PREBEND,
        Technique::Vibrato => COLOR_VIBRATO,
        Technique::WideVibrato => COLOR_WIDE_VIBRATO,
        Technique::Harmonic => COLOR_HARMONIC,
        Technique::PinchHarmonic => COLOR_PINCH_HARMONIC,
        Technique::Tap => COLOR_TAP,
        Technique::Slap => COLOR_SLAP,
        Technique::Pop => COLOR_POP,
        Technique::Dead => COLOR_DEAD,
        Technique::Ghost => COLOR_GHOST,
        Technique::Trill { .. } => COLOR_TRILL,
    }
}

impl Technique {
    /// The numeric modifier this technique carries and the range it accepts.
    /// `None` for the fifteen variants that take no number.
    pub fn param(self) -> Option<(u8, RangeInclusive<u8>)> {
        match self {
            Technique::SlideIn { from_fret } => Some((from_fret, 0..=24)),
            Technique::Bend { quarters }
            | Technique::BendRelease { quarters }
            | Technique::PreBend { quarters } => Some((quarters, 1..=8)),
            Technique::Trill { to_fret } => Some((to_fret, 0..=24)),
            _ => None,
        }
    }

    /// The same technique carrying `v`. Unparameterised variants come back unchanged.
    pub fn with_param(self, v: u8) -> Self {
        match self {
            Technique::SlideIn { .. } => Technique::SlideIn { from_fret: v },
            Technique::Bend { .. } => Technique::Bend { quarters: v },
            Technique::BendRelease { .. } => Technique::BendRelease { quarters: v },
            Technique::PreBend { .. } => Technique::PreBend { quarters: v },
            Technique::Trill { .. } => Technique::Trill { to_fret: v },
            other => other,
        }
    }
}

/// A single fretted note within an event.
// ponytail: notes stay `{"string":…,"fret":…}` objects rather than `[string, fret]`
// tuples. The tuple form would save roughly another 1.5 KB on a real file but needs
// a hand-written Serialize/Deserialize (like Dur's above) and makes the file harder
// to read. Upgrade path is that hand-written impl, if size ever outweighs readability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Index into [`Document::tuning`]: 0 = string 1, the top line of the tab
    /// (a guitar's high E) .. `tuning.len() - 1`, the bottom line.
    pub string: u8,
    pub fret: u8,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tech: Technique,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tie_next: bool,
}

/// Strum/tap direction for an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Strum {
    Down,
    Up,
    TapRight,
    TapLeft,
}

/// One rhythmic slot: a chord (possibly a single note) or, if `notes` is empty, a rest.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub dur: Dur,
    #[serde(default, skip_serializing_if = "is_default")]
    pub notes: Vec<Note>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub strum: Option<Strum>,
    /// Palm muting applies to everything sounding at this moment rather than to one
    /// note, and prints as a labelled dashed span over consecutive events.
    #[serde(default, skip_serializing_if = "is_default")]
    pub palm_mute: bool,
    /// Let ring, same span treatment.
    #[serde(default, skip_serializing_if = "is_default")]
    pub let_ring: bool,
}

impl Event {
    pub fn is_rest(&self) -> bool {
        self.notes.is_empty()
    }
}

/// One bar (measure) of events.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bar {
    #[serde(default, skip_serializing_if = "is_default")]
    pub events: Vec<Event>,
    /// `None` inherits the time signature of the previous bar (default (4, 4) for bar 0).
    #[serde(default, skip_serializing_if = "is_default")]
    pub time_sig: Option<(u8, u8)>,
    /// Opening repeat sign on this bar's left barline.
    #[serde(default, skip_serializing_if = "is_default")]
    pub repeat_start: bool,
    /// Closing repeat on this bar's right barline, and how many times the passage is
    /// played in total. `Some(2)` is the plain repeat; more prints as "x3", "x4".
    #[serde(default, skip_serializing_if = "is_default")]
    pub repeat_end: Option<u8>,
}

impl Bar {
    /// A bar of the given time signature, filled with rests.
    ///
    /// A bar with zero events renders with no clickable cells at all (the tablature
    /// row only emits a `Hit` per existing event), so a brand-new bar has to start
    /// out as "rests for the whole bar" rather than as an empty `Vec` — exactly how
    /// notation software shows an untouched measure. One rest per beat keeps the
    /// total in ticks exactly equal to `engrave::bar_ticks(time_sig)`, so a fresh
    /// bar is complete from the start.
    pub fn new_empty(time_sig: Option<(u8, u8)>) -> Self {
        let sig = time_sig.unwrap_or((4, 4));
        let beat = NoteValue::for_denominator(sig.1);
        let events = (0..sig.0)
            .map(|_| Event {
                dur: Dur {
                    base: beat,
                    dots: 0,
                },
                ..Default::default()
            })
            .collect();
        Bar {
            events,
            time_sig,
            repeat_start: false,
            repeat_end: None,
        }
    }

    /// The next event in this bar that plays the same string -- the partner of a
    /// hammer-on, pull-off, slide or trill; a pure data query, so both
    /// `tablature`'s glyphs and `live`'s pitch programme (which needs it for a
    /// slide's destination) share this one implementation.
    pub fn next_on_string(&self, from: usize, string: u8) -> Option<usize> {
        self.events
            .iter()
            .enumerate()
            .skip(from + 1)
            .find(|(_, e)| e.notes.iter().any(|n| n.string == string))
            .map(|(i, _)| i)
    }
}

/// One row a block can stack. A document's [`Document::rows`] lists the ones it
/// shows, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Row {
    /// The tablature staff, one line per string. Always present: its cells are
    /// what the editor clicks.
    Tab,
    /// Stems, beams and rests on their own, with no pitch.
    Rhythm,
    /// The five-line staff.
    Notation,
    /// Strumming direction and tapping marks.
    Strum,
    /// Chord names, recognised from each event's notes.
    Chords,
}

impl Row {
    /// Every row, in the order the row picker lists the ones not yet shown.
    pub const ALL: [Row; 5] = [
        Row::Tab,
        Row::Rhythm,
        Row::Notation,
        Row::Strum,
        Row::Chords,
    ];
}

fn default_rows() -> Vec<Row> {
    vec![Row::Tab, Row::Rhythm]
}

fn is_default_rows(v: &[Row]) -> bool {
    v == default_rows()
}

/// The v1/v2 `model` + `staff_order` pair, rewritten as a v3 `rows` list: the
/// one-line model carried its rhythm under the tab, and `NotationFirst` flipped
/// the stack (it never had anything to flip in a one-line block).
fn legacy_rows(model: &str, notation_first: bool) -> Vec<Row> {
    let mut rows = match model {
        "TwoLine" => vec![Row::Tab, Row::Notation],
        "ThreeLine" => vec![Row::Tab, Row::Strum, Row::Notation],
        _ => return default_rows(),
    };
    if notation_first {
        rows.reverse();
    }
    rows
}

/// The instrument a document is written for. It picks the notation clef and
/// which [`TUNINGS`] the menus offer; the strings themselves are
/// [`Document::tuning`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    #[default]
    Guitar,
    Bass,
    Ukulele,
    BaritoneGuitar,
    BaritoneUkulele,
    Banjo,
    Mandolin,
}

impl Instrument {
    pub const ALL: [Instrument; 7] = [
        Instrument::Guitar,
        Instrument::Bass,
        Instrument::Ukulele,
        Instrument::BaritoneGuitar,
        Instrument::BaritoneUkulele,
        Instrument::Banjo,
        Instrument::Mandolin,
    ];

    /// This instrument's presets, in menu order.
    pub fn tunings(self) -> impl Iterator<Item = &'static Tuning> {
        TUNINGS.iter().filter(move |t| t.instrument == self)
    }

    /// The string counts its presets come in, in table order: 6, 7, 8 for a guitar.
    pub fn string_counts(self) -> Vec<usize> {
        let mut counts: Vec<usize> = Vec::new();
        for t in self.tunings() {
            if !counts.contains(&t.notes.len()) {
                counts.push(t.notes.len());
            }
        }
        counts
    }

    /// The first preset: what choosing this instrument tunes to.
    pub fn default_tuning(self) -> &'static [u8] {
        self.tunings().next().map_or(STANDARD_GUITAR, |t| t.notes)
    }
}

/// One named preset: MIDI open strings, index 0 = string 1 (the top tab line).
pub struct Tuning {
    pub instrument: Instrument,
    /// i18n key of the preset's name.
    pub key: &'static str,
    pub notes: &'static [u8],
}

const STANDARD_GUITAR: &[u8] = &[64, 59, 55, 50, 45, 40];

/// Every preset the Edit menu offers. An instrument's first row is its default;
/// the first row of each string count is what picking that count tunes to.
#[rustfmt::skip]
pub const TUNINGS: &[Tuning] = &[
    Tuning { instrument: Instrument::Guitar, key: "tuning.standard", notes: STANDARD_GUITAR },
    Tuning { instrument: Instrument::Guitar, key: "tuning.drop_d", notes: &[64, 59, 55, 50, 45, 38] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.half_down", notes: &[63, 58, 54, 49, 44, 39] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.whole_down", notes: &[62, 57, 53, 48, 43, 38] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.drop_c", notes: &[62, 57, 53, 48, 43, 36] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.open_g", notes: &[62, 59, 55, 50, 43, 38] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.open_d", notes: &[62, 57, 54, 50, 45, 38] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.open_e", notes: &[64, 59, 56, 52, 47, 40] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.open_c", notes: &[64, 60, 55, 48, 43, 36] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.dadgad", notes: &[62, 57, 55, 50, 45, 38] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.standard", notes: &[64, 59, 55, 50, 45, 40, 35] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.drop_a", notes: &[64, 59, 55, 50, 45, 40, 33] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.standard", notes: &[64, 59, 55, 50, 45, 40, 35, 30] },
    Tuning { instrument: Instrument::Guitar, key: "tuning.drop_e", notes: &[64, 59, 55, 50, 45, 40, 35, 28] },
    Tuning { instrument: Instrument::Bass, key: "tuning.standard", notes: &[43, 38, 33, 28] },
    Tuning { instrument: Instrument::Bass, key: "tuning.drop_d", notes: &[43, 38, 33, 26] },
    Tuning { instrument: Instrument::Bass, key: "tuning.half_down", notes: &[42, 37, 32, 27] },
    Tuning { instrument: Instrument::Bass, key: "tuning.standard", notes: &[43, 38, 33, 28, 23] },
    Tuning { instrument: Instrument::Bass, key: "tuning.high_c", notes: &[48, 43, 38, 33, 28] },
    Tuning { instrument: Instrument::Bass, key: "tuning.standard", notes: &[48, 43, 38, 33, 28, 23] },
    Tuning { instrument: Instrument::Ukulele, key: "tuning.standard", notes: &[69, 64, 60, 67] },
    Tuning { instrument: Instrument::Ukulele, key: "tuning.low_g", notes: &[69, 64, 60, 55] },
    Tuning { instrument: Instrument::Ukulele, key: "tuning.d_tuning", notes: &[71, 66, 62, 69] },
    Tuning { instrument: Instrument::BaritoneGuitar, key: "tuning.b_standard", notes: &[59, 54, 50, 45, 40, 35] },
    Tuning { instrument: Instrument::BaritoneGuitar, key: "tuning.a_standard", notes: &[57, 52, 48, 43, 38, 33] },
    Tuning { instrument: Instrument::BaritoneUkulele, key: "tuning.standard", notes: &[64, 59, 55, 50] },
    Tuning { instrument: Instrument::Banjo, key: "tuning.open_g", notes: &[62, 59, 55, 50, 67] },
    Tuning { instrument: Instrument::Banjo, key: "tuning.double_c", notes: &[62, 60, 55, 48, 67] },
    Tuning { instrument: Instrument::Banjo, key: "tuning.open_d", notes: &[62, 57, 54, 50, 66] },
    Tuning { instrument: Instrument::Mandolin, key: "tuning.standard", notes: &[76, 69, 62, 55] },
];

/// Whether, and how, the page names the open strings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TuningLabel {
    #[default]
    Hidden,
    /// One line under the title, lowest string first.
    Header { letters: bool },
    /// A name on each string line at the head of every tablature staff.
    Strings { letters: bool },
}

/// A complete tablature document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// On-disk format version — see [`FORMAT_VERSION`]. Declared first so it
    /// leads the pretty-printed JSON; missing (pre-versioning files) reads as 1.
    /// Always written as [`FORMAT_VERSION`] regardless of what the in-memory
    /// value is, so re-saving a v1 file never mislabels v2 bytes as v1.
    #[serde(default = "default_format_version", serialize_with = "current_version")]
    pub format_version: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    pub title: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub author: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub instrument: Instrument,
    /// MIDI note numbers of the open strings, one per tab line: index 0 = string 1
    /// (the top line) .. the bottom line. Never empty, at most [`MAX_STRINGS`], and
    /// every note's `string` indexes into it — `from_json` refuses anything else.
    #[serde(default = "default_tuning", skip_serializing_if = "is_std_tuning")]
    pub tuning: Vec<u8>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tuning_label: TuningLabel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub capo: u8,
    #[serde(default = "default_tempo", skip_serializing_if = "is_default_tempo")]
    pub tempo: u16,
    /// Size of what is printed on the tablature staff — fret numbers and every
    /// technique glyph — multiplied by this. The string grid, the clickable
    /// cells and the staff height are unaffected. 1.0 is the default; bigger
    /// reads more easily. Screen and PDF alike.
    ///
    /// 1.0 draws what versions up to 0.3.0 drew at 1.3, so a document saved by one
    /// of those and carrying an explicit value renders larger than it did — divide
    /// the old number by 1.3, or just set it back to 1.0.
    #[serde(default = "default_scale", skip_serializing_if = "is_unit_scale")]
    pub tab_scale: f32,
    /// Horizontal density of the music: every millimetre of note spacing in
    /// `engrave` times this. Below 1.0 packs more bars onto a line (it feeds line
    /// breaking); above 1.0 loosens sparse pieces. Shared by tablature and staff.
    ///
    /// 1.0 is what versions up to 0.3.0 called 0.7, so a document saved by one of
    /// those and carrying an explicit value renders denser than it did — divide the
    /// old number by 0.7, or just set it back to 1.0.
    #[serde(default = "default_scale", skip_serializing_if = "is_unit_scale")]
    pub note_spacing: f32,
    /// Rows of every block, top to bottom. Always holds [`Row::Tab`].
    #[serde(default = "default_rows", skip_serializing_if = "is_default_rows")]
    pub rows: Vec<Row>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub bars: Vec<Bar>,
}

/// Shared `skip_serializing_if` for every field whose on-disk absence should mean
/// "this equals its type's default" — most of `Note`, `Event`, `Bar` and `Document`.
fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

fn is_default_tempo(v: &u16) -> bool {
    *v == 120
}

fn is_std_tuning(v: &[u8]) -> bool {
    v == STANDARD_GUITAR
}

fn is_unit_scale(v: &f32) -> bool {
    *v == 1.0
}

/// `serialize_with` for `format_version`: always writes [`FORMAT_VERSION`], no
/// matter what the in-memory value is (e.g. a document loaded from an older file
/// and not yet re-saved still holds the version it was loaded as).
fn current_version<S: serde::Serializer>(_: &u32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u32(FORMAT_VERSION)
}

fn default_tuning() -> Vec<u8> {
    STANDARD_GUITAR.to_vec()
}

fn default_tempo() -> u16 {
    120
}

fn default_scale() -> f32 {
    1.0
}

impl Document {
    /// Parse a `.gtab`'s JSON, refusing a file written by a newer format than
    /// this build understands.
    ///
    /// The version is read on its own first: a future format need not
    /// deserialise into today's `Document`, but it is still a JSON object with a
    /// `format_version` key, so a too-new file is reported as such rather than as
    /// corrupt. A version bump does not always need a migration step here: v1 to
    /// v2 needed none, carried instead by `Dur`'s untagged `Deserialize` (reads
    /// both the old `{base, dots}` shape and the new tick count) plus
    /// `#[serde(default)]` on every field v2 stopped writing — see the "Change
    /// the save format" recipe in CLAUDE.md. v2 to v3 does need one: the
    /// `model`/`staff_order` pair becomes `rows`, rewritten on the raw JSON
    /// between the probe and the final parse below.
    pub fn from_json(s: &str) -> Result<Document, LoadError> {
        let mut json: serde_json::Value = serde_json::from_str(s).map_err(|_| LoadError::Parse)?;
        let version = match json.get("format_version") {
            None => default_format_version(),
            Some(v) => v
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or(LoadError::Parse)?,
        };
        if version > FORMAT_VERSION {
            return Err(LoadError::TooNew(version));
        }
        if version < 3 {
            if let Some(obj) = json.as_object_mut() {
                let model = obj.remove("model");
                let order = obj.remove("staff_order");
                let rows = legacy_rows(
                    model.as_ref().and_then(|v| v.as_str()).unwrap_or("OneLine"),
                    order.as_ref().and_then(|v| v.as_str()) == Some("NotationFirst"),
                );
                obj.insert("rows".into(), serde_json::to_value(rows).unwrap());
            }
        }
        let mut doc: Document = serde_json::from_value(json).map_err(|_| LoadError::Parse)?;
        // A hand-edited tuning could leave a note with no string to sound on, and
        // `pitch` indexes the tuning with it. A time signature with no beats, or a
        // denominator that is no note value, has no bar to fill.
        let strings = doc.tuning.len();
        if !(1..=MAX_STRINGS).contains(&strings)
            || doc
                .bars
                .iter()
                .flat_map(|b| &b.events)
                .flat_map(|e| &e.notes)
                .any(|n| n.string as usize >= strings)
            || doc
                .bars
                .iter()
                .filter_map(|b| b.time_sig)
                .any(|(num, den)| num == 0 || !matches!(den, 1 | 2 | 4 | 8 | 16 | 32))
        {
            return Err(LoadError::Parse);
        }
        // Past these bounds the page stops being a page, and a number too big
        // for an f32 arrives as infinity. A pre-0.3.0 file's 1.3 or 0.7 is well
        // inside, so it still renders as its author left it.
        doc.tab_scale = doc.tab_scale.clamp(0.25, 4.0);
        doc.note_spacing = doc.note_spacing.clamp(0.25, 4.0);
        // A hand-edited file could drop the tab; without it nothing is clickable.
        if !doc.rows.contains(&Row::Tab) {
            doc.rows.insert(0, Row::Tab);
        }
        Ok(doc)
    }

    /// Serialise this document the way every `.gtab` is written: pretty-printed
    /// JSON, always declaring the current [`FORMAT_VERSION`]. `main.rs`'s save
    /// path goes through this rather than calling `serde_json::to_string_pretty`
    /// itself, so the write policy lives beside the read policy above.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Sounding MIDI pitch of `note`, given this document's tuning and capo.
    /// Capped at the top of the MIDI range rather than wrapping: a hand-edited
    /// fret or capo (or a pasted one) can push the sum past a `u8`.
    pub fn pitch(&self, note: &Note) -> u8 {
        let sum = u16::from(self.tuning[note.string as usize])
            + u16::from(note.fret)
            + u16::from(self.capo);
        sum.min(127) as u8
    }

    /// True when some string sounds higher than the one drawn above it — a
    /// ukulele's high G, a banjo's short fifth string. The lowest note of a chord
    /// is then not necessarily its bass.
    pub fn is_reentrant(&self) -> bool {
        self.tuning.windows(2).any(|w| w[0] < w[1])
    }

    /// Switch to `instrument` with open strings `tuning`.
    ///
    /// With `keep_pitches` false the tablature stays as written: every note keeps
    /// its string and fret, and so sounds whatever the new tuning makes of it;
    /// notes on strings that no longer exist are dropped. With `keep_pitches` true
    /// every note is re-fretted to sound as before, on its own string when that
    /// still reaches the pitch within [`MAX_FRET`], otherwise on the nearest free
    /// string that does; a note no string can play is dropped.
    ///
    /// ponytail: greedy, one note at a time in string order, not an optimal
    /// assignment of a chord to strings, and a tie or legato pair can split across
    /// two strings. A matching over each event's notes if a real voicing ever
    /// comes out wrong.
    pub fn retune(&mut self, instrument: Instrument, tuning: Vec<u8>, keep_pitches: bool) {
        let old = std::mem::replace(&mut self.tuning, tuning);
        self.instrument = instrument;
        let strings = self.tuning.len();
        for event in self.bars.iter_mut().flat_map(|b| &mut b.events) {
            if !keep_pitches {
                event.notes.retain(|n| (n.string as usize) < strings);
                continue;
            }
            let mut placed: Vec<Note> = Vec::with_capacity(event.notes.len());
            for mut note in std::mem::take(&mut event.notes) {
                let pitch = i32::from(old[note.string as usize]) + i32::from(note.fret);
                let from = i32::from(note.string);
                let mut order: Vec<usize> = (0..strings).collect();
                order.sort_by_key(|&s| (s as i32 - from).abs());
                let Some(s) = order.into_iter().find(|&s| {
                    (0..=MAX_FRET).contains(&(pitch - i32::from(self.tuning[s])))
                        && !placed.iter().any(|p| p.string as usize == s)
                }) else {
                    continue;
                };
                let fret = pitch - i32::from(self.tuning[s]);
                let shift =
                    |f: u8| (i32::from(f) + fret - i32::from(note.fret)).clamp(0, MAX_FRET) as u8;
                note.tech = match note.tech {
                    Technique::SlideIn { from_fret } => Technique::SlideIn {
                        from_fret: shift(from_fret),
                    },
                    Technique::Trill { to_fret } => Technique::Trill {
                        to_fret: shift(to_fret),
                    },
                    other => other,
                };
                note.string = s as u8;
                note.fret = fret as u8;
                placed.push(note);
            }
            placed.sort_by_key(|n| n.string);
            event.notes = placed;
        }
    }

    /// Time signature in effect at `bar_index`, resolving inheritance from earlier bars.
    /// Defaults to (4, 4) if no bar up to and including `bar_index` specifies one.
    pub fn time_sig_at(&self, bar_index: usize) -> (u8, u8) {
        self.bars[..=bar_index]
            .iter()
            .rev()
            .find_map(|b| b.time_sig)
            .unwrap_or((4, 4))
    }

    /// A fresh document: 8 empty (rest-filled) 4/4 bars, so a new page isn't blank
    /// and every beat already has a clickable cell.
    pub fn new_empty() -> Self {
        let bars = (0..8)
            .map(|i| Bar::new_empty(if i == 0 { Some((4, 4)) } else { None }))
            .collect();
        Document {
            format_version: FORMAT_VERSION,
            title: String::new(),
            author: String::new(),
            instrument: Instrument::Guitar,
            tuning: default_tuning(),
            tuning_label: TuningLabel::Hidden,
            capo: 0,
            tempo: default_tempo(),
            tab_scale: default_scale(),
            note_spacing: default_scale(),
            rows: default_rows(),
            bars,
        }
    }
}
