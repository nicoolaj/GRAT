//! Data model for a tablature document: notes, bars, events, and the document itself.
//!
//! Time is measured in integer ticks so that dotted values (and, later, tuplets) never
//! need floating point: a whole note is [`TICKS_WHOLE`] ticks, chosen to be evenly
//! divisible by 2, 3, 4, and 5.

use serde::{Deserialize, Serialize};

use crate::Rgb;

/// Ticks per whole note. 3840 = 2^8 * 3 * 5: divisible by 2, 3, 4, and 5, so a quarter
/// note is exactly 960 ticks and dotted/future-tuplet values stay integers.
pub const TICKS_WHOLE: u32 = 3840;

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

/// A note value plus augmentation dots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
pub const COLOR_GRACE: Rgb = Rgb(0x8E, 0x8E, 0x93);
pub const COLOR_BEND: Rgb = Rgb(0xFF, 0x3B, 0x30);
pub const COLOR_BEND_RELEASE: Rgb = Rgb(0xFF, 0x69, 0x61);
pub const COLOR_PREBEND: Rgb = Rgb(0xFF, 0x95, 0x00);
pub const COLOR_VIBRATO: Rgb = Rgb(0xAF, 0x52, 0xDE);
pub const COLOR_WIDE_VIBRATO: Rgb = Rgb(0xC7, 0x7D, 0xFF);
pub const COLOR_HARMONIC: Rgb = Rgb(0x00, 0xC7, 0xBE);
pub const COLOR_PINCH_HARMONIC: Rgb = Rgb(0xA2, 0x84, 0x5E);
pub const COLOR_TAP: Rgb = Rgb(0x5E, 0x5C, 0xE6);
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
        Technique::Grace => COLOR_GRACE,
        Technique::Bend { .. } => COLOR_BEND,
        Technique::BendRelease { .. } => COLOR_BEND_RELEASE,
        Technique::PreBend { .. } => COLOR_PREBEND,
        Technique::Vibrato => COLOR_VIBRATO,
        Technique::WideVibrato => COLOR_WIDE_VIBRATO,
        Technique::Harmonic => COLOR_HARMONIC,
        Technique::PinchHarmonic => COLOR_PINCH_HARMONIC,
        Technique::Tap => COLOR_TAP,
        Technique::Dead => COLOR_DEAD,
        Technique::Ghost => COLOR_GHOST,
        Technique::Trill { .. } => COLOR_TRILL,
    }
}

/// A single fretted note within an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// 0 = string 1 (high E, top line of the tab) .. 5 = string 6 (low E, bottom line).
    pub string: u8,
    pub fret: u8,
    pub tech: Technique,
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
    pub notes: Vec<Note>,
    pub strum: Option<Strum>,
    /// Palm muting applies to everything sounding at this moment rather than to one
    /// note, and prints as a labelled dashed span over consecutive events.
    #[serde(default)]
    pub palm_mute: bool,
    /// Let ring, same span treatment.
    #[serde(default)]
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
    pub events: Vec<Event>,
    /// `None` inherits the time signature of the previous bar (default (4, 4) for bar 0).
    pub time_sig: Option<(u8, u8)>,
    /// Opening repeat sign on this bar's left barline.
    #[serde(default)]
    pub repeat_start: bool,
    /// Closing repeat on this bar's right barline, and how many times the passage is
    /// played in total. `Some(2)` is the plain repeat; more prints as "x3", "x4".
    #[serde(default)]
    pub repeat_end: Option<u8>,
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
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    /// MIDI note numbers of the open strings, index 0 = string 1 (high E) .. index 5 =
    /// string 6 (low E).
    #[serde(default = "default_tuning")]
    pub tuning: [u8; 6],
    #[serde(default)]
    pub capo: u8,
    #[serde(default = "default_tempo")]
    pub tempo: u16,
    #[serde(default)]
    pub model: BlockModel,
    #[serde(default)]
    pub staff_order: StaffOrder,
    #[serde(default)]
    pub bars: Vec<Bar>,
}

fn default_tuning() -> [u8; 6] {
    [64, 59, 55, 50, 45, 40]
}

fn default_tempo() -> u16 {
    120
}

impl Document {
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

    /// A fresh document: 8 empty 4/4 bars, so a new page isn't blank.
    pub fn new_empty() -> Self {
        let bars = (0..8)
            .map(|i| Bar {
                events: Vec::new(),
                time_sig: if i == 0 { Some((4, 4)) } else { None },
                repeat_start: false,
                repeat_end: None,
            })
            .collect();
        Document {
            title: String::new(),
            author: String::new(),
            tuning: default_tuning(),
            capo: 0,
            tempo: default_tempo(),
            model: BlockModel::default(),
            staff_order: StaffOrder::default(),
            bars,
        }
    }
}
