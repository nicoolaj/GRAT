//! The live player: the score passing under a fixed playhead at its own tempo,
//! the beat being played highlighted at the centre of the screen — karaoke for
//! guitar.
//!
//! A module of the *binary*, like `canvas.rs`, because it drives egui. It
//! engraves nothing of its own: the timing comes from `engrave::timeline`, the
//! geometry from `layout::strip` (one endless line) or `layout::paginate` (the
//! page exactly as it prints), and both are painted through `canvas`'s `Prim`
//! painter. The live view is the same engraving as the page — only bigger, and
//! moving.

use eframe::egui;
use grat::engrave::{Beat, Cue};
use grat::layout::{BarBox, Page, Strip};
use grat::model::{self, Document};
use grat::tablature::Hit;
use grat::{engrave, i18n::t, layout, P, PAGE_H_MM, PAGE_W_MM};

use crate::canvas::{draw_prim, page_rect, to_screen, PAGE_GAP_MM};
use click::Audio;

/// Highlighter yellow. Laid on the paper *under* the ink, which is what makes it
/// read as a marker stroke rather than a coloured box on top of the notes.
const MARKER: egui::Color32 = egui::Color32::from_rgba_premultiplied(0xFF, 0xD8, 0x2A, 0xC0);
/// The fixed "now" line of the single-line view.
const PLAYHEAD: egui::Color32 = egui::Color32::from_rgb(0x5E, 0x5C, 0xE6);
/// The metronome flash's colour ramp: red on the downbeat, then every other beat
/// of the bar shades from pink to yellow -- the last beat before the next
/// downbeat always lands on pure yellow. Red and pink are Apple's systemRed and
/// systemPink (this file's `MARKER` is already this same yellow, at lower alpha).
const FLASH_DOWNBEAT: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x3B, 0x30);
const FLASH_PINK: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x2D, 0x55);
const FLASH_YELLOW: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xD8, 0x2A);
/// The loop: its edges, its A and B tabs, and (faint) the bars it covers.
const LOOP: egui::Color32 = egui::Color32::from_rgb(0x0A, 0x84, 0xFF);
const LOOP_TINT: egui::Color32 = egui::Color32::from_rgba_premultiplied(0x02, 0x1A, 0x33, 0x33);
/// An A marked and waiting for B: a colour of its own, so it never reads as the
/// start of the loop still playing.
const LOOP_PENDING: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x95, 0x00);
/// Radius of the flash dot, screen pixels -- independent of `zoom`: it is a HUD
/// element over the score, not part of it.
const FLASH_RADIUS: f32 = 24.0;
/// Air above and below a highlighted column, so the marker covers the note and
/// its stem instead of stopping at the outer string lines.
const MARKER_PAD_MM: f32 = 1.6;
/// Room kept either side of the window when the single-line view culls: a text
/// is culled by its anchor alone, and the widest one a strip prints (a slash
/// chord name) reaches about 10 mm either side of it.
const CULL_MARGIN_MM: f32 = 20.0;
/// Millimetres to pixels. Far beyond the editor's range: this is a score read at
/// arm's length, with a guitar in the way.
const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 3.0..=24.0;
/// Playing at a quarter speed is how a hard bar gets learnt; past double, the
/// scrolling is faster than anyone reads.
const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.25..=2.0;
/// A count-in longer than this is more warm-up than count-in.
const COUNT_IN_RANGE: std::ops::RangeInclusive<u32> = 0..=8;
/// A frame that took longer than this (a window drag, a hidden window) advances
/// the music by this much and no more, rather than skipping a bar.
const MAX_FRAME_SECS: f32 = 0.1;
/// How long the flash dot stays fully lit, then how much longer it takes to fade
/// out -- both in real (wall-clock) seconds, so the dot reads the same at any
/// rehearsal speed even though the beats it marks do not arrive at a steady rate.
const FLASH_ON_SECS: f64 = 0.10;
const FLASH_FADE_SECS: f64 = 0.20;
/// Base amplitude for one plucked string -- low enough that a six-string chord
/// (`Mixer::add` just sums whatever is playing, there is no gain-staging) still
/// sits under clipping. Not specified by ear yet; tune this first if the piece
/// sounds too loud or too quiet next to the click's own 0.35/0.22.
const NOTE_GAIN: f32 = 0.15;
/// Longest a synthesised note is allowed to ring. Caps a whole note at a slow
/// tempo so the fire-and-forget mixer never holds one open past the point it
/// reads as a held pad rather than a plucked string.
const MAX_TONE_SECS: f32 = 3.0;
/// Fraction of the note where a `Bend` has risen to pitch; it holds bent from
/// here to the end.
const BEND_PEAK_FRAC: f32 = 0.4;
/// Fraction of the note where a `BendRelease` has finished its (quicker) rise —
/// there is still a hold and a release to fit in after it.
const BEND_RELEASE_UP_FRAC: f32 = 0.3;
/// Where the bent plateau ends and the release back to the written pitch
/// begins — shared by `BendRelease` and `PreBend`, whose descents are the same
/// leg.
const RELEASE_FROM_FRAC: f32 = 0.55;
/// Where that release lands back on the written pitch.
const RELEASE_TO_FRAC: f32 = 0.85;
/// Fraction of the note an approach slide (`SlideIn`) spends scooping up from
/// its departure fret.
const SLIDE_IN_FRAC: f32 = 0.12;
/// 32nd notes per quarter note — `Trill` names no rate of its own in the model,
/// so this is what "every 32nd note" means in code.
const TRILL_DIVISOR: f32 = 8.0;
/// Fraction of one trill alternation spent ramping to the next pitch rather
/// than holding flat — short enough to read as a switch between two frets, not
/// a slide between them.
const TRILL_RAMP_FRAC: f32 = 0.1;

/// The metronome click and the piece's own sound, macOS/Windows only -- see the
/// dependency comment next to `rodio` in Cargo.toml and "Verified API facts" in
/// CLAUDE.md.
#[cfg(not(target_os = "linux"))]
mod click {
    use rodio::source::{SineWave, Source};
    use rodio::{ChannelCount, DeviceSinkBuilder, MixerDeviceSink, SampleRate};
    use std::num::NonZero;
    use std::time::Duration;

    /// The open OS audio sink every click and note is mixed into. Playback stops
    /// the instant this is dropped, so [`super::LiveState`] keeps it for as long
    /// as live mode is open rather than reopening a device per sound.
    pub struct Audio(MixerDeviceSink);

    impl Audio {
        /// `None` when no output device is available (a sandboxed CI runner, a
        /// machine with audio disabled) -- the metronome then still flashes, it
        /// just never clicks, rather than live mode refusing to open at all.
        pub fn open() -> Option<Audio> {
            DeviceSinkBuilder::open_default_sink().ok().map(Audio)
        }

        /// A short synthesised tick: higher-pitched and louder on the downbeat,
        /// both scaled by `gain` -- the fader's metronome side.
        pub fn click(&self, downbeat: bool, gain: f32) {
            let freq = if downbeat { 1500.0 } else { 1000.0 };
            let volume = (if downbeat { 0.35 } else { 0.22 }) * gain;
            let tick = SineWave::new(freq)
                .take_duration(Duration::from_millis(45))
                .amplify(volume);
            self.0.mixer().add(tick);
        }

        /// A plucked note whose pitch follows `points` -- (fraction of the note,
        /// Hz), how a bend, a slide and a trill are all expressed -- decaying
        /// linearly to silence over its whole length, so it stops without a
        /// click and never needs stopping by hand.
        pub fn note(&self, points: &[(f32, f32)], secs: f32, gain: f32) {
            let d = Duration::from_secs_f32(secs);
            self.0
                .mixer()
                .add(Voice::new(points, secs).amplify(gain).fade_out(d));
        }
    }

    /// Samples per second `Voice` generates at. Fixed rather than read from the
    /// device: `Mixer::add` resamples every source to the sink's own rate, so
    /// nothing downstream needs this to match the hardware.
    const SAMPLE_RATE: u32 = 48_000;

    /// A plucked note whose pitch follows a piecewise-linear programme -- how a
    /// bend, a slide and a trill are all expressed here. One oscillator for the
    /// whole note, with continuous phase, so a pitch change never clicks the way
    /// a second source started at the join would.
    struct Voice {
        /// (fraction of the note, Hz), sorted, at least one point.
        points: Vec<(f32, f32)>,
        phase: f32,
        i: u32,
        n: u32,
    }

    impl Voice {
        fn new(points: &[(f32, f32)], secs: f32) -> Voice {
            Voice {
                points: points.to_vec(),
                // The triangle's rising zero crossing, so the attack is a clean
                // edge rather than a full-amplitude step.
                phase: 0.25,
                i: 0,
                n: (secs * SAMPLE_RATE as f32).round() as u32,
            }
        }
    }

    impl Iterator for Voice {
        type Item = f32;

