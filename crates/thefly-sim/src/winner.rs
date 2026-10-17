//! W3. The winner screen (0x78): top-up, winner rule, count-up and trophy choreography
//! (ai-and-rules §8.4, screens-and-rendering §5.1).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::KeyState;
use crate::match_state::MatchState;
use crate::screen::{Fade, MatchFlow};

/// Counters add this per tick.
pub const COUNT_STEP: i32 = 10;
pub const BAR_MAX: i32 = 413;
pub const INTRO_WAIT_MS: u64 = 2000;
pub const KEY_ARM_MS: u64 = 1500;
pub const TROPHY_FLOOR: i32 = 587;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    FadeIn,
    CountUp,
    TrophyFall,
    Hops,
    FadeOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Winner {
    /// The fly wins ties.
    pub fly_won: bool,
    pub fly_score: i32,
    pub farmer_score: i32,
    pub stage: Stage,
    pub fade: Fade,
    pub wait: Timer,
    pub fly_counter: i32,
    pub farmer_counter: i32,
    pub trophy_y: f64,
    pub trophy_vy: f64,
    pub loser_crushed: bool,
    pub keys_armed: Timer,
    pub hop: Timer,
}

impl Winner {
    /// Applies [`crate::economy::Economy::top_up`] and decides the winner.
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        let _ = (m, cx);
        todo!("W3 ai-and-rules §8.4")
    }

    /// Enter/Esc → stats.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let _ = (keys, m, cx);
        todo!("W3 screens-and-rendering §5.1")
    }
}
