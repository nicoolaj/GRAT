//! Drawing furniture shared by the tablature row and the notation staff.
//!
//! Both rows of a block are drawn from the same [`crate::engrave::Spacing`], so a
//! barline — a repeat sign above all — lands at the same x on every row and reads
//! as one continuous mark down the block. Keeping that geometry here is what makes
//! the two renderers agree instead of drifting apart.

use crate::model::NoteValue;
use crate::{Align, Prim, Rgb, P};

pub const INK: Rgb = Rgb(0x1D, 0x1D, 0x1F);
pub const PAPER: Rgb = Rgb(0xFF, 0xFF, 0xFF);
pub const FAINT: Rgb = Rgb(0x8E, 0x8E, 0x93);

/// Cap height of Helvetica as a fraction of the em, and millimetres per point.
/// Text is sized by the height it actually draws rather than by a point size.
const HELVETICA_CAP: f32 = 0.717;
pub const MM_PER_PT: f32 = 0.352_777_8;

pub fn pt_for_cap(cap_mm: f32) -> f32 {
    cap_mm / HELVETICA_CAP / MM_PER_PT
}

/// Width of a label in millimetres. Helvetica digits all advance 0.556 em, which
/// is exact for fret numbers and close enough for the short labels around them.
pub fn label_width(text: &str, pt: f32) -> f32 {
    text.chars().count() as f32 * 0.556 * pt * MM_PER_PT
}

/// An axis-aligned filled bar.
pub fn quad(x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb) -> Prim {
    Prim::Poly {
        pts: vec![
            P { x: x0, y: y0 },
            P { x: x1, y: y0 },
            P { x: x1, y: y1 },
            P { x: x0, y: y1 },
        ],
        color,
    }
}

/// An ellipse sampled into a polygon: the one curved shape both back-ends fill
/// identically, used for note heads, dots and blobs.
pub fn ellipse(c: P, rx: f32, ry: f32, tilt_deg: f32) -> Vec<P> {
    let (s, co) = tilt_deg.to_radians().sin_cos();
    (0..20)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 20.0;
            let (x, y) = (rx * a.cos(), ry * a.sin());
            P {
                x: c.x + x * co - y * s,
                y: c.y + x * s + y * co,
            }
        })
        .collect()
}

/// A dashed horizontal rule, for palm-mute and let-ring spans.
pub fn dashed(x0: f32, x1: f32, y: f32, color: Rgb, out: &mut Vec<Prim>) {
    let mut x = x0;
    while x < x1 {
        out.push(Prim::Line {
            a: P { x, y },
            b: P {
                x: (x + 1.0).min(x1),
                y,
            },
            w: 0.18,
            color,
        });
        x += 1.8;
    }
}

/// A filled arrowhead at `tip`, pointing along `(dx, dy)`.
pub fn arrow_head(tip: P, dx: f32, dy: f32, size: f32, color: Rgb, out: &mut Vec<Prim>) {
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    out.push(Prim::Poly {
        pts: vec![
            tip,
            P {
                x: tip.x - ux * size + px * size * 0.45,
                y: tip.y - uy * size + py * size * 0.45,
            },
            P {
                x: tip.x - ux * size - px * size * 0.45,
                y: tip.y - uy * size - py * size * 0.45,
            },
        ],
        color,
    });
}

/// A wavy line: vibrato, and the tail of a trill.
pub fn wave(x0: f32, x1: f32, y: f32, amp: f32, color: Rgb, out: &mut Vec<Prim>) {
    let half = 0.7;
    let (mut x, mut up) = (x0, true);
    let end = x1.max(x0 + half * 2.0);
    while x + half <= end {
        let next = x + half;
        let peak = if up { amp } else { -amp };
        out.push(Prim::Curve {
            a: P { x, y },
            c1: P {
                x: x + half * 0.3,
                y: y + peak,
            },
            c2: P {
                x: next - half * 0.3,
                y: y + peak,
            },
            b: P { x: next, y },
            w: 0.22,
            color,
        });
        x = next;
        up = !up;
    }
}

/// A slur or tie arc between two points, bulging by `bulge` (signed).
pub fn arc(a: P, b: P, bulge: f32, w: f32, color: Rgb) -> Prim {
    let span = (b.x - a.x).max(0.1);
    Prim::Curve {
        a,
        c1: P {
            x: a.x + span * 0.25,
            y: a.y + bulge,
        },
        c2: P {
            x: b.x - span * 0.25,
            y: b.y + bulge,
        },
        b,
        w,
        color,
    }
}

/// What kind of barline to draw. Repeats are the reason this is an enum: a bar can
/// both close one repeated passage and open the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Barline {
    Single,
    /// End of a system that is not the end of the piece.
    Double,
    /// End of the piece.
    Final,
    RepeatStart,
    /// Closing repeat, with the total number of plays when it is more than two.
    RepeatEnd(u8),
    /// A closing repeat immediately followed by an opening one.
    RepeatBoth(u8),
}

const THIN_W: f32 = 0.22;
const THICK_W: f32 = 0.75;
const GAP: f32 = 1.1;
const DOT_R: f32 = 0.36;

