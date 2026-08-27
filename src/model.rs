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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteValue {
    Whole,
    Half,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Technique {
    Plain,
    Slide { to_fret: u8 },
    Grace,
    Bend { quarters: u8 },
    PreBend { quarters: u8 },
    Vibrato,
}

// Technique colours, shared verbatim by the on-screen and PDF backends.
pub const COLOR_PLAIN: Rgb = Rgb(0x1D, 0x1D, 0x1F);
pub const COLOR_SLIDE: Rgb = Rgb(0x0A, 0x84, 0xFF);
pub const COLOR_GRACE: Rgb = Rgb(0x8E, 0x8E, 0x93);
pub const COLOR_BEND: Rgb = Rgb(0xFF, 0x3B, 0x30);
pub const COLOR_PREBEND: Rgb = Rgb(0xFF, 0x95, 0x00);
pub const COLOR_VIBRATO: Rgb = Rgb(0xAF, 0x52, 0xDE);

pub fn technique_color(t: &Technique) -> Rgb {
    match t {
        Technique::Plain => COLOR_PLAIN,
        Technique::Slide { .. } => COLOR_SLIDE,
        Technique::Grace => COLOR_GRACE,
        Technique::Bend { .. } => COLOR_BEND,
        Technique::PreBend { .. } => COLOR_PREBEND,
        Technique::Vibrato => COLOR_VIBRATO,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub dur: Dur,
    pub notes: Vec<Note>,
    pub strum: Option<Strum>,
}

impl Event {
    pub fn is_rest(&self) -> bool {
        self.notes.is_empty()
    }
}

/// One bar (measure) of events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bar {
    pub events: Vec<Event>,
    /// `None` inherits the time signature of the previous bar (default (4, 4) for bar 0).
    pub time_sig: Option<(u8, u8)>,
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
