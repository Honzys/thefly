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
        let _ = ms;
        todo!("W3 ai-and-rules §8.7")
    }

    pub fn page(&self) -> StatsPage {
        todo!("W3 ai-and-rules §8.7 page values; open item: the float constant at 0x45debc")
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
