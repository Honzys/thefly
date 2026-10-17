//! Fixed-timestep accumulator: simulation ticks at a fixed rate, rendering gets the leftover fraction.

use std::time::Duration;

/// Most ticks run in one frame; time beyond that is dropped so a slow frame cannot spiral.
pub const MAX_CATCH_UP_TICKS: u32 = 8;

#[derive(Debug, Clone)]
pub struct Timestep {
    tick: Duration,
    acc: Duration,
}

impl Timestep {
    /// `tick_hz` of 0 is treated as 1.
    pub fn new(tick_hz: u32) -> Self {
        Self {
            tick: Duration::from_secs(1) / tick_hz.max(1),
            acc: Duration::ZERO,
        }
    }

    /// Adds `elapsed` wall time and returns how many ticks to simulate now (at most [`MAX_CATCH_UP_TICKS`]).
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.acc += elapsed;
        let mut ticks = 0;
        while self.acc >= self.tick && ticks < MAX_CATCH_UP_TICKS {
            self.acc -= self.tick;
            ticks += 1;
        }
        if self.acc >= self.tick {
            // drop the backlog, keep only the sub-tick remainder
            let rem = self.acc.as_nanos() % self.tick.as_nanos();
            self.acc = Duration::from_nanos(rem as u64);
        }
        ticks
    }

    /// Fraction of a tick left in the accumulator, in `[0, 1)`; the render interpolation factor.
    pub fn alpha(&self) -> f32 {
        self.acc.as_secs_f32() / self.tick.as_secs_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn runs_whole_ticks_and_keeps_remainder_as_alpha() {
        let mut t = Timestep::new(100); // 10 ms ticks
        assert_eq!(t.advance(MS(25)), 2);
        assert!((t.alpha() - 0.5).abs() < 1e-6);
        assert_eq!(t.advance(MS(5)), 1);
        assert_eq!(t.alpha(), 0.0);
    }

    #[test]
    fn short_frames_accumulate() {
        let mut t = Timestep::new(100);
        assert_eq!(t.advance(MS(4)), 0);
        assert_eq!(t.advance(MS(4)), 0);
        assert_eq!(t.advance(MS(4)), 1);
        assert!((t.alpha() - 0.2).abs() < 1e-6);
    }

    #[test]
    fn caps_catch_up_and_drops_backlog() {
        let mut t = Timestep::new(100);
        assert_eq!(t.advance(Duration::from_secs(2)), MAX_CATCH_UP_TICKS);
        assert!(t.alpha() < 1.0);
        // the dropped 1.92 s must not come back on the next frame
        assert_eq!(t.advance(MS(10)), 1);
    }

    #[test]
    fn sixty_hz_ticks_sixty_times_a_second() {
        let mut t = Timestep::new(60);
        let ticks: u32 = (0..1000).map(|_| t.advance(MS(1))).sum();
        assert_eq!(ticks, 60);
    }
}
