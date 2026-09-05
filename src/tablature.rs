//! Tablature row: six string lines, fret numbers, every playing-technique glyph,
//! the strum/tapping row, and the rhythm stems.
//!
//! Drawn geometrically in page millimetres, like [`crate::notation`], so screen and
//! PDF agree exactly. The horizontal positions come from [`crate::engrave`] — the
//! same [`Spacing`] the staff above uses, which is what makes a fret number, its
//! strum arrow and its note head line up in one vertical column.

use crate::engrave::{beam_groups, beam_runs, Spacing};
use crate::model::{technique_color, Bar, Document, Note, NoteValue, Strum, Technique};
use crate::staff::{
    arc, arrow_head, barline, dashed, ellipse, label_width, pt_for_cap, quad, wave, Barline, FAINT,
    INK, PAPER,
};
use crate::{Align, Prim, P};

/// Distance between two string lines. Fixed: `tab_scale` grows the numbers and
/// technique glyphs printed on the staff, never the string grid itself.
pub const STRING_MM: f32 = 3.2;
/// The six-line staff itself.
pub const STAFF_MM: f32 = 5.0 * STRING_MM;
/// Band above the staff: bends, vibrato, slurs, palm-mute spans, labels.
///
/// ponytail: `tab_scale` grows the technique glyphs drawn in this band but not
/// the band itself, so past ~1.04 a bend label pokes ~1 mm above it — absorbed by
/// `BLOCK_GAP_MM` between stacked blocks. Grow `BAND_MM` with `tab_scale` if that
/// ever shows.
pub const BAND_MM: f32 = 8.0;
/// Band below the staff: stems and beams, when the block shows rhythm.
pub const RHYTHM_MM: f32 = 8.0;
/// Height of the strumming / tapping row of a three-row block.
pub const STRUM_MM: f32 = 5.5;
/// Room reserved at the left of the system for the "TAB" label.
pub const HEAD_MM: f32 = 13.0;

const LINE_W: f32 = 0.22;
const FRET_CAP_MM: f32 = 2.0;
const HIT_HALF_MM: f32 = 2.2;

/// What `Document::tab_scale == 1.0` draws, as a multiplier on every millimetre
/// figure in this file.
///
/// The standard size moved to what used to be `tab_scale = 1.3`. Every glyph in
/// here is built from hand-tuned literals fed through `sc()`, and they are checked
/// by eye, not by assertion — folding the move into one multiplier keeps that set
/// of numbers intact and self-consistent instead of rescaling forty of them by
/// hand and hoping nothing drifted. Contrast `engrave::natural_event_width`, which
/// is a six-value table and so was retuned outright.
const SCALE_BASE: f32 = 1.3;

/// The multiplier the renderer applies for this document.
fn scale(doc: &Document) -> f32 {
    doc.tab_scale * SCALE_BASE
}

/// A tablature row shows the rhythm itself when no notation staff is there to
/// carry it — otherwise the block would say which frets to play but never when.
pub fn shows_rhythm(doc: &Document) -> bool {
    matches!(doc.model, crate::model::BlockModel::OneLine)
}

/// A clickable cell: one string at one event. The editor turns a mouse position
/// into `(bar, event, string)` by looking these up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub bar: usize,
    pub event: usize,
    pub string: u8,
    pub min: P,
    pub max: P,
}

/// y of a string line. String 0 is the high E, drawn on top.
fn string_y(origin: P, string: u8) -> f32 {
    origin.y + (5 - string.min(5)) as f32 * STRING_MM
}

/// What is printed on the string line for this note.
fn fret_label(note: &Note) -> String {
    match note.tech {
        Technique::Dead => "x".to_string(),
        Technique::Ghost => format!("({})", note.fret),
        Technique::Harmonic => format!("<{}>", note.fret),
        Technique::Trill { to_fret } => format!("{}({})", note.fret, to_fret),
        _ => note.fret.to_string(),
    }
}

