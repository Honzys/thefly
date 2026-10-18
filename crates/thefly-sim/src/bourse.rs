//! W3. The bourse / price board (0x50): count-up bars, totals, leader badge, blinking column
//! (screens-and-rendering §3.3). It writes no game data; `rand` is used for visuals only.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::KeyState;
use crate::match_state::MatchState;
use crate::screen::MatchFlow;

pub const COLUMNS: usize = 22;
/// Column → object index (`0x4672b8`).
pub const COLUMN_OBJECT: [usize; COLUMNS] = [
    3, 4, 20, 8, 17, 2, 1, 15, 7, 11, 5, 13, 18, 19, 12, 16, 10, 0, 21, 6, 14, 9,
];
/// Bars grow by 1 on every edge of this timer.
pub const GROW_MS: u64 = 10;
pub const BLINK_MS: u64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bourse {
    /// Current bar heights; targets are trunc(price).
    pub bars: [i32; COLUMNS],
    pub grow: Timer,
    pub blink: Timer,
    pub blink_on: bool,
    pub ticking: bool,
}

impl Bourse {
    pub fn new(cx: &mut Ctx) -> Self {
        let _ = cx;
        todo!("W3 screens-and-rendering §3.3")
    }

    /// Enter → gameplay ([`crate::screen::Gameplay::enter`]).
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let _ = (keys, m, cx);
        todo!("W3 screens-and-rendering §3.3; open item O18: the scene class mask at 0x4672e8")
    }
}
