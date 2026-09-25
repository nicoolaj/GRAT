//! Standard-notation engraving: the five-line staff row of a block.
//!
//! Takes the rhythmic decisions from [`crate::engrave`] and the pitches from the
//! document, and emits [`Prim`]s in page millimetres. Everything is drawn
//! geometrically — heads, rests, accidentals, flags, clef — so the screen and the
//! PDF are pixel-identical by construction and no music font has to be shipped.
//!
//! ponytail: geometric glyphs; embedding a SMuFL font (Bravura) would give finer
//! shapes at the cost of a ~500 KB asset and font subsetting on the PDF side.

use std::ops::Range;

use crate::engrave::{beam_groups, beam_run_span, beam_runs, BeamGroup, Spacing};
use crate::model::{technique_color, Document, Event, Instrument, Note, NoteValue, Technique};
use crate::staff::{barline, barlines, ellipse, rest, time_signature, HEAD_MM, INK, PAPER};
use crate::{Prim, Rgb, P};

/// Distance between two staff lines, in millimetres.
pub const SPACE_MM: f32 = 2.5;
/// Vertical room the notation row needs: the staff plus the usual ledger territory.
/// This is the floor `row_extent` never shrinks below, not the room a system with
/// tall ledger lines actually gets.
pub const ROW_MM: f32 = 21.0;
/// Where the bottom staff line sits inside that row.
pub const BASELINE_OFFSET_MM: f32 = 7.0;

/// Breathing room kept beyond the highest/lowest note's ledger line. Chosen so
/// `row_extent` reproduces `ROW_MM`/`BASELINE_OFFSET_MM` exactly for a system that
/// never leaves the staff (half_range == (0, 8)): `8 * 0.5 * SPACE_MM + 4.0 == 14.0`.
/// 14 mm is also what the G clef's head needs, a space and a half over the staff.
const LEDGER_BREATH_MM: f32 = 4.0;

/// Highest and lowest half-space (see [`staff_position`]) any note in `bars`
/// reaches, floored to the plain staff `(0, 8)` so a system with no notes — or an
/// out-of-range index — still returns today's shape.
pub fn half_range(doc: &Document, bars: Range<usize>) -> (i32, i32) {
    let mut lo = 0;
    let mut hi = 8;
    if let Some(slice) = doc.bars.get(bars) {
        for bar in slice {
            for event in &bar.events {
                for note in &event.notes {
                    let (half, _) = staff_position(doc, note);
                    lo = lo.min(half);
                    hi = hi.max(half);
                }
            }
        }
    }
    (lo, hi)
}

/// Room the notation row needs above/below its origin (the bottom staff line) to
/// clear every note and its ledger lines in `half_range`, never less than the
/// plain-staff default. Grows a system with a high or low run; leaves an ordinary
/// system exactly as it was.
pub fn row_extent(half_range: (i32, i32)) -> (f32, f32) {
    let (lo, hi) = half_range;
    let above = (hi as f32 * 0.5 * SPACE_MM + LEDGER_BREATH_MM).max(ROW_MM - BASELINE_OFFSET_MM);
    let below = ((-lo).max(0) as f32 * 0.5 * SPACE_MM + LEDGER_BREATH_MM).max(BASELINE_OFFSET_MM);
    (above, below)
}

// Geometry, in multiples of the staff space.
const LINE_W: f32 = 0.09;
const HEAD_RX: f32 = 0.66;
const HEAD_RY: f32 = 0.48;
const HEAD_TILT_DEG: f32 = -21.0;
const HEAD_HOLLOW: f32 = 0.62; // inner ellipse ratio for half and whole heads
const STEM_W: f32 = 0.13;
const STEM_LEN: f32 = 3.5;
const STEM_MIN: f32 = 2.0;
const FLAG_STEP: f32 = 0.74; // distance between stacked flags, in spaces
const FLAG_EXTRA: f32 = 0.75; // stem lengthening per flag past the first, in spaces
const BEAM_H: f32 = 0.5;
const BEAM_GAP: f32 = 0.32;
const BEAMLET: f32 = 1.1; // length of a partial beam, in spaces
const LEDGER_EXT: f32 = 0.42;
const MAX_SLOPE: f32 = 1.5; // total beam rise, in spaces

/// Which staff an instrument reads from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Clef {
    /// Treble clef *8va bassa*: written an octave above what sounds. Guitars.
    Treble8,
    /// Treble clef at pitch: ukulele, mandolin.
    Treble,
    /// Bass clef *8va bassa*, the bass guitar's.
    Bass8,
}

