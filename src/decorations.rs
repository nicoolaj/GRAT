//! Calendar decorations for the GRAT logo: on ~16 dates a year, a small themed badge
//! is drawn around the logo. Every other day of the year, nothing changes.
//!
//! A decoration is a handful of abstract shapes (circle / filled convex polygon /
//! line), in coordinates centered on the logo with unit ~= the logo's own radius --
//! the same "build the data once, render it through more than one backend" idea the
//! app already uses for `Prim` (mm-space page data, painted on screen and to PDF).
//! Here the two backends are [`bake`], which stamps the shapes once into a raw RGBA
//! buffer for the OS dock/taskbar icon (egui only takes an icon at startup, via
//! `ViewportBuilder::with_icon` -- there is no live-update path), and the live egui
//! painter in `canvas::paint_decoration` (a module of the *binary*, since this file
//! stays GUI-free like the rest of the library).
//!
//! `GRAT_ICON_DATE=MM-DD` overrides the real date at startup, so any of the other
//! fifteen decorations can be previewed without waiting for its one real day a year.

use crate::model::{COLOR_BEND, COLOR_HAMMER_ON, COLOR_HARMONIC, COLOR_TRILL, COLOR_VIBRATO};
use crate::Rgb;

/// Every (month, day) this module decorates -- the single source of truth for both
/// [`decoration_for`]'s match and the "everything else is `None`" test below.
pub const DECORATED_DATES: &[(u32, u32)] = &[
    (1, 1),
    (12, 31),
    (5, 1),
    (6, 21),
    (10, 31),
    (3, 17),
    (7, 14),
    (7, 4),
    (10, 3),
    (7, 21),
    (10, 12),
    (8, 1),
    (7, 1),
    (12, 6),
    (12, 25),
    (1, 6),
];

/// One drawing instruction for a decoration, centered on the logo with unit ~= the
/// logo's own radius.
#[derive(Clone, Debug)]
pub enum Shape {
    Circle {
        c: (f32, f32),
        r: f32,
        color: Rgb,
    },
    /// Filled simple polygon, convex or not: a star is not.
    Poly {
        pts: Vec<(f32, f32)>,
        color: Rgb,
    },
    Line {
        a: (f32, f32),
        b: (f32, f32),
        w: f32,
        color: Rgb,
    },
}

/// A calendar decoration: its shapes, plus the i18n key for its caption.
#[derive(Clone, Debug)]
pub struct Decoration {
    pub shapes: Vec<Shape>,
    pub caption_key: &'static str,
}

/// Today's decoration, if any. `GRAT_ICON_DATE=MM-DD` overrides the real date.
pub fn decoration_for_today() -> Option<Decoration> {
    let (_, month, day) = std::env::var("GRAT_ICON_DATE")
        .ok()
        .and_then(|s| parse_mm_dd(&s))
        .unwrap_or_else(today);
    decoration_for(month, day)
}

fn parse_mm_dd(s: &str) -> Option<(i64, u32, u32)> {
    let (m, d) = s.split_once('-')?;
    let month: u32 = m.parse().ok()?;
    let day: u32 = d.parse().ok()?;
    if (1..=12).contains(&month) && (1..=31).contains(&day) {
        Some((0, month, day))
    } else {
        None
    }
}

/// Today's (year, month, day), UTC.
/// ponytail: UTC, not the user's local date -- right at a midnight boundary the
/// decoration can be a few hours early or late depending on timezone. Not worth a
/// `chrono` dependency for a cosmetic feature; revisit only if that actually bothers
/// someone.
fn today() -> (i64, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    civil_from_days((secs / 86_400) as i64)
}

/// Days-since-epoch (1970-01-01 = 0) to a Gregorian (year, month, day). Howard
/// Hinnant's `civil_from_days`, the standard way to do this without a calendar
/// dependency: <https://howardhinnant.github.io/date_algorithms.html> (public domain).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (y + i64::from(m <= 2), m, d)
}

