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
pub const FORMAT_VERSION: u32 = 2;

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
        const VALUES: [NoteValue; 6] = [
            NoteValue::Whole,
            NoteValue::Half,
            NoteValue::Quarter,
            NoteValue::Eighth,
            NoteValue::Sixteenth,
            NoteValue::ThirtySecond,
        ];
        VALUES.into_iter().find_map(|base| {
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
            DurWire::V1 { base, dots } => Ok(Dur { base, dots }),
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
    /// 0 = string 1 (high E, top line of the tab) .. 5 = string 6 (low E, bottom line).
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
        let beat = match sig.1 {
            1 => NoteValue::Whole,
            2 => NoteValue::Half,
            8 => NoteValue::Eighth,
            16 => NoteValue::Sixteenth,
            32 => NoteValue::ThirtySecond,
            _ => NoteValue::Quarter,
        };
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
}

/// How many staff lines (tab / strum / notation) make up one block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockModel {
    #[default]
    OneLine,
    TwoLine,
    ThreeLine,
}

/// Vertical order of the tab and notation staves within a block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaffOrder {
    #[default]
    TabFirst,
    NotationFirst,
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
    /// MIDI note numbers of the open strings, index 0 = string 1 (high E) .. index 5 =
    /// string 6 (low E).
    #[serde(default = "default_tuning", skip_serializing_if = "is_std_tuning")]
    pub tuning: [u8; 6],
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
    #[serde(default, skip_serializing_if = "is_default")]
    pub model: BlockModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub staff_order: StaffOrder,
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

fn is_std_tuning(v: &[u8; 6]) -> bool {
    *v == [64, 59, 55, 50, 45, 40]
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

fn default_tuning() -> [u8; 6] {
    [64, 59, 55, 50, 45, 40]
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
    /// the save format" recipe in CLAUDE.md. When a future bump does need one,
    /// migrate older JSON up to the current shape between the probe and the
    /// final parse below.
    pub fn from_json(s: &str) -> Result<Document, LoadError> {
        #[derive(Deserialize)]
        struct Probe {
            #[serde(default = "default_format_version")]
            format_version: u32,
        }
        let probe: Probe = serde_json::from_str(s).map_err(|_| LoadError::Parse)?;
        if probe.format_version > FORMAT_VERSION {
            return Err(LoadError::TooNew(probe.format_version));
        }
        serde_json::from_str(s).map_err(|_| LoadError::Parse)
    }

    /// Serialise this document the way every `.gtab` is written: pretty-printed
    /// JSON, always declaring the current [`FORMAT_VERSION`]. `main.rs`'s save
    /// path goes through this rather than calling `serde_json::to_string_pretty`
    /// itself, so the write policy lives beside the read policy above.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Sounding MIDI pitch of `note`, given this document's tuning and capo.
    pub fn pitch(&self, note: &Note) -> u8 {
        self.tuning[note.string as usize] + note.fret + self.capo
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
            tuning: default_tuning(),
            capo: 0,
            tempo: default_tempo(),
            tab_scale: default_scale(),
            note_spacing: default_scale(),
            model: BlockModel::default(),
            staff_order: StaffOrder::default(),
            bars,
        }
    }
}