/// ponytail: no small "8" is engraved under an octave clef, so the guitar's staff
/// looks exactly as it always has; a `Prim::Text` under the clef if anyone asks.
fn clef(instrument: Instrument) -> Clef {
    match instrument {
        Instrument::Guitar
        | Instrument::BaritoneGuitar
        | Instrument::BaritoneUkulele
        | Instrument::Banjo => Clef::Treble8,
        Instrument::Ukulele | Instrument::Mandolin => Clef::Treble,
        Instrument::Bass => Clef::Bass8,
    }
}

/// A note's vertical position on the staff, in half-spaces above the bottom line,
/// plus whether its spelling needs a sharp. The clef comes from the document's
/// instrument, the pitch from its tuning.
fn staff_position(doc: &Document, note: &Note) -> (i32, bool) {
    let clef = clef(doc.instrument);
    // Diatonic step within the octave, and whether the pitch class is a black key.
    const STEPS: [(i32, bool); 12] = [
        (0, false), // C
        (0, true),  // C#
        (1, false), // D
        (1, true),  // D#
        (2, false), // E
        (3, false), // F
        (3, true),  // F#
        (4, false), // G
        (4, true),  // G#
        (5, false), // A
        (5, true),  // A#
        (6, false), // B
    ];
    let octave = if clef == Clef::Treble { 0 } else { 12 };
    let written = doc.pitch(note) as i32 + octave;
    let (step, sharp) = STEPS[(written.rem_euclid(12)) as usize];
    let diatonic = 7 * written.div_euclid(12) + step;
    // The bottom line of a treble staff is E4 (MIDI 64), diatonic 37; of a bass
    // staff G2 (MIDI 43), diatonic 25.
    let bottom = if clef == Clef::Bass8 { 25 } else { 37 };
    (diatonic - bottom, sharp)
}

/// Draw the notation staff for one system.
///
/// `origin` is the first barline: x of the music start, y of the BOTTOM staff line.
pub fn render(doc: &Document, spacing: &Spacing, origin: P, out: &mut Vec<Prim>) {
    if spacing.bars.is_empty() {
        return;
    }
    let sp = SPACE_MM;
    let right = origin.x + spacing.width;
    // The lines run on under a courtesy time signature past the closing barline.
    let next = spacing.bars.last().map_or(0, |b| b.index + 1);
    let lines_end = right + crate::engrave::time_sig_lead(doc, next, false);

    for line in 0..5 {
        let y = origin.y + line as f32 * sp;
        out.push(Prim::Line {
            a: P {
                x: origin.x - HEAD_MM,
                y,
            },
            b: P { x: lines_end, y },
            w: LINE_W * sp,
            color: INK,
        });
    }
    if clef(doc.instrument) == Clef::Bass8 {
        f_clef(origin.x - HEAD_MM + 1.3, origin.y, sp, out);
    } else {
        g_clef(origin.x - HEAD_MM + 3.6, origin.y, sp, out);
    }

    // Barlines, and the time signature. A repeat mark between two bars carries the
    // closing repeat of the bar on its left and the opening one of the bar on its
    // right, so they are resolved together — and at the same x as the tablature row
    // above or below.
    let top = origin.y + 4.0 * sp;
    let dot_ys = [origin.y + 1.5 * sp, origin.y + 2.5 * sp];
    for (x, kind) in barlines(doc, spacing) {
        barline(kind, origin.x + x, origin.y, top, &dot_ys, out);
    }
    // The metre: at the start of the piece, where it changes, and a courtesy
    // signature closing a system whose successor opens with a change.
    for (x, sig) in crate::engrave::time_sig_marks(doc, spacing) {
        time_signature(
            origin.x + x,
            origin.y + sp,
            origin.y + 3.0 * sp,
            1.9 * sp,
            sig,
            false,
            out,
        );
    }

    for i in 0..spacing.bars.len() {
        render_bar(doc, spacing, i, origin, out);
    }
}