pub fn decoration_for(month: u32, day: u32) -> Option<Decoration> {
    Some(match (month, day) {
        (1, 1) | (12, 31) => confetti(),
        (5, 1) => lily_of_the_valley(),
        (6, 21) => music_notes(),
        (10, 31) => pumpkin(),
        (3, 17) => clover(),
        (7, 14) => flag_fireworks(
            [
                Rgb(0x00, 0x55, 0xA4),
                Rgb(0xFF, 0xFF, 0xFF),
                Rgb(0xEF, 0x41, 0x35),
            ],
            true,
            "decoration.bastille_day",
        ),
        (10, 3) => flag_fireworks(
            [
                Rgb(0x00, 0x00, 0x00),
                Rgb(0xDD, 0x00, 0x00),
                Rgb(0xFF, 0xCE, 0x00),
            ],
            false,
            "decoration.german_unity_day",
        ),
        (7, 21) => flag_fireworks(
            [
                Rgb(0x00, 0x00, 0x00),
                Rgb(0xFD, 0xDA, 0x24),
                Rgb(0xEF, 0x33, 0x40),
            ],
            true,
            "decoration.belgian_national_day",
        ),
        (10, 12) => flag_fireworks(
            [
                Rgb(0xAA, 0x15, 0x1B),
                Rgb(0xF1, 0xBF, 0x00),
                Rgb(0xAA, 0x15, 0x1B),
            ],
            false,
            "decoration.spanish_national_day",
        ),
        (7, 1) => canada_flag_fireworks(),
        (7, 4) => usa_flag_fireworks(),
        (8, 1) => swiss_flag_fireworks(),
        (12, 6) => gifts(1, "decoration.st_nicolas"),
        (12, 25) => gifts(3, "decoration.christmas"),
        (1, 6) => three_kings(),
        _ => return None,
    })
}

// --- small reusable shape builders -----------------------------------------------

fn square(cx: f32, cy: f32, s: f32, color: Rgb) -> Shape {
    let h = s / 2.0;
    Shape::Poly {
        pts: vec![
            (cx - h, cy - h),
            (cx + h, cy - h),
            (cx + h, cy + h),
            (cx - h, cy + h),
        ],
        color,
    }
}

/// A lens-shaped leaf from `(cx, cy)` extending by `(dx, dy)`.
fn leaf(cx: f32, cy: f32, dx: f32, dy: f32, color: Rgb) -> Shape {
    let (nx, ny) = (-dy, dx);
    let w = 0.12;
    Shape::Poly {
        pts: vec![
            (cx, cy),
            (cx + dx * 0.5 + nx * w, cy + dy * 0.5 + ny * w),
            (cx + dx, cy + dy),
            (cx + dx * 0.5 - nx * w, cy + dy * 0.5 - ny * w),
        ],
        color,
    }
}

fn star(cx: f32, cy: f32, outer_r: f32, inner_r: f32, points: usize, color: Rgb) -> Shape {
    let mut pts = Vec::with_capacity(points * 2);
    for i in 0..points * 2 {
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let angle =
            std::f32::consts::PI * (i as f32) / (points as f32) - std::f32::consts::FRAC_PI_2;
        pts.push((cx + r * angle.cos(), cy + r * angle.sin()));
    }
    Shape::Poly { pts, color }
}

/// A handful of short rays radiating from `(cx, cy)`, tipped with a dot -- one
/// firework burst.
fn firework(cx: f32, cy: f32, r: f32, color: Rgb) -> Vec<Shape> {
    let rays = 8;
    (0..rays)
        .flat_map(|i| {
            let a = std::f32::consts::TAU * i as f32 / rays as f32;
            let tip = (cx + a.cos() * r, cy + a.sin() * r);
            vec![
                Shape::Line {
                    a: (cx, cy),
                    b: tip,
                    w: 0.03,
                    color,
                },
                Shape::Circle {
                    c: tip,
                    r: 0.045,
                    color,
                },
            ]
        })
        .collect()
}

