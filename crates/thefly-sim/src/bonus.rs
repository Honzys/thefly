//! W3. The bonus round (0x64): handicaps, countdown, coins, targets, counting, the end tally and
//! the bonus AI (ai-and-rules §8.3). The AI reuses W2's [`Aligner`].
//!
//! Coin entry is decoded (gaps §7.3, `sin`). Open item O3: the layout, the intro, the coins'
//! leftward motion and how the bonus fly and swatter move (proposal: the world's fly physics and
//! farmer movement on the bonus play field).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::KeyState;
use crate::match_state::MatchState;
use crate::screen::MatchFlow;
use crate::world::ai::{AiLevel, Aligner, Controller};

pub const COINS: usize = 20;
pub const TARGETS: usize = 10;
pub const COUNTDOWN_FROM: i32 = 3;
pub const COUNTDOWN_STEP_MS: u64 = 1000;
pub const ROUND_MS: u64 = 35_000;
pub const COIN_SPAWN_MS: u64 = 50;
pub const COIN_VALUE: i32 = 2;
pub const HIT_VALUE: i32 = 10;
pub const RESULT_SOUND_DELAY_MS: u64 = 2000;
/// The bonus AI's target ring.
pub const TARGET_RING: i32 = 920;
/// Handicap edge values for a meter of exactly 0.0 and 1.0.
pub const H_AT_METER_0: f64 = 0.99;
pub const H_AT_METER_1: f64 = 0.001;

/// Bonus AI column (ai-and-rules §8.3 table).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BonusAiParams {
    pub align_tol: i32,
    /// Aim spread as a fraction of the bonus swatter width.
    pub spread_of_width: f64,
    pub react_base: i32,
    pub react_rand: i32,
    pub off_base: i32,
    pub off_rand: i32,
    pub on_base: i32,
    pub on_rand: i32,
    pub p_frenzy: i32,
}

/// Indexed by [`AiLevel`] as usize.
pub const BONUS_AI_PARAMS: [BonusAiParams; 3] = [
    BonusAiParams {
        align_tol: 30,
        spread_of_width: 1.0,
        react_base: 195,
        react_rand: 150,
        off_base: 750,
        off_rand: 1200,
        on_base: 1500,
        on_rand: 2200,
        p_frenzy: 15,
    },
    BonusAiParams {
        align_tol: 20,
        spread_of_width: 0.5,
        react_base: 130,
        react_rand: 100,
        off_base: 500,
        off_rand: 1000,
        on_base: 2000,
        on_rand: 3000,
        p_frenzy: 30,
    },
    BonusAiParams {
        align_tol: 10,
        spread_of_width: 0.35,
        react_base: 100,
        react_rand: 70,
        off_base: 300,
        off_rand: 600,
        on_base: 2500,
        on_rand: 4000,
        p_frenzy: 45,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BonusPhase {
    /// 3, 2, 1 at 1000 ms; play starts when it goes below 0.
    Countdown {
        count: i32,
        next: Timer,
    },
    Play {
        round: Timer,
    },
    /// The AI paused, spawners stopped, scores credited; the result sound after 2000 ms.
    End {
        sound: Timer,
    },
    /// The result dialog waits for a key.
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Piece {
    pub active: bool,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
}

/// A bonus-round actor's position and velocity. Open item O3: its motion rule.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BonusMover {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BonusAi {
    pub params: BonusAiParams,
    pub controller: Controller,
    pub aligner: Aligner,
    pub frenzy: bool,
    pub frenzy_timer: Timer,
    pub paused: bool,
}

impl BonusAi {
    pub fn new(level: AiLevel, h_farmer: f64, cx: &mut Ctx) -> Self {
        let _ = (level, h_farmer, cx);
        todo!("W3 ai-and-rules §8.3 step 7")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BonusRound {
    pub phase: BonusPhase,
    pub h_fly: f64,
    pub h_farmer: f64,
    pub coins: [Piece; COINS],
    pub targets: [Piece; TARGETS],
    pub coin_timer: Timer,
    pub target_timer: Timer,
    /// Coin pattern rate, initially (1 + rand % 60) / 100.
    pub coin_rate: f64,
    pub coin_spawns: i32,
    pub coins_taken: i32,
    pub hits: i32,
    pub fly: BonusMover,
    pub swatter: BonusMover,
    pub ai: Option<BonusAi>,
}

impl BonusRound {
    /// Handicaps from [`crate::economy::Economy::meter`], countdown armed.
    pub fn new(m: &MatchState, cx: &mut Ctx) -> Self {
        let _ = (m, cx);
        todo!("W3 ai-and-rules §8.3 steps 1–2")
    }

    /// After the result: match over → winner, else → goodies shop.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let _ = (keys, m, cx);
        todo!("W3 ai-and-rules §8.3 steps 3–6")
    }
}
