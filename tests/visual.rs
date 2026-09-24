//! Visual proof sheet for the engraving: renders a bar of every rhythmic case to
//! `dist/notation-preview.svg`, which is a plain file you can open in a browser.
//!
//! This is a developer tool, not an assertion about beauty — but it does assert
//! that the engraver produces primitives for every case, and it is the only way to
//! actually look at beams, rests and the clef without building a PDF.

use grat::{engrave, model::*, notation, tablature, Align, Prim, P};

/// Millimetre page primitives to an SVG whose y axis is flipped back to screen order.
fn svg(prims: &[Prim], w: f32, h: f32) -> String {
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" \
         viewBox=\"0 0 {w} {h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>\
         <g transform=\"translate(0,{h}) scale(1,-1)\">"
    );
    let hex = |c: grat::Rgb| format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2);
    for p in prims {
        match p {
            Prim::Line { a, b, w, color } => s += &format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                a.x, a.y, b.x, b.y, hex(*color), w
            ),
            Prim::Poly { pts, color } => {
                let d: Vec<String> = pts.iter().map(|p| format!("{},{}", p.x, p.y)).collect();
                s += &format!("<polygon points=\"{}\" fill=\"{}\"/>", d.join(" "), hex(*color));
            }
            Prim::Curve { a, c1, c2, b, w, color } => s += &format!(
                "<path d=\"M{},{} C{},{} {},{} {},{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                a.x, a.y, c1.x, c1.y, c2.x, c2.y, b.x, b.y, hex(*color), w
            ),
            Prim::Text { pos, s: text, pt, color, align } => {
                // Fret labels carry <12> and (5); SVG text needs them escaped.
                let text = text
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                let anchor = match align {
                    Align::Left => "start",
                    Align::Center => "middle",
                    Align::Right => "end",
                };
                // Flip back locally so the glyphs are not mirrored.
                s += &format!(
                    "<g transform=\"translate({},{}) scale(1,-1)\"><text x=\"0\" y=\"0\" \
                     font-family=\"Helvetica,Arial,sans-serif\" font-size=\"{}\" fill=\"{}\" \
                     text-anchor=\"{}\">{}</text></g>",
                    pos.x, pos.y, pt * 0.352_777_8, hex(*color), anchor, text
                );
            }
        }
    }
    s + "</g></svg>"
}

/// Every technique glyph, one after another on a single string.
fn technique_bar() -> Bar {
    let techs = [
        Technique::Plain,
        Technique::HammerOn,
        Technique::PullOff,
        Technique::Slide,
        Technique::SlideShift,
        Technique::SlideIn { from_fret: 3 },
        Technique::Grace,
        Technique::Bend { quarters: 4 },
        Technique::BendRelease { quarters: 2 },
        Technique::PreBend { quarters: 4 },
        Technique::Vibrato,
        Technique::WideVibrato,
        Technique::Harmonic,
        Technique::PinchHarmonic,
        Technique::Tap,
        Technique::Slap,
        Technique::Pop,
        Technique::Dead,
        Technique::Ghost,
        Technique::Trill { to_fret: 9 },
    ];
    Bar {
        events: techs
            .iter()
            .enumerate()
            .map(|(i, &tech)| Event {
                dur: Dur {
                    base: NoteValue::Quarter,
                    dots: 0,
                },
                notes: vec![Note {
                    string: 2,
                    fret: 5 + (i % 3) as u8,
                    tech,
                    tie_next: false,
                }],
                ..Default::default()
            })
            .collect(),
        time_sig: Some((20, 4)),
        ..Default::default()
    }
}

/// Palm mute and let ring spans, inside a repeated passage.
fn span_bar() -> Bar {
    Bar {
        events: (0..8)
            .map(|i| Event {
                dur: Dur {
                    base: NoteValue::Eighth,
                    dots: 0,
                },
                notes: vec![Note {
                    string: 5,
                    fret: 3,
                    tech: Technique::Plain,
                    tie_next: false,
                }],
                strum: Some(if i % 2 == 0 { Strum::Down } else { Strum::Up }),
                palm_mute: i < 4,
                let_ring: i >= 6,
            })
            .collect(),
        time_sig: Some((4, 4)),
        repeat_start: true,
        repeat_end: Some(3),
    }
}

/// Mixed rhythm, to exercise the stems and beams under a tablature-only block.
fn rhythm_bar() -> Bar {
    let e = |base, dots, notes| Event {
        dur: Dur { base, dots },
        notes,
        ..Default::default()
    };
    let n = |fret| {
        vec![Note {
            string: 3,
            fret,
            tech: Technique::Plain,
            tie_next: false,
        }]
    };
    Bar {
        // Exactly one bar of 4/4: a dot, a flagged eighth alone on its beat, then a
        // beamed run whose secondary beam covers only the sixteenths.
        events: vec![
            e(NoteValue::Quarter, 1, n(5)),
            e(NoteValue::Eighth, 0, n(7)),
            e(NoteValue::Quarter, 0, n(8)),
            e(NoteValue::Eighth, 0, n(10)),
            e(NoteValue::Sixteenth, 0, n(12)),
            e(NoteValue::Sixteenth, 0, n(3)),
        ],
        time_sig: Some((4, 4)),
        ..Default::default()
    }
}