/// Three equal bands (vertical or horizontal) filling `(x0,y0)..(x1,y1)`.
fn flag_bands(x0: f32, y0: f32, x1: f32, y1: f32, colors: [Rgb; 3], vertical: bool) -> Vec<Shape> {
    (0..3)
        .map(|i| {
            let pts = if vertical {
                let w = (x1 - x0) / 3.0;
                let (a, b) = (x0 + w * i as f32, x0 + w * (i as f32 + 1.0));
                vec![(a, y0), (b, y0), (b, y1), (a, y1)]
            } else {
                let h = (y1 - y0) / 3.0;
                let (a, b) = (y0 + h * i as f32, y0 + h * (i as f32 + 1.0));
                vec![(x0, a), (x1, a), (x1, b), (x0, b)]
            };
            Shape::Poly {
                pts,
                color: colors[i],
            }
        })
        .collect()
}

/// A thin dark outline around a rect -- every flag needs one, since a white band
/// (France, Canada, the US's stripes) would otherwise vanish against a light or
/// white backdrop (the About window in light theme, in particular).
fn rect_border(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Shape> {
    let color = Rgb(0x1D, 0x1D, 0x1F);
    let w = 0.025;
    vec![
        Shape::Line {
            a: (x0, y0),
            b: (x1, y0),
            w,
            color,
        },
        Shape::Line {
            a: (x1, y0),
            b: (x1, y1),
            w,
            color,
        },
        Shape::Line {
            a: (x1, y1),
            b: (x0, y1),
            w,
            color,
        },
        Shape::Line {
            a: (x0, y1),
            b: (x0, y0),
            w,
            color,
        },
    ]
}

// --- decorations -------------------------------------------------------------------

fn confetti() -> Decoration {
    let colors = [
        Rgb(0xFF, 0x3B, 0x30),
        Rgb(0xFF, 0x95, 0x00),
        Rgb(0xFF, 0xCC, 0x00),
        Rgb(0x34, 0xC7, 0x59),
        Rgb(0x0A, 0x84, 0xFF),
        Rgb(0xAF, 0x52, 0xDE),
    ];
    let positions = [
        (-1.15, 1.05),
        (1.1, 1.1),
        (-1.25, -0.15),
        (1.2, -0.25),
        (-0.55, 1.35),
        (0.65, 1.4),
        (-1.3, 0.5),
        (1.3, 0.55),
        (0.0, -1.35),
        (-0.4, -1.25),
        (0.4, -1.3),
    ];
    let shapes = positions
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let color = colors[i % colors.len()];
            if i % 2 == 0 {
                Shape::Circle {
                    c: (x, y),
                    r: 0.06,
                    color,
                }
            } else {
                square(x, y, 0.09, color)
            }
        })
        .collect();
    Decoration {
        shapes,
        caption_key: "decoration.new_year",
    }
}

fn lily_of_the_valley() -> Decoration {
    let green = Rgb(0x34, 0xC7, 0x59);
    let white = Rgb(0xFF, 0xFF, 0xFF);
    let mut shapes = vec![
        Shape::Line {
            a: (1.0, -0.3),
            b: (1.25, 1.15),
            w: 0.05,
            color: green,
        },
        leaf(0.9, 0.0, -0.45, 0.9, green),
        leaf(1.15, -0.15, 0.35, 0.75, green),
    ];
    for i in 0..4u32 {
        let t = i as f32 / 3.0;
        let c = (1.0 + t * 0.25 + 0.08, -0.05 + t * 1.05);
        // A green ring behind each bell so it stays visible even against a white or
        // light backdrop (the About window in light theme, in particular) -- a plain
        // white circle would vanish there.
        shapes.push(Shape::Circle {
            c,
            r: 0.09,
            color: green,
        });
        shapes.push(Shape::Circle {
            c,
            r: 0.065,
            color: white,
        });
    }
    Decoration {
        shapes,
        caption_key: "decoration.may_day",
    }
}