/// One bar's heads, accidentals, rests, stems, beams and ties.
fn render_bar(
    doc: &Document,
    spacing: &Spacing,
    bar_in_system: usize,
    origin: P,
    out: &mut Vec<Prim>,
) {
    let sp = SPACE_MM;
    let layout = &spacing.bars[bar_in_system];
    let Some(bar) = doc.bars.get(layout.index) else {
        return;
    };

    let groups = beam_groups(doc, layout.index);

    // A beamed run shares one stem direction, and the heads have to agree with it:
    // which side of the stem a second sits on depends on it.
    let mut direction: Vec<Option<bool>> = vec![None; bar.events.len()];
    for group in &groups {
        let up = stems_up(
            doc,
            group.events.iter().flat_map(|&ei| &bar.events[ei].notes),
        );
        for &ei in &group.events {
            direction[ei] = Some(up);
        }
    }
    let dir_of = |ei: usize, event: &Event| {
        direction[ei].unwrap_or_else(|| stems_up(doc, event.notes.iter()))
    };

    // Accidentals hold until the end of the bar, so this memory is per bar.
    let mut sharped: Vec<i32> = Vec::new();

    for (ei, event) in bar.events.iter().enumerate() {
        let Some(&ex) = layout.events.get(ei) else {
            continue;
        };
        let x = origin.x + ex;

        if event.is_rest() {
            rest(
                x,
                origin.y + 2.0 * sp,
                sp,
                event.dur.base,
                event.dur.dots,
                out,
            );
            continue;
        }

        let up = dir_of(ei, event);
        let heads = head_positions(doc, event, x, origin.y, sp, up);
        for (head, note) in heads.iter().zip(&event.notes) {
            // A grace note is an ornament: small head, no rhythmic space of its own.
            let small = matches!(note.tech, Technique::Grace);
            let scale = if small { 0.62 } else { 1.0 };
            let (rx, ry) = (HEAD_RX * scale * sp, HEAD_RY * scale * sp);
            ledger_lines(head.half, head.x, origin.y, sp, rx, out);
            note_head(
                P {
                    x: head.x,
                    y: head.y,
                },
                rx,
                ry,
                event.dur.base,
                technique_color(&note.tech),
                out,
            );
            if event.dur.dots > 0 && !small {
                dots(head, sp, event.dur.dots, out);
            }
            accidental(head, sp, &mut sharped, out);
        }

        // Only unbeamed notes carry their own stem and flag; beamed ones get theirs
        // from the group, which knows where the beam ended up.
        if direction[ei].is_none() && !matches!(event.dur.base, NoteValue::Whole) {
            let flags = event.dur.base.flags();
            // Each flag past the first lengthens the stem so the flags don't crowd the head.
            let len = STEM_LEN + flags.saturating_sub(1) as f32 * FLAG_EXTRA;
            if let Some(tip) = stem(&heads, sp, up, len, origin.y + 2.0 * sp, out) {
                if flags > 0 {
                    flag(tip, sp, up, flags, out);
                }
            }
        }
    }

    for group in &groups {
        beam_group(doc, bar, layout, group, origin, sp, out);
    }
    ties(doc, bar, layout, &direction, origin, sp, out);
}

/// One note head placed on the staff.
struct Head {
    /// Half-spaces above the bottom staff line.
    half: i32,
    sharp: bool,
    x: f32,
    y: f32,
    /// True when the head had to move to the other side of the stem (a second).
    offset: bool,
}

/// Which way the stems of `notes` point -- one chord, or a whole beamed run,
/// which shares one direction. The head furthest from the middle line decides;
/// on a tie the stems go down. Averaging the heads instead would get wide chords
/// wrong.
fn stems_up<'a>(doc: &Document, notes: impl Iterator<Item = &'a Note>) -> bool {
    let furthest = notes
        .map(|note| staff_position(doc, note).0 - 4)
        .max_by_key(|d| d.abs())
        .unwrap_or(0);
    furthest < 0
}

fn head_positions(doc: &Document, event: &Event, x: f32, y0: f32, sp: f32, up: bool) -> Vec<Head> {
    let mut heads: Vec<Head> = event
        .notes
        .iter()
        .map(|note| {
            let (half, sharp) = staff_position(doc, note);
            Head {
                half,
                sharp,
                x,
                y: y0 + half as f32 * 0.5 * sp,
                offset: false,
            }
        })
        .collect();

    // Two notes a second apart cannot share a side of the stem: the upper one of
    // each pair steps across.
    let mut order: Vec<usize> = (0..heads.len()).collect();
    order.sort_by_key(|&i| heads[i].half);
    let mut prev: Option<i32> = None;
    let mut prev_offset = false;
    for &i in &order {
        let half = heads[i].half;
        let offset = matches!(prev, Some(p) if (half - p).abs() == 1) && !prev_offset;
        heads[i].offset = offset;
        if offset {
            heads[i].x = x + if up {
                2.0 * HEAD_RX * sp
            } else {
                -2.0 * HEAD_RX * sp
            };
        }
        prev = Some(half);
        prev_offset = offset;
    }
    heads
}