/// Lone flagged notes, stems up and down, one on ledger lines, each walled off by a
/// rest so nothing beams them.
fn flag_bar() -> Bar {
    let e = |base, dots, notes| Event {
        dur: Dur { base, dots },
        notes,
        ..Default::default()
    };
    let n = |string, fret| {
        vec![Note {
            string,
            fret,
            tech: Technique::Plain,
            tie_next: false,
        }]
    };
    Bar {
        events: vec![
            e(NoteValue::Eighth, 0, n(0, 5)),
            e(NoteValue::Eighth, 0, vec![]),
            e(NoteValue::Sixteenth, 0, n(4, 2)),
            e(NoteValue::Eighth, 1, vec![]),
            e(NoteValue::Sixteenth, 0, n(0, 8)),
            e(NoteValue::Eighth, 1, vec![]),
            e(NoteValue::Sixteenth, 0, n(5, 0)),
            e(NoteValue::Eighth, 1, vec![]),
        ],
        ..Default::default()
    }
}

#[test]
fn proof_sheet() {
    let mut doc = Document::new_empty();
    doc.title = "Engraving proof".into();
    doc.bars = vec![technique_bar(), span_bar(), rhythm_bar(), flag_bar()];

    let music = 180.0 - tablature::HEAD_MM;
    let left = 15.0 + tablature::HEAD_MM;
    let mut prims = Vec::new();
    let mut hits = Vec::new();

    // Row 1 — every technique glyph.
    let sp = engrave::system_spacing(&doc, 0..1, Some(music));
    tablature::render(&doc, &sp, P { x: left, y: 157.0 }, &mut prims, &mut hits);

    // Row 2 — a full three-row block: tablature, strumming, staff, one Spacing for
    // all three, which is what makes the columns line up.
    let sp = engrave::system_spacing(&doc, 1..2, Some(music));
    tablature::render(&doc, &sp, P { x: left, y: 111.0 }, &mut prims, &mut hits);
    tablature::render_strum(&doc, &sp, P { x: left, y: 104.0 }, &mut prims);
    notation::render(&doc, &sp, P { x: left, y: 87.0 }, &mut prims);

    // Row 3 — chord names over a tablature row with no staff, so the rhythm row
    // under it carries the rhythm.
    let sp = engrave::system_spacing(&doc, 2..3, Some(music));
    tablature::render_chords(&doc, &sp, P { x: left, y: 75.0 }, &mut prims);
    tablature::render(&doc, &sp, P { x: left, y: 51.0 }, &mut prims, &mut hits);
    tablature::render_rhythm(&doc, &sp, P { x: left, y: 51.0 }, &mut prims);

    // Row 4 — the staff alone, lone flags up and down and a stem stretched to the
    // middle line.
    let sp = engrave::system_spacing(&doc, 3..4, Some(music));
    notation::render(&doc, &sp, P { x: left, y: 16.0 }, &mut prims);

    assert!(
        prims.len() > 300,
        "expected a full sheet, got {}",
        prims.len()
    );
    // The three rows of the middle block must agree column by column, or a fret
    // number, its strum arrow and its note head would drift apart on the page.
    let block = engrave::system_spacing(&doc, 1..2, Some(music));
    assert!(
        block.bars[0].events.windows(2).all(|w| w[1] > w[0]),
        "one shared Spacing drives all three rows"
    );
    assert_eq!(
        hits.len(),
        (20 + 8 + 6) * 6,
        "one clickable cell per string per event"
    );
    assert!(
        hits.iter().all(|h| h.max.x > h.min.x && h.max.y > h.min.y),
        "every hit box is non-empty"
    );

    std::fs::create_dir_all("dist").ok();
    std::fs::write("dist/notation-preview.svg", svg(&prims, 210.0, 185.0)).unwrap();
    eprintln!("wrote dist/notation-preview.svg");
}