fn music_notes() -> Decoration {
    let colors = [
        COLOR_BEND,
        COLOR_HAMMER_ON,
        COLOR_HARMONIC,
        COLOR_TRILL,
        COLOR_VIBRATO,
    ];
    let positions = [
        (-1.15, 0.9),
        (1.1, 1.0),
        (-1.2, -0.5),
        (1.15, -0.6),
        (0.0, 1.35),
    ];
    let mut shapes = Vec::new();
    for (i, &(x, y)) in positions.iter().enumerate() {
        let color = colors[i % colors.len()];
        shapes.push(Shape::Circle {
            c: (x, y - 0.05),
            r: 0.09,
            color,
        });
        shapes.push(Shape::Line {
            a: (x + 0.08, y - 0.02),
            b: (x + 0.08, y + 0.35),
            w: 0.035,
            color,
        });
    }
    Decoration {
        shapes,
        caption_key: "decoration.music_day",
    }
}

fn pumpkin() -> Decoration {
    let orange = Rgb(0xFF, 0x95, 0x00);
    let green = Rgb(0x34, 0xC7, 0x59);
    let black = Rgb(0x1D, 0x1D, 0x1F);
    let (cx, cy, r) = (1.05, -0.95, 0.4);
    let shapes = vec![
        Shape::Circle {
            c: (cx - r * 0.5, cy),
            r: r * 0.85,
            color: orange,
        },
        Shape::Circle {
            c: (cx, cy),
            r,
            color: orange,
        },
        Shape::Circle {
            c: (cx + r * 0.5, cy),
            r: r * 0.85,
            color: orange,
        },
        Shape::Line {
            a: (cx, cy + r * 0.7),
            b: (cx + 0.05, cy + r * 1.3),
            w: 0.06,
            color: green,
        },
        square(cx - r * 0.35, cy + r * 0.1, r * 0.25, black),
        square(cx + r * 0.35, cy + r * 0.1, r * 0.25, black),
        Shape::Poly {
            pts: vec![
                (cx - r * 0.3, cy - r * 0.25),
                (cx + r * 0.3, cy - r * 0.25),
                (cx, cy - r * 0.55),
            ],
            color: black,
        },
    ];
    Decoration {
        shapes,
        caption_key: "decoration.halloween",
    }
}

fn clover() -> Decoration {
    let green = Rgb(0x2E, 0xA6, 0x4A);
    let (cx, cy, r) = (1.0, 0.65, 0.22);
    let mut shapes: Vec<Shape> = (0..3)
        .map(|i| {
            let a = std::f32::consts::TAU * (i as f32) / 3.0 - std::f32::consts::FRAC_PI_2;
            Shape::Circle {
                c: (cx + a.cos() * r * 0.9, cy + a.sin() * r * 0.9),
                r,
                color: green,
            }
        })
        .collect();
    shapes.push(Shape::Line {
        a: (cx, cy - r * 0.6),
        b: (cx, cy - r * 2.2),
        w: 0.05,
        color: green,
    });
    Decoration {
        shapes,
        caption_key: "decoration.st_patrick",
    }
}

fn flag_fireworks(colors: [Rgb; 3], vertical: bool, caption_key: &'static str) -> Decoration {
    let mut shapes = flag_bands(-0.45, -1.35, 0.45, -0.95, colors, vertical);
    shapes.extend(rect_border(-0.45, -1.35, 0.45, -0.95));
    shapes.extend(firework(-0.9, 1.1, 0.35, colors[0]));
    shapes.extend(firework(0.9, 1.15, 0.35, Rgb(0xFF, 0xCC, 0x00)));
    Decoration {
        shapes,
        caption_key,
    }
}

