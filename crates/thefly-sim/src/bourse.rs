//! W3. The bourse / price board (0x50): count-up bars, totals, leader badge, blinking column
//! (screens-and-rendering §3.3). It writes no game data; `rand` is used for visuals only.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{PlayParams, SoundOp, SoundSlot};
use crate::input::{KeyState, Scancode};
use crate::match_state::{MatchState, Meal};
use crate::screen::{Gameplay, MatchFlow};

pub const COLUMNS: usize = 22;
/// Column → object index (`0x4672b8`).
pub const COLUMN_OBJECT: [usize; COLUMNS] = [
    3, 4, 20, 8, 17, 2, 1, 15, 7, 11, 5, 13, 18, 19, 12, 16, 10, 0, 21, 6, 14, 9,
];
/// Column → the meals it is served at (`0x4672e8`): 1 breakfast, 2 lunch, 4 dinner.
pub const COLUMN_MEALS: [u8; COLUMNS] = [
    4, 4, 2, 4, 2, 4, 4, 1, 6, 1, 7, 1, 2, 2, 1, 2, 1, 4, 2, 4, 1, 4,
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
    /// Columns served at the next meal (drawn bright).
    pub in_play: [bool; COLUMNS],
    /// The in-play column with the highest price (the later one on a tie); it blinks once its
    /// bar is done.
    pub leader: Option<usize>,
}

/// `FUN_0042008e`: the next meal's bit from the period just played (data+0x20); dinner and
/// "before the first period" both give breakfast.
fn next_meal_bit(meal: Option<Meal>) -> u8 {
    match meal {
        Some(Meal::Breakfast) => Meal::Lunch as u8,
        Some(Meal::Lunch) => Meal::Dinner as u8,
        Some(Meal::Dinner) | None => Meal::Breakfast as u8,
    }
}

impl Bourse {
    /// The in-play columns and the leader are fixed here, at init.
    pub fn new(m: &MatchState, cx: &mut Ctx) -> Self {
        let ring = SoundSlot(SoundSlot::RING_A.0 + cx.rng.r(2) as u8);
        let freq = 21_500 + cx.rng.r(500) as u32;
        cx.sound(
            ring,
            SoundOp::Loop(PlayParams {
                freq_hz: Some(freq),
                ..PlayParams::default()
            }),
        );
        let bit = next_meal_bit(m.progress.meal);
        let in_play: [bool; COLUMNS] = std::array::from_fn(|i| COLUMN_MEALS[i] & bit != 0);
        let leader = (0..COLUMNS)
            .filter(|&i| in_play[i])
            .max_by_key(|&i| target(m, i));
        Self {
            bars: [0; COLUMNS],
            grow: Timer::armed(cx.now, GROW_MS),
            blink: Timer::armed(cx.now, BLINK_MS),
            blink_on: false,
            ticking: true,
            ring,
            in_play,
            leader,
        }
    }

    /// Enter → gameplay ([`crate::screen::Gameplay::enter`]).
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        if self.grow.expired_rearm(cx.now) {
            for (i, bar) in self.bars.iter_mut().enumerate() {
                *bar += i32::from(*bar < target(m, i));
            }
        }
        let done = (0..COLUMNS).all(|i| self.bars[i] >= target(m, i));
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

/// A column's bar target: trunc(price).
fn target(m: &MatchState, i: usize) -> i32 {
    m.economy
        .food_prices
        .get(COLUMN_OBJECT[i])
        .map_or(0, |&p| p as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_state::{MatchConfig, Players};
    use crate::screen::rig::Rig;

    /// answers.md "Bourse in-play columns": after breakfast the lunch columns (bit 2) are in
    /// play; on a price tie the later column leads.
    #[test]
    fn the_in_play_columns_are_the_next_meals_and_the_later_tie_leads() {
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = rig.cx(|cx| MatchState::new(config, false, cx));
        let lit = |b: &Bourse| (0..COLUMNS).filter(|&i| b.in_play[i]).collect::<Vec<_>>();
        let b = rig.cx(|cx| Bourse::new(&m, cx));
        assert_eq!(lit(&b), [7, 9, 10, 11, 14, 16, 20]);
        rig.cx(|cx| m.advance(cx));
        assert_eq!(m.progress.meal, Some(Meal::Breakfast));
        m.economy.food_prices.iter_mut().for_each(|p| *p = 10.0);
        let b = rig.cx(|cx| Bourse::new(&m, cx));
        assert_eq!(lit(&b), [2, 4, 8, 10, 12, 13, 15, 18]);
        assert_eq!(b.leader, Some(18));
        m.economy.food_prices[COLUMN_OBJECT[4]] = 11.0;
        assert_eq!(rig.cx(|cx| Bourse::new(&m, cx)).leader, Some(4));
        rig.cx(|cx| m.advance(cx));
        let b = rig.cx(|cx| Bourse::new(&m, cx));
        assert_eq!(lit(&b), [0, 1, 3, 5, 6, 8, 10, 17, 19, 21]);
    }
}