fn note_head(c: P, rx: f32, ry: f32, value: NoteValue, color: Rgb, out: &mut Vec<Prim>) {
    out.push(Prim::Poly {
        pts: ellipse(c, rx, ry, HEAD_TILT_DEG),
        color,
    });
    // Half notes and whole notes are hollow. With filled polygons only, the hole is
    // a paper-coloured ellipse on top — the page is white in both back-ends.
    if matches!(value, NoteValue::Whole | NoteValue::Half) {
        out.push(Prim::Poly {
            pts: ellipse(c, rx * 0.46, ry * HEAD_HOLLOW, HEAD_TILT_DEG + 68.0),
            color: PAPER,
        });
    }
}

fn ledger_lines(half: i32, x: f32, y0: f32, sp: f32, rx: f32, out: &mut Vec<Prim>) {
    let mut push = |h: i32| {
        let y = y0 + h as f32 * 0.5 * sp;
        out.push(Prim::Line {
            a: P {
                x: x - rx - LEDGER_EXT * sp,
                y,
            },
            b: P {
                x: x + rx + LEDGER_EXT * sp,
                y,
            },
            w: LINE_W * 1.3 * sp,
            color: INK,
        });
    };
    let mut h = 10;
    while h <= half {
        push(h);
        h += 2;
    }
    let mut h = -2;
    while h >= half {
        push(h);
        h -= 2;
    }
}

fn dots(head: &Head, sp: f32, n: u8, out: &mut Vec<Prim>) {
    // A dot never sits on a line: on a line, it rides in the space above.
    let dy = if head.half % 2 == 0 { 0.5 * sp } else { 0.0 };
    let mut dx = HEAD_RX * sp + 0.55 * sp;
    for _ in 0..n {
        out.push(Prim::Poly {
            pts: ellipse(
                P {
                    x: head.x + dx,
                    y: head.y + dy,
                },
                0.16 * sp,
                0.16 * sp,
                0.0,
            ),
            color: INK,
        });
        dx += 0.52 * sp;
    }
}

/// Accidentals last until the end of the bar, so a position that was sharpened
/// earlier needs a natural to come back.
fn accidental(head: &Head, sp: f32, sharped: &mut Vec<i32>, out: &mut Vec<Prim>) {
    let already = sharped.contains(&head.half);
    let x = head.x - HEAD_RX * sp - 1.0 * sp;
    if head.sharp && !already {
        sharped.push(head.half);
        sharp_sign(x, head.y, sp, out);
    } else if !head.sharp && already {
        sharped.retain(|&p| p != head.half);
        natural_sign(x, head.y, sp, out);
    }
}

fn sharp_sign(x: f32, y: f32, sp: f32, out: &mut Vec<Prim>) {
    for dx in [-0.17, 0.17] {
        out.push(Prim::Line {
            a: P {
                x: x + dx * sp,
                y: y - 0.68 * sp,
            },
            b: P {
                x: x + dx * sp,
                y: y + 0.62 * sp,
            },
            w: 0.1 * sp,
            color: INK,
        });
    }
    for dy in [-0.26, 0.24] {
        out.push(Prim::Poly {
            pts: vec![
                P {
                    x: x - 0.36 * sp,
                    y: y + dy * sp - 0.09 * sp,
                },
                P {
                    x: x + 0.36 * sp,
                    y: y + dy * sp + 0.06 * sp,
                },
                P {
                    x: x + 0.36 * sp,
                    y: y + dy * sp + 0.22 * sp,
                },
                P {
                    x: x - 0.36 * sp,
                    y: y + dy * sp + 0.07 * sp,
                },
            ],
            color: INK,
        });
    }
}

fn natural_sign(x: f32, y: f32, sp: f32, out: &mut Vec<Prim>) {
    out.push(Prim::Line {
        a: P {
            x: x - 0.2 * sp,
            y: y - 0.3 * sp,
        },
        b: P {
            x: x - 0.2 * sp,
            y: y + 0.75 * sp,
        },
        w: 0.1 * sp,
        color: INK,
    });
    out.push(Prim::Line {
        a: P {
            x: x + 0.2 * sp,
            y: y - 0.75 * sp,
        },
        b: P {
            x: x + 0.2 * sp,
            y: y + 0.3 * sp,
        },
        w: 0.1 * sp,
        color: INK,
    });
    for dy in [-0.3, 0.16] {
        out.push(Prim::Poly {
            pts: vec![
                P {
                    x: x - 0.2 * sp,
                    y: y + dy * sp,
                },
                P {
                    x: x + 0.2 * sp,
                    y: y + dy * sp + 0.1 * sp,
                },
                P {
                    x: x + 0.2 * sp,
                    y: y + dy * sp + 0.24 * sp,
                },
                P {
                    x: x - 0.2 * sp,
                    y: y + dy * sp + 0.14 * sp,
                },
            ],
            color: INK,
        });
    }
}

