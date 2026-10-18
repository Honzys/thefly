//! Interpolation between the previous and the current 65 Hz state, so motion is smooth at any
//! refresh rate. Positions only; anything discrete (frames, flags, text) shows the current tick.

/// A per-tick move longer than this is a teleport (respawn, a new period's world, a screen
/// change), drawn at the new place instead of sliding there. The fastest regular mover, the fly
/// on turbo, covers well under this per tick.
pub const SNAP_PX: f32 = 150.0;

/// Interpolation for one frame: `alpha` in `[0, 1]` between the previous and the current tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lerp {
    pub alpha: f32,
}

impl Lerp {
    /// `prev → cur`; `None` (no previous value) or a teleport gives `cur`.
    pub fn f(self, prev: Option<f64>, cur: f64) -> f32 {
        let cur = cur as f32;
        let Some(prev) = prev.map(|p| p as f32) else {
            return cur;
        };
        if (cur - prev).abs() > SNAP_PX {
            return cur;
        }
        prev + (cur - prev) * self.alpha
    }

    pub fn i(self, prev: Option<i32>, cur: i32) -> f32 {
        self.f(prev.map(f64::from), f64::from(cur))
    }

    /// On a ring of `ring` units (the 4000-px farmer ring, a layer's scroll), taking the short
    /// way round, so a wrap keeps moving forward. The result is in `[0, ring)`.
    pub fn ring(self, prev: Option<f64>, cur: f64, ring: f64) -> f32 {
        let Some(prev) = prev else {
            return cur.rem_euclid(ring) as f32;
        };
        let mut d = (cur - prev).rem_euclid(ring);
        if d > ring / 2.0 {
            d -= ring;
        }
        if d.abs() > f64::from(SNAP_PX) {
            return cur.rem_euclid(ring) as f32;
        }
        (prev + d * f64::from(self.alpha)).rem_euclid(ring) as f32
    }
}

/// Source x of a scenery layer (gaps §4.3: src = trunc(acc × factor), wrapped to the layer width),
/// with the accumulator interpolated so the scroll is sub-pixel smooth.
pub fn layer_src(l: Lerp, prev_acc: Option<i64>, acc: i64, factor: f64, width: i32) -> f32 {
    let width = f64::from(width.max(1));
    let src = |a: i64| a as f64 * factor;
    l.ring(prev_acc.map(src), src(acc), width)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 144 Hz frames over 65 Hz ticks: each frame's alpha is how far it sits into the tick.
    fn frames_144hz(ticks: u32) -> impl Iterator<Item = (u32, f32)> {
        const FRAMES_PER_SECOND: f64 = 144.0;
        const TICKS_PER_SECOND: f64 = 65.0;
        let frames = (f64::from(ticks) / TICKS_PER_SECOND * FRAMES_PER_SECOND) as u32;
        (0..frames).map(|f| {
            let t = f64::from(f) / FRAMES_PER_SECOND * TICKS_PER_SECOND;
            (t.floor() as u32, t.fract() as f32)
        })
    }

    /// Unwraps a sequence of positions on a ring and asserts it never goes backwards.
    fn assert_monotonic_on_ring(xs: &[f32], ring: f32) {
        let mut unwrapped = 0.0f32;
        for w in xs.windows(2) {
            let step = (w[1] - w[0]).rem_euclid(ring);
            assert!(step < ring / 2.0, "went backwards: {} -> {}", w[0], w[1]);
            unwrapped += step;
        }
        assert!(unwrapped > 0.0);
    }

    #[test]
    fn fly_at_constant_vx_is_monotonic_at_144hz_across_the_4000px_wrap() {
        // the farmer ring position the camera follows: 9 px per tick, starting near the wrap
        const VX_PER_TICK: i64 = 9;
        const START: i64 = 3900;
        let pos = |tick: u32| (START + VX_PER_TICK * i64::from(tick)).rem_euclid(4000);
        let xs: Vec<f32> = frames_144hz(40)
            .map(|(tick, alpha)| {
                let prev = pos(tick) as f64;
                let cur = pos(tick + 1) as f64;
                Lerp { alpha }.ring(Some(prev), cur, 4000.0)
            })
            .collect();
        assert!(xs.iter().any(|&x| x < 100.0), "never crossed the wrap");
        assert_monotonic_on_ring(&xs, 4000.0);
    }

    #[test]
    fn layer_scroll_is_monotonic_across_its_wrap() {
        // the 1.25 table layer (900 px wide) under a camera moving 7 px per tick
        let acc = |tick: u32| 700 + 7 * i64::from(tick);
        let xs: Vec<f32> = frames_144hz(65)
            .map(|(tick, alpha)| {
                layer_src(Lerp { alpha }, Some(acc(tick)), acc(tick + 1), 1.25, 900)
            })
            .collect();
        assert!(xs.windows(2).any(|w| w[1] < w[0]), "never wrapped");
        assert_monotonic_on_ring(&xs, 900.0);
    }

    #[test]
    fn plain_lerp_moves_between_ticks_and_snaps_on_teleport() {
        let l = Lerp { alpha: 0.25 };
        assert_eq!(l.i(Some(100), 140), 110.0);
        assert_eq!(l.i(None, 140), 140.0);
        assert_eq!(l.i(Some(100), 700), 700.0);
    }

    #[test]
    fn ring_takes_the_short_way() {
        let l = Lerp { alpha: 0.5 };
        assert_eq!(l.ring(Some(3990.0), 10.0, 4000.0), 0.0);
        assert_eq!(l.ring(Some(10.0), 3990.0, 4000.0), 0.0);
    }
}