fn canada_flag_fireworks() -> Decoration {
    let red = Rgb(0xFF, 0x00, 0x00);
    let white = Rgb(0xFF, 0xFF, 0xFF);
    let mut shapes = flag_bands(-0.45, -1.35, 0.45, -0.95, [red, white, red], true);
    shapes.extend(rect_border(-0.45, -1.35, 0.45, -0.95));
    // A small diamond stands in for the maple leaf -- fine detail is not worth the
    // extra shapes at badge/icon size.
    shapes.push(Shape::Poly {
        pts: vec![(0.0, -0.98), (0.06, -1.1), (0.0, -1.22), (-0.06, -1.1)],
        color: red,
    });
    shapes.extend(firework(-0.9, 1.1, 0.35, red));
    shapes.extend(firework(0.9, 1.15, 0.35, Rgb(0xFF, 0xCC, 0x00)));
    Decoration {
        shapes,
        caption_key: "decoration.canada_day",
    }
}

fn usa_flag_fireworks() -> Decoration {
    let red = Rgb(0xB2, 0x22, 0x34);
    let white = Rgb(0xFF, 0xFF, 0xFF);
    let blue = Rgb(0x0A, 0x31, 0x61);
    let (x0, y0, x1, y1) = (-0.45, -1.35, 0.45, -0.95);
    let stripes = 5;
    let h = (y1 - y0) / stripes as f32;
    let mut shapes: Vec<Shape> = (0..stripes)
        .map(|i| {
            let color = if i % 2 == 0 { red } else { white };
            let (a, b) = (y0 + h * i as f32, y0 + h * (i as f32 + 1.0));
            Shape::Poly {
                pts: vec![(x0, a), (x1, a), (x1, b), (x0, b)],
                color,
            }
        })
        .collect();
    shapes.push(Shape::Poly {
        pts: vec![
            (x0, y0 + h * 2.0),
            (x0 + (x1 - x0) * 0.4, y0 + h * 2.0),
            (x0 + (x1 - x0) * 0.4, y1),
            (x0, y1),
        ],
        color: blue,
    });
    shapes.extend(rect_border(x0, y0, x1, y1));
    shapes.extend(firework(-0.9, 1.1, 0.35, red));
    shapes.extend(firework(0.9, 1.15, 0.35, blue));
    Decoration {
        shapes,
        caption_key: "decoration.independence_day_us",
    }
}

fn swiss_flag_fireworks() -> Decoration {
    let red = Rgb(0xD5, 0x29, 0x2E);
    let white = Rgb(0xFF, 0xFF, 0xFF);
    let (x0, y0, x1, y1) = (-0.35, -1.3, 0.35, -0.98);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (w, h) = (x1 - x0, y1 - y0);
    let mut shapes = vec![Shape::Poly {
        pts: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)],
        color: red,
    }];
    let bar_w = w * 0.22;
    let bar_h = h * 0.22;
    shapes.push(Shape::Poly {
        pts: vec![
            (cx - bar_w / 2.0, y0 + h * 0.2),
            (cx + bar_w / 2.0, y0 + h * 0.2),
            (cx + bar_w / 2.0, y1 - h * 0.2),
            (cx - bar_w / 2.0, y1 - h * 0.2),
        ],
        color: white,
    });
    shapes.push(Shape::Poly {
        pts: vec![
            (x0 + w * 0.2, cy - bar_h / 2.0),
            (x1 - w * 0.2, cy - bar_h / 2.0),
            (x1 - w * 0.2, cy + bar_h / 2.0),
            (x0 + w * 0.2, cy + bar_h / 2.0),
        ],
        color: white,
    });
    shapes.extend(rect_border(x0, y0, x1, y1));
    shapes.extend(firework(-0.9, 1.1, 0.35, red));
    shapes.extend(firework(0.9, 1.15, 0.35, Rgb(0xFF, 0xCC, 0x00)));
    Decoration {
        shapes,
        caption_key: "decoration.swiss_national_day",
    }
}

