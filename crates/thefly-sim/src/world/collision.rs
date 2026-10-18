//! W1. Boxes and `.bor` outline overlap (fly-and-world §4.2–§4.5). Pure functions; every other
//! worker uses these instead of writing its own test.

use super::SCREEN_W;
use crate::data::{Outline, Size};

/// The fly outline's edge rows and x span, in `.bor` units (FW §4.2).
const FLY_TOP: usize = 29;
const FLY_BOTTOM: usize = 84;
const FLY_LEFT: u16 = 15;
const FLY_RIGHT: u16 = 45;

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

    /// Strict box overlap. The flying-goodie catch and the midge contact test are inclusive on
    /// both bounds (gaps §5.1, §8.1); those use [`Rect::catches`].
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }

    /// The flying-goodie catch: goodie x ∈ [px − gw, px + pw] and y ∈ [py − gh, py + ph].
    pub fn catches(&self, goodie: &Rect) -> bool {
        (self.x - goodie.w..=self.x + self.w).contains(&goodie.x)
            && (self.y - goodie.h..=self.y + self.h).contains(&goodie.y)
    }
}

/// The fly's fixed outline (`FUN_0042d67c`): rows 29 and 84 hold 15..=45, rows 30..=83 hold 15
/// and 45, in 2-px units.
pub fn fly_outline() -> Outline {
    let rows = (0..=FLY_BOTTOM)
        .map(|r| match r {
            FLY_TOP | FLY_BOTTOM => (FLY_LEFT..=FLY_RIGHT).collect(),
            r if r > FLY_TOP && r < FLY_BOTTOM => vec![FLY_LEFT, FLY_RIGHT],
            _ => Vec::new(),
        })
        .collect();
    Outline { rows }
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
/// row.
///
/// answers.md "Outline collision: the shifted y quirk never changes a result": the plain row
/// test is exact. The original's straddle is x + w ≥ wrap; `>` here differs only at
/// x + w == wrap, where the strict box test rejects the shifted pair anyway.
pub fn outlines_overlap(a: &Shape<'_>, b: &Shape<'_>, wrap_width: i32) -> bool {
    let straddles = |s: &Shape<'_>| s.x + s.size.w > wrap_width;
    let shift = straddles(a) || straddles(b);
    let ring_x = |x: i32| {
        if shift && x < SCREEN_W {
            x + wrap_width
        } else {
            x
        }
    };
    let (ax, bx) = (ring_x(a.x), ring_x(b.x));
    let a_box = Rect::new(ax, a.y, a.size.w, a.size.h);
    if !a_box.overlaps(&Rect::new(bx, b.y, b.size.w, b.size.h)) {
        return false;
    }
    let (top, bottom) = (a.y.max(b.y), (a.y + a.size.h).min(b.y + b.size.h));
    (top..bottom).any(|y| {
        let (ra, rb) = (row(a.outline, y - a.y), row(b.outline, y - b.y));
        ra.iter().any(|&va| {
            rb.iter()
                .any(|&vb| i32::from(va) + ax / 2 == i32::from(vb) + bx / 2)
        })
    })
}

/// Row `r` of an outline; rows past its end are empty.
fn row(o: &Outline, r: i32) -> &[u16] {
    usize::try_from(r)
        .ok()
        .and_then(|r| o.rows.get(r))
        .map_or(&[], Vec::as_slice)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLY: Size = Size { w: 120, h: 94 };

    #[test]
    fn box_overlap_is_strict_and_the_catch_is_inclusive() {
        let a = Rect::new(0, 0, 10, 10);
        assert!(a.overlaps(&Rect::new(9, 9, 10, 10)));
        assert!(!a.overlaps(&Rect::new(10, 0, 10, 10)));
        assert!(!a.overlaps(&Rect::new(0, 10, 10, 10)));
        // FW §6.3: goodie x ∈ [px − gw, px + pw], y ∈ [py − gh, py + ph].
        let catcher = Rect::new(100, 100, 20, 40);
        assert!(catcher.catches(&Rect::new(100 - 51, 100 - 50, 51, 50)));
        assert!(catcher.catches(&Rect::new(120, 140, 51, 50)));
        assert!(!catcher.catches(&Rect::new(121, 140, 51, 50)));
        assert!(!catcher.catches(&Rect::new(100, 100 - 51, 51, 50)));
    }

    #[test]
    fn the_fly_outline_is_the_fixed_rectangle() {
        let o = fly_outline();
        let full: Vec<u16> = (15..=45).collect();
        assert_eq!(o.rows[29], full);
        assert_eq!(o.rows[84], full);
        assert!((30..=83).all(|r| o.rows[r] == [15, 45]));
        assert!(o.rows[28].is_empty() && o.rows.get(85).is_none_or(Vec::is_empty));
    }

    fn fly(x: i32, y: i32, outline: &Outline) -> Shape<'_> {
        Shape {
            x,
            y,
            size: FLY,
            outline,
        }
    }

    #[test]
    fn outlines_hit_only_where_edge_points_coincide() {
        let o = fly_outline();
        // Same place: every edge point coincides.
        assert!(outlines_overlap(
            &fly(100, 100, &o),
            &fly(100, 100, &o),
            5000
        ));
        // Shifted 60 px right: the left edge (15 + 160/2 = 95) meets the right edge
        // (45 + 100/2 = 95) in the shared rows.
        assert!(outlines_overlap(
            &fly(100, 100, &o),
            &fly(160, 100, &o),
            5000
        ));
        // Shifted 2 px down and right: B's top edge (world row 131, x 66..=96) crosses A's
        // sides (65 and 95).
        assert!(outlines_overlap(
            &fly(100, 100, &o),
            &fly(102, 102, &o),
            5000
        ));
        // Boxes overlap, outlines are 70 px apart in x: no shared point.
        assert!(!outlines_overlap(
            &fly(100, 100, &o),
            &fly(170, 100, &o),
            5000
        ));
        // No box overlap at all.
        assert!(!outlines_overlap(
            &fly(100, 100, &o),
            &fly(100, 194, &o),
            5000
        ));
    }

    #[test]
    fn a_shape_wholly_inside_another_does_not_register() {
        let o = fly_outline();
        let big = Outline::rect(Size { w: 400, h: 300 });
        let outer = Shape {
            x: 0,
            y: 0,
            size: Size { w: 400, h: 300 },
            outline: &big,
        };
        assert!(!outlines_overlap(&outer, &fly(100, 100, &o), 5000));
    }

    /// FW §4.3 step 1: a box straddling the wrap width moves any x below 800 up by the wrap.
    #[test]
    fn the_wrap_shift_joins_both_ends_of_the_ring() {
        let o = fly_outline();
        // A at 4950 covers 4950..5070; B at 10 is compared at 5010, 60 px right of A.
        assert!(outlines_overlap(
            &fly(4950, 100, &o),
            &fly(10, 100, &o),
            5000
        ));
        // Neither straddles: B at 10 stays at 10, far from A at 4800.
        assert!(!outlines_overlap(
            &fly(4800, 100, &o),
            &fly(10, 100, &o),
            5000
        ));
    }
}
