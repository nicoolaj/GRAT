//! Standard-notation engraving: the five-line staff of a two- or three-row block.
//!
//! Takes the rhythmic decisions from [`crate::engrave`] and the pitches from the
//! document, and emits [`Prim`]s in page millimetres. Everything is drawn
//! geometrically — heads, rests, accidentals, flags, clef — so the screen and the
//! PDF are pixel-identical by construction and no music font has to be shipped.
//!
//! ponytail: geometric glyphs; embedding a SMuFL font (Bravura) would give finer
//! shapes at the cost of a ~500 KB asset and font subsetting on the PDF side.

use crate::engrave::{beam_groups, beam_runs, BeamGroup, Spacing};
use crate::model::{technique_color, Document, Event, NoteValue, Technique};
use crate::staff::{barline, ellipse, pt_for_cap, rest, Barline, INK, PAPER};
use crate::{Align, Prim, Rgb, P};

/// Distance between two staff lines, in millimetres.
pub const SPACE_MM: f32 = 2.5;
/// Vertical room the notation row needs: the staff plus the usual ledger territory.
pub const ROW_MM: f32 = 20.0;
/// Where the bottom staff line sits inside that row.
pub const BASELINE_OFFSET_MM: f32 = 7.0;

// Geometry, in multiples of the staff space.
const LINE_W: f32 = 0.09;
const HEAD_RX: f32 = 0.66;
const HEAD_RY: f32 = 0.48;
const HEAD_TILT_DEG: f32 = -21.0;
const HEAD_HOLLOW: f32 = 0.62; // inner ellipse ratio for half and whole heads
const STEM_W: f32 = 0.13;
const STEM_LEN: f32 = 3.5;
const STEM_MIN: f32 = 2.0;
const BEAM_H: f32 = 0.5;
const BEAM_GAP: f32 = 0.32;
const BEAMLET: f32 = 1.1; // length of a partial beam, in spaces
const LEDGER_EXT: f32 = 0.42;
const MAX_SLOPE: f32 = 1.5; // total beam rise, in spaces

/// A note's vertical position on the staff, in half-spaces above the bottom line,
/// plus whether its spelling needs a sharp.
///
/// Guitar notation is treble clef *8va bassa*: what is written sounds an octave
/// lower, so the written pitch is the sounding pitch plus twelve.
fn staff_position(sounding_midi: u8) -> (i32, bool) {
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
    let written = sounding_midi as i32 + 12;
    let (step, sharp) = STEPS[(written.rem_euclid(12)) as usize];
    let diatonic = 7 * written.div_euclid(12) + step;
    // The bottom line of a treble staff is E4 (MIDI 64), diatonic 37.
    (diatonic - 37, sharp)
}

/// Horizontal room reserved at the left of every system for the clef and the
/// time signature. The layout starts the music at `origin.x`; the staff head is
/// drawn in `[origin.x - HEAD_MM, origin.x]`.
pub const HEAD_MM: f32 = 13.0;

