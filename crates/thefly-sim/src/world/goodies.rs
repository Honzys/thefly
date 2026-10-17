//! W2. Goodie selectors and use for both sides, and the mystery effects' expiry
//! (game-spec §4.7, ai-and-rules §5, fly-and-world §2 steps 3, 4 and 15).

use super::World;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::{FarmerPad, FlyPad};
use crate::match_state::MatchState;

/// A mystery effect's timer (both sides).
pub const MYSTERY_MS: u64 = 15_000;
/// After a goodie use or an unpause, the action/swing memory is set to this (blocks until the
/// key is released).
pub const USE_BLOCK: i32 = 10;

/// Selector values 1..=3 for the fly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlyGoodie {
    Turbo,
    SuperFly,
    Midges,
}

/// Selector values 1..=3 for the farmer, also the AI's request codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FarmerGoodie {
    Triple,
    Nuclear,
    Boot,
}

/// A side's active mystery (game+0x40898 for the fly, game+0x40899 for the farmer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mystery {
    #[default]
    None,
    /// Fly: inverted controls. Farmer: the swatter cannot hit.
    First,
    /// Fly: 25 s Turbo. Farmer: 20 s Nuclear.
    Second,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Goodies {
    /// game+0x408a8, 0..=3; 0 is closed.
    pub fly_selector: u8,
    /// game+0x408aa, 0..=3.
    pub farmer_selector: u8,
    pub fly_mystery: Mystery,
    pub fly_mystery_timer: Timer,
    pub farmer_mystery: Mystery,
    pub farmer_mystery_timer: Timer,
    /// The AI's request collected this tick (`FUN_0043f885`), spent by step 4.
    pub farmer_request: Option<FarmerGoodie>,
}

impl World {
    /// Step 3 (`FUN_00404198`): Right Shift cycles; Enter uses when the count > 0 (stardust,
    /// slot 38; Midges also slot 48). Returns the action, cancelled on any use attempt.
    pub fn fly_goodie(
        &mut self,
        pad: &FlyPad,
        action: bool,
        m: &mut MatchState,
        cx: &mut Ctx,
    ) -> bool {
        let _ = (pad, action, m, cx);
        todo!("W2 fly-and-world §2 step 3, game-spec §4.7")
    }

    /// Step 4 (`FUN_004043c1`): the farmer's selector, or the AI's request. Triple and Nuclear
    /// only while Nuclear is off; slot 37.
    pub fn farmer_goodie(&mut self, pad: &FarmerPad, m: &mut MatchState, cx: &mut Ctx) {
        let _ = (pad, m, cx);
        todo!("W2 ai-and-rules §5")
    }

    /// Step 15: fly side ends Turbo when the mystery was Turbo; farmer side lets the swatter hit
    /// again or ends Nuclear.
    pub fn mystery_expiry(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 fly-and-world §2 step 15")
    }
}