/// How much of a tone a bend is worth, written the way players read it.
fn bend_label(quarters: u8) -> String {
    match quarters {
        1 => "1/4".into(),
        2 => "1/2".into(),
        3 => "3/4".into(),
        4 => "full".into(),
        6 => "1 1/2".into(),
        8 => "2".into(),
        q => format!("{}/4", q),
    }
}

/// Draw the tablature row of one system.
///
/// `origin` is the first barline: x of the music start, y of the BOTTOM string
/// line (the low E). Clickable cells are appended to `hits`.
pub fn render(
    doc: &Document,
    spacing: &Spacing,
    origin: P,
    show_rhythm: bool,
    out: &mut Vec<Prim>,
    hits: &mut Vec<Hit>,
) {
    let Some(first) = spacing.bars.first() else {
        return;
    };
    let right = origin.x + spacing.width;
    let top = origin.y + STAFF_MM;

    for s in 0..6u8 {
        let y = string_y(origin, s);
        out.push(Prim::Line {
            a: P {
                x: origin.x - HEAD_MM,
                y,
            },
            b: P { x: right, y },
            w: LINE_W,
            color: INK,
        });
    }
    tab_label(origin.x - HEAD_MM + 2.0, origin.y, out);

    // Barlines. The mark between two bars carries the closing repeat of the one on
    // its left and the opening repeat of the one on its right, which is why they
    // are resolved together rather than per bar.
    let dot_ys = [origin.y + 1.5 * STRING_MM, origin.y + 3.5 * STRING_MM];
    for (i, bar) in spacing.bars.iter().enumerate() {
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
        barline(kind, origin.x + bar.x, origin.y, top, &dot_ys, out);
    }
    let last = spacing.bars.last().unwrap_or(first);
    let closing = match doc.bars.get(last.index).and_then(|b| b.repeat_end) {
        Some(t) => Barline::RepeatEnd(t),
        None if last.index + 1 >= doc.bars.len() => Barline::Final,
        None => Barline::Double,
    };
    barline(closing, right, origin.y, top, &dot_ys, out);

    for i in 0..spacing.bars.len() {
        render_bar(doc, spacing, i, origin, out, hits);
    }
    spans(doc, spacing, origin, out);
    if show_rhythm {
        rhythm(doc, spacing, origin, out);
    }
}

/// The stacked "TAB" that opens a tablature staff, centred on the staff mid-line.
fn tab_label(x: f32, y0: f32, out: &mut Vec<Prim>) {
    const CAP: f32 = 2.6;
    const GAP: f32 = 3.4; // baseline to baseline
    let pt = pt_for_cap(CAP);
    // `pos.y` is the baseline; the stack runs from B's baseline up to T's cap
    // top. Put that span's centre on the staff's own centre.
    let bottom_baseline = y0 + STAFF_MM * 0.5 - GAP - CAP * 0.5;
    for (i, letter) in ["T", "A", "B"].iter().enumerate() {
        out.push(Prim::Text {
            pos: P {
                x,
                y: bottom_baseline + (2 - i) as f32 * GAP,
            },
            s: (*letter).to_string(),
            pt,
            color: INK,
            align: Align::Left,
        });
    }
}

/// True when this event's note on `string` is a tie's continuation rather than a
/// fresh attack.
fn tied_from_prev(bar: &Bar, ei: usize, string: u8) -> bool {
    ei > 0
        && bar.events[ei - 1]
            .notes
            .iter()
            .any(|n| n.string == string && n.tie_next)
}

