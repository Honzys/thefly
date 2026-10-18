//! W3. The bourse / price board (0x50): count-up bars, totals, leader badge, blinking column
//! (screens-and-rendering §3.3). It writes no game data; `rand` is used for visuals only.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{PlayParams, SoundOp, SoundSlot};
use crate::input::{KeyState, Scancode};
use crate::match_state::MatchState;
use crate::screen::{Gameplay, MatchFlow};

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
    /// The looping tick sound, slot 49 or 50.
    pub ring: SoundSlot,
    /// Columns whose object is in play (drawn bright).
    pub in_play: [bool; COLUMNS],
    /// The in-play column with the highest price; it blinks once its bar is done.
    pub leader: Option<usize>,
}

impl Bourse {
    pub fn new(cx: &mut Ctx) -> Self {
        let ring = SoundSlot(SoundSlot::RING_A.0 + cx.rng.r(2) as u8);
        let freq = 21_500 + cx.rng.r(500) as u32;
        cx.sound(
            ring,
            SoundOp::Loop(PlayParams {
                freq_hz: Some(freq),
                ..PlayParams::default()
            }),
        );
        Self {
            bars: [0; COLUMNS],
            grow: Timer::armed(cx.now, GROW_MS),
            blink: Timer::armed(cx.now, BLINK_MS),
            blink_on: false,
            ticking: true,
            ring,
            in_play: [false; COLUMNS],
            leader: None,
        }
    }

    /// Enter → gameplay ([`crate::screen::Gameplay::enter`]).
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let e = &m.economy;
        let target = |i: usize| e.food_prices.get(COLUMN_OBJECT[i]).map_or(0, |&p| p as i32);
        // shortcut: open item O18, the scene mask at 0x4672e8 is not decoded; the last period's
        // present types stand in for it.
        self.in_play = std::array::from_fn(|i| e.present.get(COLUMN_OBJECT[i]) == Some(&true));
        self.leader = (0..COLUMNS)
            .filter(|&i| self.in_play[i])
            .max_by_key(|&i| target(i));
        if self.grow.expired_rearm(cx.now) {
            for (i, bar) in self.bars.iter_mut().enumerate() {
                *bar += i32::from(*bar < target(i));
            }
        }
        let done = (0..COLUMNS).all(|i| self.bars[i] >= target(i));
        if done && self.ticking {
            self.ticking = false;
            cx.sound(self.ring, SoundOp::Stop);
        }
        if done && self.blink.expired_rearm(cx.now) {
            self.blink_on = !self.blink_on;
        }
        if !keys.pressed(Scancode::RETURN) {
            return MatchFlow::Stay;
        }
        if self.ticking {
            cx.sound(self.ring, SoundOp::Stop);
        }
        Gameplay::enter(m, cx)
    }
}