/// A realistic piece, so the page furniture can be looked at: title block, running
/// header, footer, several blocks per page, and a repeat spanning a page break.
fn song() -> Document {
    let mut doc = Document::new_empty();
    doc.title = "GRAT".into();
    doc.author = "Nicolas Jalibert".into();
    doc.rows = vec![Row::Chords, Row::Tab, Row::Strum, Row::Notation];

    let riff = |offset: u8, strum: bool| Bar {
        events: (0..8)
            .map(|i| Event {
                dur: Dur {
                    base: NoteValue::Eighth,
                    dots: 0,
                },
                notes: vec![Note {
                    string: (i % 4) as u8 + 2,
                    fret: offset + (i % 3) as u8,
                    tech: match i % 5 {
                        1 => Technique::HammerOn,
                        2 => Technique::PullOff,
                        3 => Technique::Slide,
                        _ => Technique::Plain,
                    },
                    tie_next: false,
                }],
                strum: strum.then_some(if i % 2 == 0 { Strum::Down } else { Strum::Up }),
                palm_mute: i < 3,
                let_ring: false,
            })
            .collect(),
        time_sig: (offset == 3).then_some((4, 4)),
        repeat_start: offset == 5,
        repeat_end: (offset == 9).then_some(2),
    };
    doc.bars = (0..24)
        .map(|i| riff(3 + (i % 4) as u8 * 2, i % 2 == 0))
        .collect();
    doc
}

#[test]
fn page_proof_sheet() {
    let doc = song();
    let pages = grat::layout::paginate(&doc);
    assert!(
        pages.len() >= 2,
        "24 bars of three-row blocks need more than one page"
    );

    // Lay the pages out side by side so the furniture can be compared at a glance:
    // page 1 carries the title block, page 3 the running header, all of them a footer.
    let gap = 8.0;
    let mut prims = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let dx = i as f32 * (210.0 + gap);
        prims.push(Prim::Poly {
            pts: vec![
                P { x: dx, y: 0.0 },
                P {
                    x: dx + 210.0,
                    y: 0.0,
                },
                P {
                    x: dx + 210.0,
                    y: 297.0,
                },
                P { x: dx, y: 297.0 },
            ],
            color: grat::Rgb(0xFF, 0xFF, 0xFF),
        });
        prims.extend(page.prims.iter().cloned().map(|p| shift(p, dx)));
    }

    assert!(
        pages.iter().all(|p| p
            .hits
            .iter()
            .all(|h| h.min.x >= 0.0 && h.max.x <= 210.0 && h.min.y >= 0.0 && h.max.y <= 297.0)),
        "every clickable cell stays on the page"
    );

    let width = pages.len() as f32 * (210.0 + gap);
    std::fs::create_dir_all("dist").ok();
    std::fs::write("dist/page-preview.svg", svg(&prims, width, 297.0)).unwrap();
    eprintln!("wrote dist/page-preview.svg — {} pages", pages.len());
}

/// Move a primitive sideways, to place whole pages next to each other.
fn shift(p: Prim, dx: f32) -> Prim {
    let m = |q: P| P {
        x: q.x + dx,
        y: q.y,
    };
    match p {
        Prim::Line { a, b, w, color } => Prim::Line {
            a: m(a),
            b: m(b),
            w,
            color,
        },
        Prim::Poly { pts, color } => Prim::Poly {
            pts: pts.into_iter().map(m).collect(),
            color,
        },
        Prim::Curve {
            a,
            c1,
            c2,
            b,
            w,
            color,
        } => Prim::Curve {
            a: m(a),
            c1: m(c1),
            c2: m(c2),
            b: m(b),
            w,
            color,
        },
        Prim::Text {
            pos,
            s,
            pt,
            color,
            align,
        } => Prim::Text {
            pos: m(pos),
            s,
            pt,
            color,
            align,
        },
    }
}

#[test]
fn chord_names_are_recognised() {
    let doc = Document::new_empty();
    // (string, fret) pairs, string 0 = high E.
    let name = |frets: &[(u8, u8)]| {
        let event = Event {
            notes: frets
                .iter()
                .map(|&(string, fret)| Note {
                    string,
                    fret,
                    tech: Technique::Plain,
                    tie_next: false,
                })
                .collect(),
            ..Event::default()
        };
        tablature::chord_name(&doc, &event)
    };
    // This binary's only language user; smoke.rs flips the global language.
    grat::i18n::set_lang("en");
    assert_eq!(
        name(&[(4, 3), (3, 2), (2, 0), (1, 1), (0, 0)]).as_deref(),
        Some("C")
    );
    assert_eq!(
        name(&[(4, 0), (3, 2), (2, 2), (1, 1), (0, 0)]).as_deref(),
        Some("Am")
    );
    assert_eq!(
        name(&[(5, 0), (4, 2), (3, 0), (2, 1), (1, 0)]).as_deref(),
        Some("E7")
    );
    assert_eq!(name(&[(5, 3), (4, 5)]).as_deref(), Some("G5"));
    // C major over its third in the bass.
    assert_eq!(
        name(&[(5, 0), (4, 3), (3, 2), (2, 0), (1, 1)]).as_deref(),
        Some("C/E")
    );
    assert_eq!(name(&[(0, 3)]), None);
    assert_eq!(name(&[]), None);
}