/// Fret numbers, clickable cells and technique glyphs for one bar.
fn render_bar(
    doc: &Document,
    spacing: &Spacing,
    bar_in_system: usize,
    origin: P,
    out: &mut Vec<Prim>,
    hits: &mut Vec<Hit>,
) {
    let layout = &spacing.bars[bar_in_system];
    let Some(bar) = doc.bars.get(layout.index) else {
        return;
    };
    // `tab_scale` grows the fret numbers and technique glyphs printed here; the
    // string grid, the clickable cells and the band around it stay put.
    let cap = FRET_CAP_MM * scale(doc);
    let band = origin.y + STAFF_MM + 1.2;

    for (ei, event) in bar.events.iter().enumerate() {
        let Some(&ex) = layout.events.get(ei) else {
            continue;
        };
        let x = origin.x + ex;

        // Every string is clickable, whether or not it currently holds a note:
        // that is how a note gets placed in the first place.
        for s in 0..6u8 {
            let y = string_y(origin, s);
            hits.push(Hit {
                bar: layout.index,
                event: ei,
                string: s,
                min: P {
                    x: x - HIT_HALF_MM,
                    y: y - STRING_MM * 0.5,
                },
                max: P {
                    x: x + HIT_HALF_MM,
                    y: y + STRING_MM * 0.5,
                },
            });
        }

        for note in &event.notes {
            let y = string_y(origin, note.string);

            // A tie's continuation is not picked again. Printing its fret number
            // would read as a second attack, so the tab shows only the arc that
            // carries the sound over -- the same thing the notation staff draws.
            if tied_from_prev(bar, ei, note.string) {
                continue;
            }
            if note.tie_next {
                if let Some(&nx) = layout.events.get(ei + 1) {
                    let half = label_width(&fret_label(note), pt_for_cap(cap)) * 0.5;
                    out.push(arc(
                        P {
                            x: x + half + 0.5,
                            y: y - cap * 0.35,
                        },
                        P {
                            x: origin.x + nx,
                            y: y - cap * 0.35,
                        },
                        -cap * 0.55,
                        0.22 * scale(doc),
                        technique_color(&note.tech),
                    ));
                }
            }

            let grace = matches!(note.tech, Technique::Grace);
            let pt = pt_for_cap(if grace { cap * 0.72 } else { cap });
            let text = fret_label(note);
            let w = label_width(&text, pt);
            let color = technique_color(&note.tech);

            // Break the string line behind the number: far more legible than
            // printing a digit on top of a rule.
            out.push(quad(
                x - w * 0.5 - 0.4,
                y - cap * 0.72,
                x + w * 0.5 + 0.4,
                y + cap * 0.72,
                PAPER,
            ));
            out.push(Prim::Text {
                pos: P {
                    x,
                    y: y - cap * 0.5,
                },
                s: text,
                pt,
                color,
                align: Align::Center,
            });

            technique(
                doc,
                bar,
                layout,
                ei,
                note,
                x,
                y,
                w,
                band,
                origin,
                scale(doc),
                out,
            );
        }
    }
}

