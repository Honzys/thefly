//! W3. The goodies shop (0x1e) with payout, purchases and the side switch, and the farmer's traps
//! shop (0x28) (ai-and-rules §7.2, §7.3, §4.2, §4.3; screens-and-rendering §3.1, §3.2).
//! A confirm pressed during the conveyor animation is dropped, not queued (gaps §9.2, §11.6 [I]).
//! Open item O4 (shop render). The conveyor delay between a purchase and its
//! grant is a named constant W3 must pick and record; the docs give no duration.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::economy::Item;
use crate::input::KeyState;
use crate::match_state::MatchState;
use crate::screen::{Fade, MatchFlow};
use crate::shop_ai::{GoodiesShopAi, TrapsShopAi};
use crate::world::traps::TrapKind;

/// Item buttons 0..=4, then "done".
pub const DONE_BUTTON: usize = 5;
pub const BUTTONS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopSide {
    Farmer,
    Fly,
}

/// The payout tally: one type or one kill per conveyor cycle; a key skips to the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tally {
    pub side: ShopSide,
    /// Amounts still to credit, in order.
    pub pending: Vec<i32>,
    pub cycle: Timer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoodiesShop {
    /// data+0x22fc; the farmer shops first.
    pub side: ShopSide,
    /// Second side's turn.
    pub second: bool,
    pub cursor: usize,
    pub tally: Option<Tally>,
    /// The bought item riding the conveyor; granted when it reaches the bottom.
    pub conveyor: Option<(Item, Timer)>,
    pub ai: Option<GoodiesShopAi>,
    pub fade: Fade,
    pub leaving: bool,
}

impl GoodiesShop {
    /// Fade in, payout tallies, the AI plan when one player.
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        let _ = (m, cx);
        todo!("W3 ai-and-rules §7.3, §3; screens-and-rendering §3.1")
    }

    /// Result 1 → traps shop, 2 → bourse, 0 → quit dialog (game-spec §2.2).
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let _ = (keys, m, cx);
        todo!("W3 ai-and-rules §7.2; done with 0 tokens re-rolls the winch and clears traps")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrapsShop {
    /// Buttons: flypaper, fan (left/right by placement), zapper, done.
    pub cursor: usize,
    /// The trap being positioned and its shop x.
    pub carrying: Option<(TrapKind, i32)>,
    pub ai: Option<TrapsShopAi>,
    pub fade: Fade,
    pub leaving: bool,
}

impl TrapsShop {
    /// Clears the trap slots on open (§4.2).
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        let _ = (m, cx);
        todo!(
            "W3 ai-and-rules §4.2–§4.4; human placement controls gaps §8.2 and §1.3; render is open item O4"
        )
    }

    /// Result 1 → bourse.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let _ = (keys, m, cx);
        todo!("W3 ai-and-rules §4.3")
    }
}