/// Draw the notation staff for one system.
///
/// `origin` is the first barline: x of the music start, y of the BOTTOM staff line.
pub fn render(doc: &Document, spacing: &Spacing, origin: P, out: &mut Vec<Prim>) {
    let Some(first) = spacing.bars.first() else {
        return;
    };
    let sp = SPACE_MM;
    let right = origin.x + spacing.width;

    for line in 0..5 {
        let y = origin.y + line as f32 * sp;
        out.push(Prim::Line {
            a: P {
                x: origin.x - HEAD_MM,
                y,
            },
            b: P { x: right, y },
            w: LINE_W * sp,
            color: INK,
        });
    }
    g_clef(origin.x - HEAD_MM + 1.6, origin.y, sp, out);

    // Barlines, and the time signature. A repeat mark between two bars carries the
    // closing repeat of the bar on its left and the opening one of the bar on its
    // right, so they are resolved together — and at the same x as the tablature row
    // above or below.
    let top = origin.y + 4.0 * sp;
    let dot_ys = [origin.y + 1.5 * sp, origin.y + 2.5 * sp];
    for (i, bar) in spacing.bars.iter().enumerate() {
        let x = origin.x + bar.x;
        let closes = if i == 0 {
            None
        } else {
            doc.bars
                .get(spacing.bars[i - 1].index)
                .and_then(|b| b.repeat_end)
        };
        let opens = doc.bars.get(bar.index).is_some_and(|b| b.repeat_start);
        let kind = match (closes, opens) {
            (Some(t), true) => Barline::RepeatBoth(t),
            (Some(t), false) => Barline::RepeatEnd(t),
            (None, true) => Barline::RepeatStart,
            (None, false) => Barline::Single,
        };
        barline(kind, x, origin.y, top, &dot_ys, out);

        // Unlike the clef, a time signature is not restated on every system: it
        // appears once at the start of the piece and again only where the metre
        // actually changes.
        let sig = doc.time_sig_at(bar.index);
        let changed = bar.index == 0 || doc.time_sig_at(bar.index - 1) != sig;
        if changed {
            let at = if bar.index == first.index {
                origin.x - 3.4
            } else {
                x + 1.6
            };
            time_signature(at, origin.y, sp, sig, out);
        }
    }
    let last = spacing.bars.last().unwrap_or(first);
    let closing = match doc.bars.get(last.index).and_then(|b| b.repeat_end) {
        Some(t) => Barline::RepeatEnd(t),
        None if last.index + 1 >= doc.bars.len() => Barline::Final,
        None => Barline::Double,
    };
    barline(closing, right, origin.y, top, &dot_ys, out);

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
        let up = group_stem_up(doc, bar, group);
        for &ei in &group.events {
            direction[ei] = Some(up);
        }
    }
    let dir_of =
        |ei: usize, event: &Event| direction[ei].unwrap_or_else(|| stem_up_for(doc, event));

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
            if let Some(tip) = stem(&heads, sp, up, STEM_LEN, out) {
                let flags = event.dur.base.flags();
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

/// The head furthest from the middle line decides which way the stem points; on a
/// tie the stem goes down. Averaging the heads instead would get wide chords wrong.
fn stem_up_for(doc: &Document, event: &Event) -> bool {
    let furthest = event
        .notes
        .iter()
        .map(|note| staff_position(doc.pitch(note)).0 - 4)
        .max_by_key(|d| d.abs())
        .unwrap_or(0);
    furthest < 0
}

fn head_positions(doc: &Document, event: &Event, x: f32, y0: f32, sp: f32, up: bool) -> Vec<Head> {
    let mut heads: Vec<Head> = event
        .notes
        .iter()
        .map(|note| {
            let (half, sharp) = staff_position(doc.pitch(note));
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

/// Draw a stem of the default length; returns its tip.
fn stem(heads: &[Head], sp: f32, up: bool, len: f32, out: &mut Vec<Prim>) -> Option<P> {
    if heads.is_empty() {
        return None;
    }
    let x = stem_x(heads, sp, up);
    let (foot, far) = head_extremes(heads, up);
    let tip = far + if up { len * sp } else { -len * sp };
    out.push(Prim::Line {
        a: P { x, y: foot },
        b: P { x, y: tip },
        w: STEM_W * sp,
        color: INK,
    });
    Some(P { x, y: tip })
}

fn flag(tip: P, sp: f32, up: bool, n: u8, out: &mut Vec<Prim>) {
    let dir = if up { -1.0 } else { 1.0 };
    for i in 0..n {
        let y = tip.y + dir * i as f32 * 0.74 * sp;
        out.push(Prim::Curve {
            a: P { x: tip.x, y },
            c1: P {
                x: tip.x + 0.95 * sp,
                y: y + dir * 0.15 * sp,
            },
            c2: P {
                x: tip.x + 1.15 * sp,
                y: y + dir * 0.95 * sp,
            },
            b: P {
                x: tip.x + 0.45 * sp,
                y: y + dir * 1.65 * sp,
            },
            w: 0.26 * sp,
            color: INK,
        });
    }
}

/// One direction for a whole beamed run, decided by the head furthest from the
/// middle line across the entire run.
fn group_stem_up(doc: &Document, bar: &crate::model::Bar, group: &BeamGroup) -> bool {
    let furthest = group
        .events
        .iter()
        .flat_map(|&ei| bar.events[ei].notes.iter())
        .map(|note| staff_position(doc.pitch(note)).0 - 4)
        .max_by_key(|d| d.abs())
        .unwrap_or(0);
    furthest < 0
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
    let up = group_stem_up(doc, bar, group);

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
    let step = (BEAM_H + BEAM_GAP) * sp;
    for level in 1..=3u8 {
        for run in beam_runs(group, level) {
            let (s, e) = (*run.start(), *run.end());
            let inset = sign * (level - 1) as f32 * step;
            let (x0, x1) = if s == e {
                let x = cols[s].0;
                (
                    x,
                    if s == 0 {
                        x + BEAMLET * sp
                    } else {
                        x - BEAMLET * sp
                    },
                )
            } else {
                (cols[s].0, cols[e].0)
            };
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
        let up = direction[ei].unwrap_or_else(|| stem_up_for(doc, event));
        let next_up = direction
            .get(ei + 1)
            .copied()
            .flatten()
            .unwrap_or_else(|| stem_up_for(doc, next));
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
            let a = P {
                x: head.x + HEAD_RX * sp * 1.15,
                y: head.y + sign * 0.30 * sp,
            };
            let b = P {
                x: target.x - HEAD_RX * sp * 1.15,
                y: target.y + sign * 0.30 * sp,
            };
            let span = (b.x - a.x).max(0.1);
            // Shallow, and shallower still on a short tie: a fixed depth turns the
            // gap between two adjacent heads into a croquet hoop.
            let depth = sign * (0.62 * sp).min(span * 0.36);
            out.push(Prim::Curve {
                a,
                c1: P {
                    x: a.x + span * 0.22,
                    y: a.y + depth,
                },
                c2: P {
                    x: b.x - span * 0.22,
                    y: b.y + depth,
                },
                b,
                w: 0.11 * sp,
                color: INK,
            });
        }
    }
}

fn time_signature(x: f32, y0: f32, sp: f32, sig: (u8, u8), out: &mut Vec<Prim>) {
    let pt = pt_for_cap(1.9 * sp);
    for (value, baseline) in [(sig.0, y0 + 2.0 * sp), (sig.1, y0 + 0.05 * sp)] {
        out.push(Prim::Text {
            pos: P { x, y: baseline },
            s: value.to_string(),
            pt,
            color: INK,
            align: Align::Center,
        });
    }
}

/// A treble clef, traced as a tapered spiral plus the upper hook and the tail.
///
/// ponytail: a procedural G clef rather than a real glyph outline. It reads
/// correctly at print size; embed a music font if it ever has to be exact.
fn g_clef(x: f32, y0: f32, sp: f32, out: &mut Vec<Prim>) {
    let g = y0 + 1.0 * sp; // the clef curls around the G line, the second one up
    let (turns, start, steps) = (2.05_f32, 0.35_f32, 48);
    let mut prev: Option<P> = None;
    let mut end = P { x, y: g };
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let th = std::f32::consts::TAU * turns * t + start;
        let r = (0.13 + 0.87 * t * t) * sp;
        let pt = P {
            x: x + r * th.cos(),
            y: g + r * th.sin(),
        };
        if let Some(a) = prev {
            out.push(Prim::Line {
                a,
                b: pt,
                w: (0.10 + 0.22 * t) * sp,
                color: INK,
            });
        }
        prev = Some(pt);
        end = pt;
    }
    let peak = P {
        x: x + 0.05 * sp,
        y: g + 3.8 * sp,
    };
    out.push(Prim::Curve {
        a: end,
        c1: P {
            x: end.x - 0.7 * sp,
            y: g + 1.9 * sp,
        },
        c2: P {
            x: peak.x - 0.75 * sp,
            y: peak.y - 0.3 * sp,
        },
        b: peak,
        w: 0.3 * sp,
        color: INK,
    });
    out.push(Prim::Curve {
        a: peak,
        c1: P {
            x: peak.x + 0.7 * sp,
            y: peak.y - 0.5 * sp,
        },
        c2: P {
            x: x + 0.42 * sp,
            y: g + 1.0 * sp,
        },
        b: P {
            x: x + 0.16 * sp,
            y: g - 2.1 * sp,
        },
        w: 0.3 * sp,
        color: INK,
    });
    out.push(Prim::Curve {
        a: P {
            x: x + 0.16 * sp,
            y: g - 2.1 * sp,
        },
        c1: P {
            x: x + 0.10 * sp,
            y: g - 2.9 * sp,
        },
        c2: P {
            x: x - 0.60 * sp,
            y: g - 2.6 * sp,
        },
        b: P {
            x: x - 0.62 * sp,
            y: g - 1.9 * sp,
        },
        w: 0.26 * sp,
        color: INK,
    });
}