        fn next(&mut self) -> Option<f32> {
            if self.i >= self.n {
                return None;
            }
            let freq = super::freq_at(&self.points, self.i as f32 / self.n as f32);
            let sample = 4.0 * (self.phase - (self.phase + 0.5).floor()).abs() - 1.0;
            self.phase = (self.phase + freq / SAMPLE_RATE as f32).fract();
            self.i += 1;
            Some(sample)
        }
    }

    impl Source for Voice {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> ChannelCount {
            NonZero::new(1).unwrap()
        }

        fn sample_rate(&self) -> SampleRate {
            NonZero::new(SAMPLE_RATE).unwrap()
        }

        fn total_duration(&self) -> Option<Duration> {
            Some(Duration::from_secs_f32(self.n as f32 / SAMPLE_RATE as f32))
        }
    }
}

#[cfg(target_os = "linux")]
mod click {
    /// No audio backend on this target -- see the dependency comment next to
    /// `rodio` in Cargo.toml. The metronome flash still fires; it is just silent.
    pub struct Audio;

    impl Audio {
        pub fn open() -> Option<Audio> {
            None
        }

        pub fn click(&self, _downbeat: bool, _gain: f32) {}

        pub fn note(&self, _points: &[(f32, f32)], _secs: f32, _gain: f32) {}
    }
}

/// The player's own state. None of it belongs to the document: a rehearsal speed
/// is temporary by definition, and so is where the playhead happens to be.
pub struct LiveState {
    /// Whether the player has taken the window over from the editor.
    pub open: bool,
    /// One endless line, rather than the printed page's stacked systems.
    pub linear: bool,
    pub playing: bool,
    /// Multiplier on the document's tempo.
    pub speed: f32,
    pub zoom: f32,
    /// The playhead, in seconds from the start of the piece.
    pub t: f32,
    /// Crossfade between the metronome click and the piece's own sound: 0.0 is
    /// metronome only (today's behaviour, and the default), 1.0 is the piece
    /// alone. Equal-power, not linear -- see `fader`.
    pub mix: f32,
    /// Silent bars of metronome, next armed the next time Play starts from a
    /// stop -- a rehearsal setting like `speed`, not part of the document.
    pub count_in_bars: u32,
    /// An active pre-roll: set the moment Play starts one, cleared the moment it
    /// finishes (or live mode is left). While it holds, the metronome ticks and
    /// `t` does not move.
    count_in: Option<CountIn>,
    /// The most recent beat that fired, for the flash dot.
    flash: Option<Flash>,
    /// The looped bars, first and last, as indices into the score. Kept while
    /// the loop is switched off, and from one entry to the next, until set again.
    pub loop_bars: Option<(usize, usize)>,
    /// Whether playback goes round `loop_bars` rather than through the piece.
    pub loop_on: bool,
    /// A bar marked A, waiting for B to close the loop.
    mark_a: Option<usize>,
    /// Times round the loop since it was last set.
    passes: u32,
    /// Speed up each time round the loop, by `ramp_step` of the tempo, until
    /// `speed` reaches `ramp_target`: how a hard passage is brought up to tempo.
    pub ramp: bool,
    pub ramp_step: f32,
    pub ramp_target: f32,
    audio: Option<Audio>,
    show: Option<Show>,
}

/// One firing of the flash dot: resolved once, when the beat fires, from that
/// beat's position in its bar -- `draw_flash` only ever fades and draws it.
struct Flash {
    /// Wall-clock seconds it fired at, for a fade that reads the same at any
    /// rehearsal speed even though beats themselves do not arrive at a steady
    /// real-time rate.
    at: f64,
    color: egui::Color32,
    /// 1-based: the number painted inside the dot.
    number: u32,
}

/// The dot's colour for one beat: red on the downbeat, otherwise a point on the
/// pink-to-yellow ramp scaled by its position in the bar -- the last beat of the
/// bar is always pure yellow, whatever the metre.
fn beat_color(beat: &Beat) -> egui::Color32 {
    if beat.downbeat {
        return FLASH_DOWNBEAT;
    }
    // The ramp spans the bar's secondary beats only (the downbeat itself is
    // never on it): index 1 is pure pink, the last beat is pure yellow. A bar
    // with a single secondary beat (6/8's two-pulse grouping) has no "first" to
    // distinguish from "last" -- yellow wins that tie, since it is the beat
    // right before the next downbeat either way.
    let steps = beat.beats_in_bar.saturating_sub(2);
    let t = if steps == 0 {
        1.0
    } else {
        (beat.index_in_bar - 1) as f32 / steps as f32
    };
    lerp_color(FLASH_PINK, FLASH_YELLOW, t)
}

fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    egui::Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// A count-in's own beat grid, timed from zero and independent of `Show`: the
/// number of bars is a per-play choice, not part of the frozen score.
struct CountIn {
    beats: Vec<Beat>,
    t: f32,
    duration: f32,
}

/// Everything the player draws and sounds, built once when live mode opens and
/// dropped when it closes.
///
/// From `engrave::for_export(doc)`, not from the document the editor holds: that
/// is what drops the blank bars the editor keeps ready under the music — silence
/// the player would otherwise sit through — and writes its per-beat rests the way
/// a score writes them. It renumbers events, so the programmes, the strip and the
/// pages must all come from that one normalised document, `score`; building them
/// from it alone is what guarantees they agree.
struct Show {
    /// The document as the editor holds it, kept only to notice that it has been
    /// replaced underneath the player.
    source: Document,
    score: Document,
    /// The piece as played: repeats unrolled.
    piece: Program,
    /// The looped bars, played straight through, while a loop is on.
    looped: Option<Program>,
    strip: Strip,
    pages: Vec<Page>,
}

impl Show {
    fn build(doc: &Document) -> Show {
        let score = engrave::for_export(doc);
        Show {
            source: doc.clone(),
            piece: Program::build(&score, &engrave::play_order(&score)),
            looped: None,
            strip: layout::strip(&score),
            pages: layout::paginate(&score),
            score,
        }
    }

    /// What is playing: the loop while one is on, the piece otherwise.
    fn program(&self) -> &Program {
        self.looped.as_ref().unwrap_or(&self.piece)
    }
}

/// One run of bars, timed from zero: every event, every metronome beat, every
/// note it sounds.
struct Program {
    cues: Vec<Cue>,
    /// The metronome's own grid, independent of `cues`: a bar's rests click too.
    beats: Vec<Beat>,
    /// Every note the piece itself sounds, mixed against `beats` by the fader.
    tones: Vec<Tone>,
    /// Seconds of music.
    duration: f32,
}

impl Program {
    /// The bars of `score` in `order`: `engrave::play_order` for the piece, a
    /// plain range for a loop.
    fn build(score: &Document, order: &[usize]) -> Program {
        let cues = engrave::timeline(score, order);
        Program {
            beats: engrave::metronome_beats(score, order),
            tones: tones(score, &cues),
            duration: cues.last().map_or(0.0, |c| c.end),
            cues,
        }
    }
}

/// One note to sound: when, along what pitch programme, for how long, how
/// loud. `time` and `secs` are score seconds, like a `Cue` -- the rehearsal
/// speed divides `secs` at the moment it fires, in the transport code, not
/// here. `pitch`'s breakpoints are fractions of the note rather than seconds
/// for exactly that reason: dividing `secs` alone then scales the whole
/// programme to match, so a bend or a trill is still in proportion at any
/// rehearsal speed.
struct Tone {
    time: f32,
    /// Piecewise-linear pitch: (fraction of this tone's own length, Hz).
    pitch: Vec<(f32, f32)>,
    secs: f32,
    gain: f32,
}

/// Natural-harmonic node, in semitones **added to the stopped fret's own
/// pitch** (i.e. to what `Document::pitch` already returns for that fret) --
/// not the absolute interval above the open string. Frets 4, 5, 7 and 9 each
/// ring an overtone higher than where the finger lands (partials 5, 4, 3 and 5
/// again); fret 12 already coincides with its own stopped pitch (partial 2, one
/// octave), hence `_ => 0.0`, which also covers every fret nobody actually
/// harmonics on.
fn harmonic_offset(fret: u8) -> f32 {
    match fret {
        4 => 24.0,
        5 => 19.0,
        7 => 12.0,
        9 => 19.0,
        _ => 0.0,
    }
}