/// Draw a barline from `bottom` to `top`, with repeat dots at `dot_ys`.
///
/// `x` is the barline's own position; repeat wings are drawn inside the bar they
/// belong to, so an opening repeat grows to the right and a closing one to the left.
pub fn barline(kind: Barline, x: f32, bottom: f32, top: f32, dot_ys: &[f32], out: &mut Vec<Prim>) {
    match kind {
        Barline::Single => vline(x, bottom, top, THIN_W, out),
        Barline::Double => {
            vline(x - GAP * 0.6, bottom, top, THIN_W, out);
            vline(x, bottom, top, THIN_W, out);
        }
        Barline::Final => {
            vline(x - GAP * 0.7, bottom, top, THIN_W, out);
            vline(x - THICK_W * 0.5, bottom, top, THICK_W, out);
        }
        Barline::RepeatStart => {
            vline(x + THICK_W * 0.5, bottom, top, THICK_W, out);
            vline(x + THICK_W + GAP * 0.5, bottom, top, THIN_W, out);
            dots(x + THICK_W + GAP, dot_ys, out);
        }
        Barline::RepeatEnd(times) => {
            vline(x - THICK_W * 0.5, bottom, top, THICK_W, out);
            vline(x - THICK_W - GAP * 0.5, bottom, top, THIN_W, out);
            dots(x - THICK_W - GAP - DOT_R, dot_ys, out);
            repeat_count(times, x, top, out);
        }
        Barline::RepeatBoth(times) => {
            vline(x - THICK_W * 0.5, bottom, top, THICK_W, out);
            vline(x - THICK_W - GAP * 0.5, bottom, top, THIN_W, out);
            dots(x - THICK_W - GAP - DOT_R, dot_ys, out);
            vline(x + THICK_W * 0.5, bottom, top, THICK_W, out);
            vline(x + THICK_W + GAP * 0.5, bottom, top, THIN_W, out);
            dots(x + THICK_W + GAP, dot_ys, out);
            repeat_count(times, x, top, out);
        }
    }
}

fn vline(x: f32, bottom: f32, top: f32, w: f32, out: &mut Vec<Prim>) {
    out.push(Prim::Line {
        a: P { x, y: bottom },
        b: P { x, y: top },
        w,
        color: INK,
    });
}

fn dots(x: f32, dot_ys: &[f32], out: &mut Vec<Prim>) {
    for &y in dot_ys {
        out.push(Prim::Poly {
            pts: ellipse(P { x, y }, DOT_R, DOT_R, 0.0),
            color: INK,
        });
    }
}

/// A repeat played more than twice says so, the way printed music does.
fn repeat_count(times: u8, x: f32, top: f32, out: &mut Vec<Prim>) {
    if times <= 2 {
        return;
    }
    out.push(Prim::Text {
        pos: P { x, y: top + 1.2 },
        s: format!("x{times}"),
        pt: pt_for_cap(1.8),
        color: INK,
        align: Align::Center,
    });
}

/// Rests, drawn geometrically so screen and PDF agree exactly.
///
/// `mid` is the vertical centre the rest hangs around — the middle staff line on a
/// notation staff, the middle of the band on a tablature rhythm row.
pub fn rest(x: f32, mid: f32, sp: f32, value: NoteValue, dot_count: u8, out: &mut Vec<Prim>) {
    match value {
        // A whole rest hangs under the fourth line; a half rest sits on the third.
        NoteValue::Whole => out.push(quad(
            x - 0.6 * sp,
            mid + 0.5 * sp,
            x + 0.6 * sp,
            mid + 1.0 * sp,
            INK,
        )),
        NoteValue::Half => out.push(quad(x - 0.6 * sp, mid, x + 0.6 * sp, mid + 0.5 * sp, INK)),
        NoteValue::Quarter => {
            for (ax, ay, bx, by) in [
                (-0.25, 1.15, 0.30, 0.45),
                (0.30, 0.45, -0.22, -0.10),
                (-0.22, -0.10, 0.30, -0.75),
            ] {
                out.push(Prim::Line {
                    a: P {
                        x: x + ax * sp,
                        y: mid + ay * sp,
                    },
                    b: P {
                        x: x + bx * sp,
                        y: mid + by * sp,
                    },
                    w: 0.3 * sp,
                    color: INK,
                });
            }
            out.push(Prim::Curve {
                a: P {
                    x: x + 0.30 * sp,
                    y: mid - 0.75 * sp,
                },
                c1: P {
                    x: x - 0.05 * sp,
                    y: mid - 0.80 * sp,
                },
                c2: P {
                    x: x - 0.20 * sp,
                    y: mid - 1.00 * sp,
                },
                b: P {
                    x: x + 0.18 * sp,
                    y: mid - 1.20 * sp,
                },
                w: 0.18 * sp,
                color: INK,
            });
        }
        // Eighth and shorter: one slanted stroke carrying one blob per beam.
        v => {
            let n = v.flags().max(1);
            let top = mid + 0.55 * sp + (n as f32 - 1.0) * 0.55 * sp;
            out.push(Prim::Line {
                a: P {
                    x: x + 0.32 * sp,
                    y: top,
                },
                b: P {
                    x: x - 0.30 * sp,
                    y: mid - 1.05 * sp,
                },
                w: 0.16 * sp,
                color: INK,
            });
            for i in 0..n {
                let y = top - i as f32 * 0.55 * sp;
                out.push(Prim::Poly {
                    pts: ellipse(
                        P {
                            x: x + 0.10 * sp,
                            y,
                        },
                        0.22 * sp,
                        0.20 * sp,
                        0.0,
                    ),
                    color: INK,
                });
                out.push(Prim::Line {
                    a: P {
                        x: x + 0.10 * sp,
                        y,
                    },
                    b: P {
                        x: x + 0.40 * sp,
                        y: y + 0.10 * sp,
                    },
                    w: 0.13 * sp,
                    color: INK,
                });
            }
        }
    }
    let mut dx = 0.85 * sp;
    for _ in 0..dot_count {
        out.push(Prim::Poly {
            pts: ellipse(
                P {
                    x: x + dx,
                    y: mid + 0.5 * sp,
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
