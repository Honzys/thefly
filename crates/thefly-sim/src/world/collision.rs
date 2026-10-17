//! W1. Boxes and `.bor` outline overlap (fly-and-world §4.2–§4.5). Pure functions; every other
//! worker uses these instead of writing its own test.

use crate::data::{Outline, Size};

/// An axis-aligned box in one coordinate space (screen or a layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// Strict box overlap. Open item per call site: some original tests are inclusive (the
    /// flying-goodie catch, §6.3); those use [`Rect::catches`].
    pub fn overlaps(&self, o: &Rect) -> bool {
        let _ = o;
        todo!("W1 fly-and-world §4.5")
    }

    /// The flying-goodie catch: goodie x ∈ [px − gw, px + pw] and y ∈ [py − gh, py + ph].
    pub fn catches(&self, goodie: &Rect) -> bool {
        let _ = goodie;
        todo!("W1 fly-and-world §6.3")
    }
}

/// The fly's fixed outline (`FUN_0042d67c`): rows 29 and 84 hold 15..=45, rows 30..=83 hold 15
/// and 45, in 2-px units.
pub fn fly_outline() -> Outline {
    todo!("W1 fly-and-world §4.2")
}

/// One sprite for the outline test: screen x, y, frame size and outline.
#[derive(Debug, Clone, Copy)]
pub struct Shape<'a> {
    pub x: i32,
    pub y: i32,
    pub size: Size,
    pub outline: &'a Outline,
}

/// `FUN_0042f6de`: boxes first (with the wrap shift when either straddles `wrap_width`), then a
/// hit when an outline point of `a` coincides with one of `b` at half-x resolution in a shared
/// row. Open item: the contained-span y quirk (§4.3 step 3); reproduce only if it changes a result.
pub fn outlines_overlap(a: &Shape<'_>, b: &Shape<'_>, wrap_width: i32) -> bool {
    let _ = (a, b, wrap_width);
    todo!("W1 fly-and-world §4.3")
}