/// The glyph that goes with a note's playing technique.
///
/// `scale` is the document's `tab_scale`: the fret label `w` handed in already
/// carries it, and `sc()` applies it to the glyph's own magnitudes so a bend
/// arrow next to a big fret number grows with it.
#[allow(clippy::too_many_arguments)]
fn technique(
    doc: &Document,
    bar: &Bar,
    layout: &crate::engrave::BarSpacing,
    ei: usize,
    note: &Note,
    x: f32,
    y: f32,
    w: f32,
    band: f32,
    origin: P,
    scale: f32,
    out: &mut Vec<Prim>,
) {
    let color = technique_color(&note.tech);
    let sc = |mm: f32| mm * scale;
    let right = x + w * 0.5 + sc(0.5);
    // Where the partner note sits, for the techniques that join two notes.
    let partner = |out_fret: &mut Option<u8>| -> Option<f32> {
        let j = bar.next_on_string(ei, note.string)?;
        *out_fret = bar.events[j]
            .notes
            .iter()
            .find(|n| n.string == note.string)
            .map(|n| n.fret);
        layout.events.get(j).map(|&ex| origin.x + ex)
    };

    match note.tech {
        Technique::Plain | Technique::Dead | Technique::Ghost | Technique::Harmonic => {}

        // A slur above the pair, with the letter players look for.
        Technique::HammerOn | Technique::PullOff => {
            let mut fret = None;
            if let Some(nx) = partner(&mut fret) {
                let lift = y + sc(FRET_CAP_MM * 0.8);
                out.push(arc(
                    P { x: right, y: lift },
                    P {
                        x: nx - w * 0.5 - sc(0.5),
                        y: lift,
                    },
                    sc(1.7),
                    sc(0.22),
                    color,
                ));
                out.push(Prim::Text {
                    pos: P {
                        x: (right + nx) * 0.5,
                        y: lift + sc(1.9),
                    },
                    s: if matches!(note.tech, Technique::HammerOn) {
                        "H"
                    } else {
                        "P"
                    }
                    .into(),
                    pt: pt_for_cap(sc(1.5)),
                    color,
                    align: Align::Center,
                });
            }
        }

        // A straight line rising or falling between the two frets; the legato
        // slide adds the slur, the picked one does not.
        Technique::Slide | Technique::SlideShift => {
            let mut fret = None;
            if let Some(nx) = partner(&mut fret) {
                let rise = match fret {
                    Some(f) if f > note.fret => sc(0.7),
                    Some(f) if f < note.fret => sc(-0.7),
                    _ => 0.0,
                };
                out.push(Prim::Line {
                    a: P {
                        x: right,
                        y: y - rise,
                    },
                    b: P {
                        x: nx - w * 0.5 - sc(0.5),
                        y: y + rise,
                    },
                    w: sc(0.28),
                    color,
                });
                if matches!(note.tech, Technique::Slide) {
                    let lift = y + sc(FRET_CAP_MM * 0.8);
                    out.push(arc(
                        P { x: right, y: lift },
                        P {
                            x: nx - w * 0.5 - sc(0.5),
                            y: lift,
                        },
                        sc(1.5),
                        sc(0.2),
                        color,
                    ));
                }
            }
        }

        // A grace note on the same beat: the departure fret, small, under the bar that
        // marks it as an ornament. `engrave::event_lead_in` reserves the room it hangs
        // into — this is the one glyph drawn to the LEFT of its own column.
        Technique::SlideIn { from_fret } => {
            let cap = sc(FRET_CAP_MM);
            let gcap = cap * 0.72; // same "small note" ratio as a Grace label
            let gpt = pt_for_cap(gcap);
            let gtext = from_fret.to_string();
            let gw = label_width(&gtext, gpt);
            let gx = x - w * 0.5 - sc(0.9) - gw * 0.5;
            // The two digits sit on one baseline, the fret labels' own: a smaller
            // glyph centred on the string line instead would ride visibly high
            // next to the note it belongs to. The ornament grows upwards from
            // there, which is where its bar wants to be anyway.
            let base_y = y - cap * 0.5;
            let bar_y = base_y + gcap + sc(0.55);

            // One knockout for the digit and its bar together: the string line is broken
            // once, behind the whole ornament.
            out.push(quad(
                gx - gw * 0.5 - 0.4,
                y - cap * 0.72,
                gx + gw * 0.5 + 0.4,
                bar_y + sc(0.3),
                PAPER,
            ));
            out.push(Prim::Text {
                pos: P { x: gx, y: base_y },
                s: gtext,
                pt: gpt,
                color,
                align: Align::Center,
            });
            out.push(Prim::Line {
                a: P {
                    x: gx - gw * 0.5 - sc(0.15),
                    y: bar_y,
                },
                b: P {
                    x: gx + gw * 0.5 + sc(0.15),
                    y: bar_y,
                },
                w: sc(0.3),
                color,
            });
        }

        // A grace note leans on the note that follows it.
        Technique::Grace => {
            let mut fret = None;
            if let Some(nx) = partner(&mut fret) {
                let lift = y + sc(FRET_CAP_MM * 0.7);
                out.push(arc(
                    P { x: right, y: lift },
                    P {
                        x: nx - w * 0.5 - sc(0.5),
                        y: lift,
                    },
                    sc(1.4),
                    sc(0.2),
                    color,
                ));
            }
        }

        Technique::Bend { quarters } => {
            bend_arrow(right, y, band, quarters, false, color, scale, out)
        }
        Technique::BendRelease { quarters } => {
            bend_arrow(right, y, band, quarters, true, color, scale, out)
        }

        // A pre-bend is already bent when struck: the arrow is vertical, not curved.
        Technique::PreBend { quarters } => {
            let top = band + sc(3.4);
            out.push(Prim::Line {
                a: P {
                    x: right + sc(0.4),
                    y: y + sc(1.0),
                },
                b: P {
                    x: right + sc(0.4),
                    y: top,
                },
                w: sc(0.24),
                color,
            });
            arrow_head(
                P {
                    x: right + sc(0.4),
                    y: top,
                },
                0.0,
                1.0,
                sc(0.9),
                color,
                out,
            );
            out.push(Prim::Text {
                pos: P {
                    x: right + sc(0.4),
                    y: top + sc(0.9),
                },
                s: bend_label(quarters),
                pt: pt_for_cap(sc(1.5)),
                color,
                align: Align::Center,
            });
        }

        Technique::Vibrato => wave(
            x - sc(0.8),
            x + sc(3.2),
            band + sc(1.0),
            sc(0.45),
            color,
            out,
        ),
        Technique::WideVibrato => wave(
            x - sc(0.8),
            x + sc(4.0),
            band + sc(1.2),
            sc(0.85),
            color,
            out,
        ),

        Technique::Trill { .. } => {
            out.push(Prim::Text {
                pos: P {
                    x: x - sc(0.8),
                    y: band + sc(0.6),
                },
                s: "tr".into(),
                pt: pt_for_cap(sc(1.5)),
                color,
                align: Align::Left,
            });
            wave(
                x + sc(1.6),
                x + sc(4.6),
                band + sc(1.2),
                sc(0.45),
                color,
                out,
            );
        }

        Technique::Tap => label_above(x, band, "T", color, scale, out),
        Technique::PinchHarmonic => label_above(x, band, "P.H.", color, scale, out),
        Technique::Slap => label_above(x, band, "S", color, scale, out),
        Technique::Pop => label_above(x, band, "P", color, scale, out),
    }
    let _ = doc;
}

