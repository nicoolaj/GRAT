//! `Prim` -> printpdf `Op` translation: the paper half of the shared-layout WYSIWYG
//! promise (see `layout.rs`'s module doc comment). `layout::paginate` already
//! produces page coordinates in PDF convention -- millimetres, origin bottom-left,
//! y up -- so this module does no flipping and no scaling, unlike `canvas.rs`.

use printpdf::{
    BuiltinFont, Color, Line, LineCapStyle, LinePoint, Mm, Op, PaintMode, PdfDocument,
    PdfFontHandle, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, TextItem,
    WindingOrder,
};

use crate::layout::{self, Page};
use crate::model::Document;
use crate::{staff, Align, Prim, Rgb, P, PAGE_H_MM, PAGE_W_MM};

/// Render the whole document to PDF bytes: `layout::paginate` for the geometry (the
/// same call `canvas.rs` makes every frame), then a direct `Prim` -> `Op`
/// translation, one `PdfPage` per `layout::Page`.
pub fn export(doc: &Document) -> Vec<u8> {
    let pages: Vec<PdfPage> = layout::paginate(doc)
        .iter()
        .map(|page| PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), page_ops(page)))
        .collect();

    let mut pdf = PdfDocument::new(&doc.title);
    pdf.metadata.info.author = doc.author.clone();

    let mut warnings = Vec::new();
    pdf.with_pages(pages)
        .save(&PdfSaveOptions::default(), &mut warnings)
}

/// One page's primitives as printpdf ops. `SetLineCapStyle` is set once up front so
/// strokes round the same way they do on screen (see `canvas.rs::draw_prim`, which
/// gets rounded caps for free from egui's default stroke).
fn page_ops(page: &Page) -> Vec<Op> {
    let mut ops = vec![Op::SetLineCapStyle {
        cap: LineCapStyle::Round,
    }];
    for prim in &page.prims {
        prim_ops(prim, &mut ops);
    }
    ops
}

fn color(c: Rgb) -> Color {
    Color::Rgb(printpdf::Rgb::new(
        c.0 as f32 / 255.0,
        c.1 as f32 / 255.0,
        c.2 as f32 / 255.0,
        None,
    ))
}

fn point(p: P) -> Point {
    Point::new(Mm(p.x), Mm(p.y))
}

fn prim_ops(prim: &Prim, ops: &mut Vec<Op>) {
    match prim {
        Prim::Line { a, b, w, color: c } => {
            ops.push(Op::SetOutlineColor { col: color(*c) });
            ops.push(Op::SetOutlineThickness { pt: Mm(*w).into() });
            ops.push(Op::DrawLine {
                line: Line {
                    points: vec![
                        LinePoint {
                            p: point(*a),
                            bezier: false,
                        },
                        LinePoint {
                            p: point(*b),
                            bezier: false,
                        },
                    ],
                    is_closed: false,
                },
            });
        }
        Prim::Poly { pts, color: c } => {
            ops.push(Op::SetFillColor { col: color(*c) });
            ops.push(Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![PolygonRing {
                        points: pts
                            .iter()
                            .map(|p| LinePoint {
                                p: point(*p),
                                bezier: false,
                            })
                            .collect(),
                    }],
                    mode: PaintMode::Fill,
                    winding_order: WindingOrder::NonZero,
                },
            });
        }
        Prim::Curve {
            a,
            c1,
            c2,
            b,
            w,
            color: c,
        } => {
            // Two consecutive bezier handles followed by a plain end point emit a
            // single PDF `c` operator (verified in printpdf's own serializer) --
            // exactly the four-point shape a cubic Bezier needs.
            ops.push(Op::SetOutlineColor { col: color(*c) });
            ops.push(Op::SetOutlineThickness { pt: Mm(*w).into() });
            ops.push(Op::DrawLine {
                line: Line {
                    points: vec![
                        LinePoint {
                            p: point(*a),
                            bezier: false,
                        },
                        LinePoint {
                            p: point(*c1),
                            bezier: true,
                        },
                        LinePoint {
                            p: point(*c2),
                            bezier: true,
                        },
                        LinePoint {
                            p: point(*b),
                            bezier: false,
                        },
                    ],
                    is_closed: false,
                },
            });
        }
        Prim::Text {
            pos,
            s,
            pt,
            color: c,
            align,
        } => {
            // printpdf has no text alignment: shift the cursor by the label's own
            // width first, using the same `staff::label_width` the engraver uses to
            // centre fret numbers, so screen and paper agree on where text lands.
            let x = match align {
                Align::Left => pos.x,
                Align::Center => pos.x - staff::label_width(s, *pt) / 2.0,
                Align::Right => pos.x - staff::label_width(s, *pt),
            };
            ops.push(Op::SetFillColor { col: color(*c) });
            // Every text item gets its own BT/ET pair: `Op::SetTextCursor` emits
            // `Td`, which is relative to the previous text line. `BT` resets the
            // text matrix so the first (and only) `Td` in this section is absolute
            // -- otherwise each label on the page would drift further than the last.
            ops.push(Op::StartTextSection);
            ops.push(Op::SetFont {
                font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
                size: Pt(*pt),
            });
            ops.push(Op::SetTextCursor {
                pos: Point::new(Mm(x), Mm(pos.y)),
            });
            ops.push(Op::ShowText {
                items: vec![TextItem::Text(s.clone())],
            });
            ops.push(Op::EndTextSection);
        }
    }
}