/// x at which the stem leaves the head cluster.
fn stem_x(heads: &[Head], sp: f32, up: bool) -> f32 {
    let base = heads
        .iter()
        .find(|h| !h.offset)
        .map(|h| h.x)
        .unwrap_or(heads[0].x);
    base + if up {
        HEAD_RX * sp * 0.92
    } else {
        -HEAD_RX * sp * 0.92
    }
}

/// The head a stem starts from, and the one it has to clear.
fn head_extremes(heads: &[Head], up: bool) -> (f32, f32) {
    let lo = heads.iter().map(|h| h.y).fold(f32::INFINITY, f32::min);
    let hi = heads.iter().map(|h| h.y).fold(f32::NEG_INFINITY, f32::max);
    if up {
        (lo, hi)
    } else {
        (hi, lo)
    }
}

/// Draw a stem of `len` spaces, stretched if needed to reach the middle line `mid`
/// (a note on ledger lines would otherwise hang off a stem that never meets the staff);
/// returns its tip.
fn stem(heads: &[Head], sp: f32, up: bool, len: f32, mid: f32, out: &mut Vec<Prim>) -> Option<P> {
    if heads.is_empty() {
        return None;
    }
    let x = stem_x(heads, sp, up);
    let (foot, far) = head_extremes(heads, up);
    let tip = if up {
        (far + len * sp).max(mid)
    } else {
        (far - len * sp).min(mid)
    };
    out.push(Prim::Line {
        a: P { x, y: foot },
        b: P { x, y: tip },
        w: STEM_W * sp,
        color: INK,
    });
    Some(P { x, y: tip })
}

/// Flags always hang to the right of the stem and curl back towards the head. Each
/// is a filled outline around a cubic spine: it swells to its widest a third of the
/// way along, then thins to a fine point, as engraved flags do.
fn flag(tip: P, sp: f32, up: bool, n: u8, out: &mut Vec<Prim>) {
    const STEPS: usize = 16;
    let dir = if up { -1.0 } else { 1.0 };
    for i in 0..n {
        let y = tip.y + dir * i as f32 * FLAG_STEP * sp;
        // Only the flag nearest the head curls back; one further out has a shorter,
        // straighter tail, or its curl would cut through the flag beneath it.
        let spine = if i + 1 == n {
            [(0.0, 0.0), (0.95, 0.15), (1.15, 0.95), (0.45, 1.65)]
        } else {
            [(0.0, 0.0), (0.9, 0.15), (1.1, 0.55), (0.95, 1.0)]
        };
        let ctrl = spine.map(|(dx, dy)| (tip.x + dx * sp, y + dir * dy * sp));
        let at = |t: f32| {
            let u = 1.0 - t;
            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            (0..4).fold((0.0, 0.0), |(x, y), k| {
                (x + w[k] * ctrl[k].0, y + w[k] * ctrl[k].1)
            })
        };
        let mut left = Vec::with_capacity(STEPS + 1);
        let mut right = Vec::with_capacity(STEPS + 1);
        for s in 0..=STEPS {
            let t = s as f32 / STEPS as f32;
            let (x, y) = at(t);
            let (x1, y1) = at((t + 0.01).min(1.0));
            let (x0, y0) = at((t - 0.01).max(0.0));
            let (tx, ty) = (x1 - x0, y1 - y0);
            let len = (tx * tx + ty * ty).sqrt().max(1e-6);
            let half =
                0.5 * sp * (0.34 * (std::f32::consts::PI * (0.25 + 0.75 * t)).sin()).max(0.03);
            let (nx, ny) = (-ty / len * half, tx / len * half);
            left.push(P {
                x: x + nx,
                y: y + ny,
            });
            right.push(P {
                x: x - nx,
                y: y - ny,
            });
        }
        right.reverse();
        left.extend(right);
        out.push(Prim::Poly {
            pts: left,
            color: INK,
        });
    }
}

