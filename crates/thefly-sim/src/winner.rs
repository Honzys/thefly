//! W3. The winner screen (0x78): top-up, winner rule, count-up and trophy choreography
//! (ai-and-rules §8.4, screens-and-rendering §5.1).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, Music, PlayParams, SoundOp, SoundSlot};
use crate::input::KeyState;
use crate::match_state::MatchState;
use crate::screen::{FADE_END_MS, Fade, MatchFlow, MatchScreen};
use crate::stats::StatsScreen;

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

/// shortcut: the trophy's gravity mover, its hop and the sprite tops are not decoded (SR §5.1
/// gives the landing only); revisit if `FUN_0044b431` is re-read.
pub const TROPHY_GRAVITY: f64 = 0.5;
pub const TROPHY_START_Y: f64 = -50.0;
const TROPHY_H: i32 = 120;
/// Loser sprite tops: `winnerfly` at y 326, `winnerfarmer` at y 235.
const FLY_TOP: f64 = 326.0;
const FARMER_TOP: f64 = 235.0;
const HOP_REST_MS: u64 = 1000;
const HOP_REST_RAND: i32 = 5000;

impl Winner {
    /// Applies [`crate::economy::Economy::top_up`] and decides the winner.
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        m.economy.top_up();
        let (fly_score, farmer_score) = (m.economy.fly.score, m.economy.farmer.score);
        cx.emit(Event::Music(Music::Winner));
        Self {
            fly_won: fly_score >= farmer_score,
            fly_score,
            farmer_score,
            stage: Stage::FadeIn,
            fade: Fade::new(false, FADE_END_MS),
            wait: Timer::UNARMED,
            fly_counter: 0,
            farmer_counter: 0,
            trophy_y: TROPHY_START_Y,
            trophy_vy: 0.0,
            loser_crushed: false,
            keys_armed: Timer::UNARMED,
            hop: Timer::UNARMED,
        }
    }

    /// x 50 when the fly won, 430 when the farmer won: it falls on the loser.
    pub fn trophy_x(&self) -> i32 {
        if self.fly_won { 50 } else { 430 }
    }

    /// Enter/Esc → stats.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let faded = self.fade.step();
        match self.stage {
            Stage::FadeIn => self.fade_in(faded, cx.now),
            Stage::CountUp => self.count_up(cx),
            Stage::TrophyFall => self.fall(cx),
            Stage::Hops => self.hops(keys, cx),
            Stage::FadeOut if faded => {
                return MatchFlow::Next(MatchScreen::Stats(Box::new(StatsScreen::new(m, cx))));
            }
            Stage::FadeOut => {}
        }
        MatchFlow::Stay
    }

    fn fade_in(&mut self, faded: bool, now: u64) {
        if faded {
            self.wait.arm(now, INTRO_WAIT_MS);
        }
        if self.wait.expired(now) {
            self.stage = Stage::CountUp;
        }
    }

    fn count_up(&mut self, cx: &mut Ctx) {
        self.fly_counter = (self.fly_counter + COUNT_STEP).min(self.fly_score.max(0));
        self.farmer_counter = (self.farmer_counter + COUNT_STEP).min(self.farmer_score.max(0));
        // shortcut: SR §5.1 names no slot for the count tick; the bourse's ring slot is used.
        let freq = 21_500 + cx_rand(cx, 500);
        cx.sound(SoundSlot::RING_A, play_at(freq));
        let (counter, score) = if self.fly_won {
            (self.fly_counter, self.fly_score)
        } else {
            (self.farmer_counter, self.farmer_score)
        };
        if counter >= score {
            self.stage = Stage::TrophyFall;
        }
    }

    fn fall(&mut self, cx: &mut Ctx) {
        let h = cx.data.sizes.get("winnercoupe").map_or(TROPHY_H, |s| s.h);
        self.trophy_vy += TROPHY_GRAVITY;
        self.trophy_y += self.trophy_vy;
        let bottom = self.trophy_y + f64::from(h);
        let loser_top = if self.fly_won { FARMER_TOP } else { FLY_TOP };
        self.loser_crushed |= bottom >= loser_top;
        if bottom < f64::from(TROPHY_FLOOR) {
            return;
        }
        self.trophy_y = f64::from(TROPHY_FLOOR - h);
        cx.sound(
            SoundSlot::TROPHY,
            SoundOp::Play(PlayParams {
                volume_cb: 2000,
                ..PlayParams::default()
            }),
        );
        let cheer = if self.fly_won {
            SoundSlot::CHEER_FLY
        } else {
            SoundSlot::CHEER_FARMER
        };
        cx.emit(Event::play(cheer));
        self.keys_armed.arm(cx.now, KEY_ARM_MS);
        self.arm_hop(cx);
        self.stage = Stage::Hops;
    }

    fn arm_hop(&mut self, cx: &mut Ctx) {
        let rest = HOP_REST_MS + cx_rand(cx, HOP_REST_RAND) as u64;
        self.hop.arm(cx.now, rest);
    }

    fn hops(&mut self, keys: &KeyState, cx: &mut Ctx) {
        if self.keys_armed.expired(cx.now) && keys.skip() {
            self.stage = Stage::FadeOut;
            self.fade = Fade::new(true, FADE_END_MS);
            return;
        }
        if !self.hop.expired(cx.now) {
            return;
        }
        // Slots 55/56 (w_vibrofly) when the farmer won, 57/58 (w_vibrofarmer) when the fly won.
        let base = if self.fly_won { 57 } else { 55 };
        let slot = SoundSlot(base + cx_rand(cx, 2) as u8);
        let freq = 20_500 + cx_rand(cx, 2000);
        cx.sound(slot, play_at(freq));
        self.arm_hop(cx);
    }
}

fn cx_rand(cx: &mut Ctx, n: i32) -> u32 {
    cx.rng.r(n) as u32
}

fn play_at(freq: u32) -> SoundOp {
    SoundOp::Play(PlayParams {
        freq_hz: Some(freq),
        ..PlayParams::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_state::{MatchConfig, Players};
    use crate::screen::rig::Rig;

    /// AR §8.4: after the top-up, a tie goes to the fly.
    #[test]
    fn a_tie_after_the_top_up_goes_to_the_fly() {
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = rig.cx(|cx| MatchState::new(config, false, cx));
        m.economy.fly.score = 500;
        m.economy.farmer.score = 458;
        m.economy.kills = 1;
        let w = rig.cx(|cx| Winner::new(&mut m, cx));
        assert_eq!((m.economy.fly.score, m.economy.farmer.score), (500, 500));
        assert!(w.fly_won);
        m.economy.farmer.score = 501;
        assert!(!rig.cx(|cx| Winner::new(&mut m, cx)).fly_won);
    }
}