/// Hz for a MIDI note number, A440 -- the tuning every pitch in this file
/// assumes.
fn hz(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

/// The ratio a bend of `quarters` quarter-tones multiplies the written
/// frequency by. `quarters / 2` is semitones (`bend_label`,
/// `tablature.rs:90-100`: 4 = "full" = 2 semitones), and `2^(semitones / 12)`
/// is the usual equal-tempered ratio for that many semitones.
fn bend_ratio(quarters: u8) -> f32 {
    2f32.powf(quarters as f32 / 24.0)
}

/// Frequency at `frac` of the way through a pitch programme: linear between
/// breakpoints, flat before the first and after the last.
///
/// Free rather than a method on `click::Voice` so it is unit-testable on every
/// platform, including Linux, where `Voice` (the only thing that calls it at
/// runtime) does not exist.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn freq_at(points: &[(f32, f32)], frac: f32) -> f32 {
    let Some(&(t0, f0)) = points.first() else {
        return 0.0;
    };
    if frac <= t0 {
        return f0;
    }
    for pair in points.windows(2) {
        let (t0, f0) = pair[0];
        let (t1, f1) = pair[1];
        if frac <= t1 {
            let k = if t1 > t0 {
                (frac - t0) / (t1 - t0)
            } else {
                1.0
            };
            return f0 + (f1 - f0) * k;
        }
    }
    points.last().unwrap().1
}

/// A trill's pitch programme: `f0` (the written fret) and `f1` (`to_fret`)
/// alternating every 32nd note at `tempo`, over a note `secs` long. `Trill`
/// carries no rate of its own in the model, so `TRILL_DIVISOR` is what "every
/// 32nd note" means in code.
///
/// Each alternation is a flat plateau; the switch to the next one is a short
/// ramp ending exactly on the segment boundary rather than a true jump, so the
/// whole programme stays one piecewise-linear function like every other
/// technique's, instead of growing a second interpolation mode.
fn trill_points(f0: f32, f1: f32, tempo: u16, secs: f32) -> Vec<(f32, f32)> {
    let thirty_second = 60.0 / (tempo.max(1) as f32 * TRILL_DIVISOR);
    let n = (secs / thirty_second).round().max(1.0) as u32;
    let seg = 1.0 / n as f32;
    let ramp = seg * TRILL_RAMP_FRAC;
    let mut points = vec![(0.0, f0)];
    for k in 0..n {
        let end = (k + 1) as f32 / n as f32;
        let here = if k % 2 == 0 { f0 } else { f1 };
        if k + 1 == n {
            points.push((1.0, here));
        } else {
            let next = if (k + 1) % 2 == 0 { f0 } else { f1 };
            points.push((end - ramp, here));
            points.push((end, next));
        }
    }
    points
}

/// Every note `doc` (already normalised by `engrave::for_export`) actually
/// sounds, one per struck string, built alongside `cues` from that same
/// document -- invariant 8. A note's pitch is a piecewise-linear programme
/// (played by `click::Voice`, one continuous-phase oscillator per note)
/// instead of one frequency, which is how a bend, a slide and a trill are told
/// apart from a plain, flat tone.
///
/// ponytail: only pitch is a programme -- amplitude is still one flat gain for
/// the whole tone, so a trill's alternations never re-pick and vibrato has no
/// wobble at all. Both want an amplitude programme running beside the pitch
/// one, which is the same shape again (`Voice` reading a second
/// `Vec<(f32, f32)>`) and can wait for someone to ask for it.
fn tones(doc: &Document, cues: &[Cue]) -> Vec<Tone> {
    let mut out = Vec::new();
    for (i, cue) in cues.iter().enumerate() {
        let event = &doc.bars[cue.bar].events[cue.event];
        for note in &event.notes {
            // A continuation of a tie split at a beat boundary: already
            // sounded as part of the chain its first piece started below.
            if i > 0 && cues[i - 1].step == cue.step {
                let prev_event = &doc.bars[cue.bar].events[cues[i - 1].event];
                if prev_event
                    .notes
                    .iter()
                    .any(|n| n.string == note.string && n.tie_next)
                {
                    continue;
                }
            }

            // This note's own length, extended through however many tied
            // pieces follow it on the same string -- notation.rs:689 draws
            // the same chain the same way.
            let mut last = i;
            loop {
                let here = &doc.bars[cues[last].bar].events[cues[last].event];
                if !here
                    .notes
                    .iter()
                    .any(|n| n.string == note.string && n.tie_next)
                {
                    break;
                }
                let Some(next) = cues.get(last + 1) else {
                    break;
                };
                if next.step != cues[last].step {
                    break;
                }
                let next_event = &doc.bars[next.bar].events[next.event];
                if !next_event.notes.iter().any(|n| n.string == note.string) {
                    break;
                }
                last += 1;
            }

            let (gain, secs) = match note.tech {
                model::Technique::Ghost => (NOTE_GAIN * 0.5, cues[last].end - cue.start),
                model::Technique::Dead => (NOTE_GAIN * 0.4, 0.05),
                _ => (NOTE_GAIN, cues[last].end - cue.start),
            };
            // The programme's fractions describe the note as it is actually
            // played, so `secs` is capped before it feeds them, not after.
            let secs = secs.min(MAX_TONE_SECS);

            let base_midi = doc.pitch(note) as f32
                + match note.tech {
                    model::Technique::Harmonic | model::Technique::PinchHarmonic => {
                        harmonic_offset(note.fret)
                    }
                    _ => 0.0,
                };
            let f = hz(base_midi);

            // A fret on this same note's string, at the pitch it would sound
            // played plain -- what `SlideIn`'s departure fret and `Trill`'s
            // `to_fret` both are.
            let fret_pitch = |fret: u8| {
                hz(doc.pitch(&model::Note {
                    string: note.string,
                    fret,
                    tech: model::Technique::Plain,
                    tie_next: false,
                }) as f32)
            };

            let pitch = match note.tech {
                model::Technique::Bend { quarters } => {
                    let b = f * bend_ratio(quarters);
                    vec![(0.0, f), (BEND_PEAK_FRAC, b), (1.0, b)]
                }
                model::Technique::BendRelease { quarters } => {
                    let b = f * bend_ratio(quarters);
                    vec![
                        (0.0, f),
                        (BEND_RELEASE_UP_FRAC, b),
                        (RELEASE_FROM_FRAC, b),
                        (RELEASE_TO_FRAC, f),
                        (1.0, f),
                    ]
                }
                // Drawn as a bare vertical arrow with no return arrow -- the
                // glyph says only "already bent", nothing about coming down
                // (tablature.rs's `PreBend` case). The sound says more: it
                // releases to the written pitch anyway, because that is how a
                // pre-bend is actually played. Settled with the user, not an
                // oversight; a flat bent tone is one breakpoint list away if
                // that ever changes.
                model::Technique::PreBend { quarters } => {
                    let b = f * bend_ratio(quarters);
                    vec![
                        (0.0, b),
                        (RELEASE_FROM_FRAC, b),
                        (RELEASE_TO_FRAC, f),
                        (1.0, f),
                    ]
                }
                // The partner is looked up from `cues[last]`, past however many
                // `Plain` continuations `split_notes_at_beats` inserted between
                // this note and it -- looking up from this note's own event
                // would find the nearest continuation instead, on the same
                // fret as the note itself, and the slide would glide nowhere.
                model::Technique::Slide | model::Technique::SlideShift => {
                    let tail_bar = &doc.bars[cues[last].bar];
                    let partner = tail_bar
                        .next_on_string(cues[last].event, note.string)
                        .and_then(|j| {
                            tail_bar.events[j]
                                .notes
                                .iter()
                                .find(|n| n.string == note.string)
                                .map(|n| hz(doc.pitch(n) as f32))
                        });
                    match partner {
                        Some(p) => vec![(0.0, f), (1.0, p)],
                        // No partner (a slide on a bar's last event): silent in
                        // the glyph too (tablature.rs), so plain in the sound.
                        None => vec![(0.0, f)],
                    }
                }
                model::Technique::SlideIn { from_fret } => {
                    vec![(0.0, fret_pitch(from_fret)), (SLIDE_IN_FRAC, f), (1.0, f)]
                }
                model::Technique::Trill { to_fret } => {
                    trill_points(f, fret_pitch(to_fret), doc.tempo, secs)
                }
                _ => vec![(0.0, f)],
            };

            out.push(Tone {
                time: cue.start,
                pitch,
                secs,
                gain,
            });
        }
    }
    out
}

impl Default for LiveState {
    fn default() -> Self {
        LiveState {
            open: false,
            linear: false,
            playing: false,
            speed: 1.0,
            zoom: 9.0,
            t: 0.0,
            mix: 0.0,
            count_in_bars: 2,
            count_in: None,
            flash: None,
            loop_bars: None,
            loop_on: false,
            mark_a: None,
            passes: 0,
            ramp: false,
            ramp_step: 0.05,
            ramp_target: 1.0,
            audio: None,
            show: None,
        }
    }
}

impl LiveState {
    /// Take the window over, with `doc` frozen as it is now. `selection`, the
    /// bars selected in the editor, becomes the loop.
    pub fn enter(&mut self, doc: &Document, selection: Option<(usize, usize)>) {
        self.open = true;
        self.audio = Audio::open();
        self.freeze(doc, selection);
    }

