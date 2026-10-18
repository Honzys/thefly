//! W3. Match statistics counters and the stats page values (ai-and-rules §8.7). Reset per match.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::highscores::HighScoreTable;
use crate::input::KeyState;
use crate::match_state::MatchState;
use crate::screen::{Fade, MatchFlow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub periods: i32,
    pub delivered: i32,
    pub available: i32,
    pub price_total: i32,
    pub swings: i32,
    pub hits: i32,
    pub damage: i32,
    pub kills: i32,
    pub survival_total_ms: u64,
    pub survival_max_ms: u64,
}

/// The values the stats page shows; a zero denominator shows 0.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatsPage {
    pub delivered: i32,
    pub delivered_percent: f64,
    pub max_survival_s: f64,
    pub avg_survival_s: f64,
    pub avg_price: f64,
    pub swings: i32,
    pub kills: i32,
    pub kills_per_period: f64,
    pub hit_ratio_percent: f64,
    pub avg_damage: f64,
}

impl Stats {
    /// Fly life ended: add to the total and the max.
    pub fn record_life(&mut self, ms: u64) {
        self.survival_total_ms += ms;
        self.survival_max_ms = self.survival_max_ms.max(ms);
    }

    pub fn page(&self) -> StatsPage {
        let ratio = |num: f64, den: i32| if den == 0 { 0.0 } else { num / f64::from(den) };
        let average_price = if self.delivered == 0 {
            0
        } else {
            self.price_total / self.delivered
        };
        StatsPage {
            delivered: self.delivered,
            delivered_percent: ratio(f64::from(self.delivered), self.available) * 100.0,
            max_survival_s: self.survival_max_ms as f64 / 1000.0,
            avg_survival_s: self.survival_total_ms as f64 / 1000.0 / f64::from(self.kills + 1),
            // Shown as a whole number (screens-and-rendering §5.2).
            avg_price: f64::from(average_price),
            swings: self.swings,
            kills: self.kills,
            kills_per_period: ratio(f64::from(self.kills), self.periods),
            hit_ratio_percent: ratio(f64::from(self.hits), self.swings) * 100.0,
            avg_damage: ratio(f64::from(self.damage), self.swings),
        }
    }
}

/// The statistics screen (0xaa), with the 1P highscore breakdown (screens-and-rendering §5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct StatsScreen {
    pub page: StatsPage,
    pub fade: Fade,
    pub leaving: bool,
    /// The fade-out runs once this has passed and a key is pressed.
    pub min_show: Timer,
    /// Highscore mode: points counted up by 10 per tick, then kills one per 30 ms.
    pub breakdown: Option<Breakdown>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Breakdown {
    pub points: i32,
    pub kills: i32,
    pub kills_after: Timer,
    pub kill_step: Timer,
    /// First Enter/Esc jumps to the final numbers.
    pub finished: bool,
}

pub const MIN_SHOW_MS: u64 = 3000;
pub const BREAKDOWN_KILLS_AFTER_MS: u64 = 1500;
pub const BREAKDOWN_KILL_STEP_MS: u64 = 30;

impl StatsScreen {
    pub fn new(m: &MatchState, cx: &mut Ctx) -> Self {
        let _ = (m, cx);
        todo!("W3 screens-and-rendering §5.2")
    }

    /// Highscore mode with a qualifying score → high scores with name entry, else → menu.
    pub fn tick(
        &mut self,
        keys: &KeyState,
        m: &MatchState,
        table: &HighScoreTable,
        cx: &mut Ctx,
    ) -> MatchFlow {
        let _ = (keys, m, table, cx);
        todo!("W3 game-spec §2.2 statistics, ai-and-rules §8.5")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lives_add_to_the_total_and_the_max() {
        let mut s = Stats::default();
        s.record_life(4000);
        s.record_life(1500);
        assert_eq!((s.survival_total_ms, s.survival_max_ms), (5500, 4000));
    }

    /// AR §8.7 page values.
    #[test]
    fn the_page_divides_with_zero_for_an_empty_denominator() {
        assert_eq!(Stats::default().page(), StatsPage::default());
        let s = Stats {
            periods: 4,
            delivered: 6,
            available: 24,
            price_total: 200,
            swings: 40,
            hits: 10,
            damage: 300,
            kills: 3,
            survival_total_ms: 60_000,
            survival_max_ms: 30_500,
        };
        let p = s.page();
        assert_eq!(p.delivered_percent, 25.0);
        assert_eq!(p.max_survival_s, 30.5);
        assert_eq!(p.avg_survival_s, 15.0); // 60 s / (3 + 1)
        assert_eq!(p.avg_price, 33.0); // 200 / 6, whole
        assert_eq!(p.kills_per_period, 0.75);
        assert_eq!(p.hit_ratio_percent, 25.0);
        assert_eq!(p.avg_damage, 7.5);
    }
}
