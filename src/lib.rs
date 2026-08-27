//! Core library for the Tablatures editor: data model, i18n, and the shared drawing
//! vocabulary (`Prim`) that the on-screen and PDF backends both render from.
//!
//! Kept free of any GUI dependency so it can be exercised from integration tests and
//! from the `--export` CLI path without opening a window.

pub mod engrave;
pub mod i18n;
pub mod layout;
pub mod model;
pub mod notation;
pub mod pdf;
pub mod staff;
pub mod tablature;

/// A point on the page, in millimetres, origin at the BOTTOM-LEFT corner (PDF convention).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct P {
    pub x: f32,
    pub y: f32,
}

impl P {
    pub fn new(x: f32, y: f32) -> Self {
        P { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// Horizontal anchoring of a text primitive around its position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// One drawing instruction, in page millimetres. Rendered identically by the
/// on-screen egui backend and the PDF backend.
#[derive(Clone, Debug)]
pub enum Prim {
    Line {
        a: P,
        b: P,
        w: f32,
        color: Rgb,
    },
    /// Filled polygon: note heads (ellipse sampled into points), beams, strum arrows.
    Poly {
        pts: Vec<P>,
        color: Rgb,
    },
    /// Cubic Bezier stroke: ties, slurs, slides.
    Curve {
        a: P,
        c1: P,
        c2: P,
        b: P,
        w: f32,
        color: Rgb,
    },
    /// `pos` is the text BASELINE point.
    Text {
        pos: P,
        s: String,
        pt: f32,
        color: Rgb,
        align: Align,
    },
}

pub const PAGE_W_MM: f32 = 210.0;
pub const PAGE_H_MM: f32 = 297.0;
pub const MARGIN_MM: f32 = 15.0;
