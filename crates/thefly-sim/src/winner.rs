//! W3. The winner screen (0x78): top-up, winner rule, count-up and trophy choreography
//! (ai-and-rules §8.4, screens-and-rendering §5.1).

use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::event::{Event, Music, PlayParams, SoundOp, SoundSlot};
use crate::input::KeyState;
use crate::match_state::MatchState;
use crate::mover::Wander;
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
    /// 50 when the fly won, 430 when the farmer won: it falls on the loser. Hops move it.
    pub trophy_x: i32,
    pub trophy_y: f64,
    /// px/s; +90 every falling tick (`FUN_004172a8` with +0x130 = 1).
    pub trophy_vy: i32,
    pub loser_crushed: bool,
    pub keys_armed: Timer,
    /// The rest between hops, or the hop itself while `hopping`.
    pub hop: Timer,
    pub hopping: bool,
    /// The hop motion, on only while `hopping`.
    pub mover: Wander,
}

/// `FUN_0044aedb` / `FUN_0044b431` (answers.md "Winner trophy").
pub const TROPHY_ACCEL: i32 = 90;
pub const TROPHY_START_Y: f64 = -50.0;
const TROPHY_H: i32 = 120;
/// Loser sprite tops: `winnerfly` at y 326, `winnerfarmer` at y 235.
const FLY_TOP: i32 = 326;
const FARMER_TOP: i32 = 235;
/// shortcut: the loser heights fall back to "bottom on the floor" when the sprite is not sized;
/// the real values are the asset heights.
const FLY_H: i32 = TROPHY_FLOOR + 1 - FLY_TOP;
const FARMER_H: i32 = TROPHY_FLOOR + 1 - FARMER_TOP;
const FIRST_REST_MS: u64 = 1000;
const FIRST_REST_RAND: i32 = 1000;
const HOP_MS: u64 = 500;
const HOP_RAND: i32 = 500;
const REST_MS: u64 = 1000;
const REST_RAND: i32 = 5000;
const LAND_VOLUME_CB: i32 = 2000;
const HOP_LAND_VOLUME_CB: i32 = -550;
/// Hop wander (answers.md "Winner trophy"): true gravity, bounds right = x + w + 2, top = y − 5,
/// bottom 587; the left bound is never applied.
const HOP_REROLL_MS: i32 = 100;
const HOP_SPEED: (i32, i32) = (10, 50);
const HOP_RUNS: (i32, i32) = (1, 2);
const HOP_ACCEL: (i32, i32) = (5, 5);
const HOP_RIGHT_SLACK: i32 = 2;
const HOP_TOP_SLACK: f64 = 5.0;

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
            trophy_x: home_x(fly_score >= farmer_score),
            trophy_y: TROPHY_START_Y,
            trophy_vy: 0,
            loser_crushed: false,
            keys_armed: Timer::UNARMED,
            hop: Timer::UNARMED,
            hopping: false,
            mover: Wander::new(HOP_REROLL_MS, HOP_SPEED, HOP_RUNS, HOP_ACCEL).with_gravity(),
        }
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
        if self.wait.is_armed() && self.wait.expired(now) {
            self.stage = Stage::CountUp;
        }
    }

    /// `FUN_0044b431` state 1: the loser's counter, then the winner's; reaching the winner's
    /// score starts the fall. Slot 49 ticks every time, with a new R(500) + 21500 only while the
    /// loser's counter (after its step) is below the loser's score.
    fn count_up(&mut self, cx: &mut Ctx) {
        let (fly, farmer) = (self.fly_score.max(0), self.farmer_score.max(0));
        self.fly_counter = (self.fly_counter + COUNT_STEP).min(fly);
        self.farmer_counter = (self.farmer_counter + COUNT_STEP).min(farmer);
        let (loser, loser_score, winner, winner_score) = if self.fly_won {
            (self.farmer_counter, farmer, self.fly_counter, fly)
        } else {
            (self.fly_counter, fly, self.farmer_counter, farmer)
        };
        // Port decision: the original switches to slot 50 when 49 has stopped and 50 has not
        // (`FUN_0040cf55`); the sim has no slot state, so always 49.
        let freq = (loser < loser_score).then(|| 21_500 + cx_rand(cx, 500));
        cx.sound(SoundSlot::RING_A, play_at(freq, 0));
        if winner >= winner_score {
            self.stage = Stage::TrophyFall;
        }
    }

    /// The accel is added every tick, then the FPS mover moves it; v changes every tick, so the
    /// run restarts and the step is trunc(v / FPS).
    fn fall(&mut self, cx: &mut Ctx) {
        let size = |name: &str, h: i32| cx.data.sizes.get(name).map_or(h, |s| s.h);
        let h = size("winnercoupe", TROPHY_H);
        self.trophy_vy += TROPHY_ACCEL;
        self.trophy_y += f64::from(self.trophy_vy / FPS);
        let bottom = self.trophy_y + f64::from(h);
        let (top, loser_h) = if self.fly_won {
            (FARMER_TOP, size("winnerfarmer", FARMER_H))
        } else {
            (FLY_TOP, size("winnerfly", FLY_H))
        };
        self.loser_crushed |= bottom > f64::from(top + loser_h * 50 / 100);
        if bottom < f64::from(TROPHY_FLOOR + 1) {
            return;
        }
        self.trophy_y = f64::from(TROPHY_FLOOR - h);
        self.trophy_vy = 0;
        cx.sound(SoundSlot::TROPHY, play_at(None, LAND_VOLUME_CB));
        let cheer = if self.fly_won {
            SoundSlot::CHEER_FLY
        } else {
            SoundSlot::CHEER_FARMER
        };
        cx.emit(Event::play(cheer));
        self.keys_armed.arm(cx.now, KEY_ARM_MS);
        let rest = FIRST_REST_MS + cx_rand(cx, FIRST_REST_RAND) as u64;
        self.hop.arm(cx.now, rest);
        self.stage = Stage::Hops;
    }

    /// Rest, hop (three draws: length, parity, frequency), rest R(5000) + 1000, … The trophy
    /// wanders while hopping.
    // shortcut: the mover steps after the choreography in a tick, so a hop's first wander draws
    // follow the cheer's; answers.md "Winner trophy" gives no order [I]. Revisit with a capture.
    fn hops(&mut self, keys: &KeyState, cx: &mut Ctx) {
        if self.keys_armed.expired(cx.now) && keys.skip() {
            self.stage = Stage::FadeOut;
            self.fade = Fade::new(true, FADE_END_MS);
            return;
        }
        if self.hop.expired(cx.now) {
            self.toggle_hop(cx);
        }
        if self.hopping {
            self.hop_step(cx);
        }
    }

    fn toggle_hop(&mut self, cx: &mut Ctx) {
        self.hopping = !self.hopping;
        if !self.hopping {
            self.mover.v = [0, 0];
            self.trophy_y = f64::from(TROPHY_FLOOR - trophy_h(cx));
            let rest = REST_MS + cx_rand(cx, REST_RAND) as u64;
            self.hop.arm(cx.now, rest);
            cx.sound(SoundSlot::TROPHY, play_at(None, HOP_LAND_VOLUME_CB));
            return;
        }
        let hop = HOP_MS + cx_rand(cx, HOP_RAND) as u64;
        self.hop.arm(cx.now, hop);
        // {55, 56, 57, 58}[parity + 2 × winner flag]; the flag is read as "the fly won" [I].
        let slot = 55 + u8::from(cx.rng.odd()) + 2 * u8::from(self.fly_won);
        let freq = 20_500 + cx_rand(cx, 2000);
        cx.sound(SoundSlot(slot), play_at(Some(freq), 0));
    }

    fn hop_step(&mut self, cx: &mut Ctx) {
        let (dx, dy) = self.mover.step(cx);
        self.trophy_x = (self.trophy_x + dx).min(home_x(self.fly_won) + HOP_RIGHT_SLACK);
        let floor = f64::from(TROPHY_FLOOR - trophy_h(cx));
        self.trophy_y = (self.trophy_y + f64::from(dy)).clamp(floor - HOP_TOP_SLACK, floor);
    }
}