    /// Freeze `doc` as the score, back at its start -- the loop's, while one is
    /// on -- with the loop set from `selection` when there is one.
    fn freeze(&mut self, doc: &Document, selection: Option<(usize, usize)>) {
        self.playing = false;
        self.t = 0.0;
        self.count_in = None;
        self.flash = None;
        self.mark_a = None;
        if selection.is_some() {
            self.loop_bars = selection;
            self.loop_on = true;
        }
        let mut show = Show::build(doc);
        set_loop(self, &mut show);
        self.show = Some(show);
    }

    /// A new piece in the editor: the loop belonged to the old one.
    pub fn forget_loop(&mut self) {
        self.loop_bars = None;
        self.loop_on = false;
        self.mark_a = None;
    }

    /// Hand the window back to the editor. The speed, size and view the player
    /// was left on survive to the next entry; the frozen score does not.
    pub fn exit(&mut self) {
        self.open = false;
        self.playing = false;
        self.count_in = None;
        self.flash = None;
        self.audio = None;
        self.show = None;
    }
}

/// Index of the cue sounding at `t`. Cue starts only ever increase, which is what
/// makes the binary search legitimate.
fn cue_index(cues: &[Cue], t: f32) -> usize {
    cues.partition_point(|c| c.start <= t).saturating_sub(1)
}

/// The items (by their `time`) that fall in `[prev, now)`. Recomputed from the
/// current position every frame (two binary searches, not a running cursor) so
/// a seek -- a bar jump, the position slider, "back to start" -- can never
/// desync it into replaying a beat or note already passed, or firing a burst of
/// everything it skipped over. Shared by the metronome grid and the tone list:
/// both are just "things with a time," sorted.
fn crossed<T>(items: &[T], time: impl Fn(&T) -> f32, prev: f32, now: f32) -> &[T] {
    let start = items.partition_point(|x| time(x) < prev);
    let end = items.partition_point(|x| time(x) < now);
    &items[start..end]
}

/// Equal-power crossfade: both sides sit at ~0.71 in the middle, so sliding the
/// fader never dips the total loudness the way a linear (1-x, x) pair does.
/// Returns `(metronome gain, piece gain)`.
fn fader(mix: f32) -> (f32, f32) {
    let a = mix.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    // cos(FRAC_PI_2) is -4e-8 in f32, not exactly 0 -- the `max` is what makes a
    // fader hard right (or hard left, for `sin` at 0) truly silent instead of a
    // hiss 148 dB down.
    (a.cos().max(0.0), a.sin().max(0.0))
}

/// Build a fresh count-in: `bars` bars of `sig`, silent but for the metronome,
/// starting on its own clock at zero. Reuses `Bar::new_empty` and
/// `metronome_beats` on a throwaway document rather than re-deriving the same
/// beat-grid arithmetic a third time.
fn start_count_in(bars: u32, sig: (u8, u8), tempo: u16) -> CountIn {
    let mut doc = Document::new_empty();
    doc.tempo = tempo;
    doc.bars = (0..bars)
        .map(|_| model::Bar::new_empty(Some(sig)))
        .collect();
    CountIn {
        beats: engrave::metronome_beats(&doc, &(0..bars as usize).collect::<Vec<_>>()),
        t: 0.0,
        duration: engrave::bar_duration_secs(sig, tempo) * bars as f32,
    }
}

/// The bar under the playhead.
fn current_bar(state: &LiveState, show: &Show) -> usize {
    let prog = show.program();
    prog.cues
        .get(cue_index(&prog.cues, state.t))
        .map_or(0, |c| c.bar)
}

/// Point playback at the loop as it is now set -- build the looped programme,
/// or drop it -- and carry the playhead across onto the same event where the
/// new programme plays it, else to the new programme's start.
fn set_loop(state: &mut LiveState, show: &mut Show) {
    let prog = show.program();
    let at = prog
        .cues
        .get(cue_index(&prog.cues, state.t))
        .map(|c| (c.bar, c.event, state.t - c.start));
    let last = show.score.bars.len().saturating_sub(1);
    show.looped = match state.loop_bars {
        Some((a, b)) if state.loop_on && !show.score.bars.is_empty() => {
            let order: Vec<usize> = (a.min(last)..=b.min(last)).collect();
            Some(Program::build(&show.score, &order))
        }
        _ => None,
    };
    let prog = show.program();
    state.t = at
        .and_then(|(bar, event, into)| {
            prog.cues
                .iter()
                .find(|c| c.bar == bar && c.event == event)
                .map(|c| c.start + into)
        })
        .unwrap_or(0.0);
    state.passes = 0;
}

/// Switch the loop on or off; on with none set, it loops the bar playing now.
fn toggle_loop(state: &mut LiveState, show: &mut Show) {
    state.loop_on = !state.loop_on;
    if state.loop_bars.is_none() {
        let bar = current_bar(state, show);
        state.loop_bars = Some((bar, bar));
    }
    set_loop(state, show);
}

/// A: the loop will start at the bar playing now, once B closes it.
fn mark_a(state: &mut LiveState, show: &Show) {
    state.mark_a = Some(current_bar(state, show));
}

/// B: the loop ends at the bar playing now, back to A -- or, with no A marked,
/// to the loop's own first bar, else this bar alone -- and goes on.
fn mark_b(state: &mut LiveState, show: &mut Show) {
    let b = current_bar(state, show);
    let a = state
        .mark_a
        .take()
        .or(state.loop_bars.map(|(a, _)| a))
        .unwrap_or(b);
    state.loop_bars = Some((a.min(b), a.max(b)));
    state.loop_on = true;
    set_loop(state, show);
}

/// Move the playhead `dt` seconds of music on -- round the loop when one is on,
/// straight on from its first bar with no gap, a little faster each time when
/// the ramp is on -- and return what it crossed: the metronome's beats, the
/// piece's notes.
fn advance<'a>(state: &mut LiveState, show: &'a Show, dt: f32) -> (Vec<Beat>, Vec<&'a Tone>) {
    let prog = show.program();
    let (mut beats, mut tones) = (Vec::new(), Vec::new());
    let (mut from, mut to) = (state.t, state.t + dt);
    loop {
        let end = to.min(prog.duration);
        beats.extend_from_slice(crossed(&prog.beats, |b| b.time, from, end));
        tones.extend(crossed(&prog.tones, |x| x.time, from, end));
        if to < prog.duration {
            state.t = to;
            break;
        }
        if show.looped.is_none() || prog.duration <= 0.0 {
            state.t = prog.duration;
            state.playing = false;
            break;
        }
        (from, to) = (0.0, to - prog.duration);
        state.passes += 1;
        if state.ramp && state.speed < state.ramp_target {
            state.speed = (state.speed + state.ramp_step).min(state.ramp_target);
        }
    }
    (beats, tones)
}

/// Play/Pause: pausing never loses a count-in in progress (it simply stops
/// advancing, like the piece itself does), but starting fresh from a stop arms
/// one if `count_in_bars` calls for it, timed at the metre of the bar `t` is
/// about to resume into.
fn toggle_play(state: &mut LiveState, show: &Show) {
    if state.playing {
        state.playing = false;
        return;
    }
    if state.count_in.is_none() && state.count_in_bars > 0 {
        let bar = current_bar(state, show);
        let sig = show
            .score
            .time_sig_at(bar.min(show.score.bars.len().saturating_sub(1)));
        state.count_in = Some(start_count_in(state.count_in_bars, sig, show.score.tempo));
    }
    state.playing = true;
}

/// Move the playhead one bar back or forward in the order played -- through a
/// repeat, not across it -- to that bar's first event.
fn seek_bar(state: &mut LiveState, show: &Show, delta: i32) {
    let prog = show.program();
    let step = prog
        .cues
        .get(cue_index(&prog.cues, state.t))
        .map_or(0, |c| c.step);
    let target = (step as i64 + i64::from(delta)).max(0) as usize;
    state.t = prog
        .cues
        .iter()
        .find(|c| c.step >= target)
        .map_or(prog.duration, |c| c.start);
}