/// Stems and beams for one beamed run.
fn beam_group(
    doc: &Document,
    bar: &crate::model::Bar,
    layout: &crate::engrave::BarSpacing,
    group: &BeamGroup,
    origin: P,
    sp: f32,
    out: &mut Vec<Prim>,
) {
    let up = stems_up(
        doc,
        group.events.iter().flat_map(|&ei| &bar.events[ei].notes),
    );

    // Column x, the head the stem grows from, and the head it has to clear.
    let mut cols: Vec<(f32, f32, f32)> = Vec::with_capacity(group.events.len());
    for &ei in &group.events {
        let Some(&ex) = layout.events.get(ei) else {
            return;
        };
        let heads = head_positions(doc, &bar.events[ei], origin.x + ex, origin.y, sp, up);
        if heads.is_empty() {
            return;
        }
        let (foot, far) = head_extremes(&heads, up);
        cols.push((stem_x(&heads, sp, up), foot, far));
    }
    let (Some(&first), Some(&last)) = (cols.first(), cols.last()) else {
        return;
    };
    let dx = last.0 - first.0;
    if dx <= 0.0 {
        return;
    }

    let sign = if up { 1.0 } else { -1.0 };
    let mut y0 = first.2 + sign * STEM_LEN * sp;
    // Beams slant with the music, but only so far — and never more steeply than
    // a quarter of the run's own width.
    let rise = (last.2 + sign * STEM_LEN * sp - y0)
        .clamp(-MAX_SLOPE * sp, MAX_SLOPE * sp)
        .clamp(-0.25 * dx, 0.25 * dx);

    // Push the beam outwards until even the shortest stem is still a stem.
    let mut deficit: f32 = 0.0;
    for &(x, _, far) in &cols {
        let at = y0 + (x - first.0) * rise / dx;
        deficit = deficit.max(sign * (far + sign * STEM_MIN * sp - at));
        // Like a lone stem, a beamed one on ledger notes still has to reach the middle line.
        deficit = deficit.max(sign * (origin.y + 2.0 * sp - at));
    }
    y0 += sign * deficit.max(0.0);
    let beam_y = |x: f32| y0 + (x - first.0) * rise / dx;

    for &(x, foot, _) in &cols {
        out.push(Prim::Line {
            a: P { x, y: foot },
            b: P { x, y: beam_y(x) },
            w: STEM_W * sp,
            color: INK,
        });
    }

    // Primary beam across the run, then secondary beams over the shorter values
    // only; a lone short value gets a stub pointing into the group.
    let xs: Vec<f32> = cols.iter().map(|&(x, _, _)| x).collect();
    let step = (BEAM_H + BEAM_GAP) * sp;
    for level in 1..=3u8 {
        for run in beam_runs(group, level) {
            let (x0, x1) = beam_run_span(&run, &xs, BEAMLET * sp);
            let inset = sign * (level - 1) as f32 * step;
            out.push(beam_quad(
                x0,
                beam_y(x0) - inset,
                x1,
                beam_y(x1) - inset,
                sp,
                up,
            ));
        }
    }
}

/// A beam is a slanted parallelogram lying on the stem tips.
fn beam_quad(x0: f32, y0: f32, x1: f32, y1: f32, sp: f32, up: bool) -> Prim {
    let d = if up { -BEAM_H * sp } else { BEAM_H * sp };
    Prim::Poly {
        pts: vec![
            P { x: x0, y: y0 },
            P { x: x1, y: y1 },
            P { x: x1, y: y1 + d },
            P { x: x0, y: y0 + d },
        ],
        color: INK,
    }
}

/// Ties between consecutive events of the same string.
///
/// ponytail: ties stop at the barline; carrying one across needs the next system's
/// geometry, which the layout owns and this function cannot see.
fn ties(
    doc: &Document,
    bar: &crate::model::Bar,
    layout: &crate::engrave::BarSpacing,
    direction: &[Option<bool>],
    origin: P,
    sp: f32,
    out: &mut Vec<Prim>,
) {
    for (ei, event) in bar.events.iter().enumerate() {
        let (Some(next), Some(&x0), Some(&x1)) = (
            bar.events.get(ei + 1),
            layout.events.get(ei),
            layout.events.get(ei + 1),
        ) else {
            continue;
        };
        if !event.notes.iter().any(|n| n.tie_next) {
            continue;
        }
        let up = direction[ei].unwrap_or_else(|| stems_up(doc, event.notes.iter()));
        let next_up = direction
            .get(ei + 1)
            .copied()
            .flatten()
            .unwrap_or_else(|| stems_up(doc, next.notes.iter()));
        let heads = head_positions(doc, event, origin.x + x0, origin.y, sp, up);
        let next_heads = head_positions(doc, next, origin.x + x1, origin.y, sp, next_up);

        for (note, head) in event.notes.iter().zip(&heads) {
            if !note.tie_next {
                continue;
            }
            let Some(target) = next
                .notes
                .iter()
                .position(|n| n.string == note.string)
                .and_then(|i| next_heads.get(i))
            else {
                continue;
            };
            // A tie bulges away from the stems, and so hangs below -- the smile a
            // reader looks for -- whenever either note's stem points up. That
            // covers the pair straddling a beat, whose two halves routinely land
            // in beam groups with opposite stems; only a pair stemmed down on
            // both sides puts the tie above, where nothing collides with it.
            let sign = if up || next_up { -1.0 } else { 1.0 };
            // On the outside of both chords -- always so for a single note -- the
            // tie tucks under (or over) the heads, its ends near their centres, the
            // way an engraver sets it: between two close heads, edge to edge, it
            // would be a speck. An inner note of a chord has a neighbour a third
            // away on that side, so its tie stays between the heads.
            let outer =
                |hs: &[Head], h: &Head| hs.iter().all(|o| sign * (h.half - o.half) as f32 >= 0.0);
            let (inset, lift) = if outer(&heads, head) && outer(&next_heads, target) {
                (0.45, HEAD_RY + 0.2)
            } else {
                (1.15, 0.30)
            };
            let a = P {
                x: head.x + HEAD_RX * sp * inset,
                y: head.y + sign * lift * sp,
            };
            let b = P {
                x: target.x - HEAD_RX * sp * inset,
                y: target.y + sign * lift * sp,
            };
            out.push(crate::staff::tie(a, b, sign * 0.62 * sp, 0.11 * sp, INK));
        }
    }
}