fn home_x(fly_won: bool) -> i32 {
    if fly_won { 50 } else { 430 }
}

fn trophy_h(cx: &Ctx) -> i32 {
    cx.data.sizes.get("winnercoupe").map_or(TROPHY_H, |s| s.h)
}

fn cx_rand(cx: &mut Ctx, n: i32) -> u32 {
    cx.rng.r(n) as u32
}

fn play_at(freq_hz: Option<u32>, volume_cb: i32) -> SoundOp {
    SoundOp::Play(PlayParams {
        freq_hz,
        volume_cb,
        ..PlayParams::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_state::{MatchConfig, Players};
    use crate::rng::MsvcRand;
    use crate::screen::rig::Rig;

    const TICK_CAP: usize = 1000;

    fn two_player(rig: &mut Rig) -> MatchState {
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        rig.cx(|cx| MatchState::new(config, false, cx))
    }

    /// (now, slot, op) of every sound.
    fn run(
        rig: &mut Rig,
        w: &mut Winner,
        m: &mut MatchState,
        ticks: usize,
    ) -> Vec<(u64, u8, SoundOp)> {
        let mut out = Vec::new();
        for _ in 0..ticks {
            let (now, before) = (rig.now(), rig.events.len());
            rig.step(&[], |k, cx| w.tick(k, m, cx));
            for e in &rig.events[before..] {
                if let Event::Sound { slot, op } = e {
                    out.push((now, slot.0, *op));
                }
            }
        }
        out
    }

    /// AR §8.4: after the top-up, a tie goes to the fly.
    #[test]
    fn a_tie_after_the_top_up_goes_to_the_fly() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        m.economy.fly.score = 500;
        m.economy.farmer.score = 458;
        m.economy.kills = 1;
        let w = rig.cx(|cx| Winner::new(&mut m, cx));
        assert_eq!((m.economy.fly.score, m.economy.farmer.score), (500, 500));
        assert!(w.fly_won);
        m.economy.farmer.score = 501;
        assert!(!rig.cx(|cx| Winner::new(&mut m, cx)).fly_won);
    }

    /// answers.md "Winner count-up": fly 30 beats farmer 15. Tick 1: farmer 10 < 15 draws
    /// (seed 1: 41 → 21541); tick 2: farmer 15, no draw; tick 3: fly 30 → the fall.
    #[test]
    fn the_count_up_draws_only_while_the_loser_counts() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        m.economy.fly.score = 30;
        m.economy.farmer.score = 15;
        let mut w = rig.cx(|cx| Winner::new(&mut m, cx));
        w.stage = Stage::CountUp;
        rig.rng = MsvcRand::new(1);
        let got = run(&mut rig, &mut w, &mut m, 3);
        let ops: Vec<_> = got.iter().map(|&(_, slot, op)| (slot, op)).collect();
        assert_eq!(
            ops,
            [
                (49, play_at(Some(21_541), 0)),
                (49, play_at(None, 0)),
                (49, play_at(None, 0)),
            ]
        );
        assert_eq!(w.stage, Stage::TrophyFall);
        let mut probe = MsvcRand::new(1);
        probe.rand();
        assert_eq!(rig.rng, probe);
    }

    /// answers.md "Winner trophy": vy += 90 per tick, step trunc(vy / 65), so after n ticks
    /// y = −50 + Σ trunc(90k / 65): Σ 340 after 22 (bottom 410, not past the farmer's mid-height
    /// 235 + trunc(353 × 50 / 100) = 411), 371 after 23 (crushed), 511 after 27 and 549 after
    /// 28, past 518, so it lands on tick 28.
    #[test]
    fn the_trophy_accelerates_by_90_and_crushes_the_loser_at_mid_height() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        m.economy.fly.score = 30;
        let mut w = rig.cx(|cx| Winner::new(&mut m, cx));
        w.stage = Stage::TrophyFall;
        run(&mut rig, &mut w, &mut m, 22);
        assert_eq!((w.trophy_y, w.loser_crushed), (290.0, false));
        run(&mut rig, &mut w, &mut m, 1);
        assert_eq!((w.trophy_y, w.loser_crushed), (321.0, true));
        run(&mut rig, &mut w, &mut m, 4);
        assert_eq!(w.stage, Stage::TrophyFall);
        run(&mut rig, &mut w, &mut m, 1);
        assert_eq!((w.stage, w.trophy_y, w.trophy_vy), (Stage::Hops, 467.0, 0));
    }

    /// Seed 1 from the landing tick: rest R(1000) + 1000 = 1041; hop R(500) + 500 = 967, parity
    /// 6334 even, R(2000) + 20500 = 21000 → slot 57 (fly won); then slot 54 at −550 when the hop
    /// ends (the rest it draws follows the hop's wander draws).
    #[test]
    fn the_hops_draw_length_parity_and_frequency() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        m.economy.fly.score = 30;
        let mut w = rig.cx(|cx| Winner::new(&mut m, cx));
        (w.stage, w.trophy_y) = (Stage::TrophyFall, 467.0);
        rig.rng = MsvcRand::new(1);
        let landed = rig.now();
        let got = run(&mut rig, &mut w, &mut m, 150);
        assert_eq!(got[0], (landed, 54, play_at(None, 2000)));
        assert_eq!(got[1], (landed, 51, SoundOp::Play(PlayParams::default())));
        let (hop_at, slot, op) = got[2];
        assert_eq!((slot, op), (57, play_at(Some(21_000), 0)));
        assert!(
            (landed + 1041..landed + 1041 + 16).contains(&hop_at),
            "{hop_at}"
        );
        let (rest_at, slot, op) = got[3];
        assert_eq!((slot, op), (54, play_at(None, -550)));
        assert!(
            (hop_at + 967..hop_at + 967 + 16).contains(&rest_at),
            "{rest_at}"
        );
        assert_eq!(got.len(), 4);
    }

    /// A Hops-stage winner (fly won, x 50) resting on the floor, rand at seed 1.
    fn resting(rig: &mut Rig, m: &mut MatchState) -> Winner {
        m.economy.fly.score = 30;
        let mut w = rig.cx(|cx| Winner::new(m, cx));
        (w.stage, w.trophy_y) = (Stage::Hops, 467.0);
        rig.rng = MsvcRand::new(1);
        w
    }

    /// answers.md "Winner trophy" and "Wander mover internals", seed 1: the hop draws 541 ms,
    /// parity 18467 odd, 6334 % 2000 → slot 58 at 20834; then the mover x: timer 26500, d 9 (+1),
    /// 15724 % 10 + 65 = 69, gravity + 5 = 74; y: timer 11478, d 8 (+1), 26962 % 50 + 65 = 77,
    /// + 5 = 82. Both move 1 px: x 51 ≤ 50 + 2, y 468 clamped to the floor 467.
    #[test]
    fn a_hop_steps_the_trophy_through_the_wander_mover() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        let mut w = resting(&mut rig, &mut m);
        let got = run(&mut rig, &mut w, &mut m, 1);
        assert_eq!(got, [(0, 58, play_at(Some(20_834), 0))]);
        assert!(w.hopping);
        assert_eq!(w.hop.remaining_ms(0), 541);
        assert_eq!(w.mover.v, [74, 82]);
        assert_eq!((w.trophy_x, w.trophy_y), (51, 467.0));
        assert_eq!(rig.rng.rand(), 24464);
    }

    /// Bounds: right = x + w + 2, top = y − 5, bottom 587; the left bound is never applied.
    /// Each hop ends with v = 0 back on the floor.
    #[test]
    fn hops_stay_inside_the_bounds_and_end_on_the_floor() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        let mut w = resting(&mut rig, &mut m);
        let mut moved = false;
        for _ in 0..TICK_CAP {
            let was_hopping = w.hopping;
            run(&mut rig, &mut w, &mut m, 1);
            assert!(w.trophy_x <= 52, "{}", w.trophy_x);
            assert!((462.0..=467.0).contains(&w.trophy_y), "{}", w.trophy_y);
            moved |= w.trophy_x != 50 || w.trophy_y != 467.0;
            if was_hopping && !w.hopping {
                assert_eq!((w.mover.v, w.trophy_y), ([0, 0], 467.0));
            }
        }
        assert!(moved);
    }

    /// The hop's end, seed 1: v = 0, y = 587 − h, rest 41 % 5000 + 1000 = 1041, slot 54 at −550.
    #[test]
    fn a_hop_ends_at_rest_on_the_floor() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        let mut w = resting(&mut rig, &mut m);
        (w.hopping, w.trophy_y, w.mover.v) = (true, 463.0, [-80, 30]);
        w.hop.arm(0, 0);
        rig.clock.advance();
        let now = rig.now();
        let got = run(&mut rig, &mut w, &mut m, 1);
        assert_eq!(got, [(now, 54, play_at(None, -550))]);
        assert_eq!((w.hopping, w.mover.v, w.trophy_y), (false, [0, 0], 467.0));
        assert_eq!(w.hop.remaining_ms(now), 1041);
    }
}