/// `m:ss`, for the position readout.
fn clock(secs: f32) -> String {
    let s = secs.max(0.0).round() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The union of one event's six string cells: the column a marker covers.
fn column(hits: &[Hit], bar: usize, event: usize) -> Option<(P, P)> {
    hits.iter()
        .filter(|h| h.bar == bar && h.event == event)
        .fold(None, |acc, h| {
            Some(match acc {
                None => (h.min, h.max),
                Some((lo, hi)) => (
                    P::new(lo.x.min(h.min.x), lo.y.min(h.min.y)),
                    P::new(hi.x.max(h.max.x), hi.y.max(h.max.y)),
                ),
            })
        })
}

/// The same lookup across a paginated score, plus the page it landed on.
fn find_column(pages: &[Page], bar: usize, event: usize) -> Option<(usize, (P, P))> {
    pages
        .iter()
        .enumerate()
        .find_map(|(i, p)| column(&p.hits, bar, event).map(|c| (i, c)))
}

/// Paint the highlighter over a column, `frame` being the page (or virtual page)
/// the millimetres are relative to.
fn marker(painter: &egui::Painter, frame: egui::Rect, zoom: f32, lo: P, hi: P) {
    let a = to_screen(frame, zoom, P::new(lo.x, hi.y + MARKER_PAD_MM));
    let b = to_screen(frame, zoom, P::new(hi.x, lo.y - MARKER_PAD_MM));
    painter.rect_filled(egui::Rect::from_two_pos(a, b), 2.0, MARKER);
}

/// The metronome dot, top-right of the screen: lit the instant a beat fires
/// (`flash` is set in the very same step that plays its click), carrying that
/// beat's number in the bar, then fading over wall-clock time so its rhythm
/// stays readable whatever the rehearsal speed.
fn draw_flash(painter: &egui::Painter, rect: egui::Rect, now: f64, flash: &Option<Flash>) {
    let Some(flash) = flash else {
        return;
    };
    let age = now - flash.at;
    if !(0.0..=FLASH_ON_SECS + FLASH_FADE_SECS).contains(&age) {
        return;
    }
    let alpha = if age < FLASH_ON_SECS {
        1.0
    } else {
        1.0 - (age - FLASH_ON_SECS) / FLASH_FADE_SECS
    } as f32;
    let center = egui::pos2(
        rect.right() - FLASH_RADIUS - 10.0,
        rect.top() + FLASH_RADIUS + 10.0,
    );
    painter.circle_filled(center, FLASH_RADIUS, flash.color.gamma_multiply(alpha));
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        flash.number.to_string(),
        egui::FontId::proportional(FLASH_RADIUS * 1.1),
        egui::Color32::BLACK.gamma_multiply(alpha),
    );
}

/// Draw the player. Everything it needs was frozen by [`LiveState::enter`].
pub fn show(ui: &mut egui::Ui, state: &mut LiveState, doc: &Document) {
    // The player works from a score frozen at the moment it opened. The menu bar
    // is still there, so that score can be replaced underneath it -- by File >
    // Open, or by an undo -- and when it is, freeze the new one rather than play
    // music that is no longer in the document.
    if state.show.as_ref().is_some_and(|s| s.source != *doc) {
        state.freeze(doc, None);
    }

    // Taken out of the state for the frame, so the drawing code can borrow the
    // frozen score while the transport writes to the rest. Put back on the one
    // exit path at the bottom — which is why nothing in between returns early.
    let Some(mut show) = state.show.take() else {
        state.open = false;
        return;
    };
    let mut leave = false;

    ui.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
            toggle_play(state, &show);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            leave = true;
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
            seek_bar(state, &show, -1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
            seek_bar(state, &show, 1);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::A) {
            mark_a(state, &show);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::B) {
            mark_b(state, &mut show);
        }
        if i.consume_key(egui::Modifiers::NONE, egui::Key::L) {
            toggle_loop(state, &mut show);
        }
    });

    if state.playing {
        let dt = ui.input(|i| i.stable_dt).min(MAX_FRAME_SECS) * state.speed;
        let now = ui.input(|i| i.time);

        // The count-in and the piece share one clock each, but never both at
        // once: while a count-in holds, it alone advances and `t` sits still at
        // the point playback will resume from. A count-in is silent by
        // definition, so the piece only ever sounds on its own clock.
        let (fired, sounded) = if let Some(ci) = state.count_in.as_mut() {
            let prev = ci.t;
            ci.t += dt;
            let hits = crossed(&ci.beats, |b| b.time, prev, ci.t).to_vec();
            if ci.t >= ci.duration {
                state.count_in = None;
            }
            (hits, Vec::new())
        } else {
            advance(state, &show, dt)
        };

        let (met_gain, piece_gain) = fader(state.mix);

        if met_gain > 0.0 {
            for beat in &fired {
                if let Some(audio) = &state.audio {
                    audio.click(beat.downbeat, met_gain);
                }
                state.flash = Some(Flash {
                    at: now,
                    color: beat_color(beat),
                    number: beat.index_in_bar + 1,
                });
            }
        }

        if piece_gain > 0.0 {
            if let Some(audio) = &state.audio {
                for tone in sounded {
                    audio.note(&tone.pitch, tone.secs / state.speed, tone.gain * piece_gain);
                }
            }
        }

        // egui repaints on input alone; music is the one thing here that moves
        // without any.
        ui.ctx().request_repaint();
    }

    egui::Panel::top("live_transport").show(ui, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let play = if state.playing {
                t("live.pause")
            } else {
                t("live.play")
            };
            if ui.button(play).clicked() {
                toggle_play(state, &show);
            }
            if ui.button(t("live.restart")).clicked() {
                state.t = 0.0;
                state.count_in = None;
            }
            ui.separator();
            ui.label(t("live.metronome"));
            ui.spacing_mut().slider_width = 90.0;
            ui.add(egui::Slider::new(&mut state.mix, 0.0..=1.0).show_value(false));
            ui.label(t("live.piece"));
            ui.label(t("live.count_in"));
            ui.add(egui::DragValue::new(&mut state.count_in_bars).range(COUNT_IN_RANGE));
            ui.separator();
            ui.label(t("live.speed"));
            ui.add(
                egui::DragValue::new(&mut state.speed)
                    .range(SPEED_RANGE)
                    .speed(0.01)
                    .max_decimals(2),
            );
            ui.weak(format!(
                "{:.0} {}",
                show.score.tempo as f32 * state.speed,
                t("live.bpm")
            ));
            ui.separator();
            ui.label(t("live.size"));
            ui.add(
                egui::DragValue::new(&mut state.zoom)
                    .range(ZOOM_RANGE)
                    .speed(0.1)
                    .max_decimals(1),
            );
            ui.separator();
            let view = if state.linear {
                t("live.view_pages")
            } else {
                t("live.view_linear")
            };
            if ui.button(view).clicked() {
                state.linear = !state.linear;
            }
            ui.separator();
            if ui.button(t("live.exit")).clicked() {
                leave = true;
            }
        });
        ui.horizontal(|ui| loop_controls(ui, state, &mut show));
        ui.add_space(4.0);
    });

    egui::Panel::bottom("live_position").show(ui, |ui| {
        ui.horizontal(|ui| {
            match &state.count_in {
                // The count-in's own bar count, not the piece's -- `t` has not
                // moved yet, so the piece's own bar readout would be stale.
                Some(ci) => {
                    let bar = ci
                        .beats
                        .iter()
                        .rev()
                        .find(|b| b.time <= ci.t)
                        .map_or(0, |b| b.bar);
                    ui.colored_label(
                        FLASH_DOWNBEAT,
                        format!(
                            "{} {} / {}",
                            t("live.count_in"),
                            bar + 1,
                            state.count_in_bars
                        ),
                    );
                }
                None => {
                    let bar = current_bar(state, &show) + 1;
                    let bars = show.score.bars.len();
                    ui.label(format!("{} {} / {}", t("status.bar"), bar, bars));
                }
            }
            ui.separator();
            let duration = show.program().duration;
            ui.label(format!("{} / {}", clock(state.t), clock(duration)));
            ui.separator();
            let width = ui.available_width().max(80.0);
            ui.add_sized(
                [width, ui.spacing().interact_size.y],
                egui::Slider::new(&mut state.t, 0.0..=duration.max(0.1)).show_value(false),
            );
        });
    });

    egui::CentralPanel::default()
        .frame(egui::Frame::default())
        .show(ui, |ui| {
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if zoom_delta != 1.0 {
                state.zoom =
                    (state.zoom * zoom_delta).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
            }
            let rect = ui.available_rect_before_wrap();
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            let i = cue_index(&show.program().cues, state.t);
            if state.linear {
                draw_linear(ui.ctx(), &painter, rect, state, &show, i);
            } else {
                draw_pages(ui.ctx(), &painter, rect, state, &show, i);
            }
            draw_flash(&painter, rect, ui.input(|i| i.time), &state.flash);
        });

    state.show = Some(show);
    if leave {
        state.exit();
    }
}