/// A treble clef, traced as one continuous stroke: the spiral, the thick upstroke,
/// the apex loop, the thin stem, the hook. `cx` is the bowl's centre, which sits on
/// the G line; everything is in staff spaces from that point, y up, with the stroke
/// width carried along so the ink swells on the rising curves and thins on the stem
/// the way an engraved clef does.
///
/// The spiral is geometric: one and a half clockwise turns from a dot on the G
/// line, its radius `r = R0 + (R - R0)·u` opening evenly in the fraction `u` of the
/// way round, so the inner turn stays small enough for the belly to pass over it.
/// The rest of the glyph is a spline through points measured off a classic engraved
/// clef: the belly rises from the spiral's left edge round lines one to three and
/// crosses the stem on the third line, and the head is a narrow,
/// pointed loop standing about a space and a half over the staff — the reason the
/// row reserves 14 mm above its baseline.
///
/// ponytail: a procedural G clef rather than a real glyph outline. It reads
/// correctly at print size; embed a music font if it ever has to be exact.
fn g_clef(cx: f32, y0: f32, sp: f32, out: &mut Vec<Prim>) {
    const R0: f32 = 0.10;
    const R: f32 = 1.05;
    const TURNS: f32 = 1.5;
    const SQUASH: f32 = 0.93; // the bowl is a touch wider than tall, as engraved
    const SPIRAL_STEPS: usize = 108;
    // Stroke width at each quarter turn: thin inside, thick over the top, thin
    // along the bottom, swelling again where the upstroke takes over.
    const SPIRAL_W: [f32; 7] = [0.10, 0.12, 0.16, 0.26, 0.24, 0.12, 0.22];
    // (x, y, width) from the spiral's end: a Catmull-Rom spline runs through them.
    const PATH: [[f32; 3]; 24] = [
        [-1.05, 0.00, 0.22],
        [-0.98, 0.50, 0.35], // the thick left side of the belly
        [-0.68, 0.86, 0.42],
        [-0.22, 1.04, 0.46],
        [0.27, 1.00, 0.50], // crosses the stem on the third line
        [0.62, 1.28, 0.45],
        [0.80, 1.80, 0.37],
        [0.84, 2.40, 0.26],
        [0.78, 3.00, 0.16],
        [0.62, 3.80, 0.24],
        [0.42, 4.18, 0.40], // the apex, a space and a half over the top line
        [0.24, 3.95, 0.32],
        [0.14, 3.50, 0.24],
        [0.09, 3.00, 0.16],
        [0.09, 2.50, 0.12],
        [0.16, 1.90, 0.13], // the stem, leaning like a pen stroke
        [0.27, 1.00, 0.13],
        [0.38, 0.00, 0.13],
        [0.47, -1.00, 0.13],
        [0.54, -1.70, 0.14],
        [0.50, -2.25, 0.16], // the hook
        [0.22, -2.48, 0.20],
        [-0.12, -2.44, 0.24],
        [-0.35, -2.25, 0.30],
    ];
    const DOT: [f32; 3] = [R0, 0.00, 0.12];
    const PEARL: [f32; 3] = [-0.43, -2.00, 0.37];

    let g = y0 + sp; // the G line: the second one up, which the spiral curls around
    let at = |x: f32, y: f32| P {
        x: cx + x * sp,
        y: g + y * sp,
    };
    let mut path: Vec<(P, f32)> = Vec::with_capacity(SPIRAL_STEPS + 1 + (PATH.len() - 1) * 6);

    for i in 0..=SPIRAL_STEPS {
        let u = i as f32 / SPIRAL_STEPS as f32;
        let turns = TURNS * u;
        let r = R0 + (R - R0) * u;
        let phi = std::f32::consts::TAU * turns; // clockwise from three o'clock
        let q = turns * 4.0;
        let k = (q as usize).min(SPIRAL_W.len() - 2);
        let w = SPIRAL_W[k] + (SPIRAL_W[k + 1] - SPIRAL_W[k]) * (q - k as f32);
        path.push((at(r * phi.cos(), -r * phi.sin() * SQUASH), w));
    }

    // Clamping the ends makes the first tangent point straight up, which is
    // exactly how the eased spiral arrives.
    spline(&PATH, at, &mut path);
    stroke(&path, sp, out);
    for [x, y, r] in [DOT, PEARL] {
        out.push(Prim::Poly {
            pts: ellipse(at(x, y), r * sp, r * sp, 0.0),
            color: INK,
        });
    }
}

