//! W3. Everything that lives for a whole match: the menu's configuration, period/day progression,
//! the economy, the statistics, the shop's trap layout and the winch slots (game-spec §2.4–§2.6,
//! ai-and-rules §9).

use crate::ctx::Ctx;
use crate::economy::Economy;
use crate::stats::Stats;
use crate::world::ai::AiLevel;
use crate::world::traps::TrapKind;

/// Period clock limit in minutes (data+0x26), and in tutorial mode.
pub const PERIOD_MINUTES: u64 = 2;
pub const TUTORIAL_PERIOD_MINUTES: u64 = 3;
/// Trap slots at data+0x2a2; the caps keep the count at 14 or fewer.
pub const TRAP_SLOTS: usize = 15;
/// The kill-type list at data+0x15c… holds 50 entries and is not bounds-checked; index 0 unused.
/// UB guard: kills past the end are counted but their type is not stored.
pub const KILL_LOG_CAP: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Players {
    /// The human is the fly; the AI plays the farmer.
    #[default]
    One,
    Two,
}

/// data+0x24: 1 normal; 2 is Highscore with one player and Extreme with two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchConfig {
    pub players: Players,
    pub mode: Mode,
    pub ai_level: AiLevel,
    /// 2, 4 or 6; 3 in Highscore mode.
    pub days: u8,
    /// Options toggle; makes periods 3 minutes and uses the tutorial inventories.
    pub tutorial: bool,
}

impl Default for MatchConfig {
    /// Menu defaults: 2 days, 1 player, mode 1, AI level 1 (game-spec §2.3).
    fn default() -> Self {
        Self {
            players: Players::One,
            mode: Mode::Normal,
            ai_level: AiLevel::Normal,
            days: 2,
            tutorial: false,
        }
    }
}

impl MatchConfig {
    /// data+0x28: two players and mode 2.
    pub fn extreme(&self) -> bool {
        self.players == Players::Two && self.mode == Mode::Special
    }

    /// `FUN_0040aac0` == 2: one player and mode 2.
    pub fn highscore(&self) -> bool {
        self.players == Players::One && self.mode == Mode::Special
    }
}

/// data+0x20, as meal bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meal {
    Breakfast = 1,
    Lunch = 2,
    Dinner = 4,
}

/// data+0x2c: 1 inside, 2 outside; extreme mode uses the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Inside,
    Outside,
    Arena,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// None before the first period (data+0x20 == 0).
    pub meal: Option<Meal>,
    pub day: u8,
    /// None before the first period (data+0x2c == 0).
    pub outside: Option<bool>,
}

/// `FUN_0040b7b2`'s result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advance {
    Next,
    Over,
}

/// The period clock (`FUN_0044214e`, `FUN_004421dd`): elapsed = trunc((now − start − paused)
/// / 1000) s; the period ends when elapsed ≥ minutes × 60.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeriodClock {
    pub start_ms: u64,
    pub paused_ms: u64,
    pub pause_started: Option<u64>,
    pub limit_s: u64,
}

impl PeriodClock {
    pub fn new(now: u64, minutes: u64) -> Self {
        let _ = (now, minutes);
        todo!("W3 ai-and-rules §8.6")
    }

    pub fn elapsed_s(&self, now: u64) -> u64 {
        let _ = now;
        todo!("W3 ai-and-rules §8.6")
    }

    pub fn expired(&self, now: u64) -> bool {
        self.elapsed_s(now) >= self.limit_s
    }

    pub fn pause(&mut self, now: u64) {
        let _ = now;
        todo!("W3 ai-and-rules §8.6")
    }

    pub fn resume(&mut self, now: u64) {
        let _ = now;
        todo!("W3 ai-and-rules §8.6")
    }
}

/// One placed trap in the shop: type and stored x (shop px / 0.152).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrapSlot {
    pub kind: TrapKind,
    pub x: i32,
    pub y: i32,
}

/// The farmer's trap layout for the next period (data+0x2a2, trapCount data+0x2fc).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrapLayout {
    pub slots: Vec<TrapSlot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceError {
    /// Overlaps a trap or a winch slot.
    Blocked,
    /// The type's cap is reached, or tokens do not cover the cost.
    Refused,
}

impl TrapLayout {
    /// `FUN_0040bd61` + `FUN_00425456`: validate shop x against traps and winch slots, store,
    /// deduct the cost (floor 0). No 15-slot guard; the caps bound the count.
    pub fn place(
        &mut self,
        kind: TrapKind,
        shop_x: i32,
        winch: &[i32; 3],
        tokens: &mut i32,
    ) -> Result<(), PlaceError> {
        let _ = (kind, shop_x, winch, tokens);
        todo!("W3 ai-and-rules §4.3")
    }

    pub fn count(&self, kind: TrapKind) -> usize {
        let _ = kind;
        todo!("W3 ai-and-rules §4.3; the two fans share their cap")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchState {
    pub config: MatchConfig,
    pub progress: Progress,
    pub economy: Economy,
    pub stats: Stats,
    pub traps: TrapLayout,
    /// The three winch slots (lift anchors), stored units.
    pub winch: [i32; 3],
    /// Picked per match when music is on.
    pub music_game2: bool,
}

impl MatchState {
    /// `FUN_00408113` steps 1–3: reset, starting inventories, extreme setup, music pick.
    pub fn new(config: MatchConfig, music_on: bool, cx: &mut Ctx) -> Self {
        let _ = (config, music_on, cx);
        todo!("W3 game-spec §2.4, §5.7")
    }

    /// `FUN_0040b7b2`.
    pub fn advance(&mut self, cx: &mut Ctx) -> Advance {
        let _ = cx;
        todo!("W3 ai-and-rules §9.1")
    }

    /// `FUN_0040b922`: a bonus round follows this period.
    pub fn bonus_due(&self) -> bool {
        todo!("W3 ai-and-rules §9.1")
    }

    /// `FUN_0040b94c`: the match ends after this period (and its bonus round, if due).
    pub fn over_after_period(&self) -> bool {
        todo!("W3 ai-and-rules §9.1")
    }

    pub fn scene(&self) -> Scene {
        todo!("W3 game-spec §6.2")
    }

    /// Period clock limit in minutes.
    pub fn period_minutes(&self) -> u64 {
        if self.config.tutorial {
            TUTORIAL_PERIOD_MINUTES
        } else {
            PERIOD_MINUTES
        }
    }

    /// `FUN_0040bc5e`: kills + 1, stats kills + 1, kill type by this life's hits (3/2/1).
    pub fn record_kill(&mut self, hits: i32) {
        let _ = hits;
        todo!("W3 ai-and-rules §7.6")
    }

    /// Delivery (ai-and-rules §7.5): `saved[type] += 1`, stats delivered and price total.
    pub fn record_delivery(&mut self, kind: usize) {
        let _ = kind;
        todo!("W3 ai-and-rules §7.5; open item: the price statistic formula")
    }
}