/// The transport's second line: the loop -- on or off, its bars, A and B to mark
/// them while playing, what is going on -- and the speed ramp.
fn loop_controls(ui: &mut egui::Ui, state: &mut LiveState, show: &mut Show) {
    let mut on = state.loop_on;
    if ui
        .checkbox(&mut on, t("live.loop"))
        .on_hover_text(t("live.loop_tip"))
        .changed()
    {
        toggle_loop(state, show);
    }
    let bars = show.score.bars.len().max(1);
    let (mut a, mut b) = state.loop_bars.map_or((1, 1), |(a, b)| (a + 1, b + 1));
    ui.label(t("live.loop_bars"));
    let first = ui.add(egui::DragValue::new(&mut a).range(1..=bars));
    ui.label(t("live.loop_to"));
    let last = ui.add(egui::DragValue::new(&mut b).range(1..=bars));
    if first.changed() || last.changed() {
        state.loop_bars = Some((a.min(b) - 1, a.max(b) - 1));
        state.loop_on = true;
        set_loop(state, show);
    }
    if ui.button("A").on_hover_text(t("live.mark_a")).clicked() {
        mark_a(state, show);
    }
    if ui.button("B").on_hover_text(t("live.mark_b")).clicked() {
        mark_b(state, show);
    }
    match state.mark_a {
        Some(a) => {
            ui.colored_label(
                LOOP_PENDING,
                format!("A : {} {} — {}", t("status.bar"), a + 1, t("live.press_b")),
            );
        }
        None if show.looped.is_some() => {
            ui.label(format!("{} {}", t("live.pass"), state.passes + 1));
        }
        None => {}
    }
    ui.separator();
    ui.checkbox(&mut state.ramp, t("live.ramp"))
        .on_hover_text(t("live.ramp_tip"));
    let mut step = (state.ramp_step * 100.0).round() as u32;
    let mut target = (state.ramp_target * 100.0).round() as u32;
    ui.add_enabled_ui(state.ramp, |ui| {
        if ui
            .add(
                egui::DragValue::new(&mut step)
                    .range(1..=25)
                    .prefix("+")
                    .suffix(" %"),
            )
            .changed()
        {
            state.ramp_step = step as f32 / 100.0;
        }
        ui.label(t("live.ramp_to"));
        let (lo, hi) = (*SPEED_RANGE.start(), *SPEED_RANGE.end());
        let range = (lo * 100.0) as u32..=(hi * 100.0) as u32;
        if ui
            .add(egui::DragValue::new(&mut target).range(range).suffix(" %"))
            .changed()
        {
            state.ramp_target = target as f32 / 100.0;
        }
    });
}

/// The loop made plain on the score, under the ink: its bars tinted. `frame`
/// is the page (or virtual page) the boxes' millimetres are relative to.
fn loop_tint(
    painter: &egui::Painter,
    frame: egui::Rect,
    zoom: f32,
    boxes: &[BarBox],
    lp: (usize, usize),
) {
    for bx in boxes.iter().filter(|b| (lp.0..=lp.1).contains(&b.bar)) {
        painter.rect_filled(bar_rect(frame, zoom, bx), 0.0, LOOP_TINT);
    }
}

/// Over the ink: a bold edge with an "A" tab where the loop starts and a "B"
/// tab where it ends, and, while B is awaited, the new "A" in its own colour.
fn loop_edges(
    painter: &egui::Painter,
    frame: egui::Rect,
    zoom: f32,
    boxes: &[BarBox],
    lp: Option<(usize, usize)>,
    mark_a: Option<usize>,
) {
    let edge = |r: egui::Rect, x: f32, label: &str, colour: egui::Color32| {
        painter.line_segment(
            [egui::pos2(x, r.top()), egui::pos2(x, r.bottom())],
            egui::Stroke::new(3.0, colour),
        );
        let tab =
            egui::Rect::from_center_size(egui::pos2(x, r.top() - 12.0), egui::vec2(22.0, 22.0));
        painter.rect_filled(tab, 5.0, colour);
        painter.text(
            tab.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(15.0),
            egui::Color32::WHITE,
        );
    };
    for bx in boxes {
        let r = bar_rect(frame, zoom, bx);
        if lp.is_some_and(|(a, _)| a == bx.bar) {
            edge(r, r.left(), "A", LOOP);
        }
        if lp.is_some_and(|(_, b)| b == bx.bar) {
            edge(r, r.right(), "B", LOOP);
        }
        // Drawn last: over the loop's own A when both fall on one barline.
        if mark_a == Some(bx.bar) {
            edge(r, r.left(), "A", LOOP_PENDING);
        }
    }
}

/// A bar's box on screen, a little taller than the block, like the marker.
fn bar_rect(frame: egui::Rect, zoom: f32, bx: &BarBox) -> egui::Rect {
    egui::Rect::from_two_pos(
        to_screen(frame, zoom, P::new(bx.min.x, bx.max.y + MARKER_PAD_MM)),
        to_screen(frame, zoom, P::new(bx.max.x, bx.min.y - MARKER_PAD_MM)),
    )
}

/// Where the playhead sits on the strip at `t`, in strip millimetres: the current
/// event's column, glided towards the next one so the ribbon moves continuously
/// instead of hopping from note to note.
fn head_x(show: &Show, t: f32) -> f32 {
    let strip = &show.strip;
    let cues = &show.program().cues;
    let Some(cue) = cues.get(cue_index(cues, t)) else {
        return strip.x;
    };
    let x0 = strip.event_x(cue.bar, cue.event).unwrap_or(strip.x);
    // Forward only: where the next event lies back up the line -- a repeat
    // going round again -- glide on to this bar's closing barline, and jump.
    let x1 = cues
        .get(cue_index(cues, t) + 1)
        .and_then(|n| strip.event_x(n.bar, n.event))
        .filter(|&x| x > x0)
        .or_else(|| strip.bar_span(cue.bar).map(|(_, end)| end))
        .unwrap_or_else(|| strip.end_x());
    let k = ((t - cue.start) / (cue.end - cue.start).max(f32::EPSILON)).clamp(0.0, 1.0);
    x0 + (x1 - x0) * k
}

/// The single-line view: the whole piece as one ribbon, sliding right to left
/// under a fixed playhead.
fn draw_linear(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &LiveState,
    show: &Show,
    i: usize,
) {
    let strip = &show.strip;
    let zoom = state.zoom;
    let centre = rect.center();
    // A virtual page frame, chosen so that `canvas::to_screen` lands the playhead
    // on the centre of the screen and the strip's own middle on the centre line.
    // The strip is then painted by the very same primitive painter as a page,
    // with no second transform to keep in step with the first.
    let frame = egui::Rect::from_min_size(
        egui::pos2(
            centre.x - head_x(show, state.t) * zoom,
            centre.y - (PAGE_H_MM - strip.height * 0.5) * zoom,
        ),
        egui::vec2(PAGE_W_MM * zoom, PAGE_H_MM * zoom),
    );

    // Paper: a ribbon across the whole window, so the music never floats on the
    // desk background.
    let top = to_screen(frame, zoom, P::new(0.0, strip.height + MARKER_PAD_MM)).y;
    let bottom = to_screen(frame, zoom, P::new(0.0, -MARKER_PAD_MM)).y;
    painter.rect_filled(
        egui::Rect::from_x_y_ranges(rect.x_range(), top..=bottom),
        0.0,
        egui::Color32::WHITE,
    );

    let lp = show.looped.is_some().then_some(state.loop_bars).flatten();
    if let Some(lp) = lp {
        loop_tint(painter, frame, zoom, &strip.bars, lp);
    }
    if let Some((lo, hi)) = show
        .program()
        .cues
        .get(i)
        .and_then(|c| column(&strip.hits, c.bar, c.event))
    {
        marker(painter, frame, zoom, lo, hi);
    }
    // Only what the window shows: the whole piece is tens of thousands of
    // prims, and this redraws sixty times a second. Filtering keeps their order,
    // which is their paint order (invariant 4).
    let left = (rect.left() - frame.left()) / zoom - CULL_MARGIN_MM;
    let right = (rect.right() - frame.left()) / zoom + CULL_MARGIN_MM;
    for prim in strip.prims.iter().filter(|p| {
        let (a, b) = p.x_span();
        b >= left && a <= right
    }) {
        draw_prim(ctx, painter, frame, zoom, prim);
    }
    loop_edges(painter, frame, zoom, &strip.bars, lp, state.mark_a);
    painter.line_segment(
        [egui::pos2(centre.x, top), egui::pos2(centre.x, bottom)],
        egui::Stroke::new(1.5, PLAYHEAD.gamma_multiply(0.6)),
    );
}

