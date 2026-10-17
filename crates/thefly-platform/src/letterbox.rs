//! Aspect-correct scaling of the logical frame into the window, and the inverse for the mouse.

/// Where the logical frame lands inside the window, in window units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Viewport {
    /// Largest rectangle with the aspect of `logical` that fits in `window`, centred.
    pub fn letterbox(window: (u32, u32), logical: (u32, u32)) -> Self {
        let (ww, wh) = (window.0 as f32, window.1 as f32);
        let (lw, lh) = (logical.0.max(1) as f32, logical.1.max(1) as f32);
        let scale = (ww / lw).min(wh / lh);
        let (w, h) = (lw * scale, lh * scale);
        Self {
            x: ((ww - w) / 2.0).floor(),
            y: ((wh - h) / 2.0).floor(),
            w,
            h,
        }
    }

    /// Maps a window position to logical coordinates, clamped to the logical frame
    /// so clicks on the bars land on the nearest edge.
    pub fn to_logical(&self, logical: (u32, u32), pos: (f32, f32)) -> (f32, f32) {
        if self.w <= 0.0 || self.h <= 0.0 {
            return (0.0, 0.0);
        }
        let (lw, lh) = (logical.0 as f32, logical.1 as f32);
        let x = (pos.0 - self.x) * lw / self.w;
        let y = (pos.1 - self.y) * lh / self.h;
        (x.clamp(0.0, lw), y.clamp(0.0, lh))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGICAL: (u32, u32) = (800, 600);

    #[test]
    fn exact_fit_has_no_bars() {
        let v = Viewport::letterbox((1600, 1200), LOGICAL);
        assert_eq!(
            v,
            Viewport {
                x: 0.0,
                y: 0.0,
                w: 1600.0,
                h: 1200.0
            }
        );
    }

    #[test]
    fn wide_window_gets_side_bars() {
        let v = Viewport::letterbox((1920, 1080), LOGICAL);
        assert_eq!(
            v,
            Viewport {
                x: 240.0,
                y: 0.0,
                w: 1440.0,
                h: 1080.0
            }
        );
    }

    #[test]
    fn tall_window_gets_top_and_bottom_bars() {
        let v = Viewport::letterbox((800, 1000), LOGICAL);
        assert_eq!(
            v,
            Viewport {
                x: 0.0,
                y: 200.0,
                w: 800.0,
                h: 600.0
            }
        );
    }

    #[test]
    fn mouse_maps_through_bars_and_scale() {
        let v = Viewport::letterbox((1920, 1080), LOGICAL);
        assert_eq!(v.to_logical(LOGICAL, (240.0, 0.0)), (0.0, 0.0));
        assert_eq!(v.to_logical(LOGICAL, (960.0, 540.0)), (400.0, 300.0));
        assert_eq!(v.to_logical(LOGICAL, (1680.0, 1080.0)), (800.0, 600.0));
    }

    #[test]
    fn mouse_on_bars_clamps_to_edge() {
        let v = Viewport::letterbox((1920, 1080), LOGICAL);
        assert_eq!(v.to_logical(LOGICAL, (10.0, 540.0)), (0.0, 300.0));
        assert_eq!(v.to_logical(LOGICAL, (1900.0, 540.0)), (800.0, 300.0));
    }

    #[test]
    fn degenerate_window_does_not_divide_by_zero() {
        let v = Viewport::letterbox((0, 0), LOGICAL);
        assert_eq!(v.to_logical(LOGICAL, (5.0, 5.0)), (0.0, 0.0));
    }
}
