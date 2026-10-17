//! The one simulated clock. Every millisecond figure in the spec reads [`Clock::now_ms`].
//!
//! `now_ms = tick * 1000 / 65`, truncated. Consecutive ticks are 15 or 16 ms apart. A timer
//! fires on the first tick whose `now_ms` reaches its deadline, so a duration that is not a whole
//! number of ticks (700 ms = 45.5 ticks) fires up to one tick late. Re-arming starts from the
//! tick it fired on, as the original's poll-and-rearm did, so the lateness repeats per period.

/// Simulation rate, the original's frame timer (`game-spec.md` §5.1).
pub const TICK_HZ: u64 = 65;

/// Physics time step in seconds. The original measured it per frame; the port fixes it.
pub const DT: f64 = 1.0 / TICK_HZ as f64;

/// The `fps` argument of the per-frame mover integrator (`fly-and-world.md` §4.1).
pub const FPS: i32 = TICK_HZ as i32;

/// Milliseconds of simulated time at the start of `tick`.
pub const fn ms_at(tick: u64) -> u64 {
    tick * 1000 / TICK_HZ
}

/// First tick whose [`ms_at`] is at least `ms`.
pub const fn first_tick_at(ms: u64) -> u64 {
    (ms * TICK_HZ).div_ceil(1000)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clock {
    tick: u64,
}

impl Clock {
    /// Index of the tick being simulated; 0 for the first.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn now_ms(&self) -> u64 {
        ms_at(self.tick)
    }

    pub fn advance(&mut self) {
        self.tick += 1;
    }
}

/// A deadline plus a period, as in `ai-and-rules.md` §0 ("Timers").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timer {
    deadline_ms: Option<u64>,
    period_ms: u64,
}

impl Timer {
    pub const UNARMED: Timer = Timer {
        deadline_ms: None,
        period_ms: 0,
    };

    /// A timer armed at `now` for `ms`.
    pub fn armed(now: u64, ms: u64) -> Self {
        let mut t = Self::UNARMED;
        t.arm(now, ms);
        t
    }

    /// Deadline = now + ms, period = ms.
    pub fn arm(&mut self, now: u64, ms: u64) {
        self.deadline_ms = Some(now + ms);
        self.period_ms = ms;
    }

    pub fn disarm(&mut self) {
        self.deadline_ms = None;
    }

    pub fn is_armed(&self) -> bool {
        self.deadline_ms.is_some()
    }

    /// Armed and `now` has reached the deadline. Stays true until re-armed or disarmed.
    pub fn expired(&self, now: u64) -> bool {
        self.deadline_ms.is_some_and(|d| now >= d)
    }

    /// Like [`Timer::expired`]; on expiry the next deadline is `now + period`.
    pub fn expired_rearm(&mut self, now: u64) -> bool {
        let fired = self.expired(now);
        if fired {
            self.deadline_ms = Some(now + self.period_ms);
        }
        fired
    }

    /// The AI helper `Tick(timer, ms)` (`FUN_0043e757`): arms an unarmed timer and returns false,
    /// otherwise [`Timer::expired_rearm`].
    pub fn ai_tick(&mut self, now: u64, ms: u64) -> bool {
        if !self.is_armed() {
            self.arm(now, ms);
            return false;
        }
        self.expired_rearm(now)
    }

    /// Milliseconds left, 0 once expired or when unarmed.
    pub fn remaining_ms(&self, now: u64) -> u64 {
        self.deadline_ms.map_or(0, |d| d.saturating_sub(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_runs_at_65_hz_with_truncated_ms() {
        assert_eq!([ms_at(0), ms_at(1), ms_at(2), ms_at(3)], [0, 15, 30, 46]);
        assert_eq!(ms_at(65), 1000);
        assert_eq!(ms_at(65 * 120), 120_000); // one 2-minute period
        let mut c = Clock::default();
        for _ in 0..65 {
            c.advance();
        }
        assert_eq!((c.tick(), c.now_ms()), (65, 1000));
    }

    #[test]
    fn first_tick_at_inverts_ms_at() {
        for ms in [0, 1, 15, 16, 700, 1000, 35_000] {
            let t = first_tick_at(ms);
            assert!(ms_at(t) >= ms);
            assert!(t == 0 || ms_at(t - 1) < ms);
        }
    }

    /// The accepted divergence: 700 ms is 45.5 ticks, so the AI's 700 ms shop replay fires on
    /// tick 46 (707 ms), and re-arming from there puts the second press on tick 92.
    #[test]
    fn a_700_ms_period_fires_on_whole_ticks() {
        let mut t = Timer::UNARMED;
        let fired: Vec<u64> = (0..=140)
            .filter(|&tick| t.ai_tick(ms_at(tick), 700))
            .collect();
        assert_eq!(fired, [46, 92, 138]);
    }

    #[test]
    fn timer_expiry_holds_until_rearmed() {
        let mut t = Timer::armed(0, 100);
        assert!(!t.expired(99));
        assert!(t.expired(100) && t.expired(500));
        assert_eq!(t.remaining_ms(40), 60);
        assert!(t.expired_rearm(120));
        assert!(!t.expired(219) && t.expired(220));
        t.disarm();
        assert!(!t.expired(10_000));
    }
}