fn gift_box(cx: f32, cy: f32, s: f32, box_color: Rgb, ribbon_color: Rgb) -> Vec<Shape> {
    let h = s / 2.0;
    vec![
        square(cx, cy, s, box_color),
        Shape::Line {
            a: (cx - h, cy),
            b: (cx + h, cy),
            w: s * 0.12,
            color: ribbon_color,
        },
        Shape::Line {
            a: (cx, cy - h),
            b: (cx, cy + h),
            w: s * 0.12,
            color: ribbon_color,
        },
        Shape::Poly {
            pts: vec![
                (cx - 0.05, cy + h),
                (cx, cy + h + 0.08),
                (cx - 0.1, cy + h + 0.1),
            ],
            color: ribbon_color,
        },
        Shape::Poly {
            pts: vec![
                (cx + 0.05, cy + h),
                (cx, cy + h + 0.08),
                (cx + 0.1, cy + h + 0.1),
            ],
            color: ribbon_color,
        },
    ]
}

fn gifts(n: usize, caption_key: &'static str) -> Decoration {
    // No white ribbon: it would vanish against a white or light backdrop.
    let palette = [
        (Rgb(0xFF, 0x3B, 0x30), Rgb(0xFF, 0xCC, 0x00)),
        (Rgb(0x0A, 0x84, 0xFF), Rgb(0xFF, 0xCC, 0x00)),
        (Rgb(0x34, 0xC7, 0x59), Rgb(0xFF, 0x3B, 0x30)),
    ];
    let positions = [(-1.1, -0.9), (1.1, -0.9), (0.0, -1.3)];
    let mut shapes = Vec::new();
    for i in 0..n.min(3) {
        let (bx, by) = positions[i];
        let (box_color, ribbon_color) = palette[i % palette.len()];
        shapes.extend(gift_box(bx, by, 0.4, box_color, ribbon_color));
    }
    Decoration {
        shapes,
        caption_key,
    }
}

fn three_kings() -> Decoration {
    let gold = Rgb(0xFF, 0xCC, 0x00);
    let positions = [(-0.95, 1.05), (0.0, 1.35), (0.95, 1.05)];
    let shapes = positions
        .iter()
        .map(|&(x, y)| star(x, y, 0.22, 0.09, 5, gold))
        .collect();
    Decoration {
        shapes,
        caption_key: "decoration.epiphany",
    }
}

// --- raster baking (OS dock/taskbar icon) ------------------------------------------

/// Bakes `deco`'s shapes into an RGBA pixel buffer of `w`x`h`, centered at
/// `(cx, cy)` with `radius` pixels -- used once, at startup, since egui only takes
/// an icon at `ViewportBuilder::with_icon` time.
pub fn bake(rgba: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, radius: f32, deco: &Decoration) {
    for shape in &deco.shapes {
        match shape {
            Shape::Circle { c, r, color } => {
                fill_circle(
                    rgba,
                    w,
                    h,
                    cx + c.0 * radius,
                    cy + c.1 * radius,
                    r * radius,
                    *color,
                );
            }
            Shape::Poly { pts, color } => {
                let points: Vec<(f32, f32)> = pts
                    .iter()
                    .map(|&(x, y)| (cx + x * radius, cy + y * radius))
                    .collect();
                fill_poly(rgba, w, h, &points, *color);
            }
            Shape::Line { a, b, w: lw, color } => {
                stroke_line(
                    rgba,
                    w,
                    h,
                    (cx + a.0 * radius, cy + a.1 * radius),
                    (cx + b.0 * radius, cy + b.1 * radius),
                    lw * radius,
                    *color,
                );
            }
        }
    }
}

