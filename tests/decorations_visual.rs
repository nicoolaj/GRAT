//! Visual proof sheet for `decorations`: every calendar decoration rendered to
//! `dist/decorations-preview.svg` (the vector shapes, as painted live on the splash
//! and About windows) and baked onto a plain disc in `dist/decorations-preview.png`
//! (the same `bake` path used for the dock/taskbar icon) -- a developer tool, the
//! only practical way to look at all sixteen without waiting a year for their real
//! dates, mirroring `tests/visual.rs`'s existing proof-sheet pattern.

use grat::decorations::{self, Shape};

const COLS: usize = 4;

fn svg_shapes(shapes: &[Shape], cx: f32, cy: f32, r: f32) -> String {
    let hex = |c: grat::Rgb| format!("#{:02X}{:02X}{:02X}", c.0, c.1, c.2);
    let mut s = format!("<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\" fill=\"#DDDDE3\"/>");
    for shape in shapes {
        match shape {
            Shape::Circle { c, r: sr, color } => {
                s += &format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>",
                    cx + c.0 * r,
                    cy - c.1 * r,
                    sr * r,
                    hex(*color)
                );
            }
            Shape::Poly { pts, color } => {
                let d: Vec<String> = pts
                    .iter()
                    .map(|&(x, y)| format!("{},{}", cx + x * r, cy - y * r))
                    .collect();
                s += &format!(
                    "<polygon points=\"{}\" fill=\"{}\"/>",
                    d.join(" "),
                    hex(*color)
                );
            }
            Shape::Line { a, b, w, color } => {
                s += &format!(
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"/>",
                    cx + a.0 * r,
                    cy - a.1 * r,
                    cx + b.0 * r,
                    cy - b.1 * r,
                    hex(*color),
                    w * r
                );
            }
        }
    }
    s
}

#[test]
fn decorations_preview_svg() {
    let cell = 140.0_f32;
    let radius = 46.0_f32;
    let n = decorations::DECORATED_DATES.len();
    let rows = n.div_ceil(COLS);
    let (w, h) = (COLS as f32 * cell, rows as f32 * cell);
    let mut body = String::new();
    for (i, &(m, d)) in decorations::DECORATED_DATES.iter().enumerate() {
        let deco = decorations::decoration_for(m, d).unwrap();
        let (col, row) = (i % COLS, i / COLS);
        let (cx, cy) = (
            col as f32 * cell + cell / 2.0,
            row as f32 * cell + cell / 2.0,
        );
        body += &format!("<g>{}", svg_shapes(&deco.shapes, cx, cy, radius));
        body += &format!(
            "<text x=\"{cx}\" y=\"{}\" font-family=\"Helvetica,Arial,sans-serif\" \
             font-size=\"10\" text-anchor=\"middle\">{m:02}-{d:02} {}</text></g>",
            row as f32 * cell + cell - 6.0,
            deco.caption_key,
        );
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>{body}</svg>"
    );
    std::fs::create_dir_all("dist").unwrap();
    std::fs::write("dist/decorations-preview.svg", svg).unwrap();
}

#[test]
fn decorations_preview_png() {
    let cell_px = 128u32;
    let n = decorations::DECORATED_DATES.len();
    let rows = n.div_ceil(COLS) as u32;
    let (w, h) = (COLS as u32 * cell_px, rows * cell_px);
    let mut rgba = vec![255u8; (w * h * 4) as usize];
    for (i, &(m, d)) in decorations::DECORATED_DATES.iter().enumerate() {
        let deco = decorations::decoration_for(m, d).unwrap();
        let (col, row) = (i % COLS, i / COLS);
        let cx = col as u32 * cell_px + cell_px / 2;
        let cy = row as u32 * cell_px + cell_px / 2;
        decorations::bake(
            &mut rgba,
            w,
            h,
            cx as f32,
            cy as f32,
            cell_px as f32 * 0.36,
            &deco,
        );
    }
    std::fs::create_dir_all("dist").unwrap();
    image::save_buffer(
        "dist/decorations-preview.png",
        &rgba,
        w,
        h,
        image::ColorType::Rgba8,
    )
    .unwrap();
}

/// Every decoration baked onto its own copy of the *real* bundled logo (not a plain
/// disc) -- the literal deliverable: what `main()` bakes into the dock/taskbar icon.
#[test]
fn icon_preview_png() {
    let logo = image::load_from_memory_with_format(
        include_bytes!("../src/assets/logo.png"),
        image::ImageFormat::Png,
    )
    .unwrap()
    .to_rgba8();
    let (logo_w, logo_h) = (logo.width(), logo.height());
    let cell_px = logo_w;
    let n = decorations::DECORATED_DATES.len();
    let rows = n.div_ceil(COLS) as u32;
    let (w, h) = (COLS as u32 * cell_px, rows * cell_px);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (i, &(m, d)) in decorations::DECORATED_DATES.iter().enumerate() {
        let deco = decorations::decoration_for(m, d).unwrap();
        let (col, row) = (i % COLS, i / COLS);
        let (ox, oy) = (col as u32 * cell_px, row as u32 * cell_px);
        // Paste this cell's copy of the real logo, then bake on top of it -- exactly
        // what `main()` does to the icon buffer at startup.
        for y in 0..logo_h {
            for x in 0..logo_w {
                let src = logo.get_pixel(x, y).0;
                let i = (((oy + y) * w + ox + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&src);
            }
        }
        let radius = logo_w.min(logo_h) as f32 * 0.36;
        decorations::bake(
            &mut rgba,
            w,
            h,
            (ox + logo_w / 2) as f32,
            (oy + logo_h / 2) as f32,
            radius,
            &deco,
        );
    }
    std::fs::create_dir_all("dist").unwrap();
    image::save_buffer(
        "dist/icon-preview.png",
        &rgba,
        w,
        h,
        image::ColorType::Rgba8,
    )
    .unwrap();
}