/// Where the page view centres at `t`, and the column the marker covers: the
/// current event's column, glided towards the next one while it lies further
/// along the same system, snapped otherwise — a line break, or a repeat going
/// round again, is a jump, not a slide.
fn page_focus(show: &Show, i: usize, t: f32) -> Option<(usize, egui::Vec2, P, P)> {
    let cues = &show.program().cues;
    let cue = cues.get(i)?;
    let (page, (lo, hi)) = find_column(&show.pages, cue.bar, cue.event)?;
    let mid = |lo: P, hi: P| egui::vec2((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5);
    let here = mid(lo, hi);
    let next = cues
        .get(i + 1)
        .and_then(|n| find_column(&show.pages, n.bar, n.event))
        .filter(|&(p, (l, h))| {
            p == page && (mid(l, h).y - here.y).abs() < 0.01 && mid(l, h).x > here.x
        })
        .map(|(_, (l, h))| mid(l, h));
    let centre = match next {
        Some(n) => {
            let k = ((t - cue.start) / (cue.end - cue.start).max(f32::EPSILON)).clamp(0.0, 1.0);
            here + (n - here) * k
        }
        None => here,
    };
    Some((page, centre, lo, hi))
}

/// The scrolling-page view: the score exactly as it prints, moved so the beat
/// being played sits in the middle of the screen.
fn draw_pages(
    ctx: &egui::Context,
    painter: &egui::Painter,
    rect: egui::Rect,
    state: &LiveState,
    show: &Show,
    i: usize,
) {
    let Some((page, focus, lo, hi)) = page_focus(show, i, state.t) else {
        return;
    };
    let zoom = state.zoom;
    let centre = rect.center();
    let content_min = egui::pos2(
        centre.x - focus.x * zoom,
        centre.y - page as f32 * (PAGE_H_MM + PAGE_GAP_MM) * zoom - (PAGE_H_MM - focus.y) * zoom,
    );

    // Two passes over the pages: every sheet of paper first, then the marker,
    // then the ink — a highlighter goes under the notes, not over them.
    let visible = |idx: usize| {
        let prect = page_rect(content_min, idx, zoom);
        prect.intersects(rect).then_some(prect)
    };
    for idx in 0..show.pages.len() {
        if let Some(prect) = visible(idx) {
            painter.rect_filled(prect, 2.0, egui::Color32::WHITE);
        }
    }
    let lp = show.looped.is_some().then_some(state.loop_bars).flatten();
    for (idx, p) in show.pages.iter().enumerate() {
        if let (Some(prect), Some(lp)) = (visible(idx), lp) {
            loop_tint(painter, prect, zoom, &p.bars, lp);
        }
    }
    marker(painter, page_rect(content_min, page, zoom), zoom, lo, hi);
    for (idx, p) in show.pages.iter().enumerate() {
        let Some(prect) = visible(idx) else { continue };
        for prim in &p.prims {
            draw_prim(ctx, painter, prect, zoom, prim);
        }
        loop_edges(painter, prect, zoom, &p.bars, lp, state.mark_a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beat(downbeat: bool, index_in_bar: u32, beats_in_bar: u32) -> Beat {
        Beat {
            bar: 0,
            time: 0.0,
            downbeat,
            index_in_bar,
            beats_in_bar,
        }
    }

    #[test]
    fn the_downbeat_is_always_red_whatever_its_place_in_the_grid() {
        assert_eq!(beat_color(&beat(true, 0, 4)), FLASH_DOWNBEAT);
    }

    #[test]
    fn the_last_beat_of_any_bar_is_always_pure_yellow() {
        for beats_in_bar in [2, 3, 4, 6] {
            let last = beat(false, beats_in_bar - 1, beats_in_bar);
            assert_eq!(
                beat_color(&last),
                FLASH_YELLOW,
                "beats_in_bar {beats_in_bar}"
            );
        }
    }

    #[test]
    fn the_ramp_runs_pink_to_yellow_in_order_across_a_bar() {
        // 4/4: beat 1 is the (red) downbeat; 2, 3, 4 should shade strictly
        // pink-to-yellow, each redder (lower blue-to-red ratio isn't the point --
        // just monotonically closer to yellow) than the last.
        let colors: Vec<_> = (1..4).map(|i| beat_color(&beat(false, i, 4))).collect();
        assert_eq!(colors[0], FLASH_PINK, "first non-downbeat starts at pink");
        assert_eq!(colors[2], FLASH_YELLOW, "last beat lands on yellow");
        // Green rises monotonically pink -> yellow; a strict, simple proxy for
        // "the ramp moves the right way and doesn't double back".
        assert!(colors[0].g() < colors[1].g() && colors[1].g() < colors[2].g());
    }

    /// Four 4/4 bars of quarter notes, as the player freezes them, and the
    /// length of one bar in seconds.
    fn four_bars() -> (LiveState, Show, f32) {
        let mut doc = Document::new_empty();
        doc.bars.truncate(4);
        for event in doc.bars.iter_mut().flat_map(|b| &mut b.events) {
            event.notes.push(model::Note {
                string: 0,
                fret: 3,
                tech: model::Technique::Plain,
                tie_next: false,
            });
        }
        let bar = engrave::bar_duration_secs((4, 4), doc.tempo);
        (LiveState::default(), Show::build(&doc), bar)
    }

    #[test]
    fn a_loop_goes_round_without_a_gap_and_counts_its_times_round() {
        let (mut state, mut show, bar) = four_bars();
        state.loop_bars = Some((1, 2));
        state.loop_on = true;
        set_loop(&mut state, &mut show);
        assert!(show.program().cues.iter().all(|c| c.bar == 1 || c.bar == 2));
        assert!((show.program().duration - 2.0 * bar).abs() < 1e-4);
        assert_eq!(state.t, 0.0, "from the loop's first bar");

        state.playing = true;
        let (beats, tones) = advance(&mut state, &show, 2.5 * bar);
        assert_eq!(beats.len(), 10, "eight beats round, two past the join");
        assert_eq!(beats.iter().filter(|b| b.downbeat).count(), 3);
        assert_eq!(tones.len(), 10, "every note, the join's too");
        assert!((state.t - 0.5 * bar).abs() < 1e-3, "{}", state.t);
        assert_eq!(state.passes, 1);
        assert!(state.playing);
    }

    #[test]
    fn the_ramp_speeds_up_each_time_round_to_its_target() {
        let (mut state, mut show, _) = four_bars();
        state.loop_bars = Some((0, 0));
        state.loop_on = true;
        set_loop(&mut state, &mut show);
        (state.ramp, state.speed, state.ramp_step, state.ramp_target) = (true, 0.5, 0.1, 0.65);
        for want in [0.6, 0.65, 0.65] {
            advance(&mut state, &show, show.program().duration);
            assert!(
                (state.speed - want).abs() < 1e-5,
                "{} vs {want}",
                state.speed
            );
        }
    }

    #[test]
    fn a_then_b_loops_the_bars_between_where_the_music_is() {
        let (mut state, mut show, bar) = four_bars();
        state.t = 1.25 * bar;
        mark_a(&mut state, &show);
        assert!(show.looped.is_none(), "A alone loops nothing yet");
        state.t = 3.5 * bar;
        mark_b(&mut state, &mut show);
        assert_eq!((state.loop_bars, state.loop_on), (Some((1, 3)), true));
        assert!((state.t - 2.5 * bar).abs() < 1e-3, "same beat, in the loop");

        toggle_loop(&mut state, &mut show);
        assert!(show.looped.is_none());
        assert!(
            (state.t - 3.5 * bar).abs() < 1e-3,
            "and back onto the piece"
        );
    }

    #[test]
    fn bars_selected_in_the_editor_become_the_loop() {
        let (mut state, _, bar) = four_bars();
        let mut doc = Document::new_empty();
        doc.bars.truncate(4);
        doc.bars[3].events[0].notes = vec![model::Note {
            string: 0,
            fret: 3,
            tech: model::Technique::Plain,
            tie_next: false,
        }];
        state.freeze(&doc, Some((2, 3)));
        let show = state.show.as_ref().unwrap();
        assert!((show.program().duration - 2.0 * bar).abs() < 1e-4);
        assert!(state.loop_on);
    }

    #[test]
    fn without_a_loop_playback_stops_at_the_end() {
        let (mut state, show, _) = four_bars();
        state.playing = true;
        advance(&mut state, &show, 10.0 * show.program().duration);
        assert!(!state.playing);
        assert_eq!(state.t, show.program().duration);
    }

    #[test]
    fn a_repeated_passage_sounds_every_time_round() {
        let mut doc = Document::new_empty();
        doc.bars.truncate(1);
        doc.bars[0].events[0].notes.push(model::Note {
            string: 0,
            fret: 3,
            tech: model::Technique::Plain,
            tie_next: false,
        });
        doc.bars[0].repeat_start = true;
        doc.bars[0].repeat_end = Some(3);
        let doc = engrave::for_export(&doc);
        let cues = engrave::timeline(&doc, &engrave::play_order(&doc));
        let bar = engrave::bar_duration_secs((4, 4), doc.tempo);
        let times: Vec<f32> = tones(&doc, &cues).iter().map(|t| t.time).collect();
        assert_eq!(times.len(), 3, "{times:?}");
        for (pass, t) in times.iter().enumerate() {
            assert!((t - pass as f32 * bar).abs() < 1e-4, "{times:?}");
        }
    }

    #[test]
    fn a_beat_split_tie_sounds_once_for_its_full_length() {
        // Two quarter notes on the open low E, tied: `for_export` produces exactly
        // this shape when a note straddles a beat, and only the first piece should
        // be struck.
        let mut doc = Document::new_empty();
        doc.bars = vec![model::Bar {
            events: vec![
                model::Event {
                    dur: model::Dur {
                        base: model::NoteValue::Quarter,
                        dots: 0,
                    },
                    notes: vec![model::Note {
                        string: 5,
                        fret: 0,
                        tech: model::Technique::Plain,
                        tie_next: true,
                    }],
                    ..Default::default()
                },
                model::Event {
                    dur: model::Dur {
                        base: model::NoteValue::Quarter,
                        dots: 0,
                    },
                    notes: vec![model::Note {
                        string: 5,
                        fret: 0,
                        tech: model::Technique::Plain,
                        tie_next: false,
                    }],
                    ..Default::default()
                },
            ],
            time_sig: Some((4, 4)),
            ..Default::default()
        }];

        let cues = engrave::timeline(&doc, &engrave::play_order(&doc));
        let out = tones(&doc, &cues);

        assert_eq!(out.len(), 1, "the tied continuation must not sound again");
        assert_eq!(out[0].time, 0.0);
        assert!((out[0].secs - (cues[1].end - cues[0].start)).abs() < 1e-6);
        let open_low_e = 440.0 * 2f32.powf((40.0 - 69.0) / 12.0); // string 5 open, standard tuning
        let played = freq_at(&out[0].pitch, 0.0);
        assert!(
            (played - open_low_e).abs() < 0.01,
            "expected {open_low_e}, got {played}"
        );
    }

    /// A one-note, one-event 4/4 bar (deliberately shorter than a full bar --
    /// `tones` and `timeline` do not care, and every other test here needs
    /// only the one event).
    fn one_note_bar(tech: model::Technique) -> Document {
        let mut doc = Document::new_empty();
        doc.bars = vec![model::Bar {
            events: vec![model::Event {
                dur: model::Dur {
                    base: model::NoteValue::Quarter,
                    dots: 0,
                },
                notes: vec![model::Note {
                    string: 5,
                    fret: 3,
                    tech,
                    tie_next: false,
                }],
                ..Default::default()
            }],
            time_sig: Some((4, 4)),
            ..Default::default()
        }];
        doc
    }

    /// Hz of string 5 (low E), standard tuning, at `fret` -- every fixture
    /// below is built on this one string.
    fn low_e_fret(fret: u8) -> f32 {
        440.0 * 2f32.powf((40.0 + fret as f32 - 69.0) / 12.0)
    }

    #[test]
    fn freq_at_lerps_between_breakpoints_and_is_flat_outside_them() {
        let points = [(0.0, 100.0), (0.5, 200.0), (1.0, 100.0)];
        assert_eq!(freq_at(&points, 0.0), 100.0);
        assert_eq!(freq_at(&points, 0.25), 150.0);
        assert_eq!(freq_at(&points, 0.5), 200.0);
        assert_eq!(freq_at(&points, 0.75), 150.0);
        assert_eq!(freq_at(&points, 1.0), 100.0);
        // Flat before the first breakpoint and after the last.
        assert_eq!(freq_at(&points, -1.0), 100.0);
        assert_eq!(freq_at(&points, 2.0), 100.0);
    }

    #[test]
    fn the_three_bends_are_audibly_distinct() {
        let f = low_e_fret(3);
        let bent = f * 2f32.powf(4.0 / 24.0); // quarters: 4 ("full" = 2 semitones)
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;

        let bend = one_note_bar(model::Technique::Bend { quarters: 4 });
        let cues = engrave::timeline(&bend, &engrave::play_order(&bend));
        let out = tones(&bend, &cues);
        assert!(close(freq_at(&out[0].pitch, 0.0), f), "Bend starts written");
        assert!(close(freq_at(&out[0].pitch, 1.0), bent), "Bend ends bent");

        let release = one_note_bar(model::Technique::BendRelease { quarters: 4 });
        let cues = engrave::timeline(&release, &engrave::play_order(&release));
        let out = tones(&release, &cues);
        assert!(
            close(freq_at(&out[0].pitch, 0.0), f),
            "BendRelease starts written"
        );
        assert!(
            close(freq_at(&out[0].pitch, 1.0), f),
            "BendRelease ends written"
        );
        assert!(
            out[0].pitch.iter().any(|&(_, hz)| close(hz, bent)),
            "BendRelease passes through the bent pitch"
        );

        let pre = one_note_bar(model::Technique::PreBend { quarters: 4 });
        let cues = engrave::timeline(&pre, &engrave::play_order(&pre));
        let out = tones(&pre, &cues);
        assert!(
            close(freq_at(&out[0].pitch, 0.0), bent),
            "PreBend starts bent"
        );
        assert!(
            close(freq_at(&out[0].pitch, 1.0), f),
            "PreBend ends written"
        );
    }

    #[test]
    fn a_slides_programme_ends_on_the_partner_looked_up_past_a_tie_split() {
        // A slide long enough to straddle beat 2 of a 4/4 bar: `for_export`
        // cuts it into a tied `Slide` head and a `Plain` continuation, exactly
        // like any other syncopated note (`split_notes_at_beats`). The real
        // partner -- what the slide should glide to -- comes after both
        // pieces. Looking it up from the head's own event, instead of from the
        // tail of its tie chain, would instead find the continuation: same
        // fret as the head, so the slide would glide nowhere.
        let mut doc = Document::new_empty();
        doc.bars = vec![model::Bar {
            events: vec![
                model::Event {
                    dur: model::Dur {
                        base: model::NoteValue::Eighth,
                        dots: 0,
                    },
                    ..Default::default()
                },
                model::Event {
                    dur: model::Dur {
                        base: model::NoteValue::Quarter,
                        dots: 1,
                    },
                    notes: vec![model::Note {
                        string: 5,
                        fret: 3,
                        tech: model::Technique::Slide,
                        tie_next: false,
                    }],
                    ..Default::default()
                },
                model::Event {
                    dur: model::Dur {
                        base: model::NoteValue::Half,
                        dots: 0,
                    },
                    notes: vec![model::Note {
                        string: 5,
                        fret: 7,
                        tech: model::Technique::Plain,
                        tie_next: false,
                    }],
                    ..Default::default()
                },
            ],
            time_sig: Some((4, 4)),
            ..Default::default()
        }];

        let doc = engrave::for_export(&doc);
        assert_eq!(
            doc.bars[0].events.len(),
            4,
            "the slide should have been split at the beat into a tied pair"
        );

        let cues = engrave::timeline(&doc, &engrave::play_order(&doc));
        let out = tones(&doc, &cues);

        assert_eq!(
            out.len(),
            2,
            "one tone for the tied slide, one for its partner"
        );
        assert!((freq_at(&out[0].pitch, 0.0) - low_e_fret(3)).abs() < 0.01);
        assert!(
            (freq_at(&out[0].pitch, 1.0) - low_e_fret(7)).abs() < 0.01,
            "the slide must glide to the real partner, not its own tied continuation"
        );
    }

    #[test]
    fn a_trill_alternates_two_frets_every_32nd_note_at_tempo() {
        // Document::new_empty's default tempo, 120, makes the quarter note
        // below exactly 0.5s -- 8 thirty-second notes -- so the first
        // alternation lands at frac 1/8.
        let doc = one_note_bar(model::Technique::Trill { to_fret: 5 });
        assert_eq!(doc.tempo, 120);
        let cues = engrave::timeline(&doc, &engrave::play_order(&doc));
        let out = tones(&doc, &cues);

        let f0 = low_e_fret(3);
        let f1 = low_e_fret(5);
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        let visits = |target: f32| {
            out[0]
                .pitch
                .iter()
                .filter(|&&(_, hz)| close(hz, target))
                .count()
        };
        assert!(
            visits(f0) > 1,
            "the written fret should sound more than once"
        );
        assert!(visits(f1) > 1, "to_fret should sound more than once");
        assert!(
            out[0]
                .pitch
                .iter()
                .all(|&(_, hz)| close(hz, f0) || close(hz, f1)),
            "only the two trilled pitches should ever appear"
        );

        let first_switch = out[0]
            .pitch
            .iter()
            .find(|&&(_, hz)| close(hz, f1))
            .unwrap()
            .0;
        assert!(
            (first_switch - 0.125).abs() < 0.02,
            "expected the first alternation near frac 0.125, got {first_switch}"
        );
    }

    #[test]
    fn fader_is_equal_power_and_mutes_each_side_at_its_own_end() {
        assert_eq!(fader(0.0), (1.0, 0.0));
        let (met, piece) = fader(1.0);
        assert_eq!(met, 0.0);
        assert!((piece - 1.0).abs() < 1e-6);

        for i in 0..=20 {
            let mix = i as f32 / 20.0;
            let (met, piece) = fader(mix);
            assert!(
                (met * met + piece * piece - 1.0).abs() < 1e-5,
                "mix {mix}: {met}^2 + {piece}^2 != 1"
            );
        }
    }
}