/// A bass clef: the knob on the F line (the fourth), the arch over the top line
/// thickening down the right side and thinning into its tail, and the two dots
/// either side of the F line. Same construction as [`g_clef`].
fn f_clef(cx: f32, y0: f32, sp: f32, out: &mut Vec<Prim>) {
    // (x, y, width) in spaces from the knob's centre on the F line.
    const PATH: [[f32; 3]; 9] = [
        [-0.25, 0.30, 0.20],
        [0.20, 0.82, 0.16],
        [0.80, 1.02, 0.18], // the top of the arch, on the top line
        [1.40, 0.80, 0.30],
        [1.72, 0.15, 0.42],
        [1.65, -0.60, 0.46], // the thick right side
        [1.30, -1.35, 0.40],
        [0.75, -2.00, 0.28],
        [0.00, -2.60, 0.12],
    ];
    const KNOB: [f32; 3] = [0.0, 0.0, 0.42];
    const DOTS: [[f32; 3]; 2] = [[2.15, 0.5, 0.16], [2.15, -0.5, 0.16]];

    let f = y0 + 3.0 * sp;
    let at = |x: f32, y: f32| P {
        x: cx + x * sp,
        y: f + y * sp,
    };
    let mut path: Vec<(P, f32)> = vec![(at(PATH[0][0], PATH[0][1]), PATH[0][2])];
    spline(&PATH, at, &mut path);
    stroke(&path, sp, out);
    for [x, y, r] in [KNOB, DOTS[0], DOTS[1]] {
        out.push(Prim::Poly {
            pts: ellipse(at(x, y), r * sp, r * sp, 0.0),
            color: INK,
        });
    }
}

/// Append a Catmull-Rom spline through `points` ((x, y, width) in the glyph's own
/// units, placed by `at`) to `path`, interpolating the width along it. The first
/// point is not pushed: the caller's path already ends there.
fn spline(points: &[[f32; 3]], at: impl Fn(f32, f32) -> P, path: &mut Vec<(P, f32)>) {
    const STEPS: usize = 6;
    let n = points.len();
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (
            points[i.saturating_sub(1)],
            points[i],
            points[i + 1],
            points[(i + 2).min(n - 1)],
        );
        let c1 = [p1[0] + (p2[0] - p0[0]) / 6.0, p1[1] + (p2[1] - p0[1]) / 6.0];
        let c2 = [p2[0] - (p3[0] - p1[0]) / 6.0, p2[1] - (p3[1] - p1[1]) / 6.0];
        // k starts at 1: the segment's start is the previous sample already.
        for k in 1..=STEPS {
            let t = k as f32 / STEPS as f32;
            let u = 1.0 - t;
            let (uu, tt) = (u * u, t * t);
            let x = uu * u * p1[0] + 3.0 * uu * t * c1[0] + 3.0 * u * tt * c2[0] + tt * t * p2[0];
            let y = uu * u * p1[1] + 3.0 * uu * t * c1[1] + 3.0 * u * tt * c2[1] + tt * t * p2[1];
            path.push((at(x, y), p1[2] + (p2[2] - p1[2]) * t));
        }
    }
}

/// Ink a sampled path, each segment as wide as the mean of its ends' widths.
fn stroke(path: &[(P, f32)], sp: f32, out: &mut Vec<Prim>) {
    for pair in path.windows(2) {
        out.push(Prim::Line {
            a: pair[0].0,
            b: pair[1].0,
            w: 0.5 * (pair[0].1 + pair[1].1) * sp,
            color: INK,
        });
    }
}