fn label_above(x: f32, band: f32, text: &str, color: crate::Rgb, scale: f32, out: &mut Vec<Prim>) {
    out.push(Prim::Text {
        pos: P {
            x,
            y: band + 0.8 * scale,
        },
        s: text.to_string(),
        pt: pt_for_cap(1.5 * scale),
        color,
        align: Align::Center,
    });
}

/// A bend: a curved arrow up to the target, and back down again on a release.
/// `scale` is the document's `tab_scale`; `sc()` grows the arrow with the frets.
#[allow(clippy::too_many_arguments)]
fn bend_arrow(
    right: f32,
    y: f32,
    band: f32,
    quarters: u8,
    release: bool,
    color: crate::Rgb,
    scale: f32,
    out: &mut Vec<Prim>,
) {
    let sc = |mm: f32| mm * scale;
    let top = band + sc(3.2);
    let up_x = right + sc(1.8);
    out.push(Prim::Curve {
        a: P {
            x: right,
            y: y + sc(0.6),
        },
        c1: P {
            x: right + sc(1.5),
            y: y + sc(0.8),
        },
        c2: P {
            x: up_x,
            y: top - sc(2.2),
        },
        b: P { x: up_x, y: top },
        w: sc(0.24),
        color,
    });
    arrow_head(P { x: up_x, y: top }, 0.0, 1.0, sc(0.9), color, out);
    out.push(Prim::Text {
        pos: P {
            x: up_x,
            y: top + sc(0.9),
        },
        s: bend_label(quarters),
        pt: pt_for_cap(sc(1.5)),
        color,
        align: Align::Center,
    });
    if release {
        let down_x = up_x + sc(2.6);
        out.push(Prim::Curve {
            a: P { x: up_x, y: top },
            c1: P {
                x: up_x + sc(1.3),
                y: top,
            },
            c2: P {
                x: down_x,
                y: y + sc(2.0),
            },
            b: P {
                x: down_x,
                y: y + sc(0.8),
            },
            w: sc(0.24),
            color,
        });
        arrow_head(
            P {
                x: down_x,
                y: y + sc(0.8),
            },
            0.0,
            -1.0,
            sc(0.9),
            color,
            out,
        );
    }
}

