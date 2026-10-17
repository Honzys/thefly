//! W3. The AI farmer in the shops: the goodies-shop plan and its timed replay (ai-and-rules §3),
//! and trap planning, placement and replay (ai-and-rules §4.4).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::match_state::{MatchState, TrapSlot};
use crate::world::ai::AiLevel;

/// The AI presses one button per expiry of this period.
pub const STEP_MS: u64 = 700;
/// The goodie split loop's cap (the original's own bound).
pub const GOODIE_LOOPS: u32 = 1000;
pub const TRAP_X_ATTEMPTS: u32 = 200;
/// The shop's x range is 0..760.
pub const SHOP_X_RANGE: i32 = 760;
/// Replay timings: select, press, placed, next.
pub const REPLAY_SELECT_MS: u64 = 800;
pub const REPLAY_PRESS_MS: u64 = 500;
pub const REPLAY_PLACED_MS: u64 = 1000;
pub const REPLAY_NEXT_MS: u64 = 800;
/// UB guard: the type-pick retry in §4.4 step 2 ends by argument, not by a bound; the port caps
/// it (the argument says it is never reached).
pub const TRAP_PICK_CAP: u32 = 10_000;
/// UB guard: the order permutation draws `rand % 5` until an unused id comes up; capped.
pub const ORDER_DRAW_CAP: u32 = 10_000;

/// `FUN_00440ac9`'s profile bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopProfile {
    pub reserve_pct: i32,
    pub reserve_jitter: i32,
    pub token_pct: i32,
    pub token_jitter: i32,
    /// Triple, Nuclear, Boot in %.
    pub split: [i32; 3],
    /// Trap gate `c`.
    pub trap_c: i32,
}

/// Indexed by [`AiLevel`] as usize.
pub const SHOP_PROFILES: [ShopProfile; 3] = [
    ShopProfile {
        reserve_pct: 70,
        reserve_jitter: 15,
        token_pct: 70,
        token_jitter: 15,
        split: [34, 33, 33],
        trap_c: 30,
    },
    ShopProfile {
        reserve_pct: 80,
        reserve_jitter: 10,
        token_pct: 80,
        token_jitter: 20,
        split: [40, 30, 30],
        trap_c: 50,
    },
    ShopProfile {
        reserve_pct: 95,
        reserve_jitter: 5,
        token_pct: 85,
        token_jitter: 5,
        split: [50, 25, 25],
        trap_c: 70,
    },
];

/// Planned counts by shop id 1..=5 (level, tokens, Triple, Nuclear, Boot), stored at index id − 1.
pub type Plan = [i32; 5];

/// A shop button press the AI makes; the shop executes it as if the farmer pressed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Move the cursor to this button and press it (0..=4 items, 5 done).
    Button(usize),
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodiesShopAi {
    pub plan: Plan,
    /// Ids 1..=5 in visit order.
    pub order: [usize; 5],
    pub next: usize,
    pub timer: Timer,
    /// Paused until the farmer's payout tally finishes.
    pub paused: bool,
}

impl GoodiesShopAi {
    /// §3.1 plan, then the §3.2 order; the timer is armed at 700 ms.
    pub fn new(level: AiLevel, m: &MatchState, cx: &mut Ctx) -> Self {
        let _ = (level, m, cx);
        todo!("W3 ai-and-rules §3.1, §3.2")
    }

    pub fn tick(&mut self, cx: &mut Ctx) -> Option<Press> {
        let _ = cx;
        todo!("W3 ai-and-rules §3.2")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayStep {
    Select,
    Press,
    Placed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrapsShopAi {
    /// Accepted traps to replay; empty when the gate said "keep the tokens".
    pub queue: Vec<TrapSlot>,
    pub next: usize,
    pub step: ReplayStep,
    pub timer: Timer,
}

/// What the traps shop does on behalf of the AI this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapAction {
    Select(usize),
    Press,
    Place(TrapSlot),
    Done,
}

impl TrapsShopAi {
    /// Gate, plan types, place with up to 200 attempts each, copy out and clear (§4.4 1–4).
    pub fn new(level: AiLevel, m: &mut MatchState, cx: &mut Ctx) -> Self {
        let _ = (level, m, cx);
        todo!("W3 ai-and-rules §4.4")
    }

    pub fn tick(&mut self, cx: &mut Ctx) -> Option<TrapAction> {
        let _ = cx;
        todo!("W3 ai-and-rules §4.4 step 4")
    }
}