fn blend_pixel(rgba: &mut [u8], w: u32, h: u32, x: i32, y: i32, color: Rgb) {
    if x < 0 || y < 0 || x as u32 >= w || y as u32 >= h {
        return;
    }
    let i = ((y as u32 * w + x as u32) * 4) as usize;
    rgba[i] = color.0;
    rgba[i + 1] = color.1;
    rgba[i + 2] = color.2;
    rgba[i + 3] = 255;
}

fn fill_circle(rgba: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32, color: Rgb) {
    let r2 = r * r;
    let (x0, x1) = ((cx - r).floor() as i32, (cx + r).ceil() as i32);
    let (y0, y1) = ((cy - r).floor() as i32, (cy + r).ceil() as i32);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= r2 {
                blend_pixel(rgba, w, h, x, y, color);
            }
        }
    }
}

/// Samples points along the segment and stamps a filled circle at each -- reuses
/// `fill_circle` for line thickness instead of a separate thick-line rasterizer.
fn stroke_line(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    a: (f32, f32),
    b: (f32, f32),
    width: f32,
    color: Rgb,
) {
    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let steps = (len / (width * 0.5).max(0.5)).ceil().max(1.0) as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        fill_circle(rgba, w, h, x, y, width / 2.0, color);
    }
}

/// Even-odd scanline fill -- every `Shape::Poly` this module builds is convex, like
/// `Prim::Poly`'s note heads.
fn fill_poly(rgba: &mut [u8], w: u32, h: u32, pts: &[(f32, f32)], color: Rgb) {
    if pts.len() < 3 {
        return;
    }
    let y0 = pts
        .iter()
        .map(|p| p.1)
        .fold(f32::INFINITY, f32::min)
        .floor() as i32;
    let y1 = pts
        .iter()
        .map(|p| p.1)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as i32;
    for y in y0..=y1 {
        let yf = y as f32 + 0.5;
        let mut xs: Vec<f32> = Vec::new();
        for i in 0..pts.len() {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % pts.len()];
            if (ay <= yf && by > yf) || (by <= yf && ay > yf) {
                let t = (yf - ay) / (by - ay);
                xs.push(ax + t * (bx - ax));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks(2) {
            if let [x0, x1] = pair {
                for x in x0.round() as i32..x1.round() as i32 {
                    blend_pixel(rgba, w, h, x, y, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(364), (1970, 12, 31));
        assert_eq!(civil_from_days(365), (1971, 1, 1));
        assert_eq!(civil_from_days(730), (1972, 1, 1));
        assert_eq!(civil_from_days(10_957), (2000, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29)); // 2000 is a leap year (div by 400)
    }

    #[test]
    fn decoration_for_covers_exactly_the_requested_dates() {
        for &(m, d) in DECORATED_DATES {
            assert!(
                decoration_for(m, d).is_some(),
                "{m}-{d} should be decorated"
            );
        }
        for &(m, d) in &[(2, 14), (9, 17), (4, 1), (11, 11), (8, 15)] {
            assert!(
                decoration_for(m, d).is_none(),
                "{m}-{d} should not be decorated"
            );
        }
    }

    #[test]
    fn parse_mm_dd_rejects_garbage() {
        assert_eq!(parse_mm_dd("07-14"), Some((0, 7, 14)));
        assert!(parse_mm_dd("13-01").is_none());
        assert!(parse_mm_dd("07-32").is_none());
        assert!(parse_mm_dd("nope").is_none());
    }

    #[test]
    fn bake_stays_in_bounds_and_paints_something() {
        let (w, h) = (64u32, 64u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for &(m, d) in DECORATED_DATES {
            let deco = decoration_for(m, d).unwrap();
            bake(
                &mut rgba,
                w,
                h,
                w as f32 / 2.0,
                h as f32 / 2.0,
                w as f32 * 0.4,
                &deco,
            );
        }
        assert!(
            rgba.iter().any(|&b| b != 0),
            "baking should have painted some pixels"
        );
    }
}