/// Palm-mute and let-ring spans, which run across consecutive events rather than
/// belonging to one note.
fn spans(doc: &Document, spacing: &Spacing, origin: P, out: &mut Vec<Prim>) {
    let sc = |mm: f32| mm * scale(doc);
    let band = origin.y + STAFF_MM + 1.2;
    let mut cells: Vec<(f32, bool, bool)> = Vec::new();
    for b in &spacing.bars {
        let Some(bar) = doc.bars.get(b.index) else {
            continue;
        };
        for (ei, event) in bar.events.iter().enumerate() {
            if let Some(&ex) = b.events.get(ei) {
                cells.push((origin.x + ex, event.palm_mute, event.let_ring));
            }
        }
    }

    for (pick, label, y) in [
        (0usize, "P.M.", band + sc(5.0)),
        (1usize, "let ring", band + sc(6.7)),
    ] {
        let pt = pt_for_cap(sc(1.4));
        let mut run: Option<(f32, f32)> = None;
        for &(x, palm, ring) in cells.iter().chain(std::iter::once(&(0.0, false, false))) {
            let on = if pick == 0 { palm } else { ring };
            match (on, run) {
                (true, None) => run = Some((x, x)),
                (true, Some((s, _))) => run = Some((s, x)),
                (false, Some((s, e))) => {
                    out.push(Prim::Text {
                        pos: P { x: s - sc(1.0), y },
                        s: label.to_string(),
                        pt,
                        color: FAINT,
                        align: Align::Left,
                    });
                    dashed(
                        s - sc(1.0) + label_width(label, pt) + sc(0.8),
                        e + sc(1.5),
                        y + sc(0.5),
                        FAINT,
                        out,
                    );
                    run = None;
                }
                (false, None) => {}
            }
        }
    }
}

/// Stems and beams under the staff. A tablature row that has no notation staff
/// beside it must carry the rhythm itself, or the block says which frets to play
/// but never when.
fn rhythm(doc: &Document, spacing: &Spacing, origin: P, out: &mut Vec<Prim>) {
    let top = origin.y - 1.4;
    let base = origin.y - RHYTHM_MM + 2.4;
    const BEAM_H: f32 = 0.65;
    const BEAM_GAP: f32 = 0.35;

    for b in &spacing.bars {
        let Some(bar) = doc.bars.get(b.index) else {
            continue;
        };
        let groups = beam_groups(doc, b.index);
        let grouped: Vec<usize> = groups.iter().flat_map(|g| g.events.clone()).collect();

        for (ei, event) in bar.events.iter().enumerate() {
            let Some(&ex) = b.events.get(ei) else {
                continue;
            };
            let x = origin.x + ex;
            if event.is_rest() {
                crate::staff::rest(
                    x,
                    (top + base) * 0.5,
                    1.5,
                    event.dur.base,
                    event.dur.dots,
                    out,
                );
                continue;
            }
            match event.dur.base {
                // A whole note has no stem; a short rule stands in for it.
                NoteValue::Whole => out.push(quad(x - 1.1, base - 0.28, x + 1.1, base + 0.28, INK)),
                value => {
                    out.push(Prim::Line {
                        a: P { x, y: top },
                        b: P { x, y: base },
                        w: 0.26,
                        color: INK,
                    });
                    if matches!(value, NoteValue::Half) {
                        out.push(Prim::Poly {
                            pts: ellipse(P { x, y: base }, 0.55, 0.42, 0.0),
                            color: INK,
                        });
                        out.push(Prim::Poly {
                            pts: ellipse(P { x, y: base }, 0.26, 0.2, 0.0),
                            color: PAPER,
                        });
                    }
                    if !grouped.contains(&ei) {
                        for f in 0..value.flags() {
                            let y = base + f as f32 * 0.8;
                            out.push(Prim::Curve {
                                a: P { x, y },
                                c1: P {
                                    x: x + 1.0,
                                    y: y + 0.1,
                                },
                                c2: P {
                                    x: x + 1.2,
                                    y: y + 0.9,
                                },
                                b: P {
                                    x: x + 0.5,
                                    y: y + 1.7,
                                },
                                w: 0.24,
                                color: INK,
                            });
                        }
                    }
                }
            }
            let mut dx = 1.3;
            for _ in 0..event.dur.dots {
                out.push(Prim::Poly {
                    pts: ellipse(
                        P {
                            x: x + dx,
                            y: base + 1.2,
                        },
                        0.2,
                        0.2,
                        0.0,
                    ),
                    color: INK,
                });
                dx += 0.65;
            }
        }

        // Every stem ends on the same line here, so the beams are horizontal and
        // stack upwards towards the staff.
        for group in &groups {
            let xs: Vec<f32> = group
                .events
                .iter()
                .filter_map(|&ei| b.events.get(ei).map(|&ex| origin.x + ex))
                .collect();
            if xs.len() != group.events.len() {
                continue;
            }
            for level in 1..=3u8 {
                for run in beam_runs(group, level) {
                    let (s, e) = (*run.start(), *run.end());
                    let y = base + (level - 1) as f32 * (BEAM_H + BEAM_GAP);
                    let (x0, x1) = if s == e {
                        (xs[s], if s == 0 { xs[s] + 1.6 } else { xs[s] - 1.6 })
                    } else {
                        (xs[s], xs[e])
                    };
                    out.push(quad(x0.min(x1), y, x0.max(x1), y + BEAM_H, INK));
                }
            }
        }
    }
}

/// The strumming and tapping row of a three-row block.
///
/// `origin` is the first barline: x of the music start, y of the row's baseline.
pub fn render_strum(doc: &Document, spacing: &Spacing, origin: P, out: &mut Vec<Prim>) {
    for b in &spacing.bars {
        let Some(bar) = doc.bars.get(b.index) else {
            continue;
        };
        for (ei, event) in bar.events.iter().enumerate() {
            let (Some(&ex), Some(strum)) = (b.events.get(ei), event.strum) else {
                continue;
            };
            let x = origin.x + ex;
            let (lo, hi) = (origin.y + 0.6, origin.y + 3.4);
            let mut line = |ax: f32, ay: f32, bx: f32, by: f32| {
                out.push(Prim::Line {
                    a: P { x: ax, y: ay },
                    b: P { x: bx, y: by },
                    w: 0.26,
                    color: INK,
                });
            };
            match strum {
                // Down-stroke: the open-bottomed bracket players read as "down".
                Strum::Down => {
                    line(x - 0.9, hi, x + 0.9, hi);
                    line(x - 0.9, hi, x - 0.9, lo);
                    line(x + 0.9, hi, x + 0.9, lo);
                }
                Strum::Up => {
                    line(x - 0.95, hi, x, lo);
                    line(x + 0.95, hi, x, lo);
                }
                Strum::TapRight | Strum::TapLeft => {
                    out.push(Prim::Text {
                        pos: P { x: x - 0.3, y: lo },
                        s: "T".into(),
                        pt: pt_for_cap(2.0),
                        color: INK,
                        align: Align::Center,
                    });
                    let dir = if matches!(strum, Strum::TapRight) {
                        1.0
                    } else {
                        -1.0
                    };
                    arrow_head(
                        P {
                            x: x + dir * 1.9,
                            y: lo + 1.0,
                        },
                        dir,
                        0.0,
                        0.8,
                        INK,
                        out,
                    );
                }
            }
        }
    }
}
