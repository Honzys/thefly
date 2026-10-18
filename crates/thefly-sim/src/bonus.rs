//! W3. The bonus round (0x64): handicaps, countdown, coins, targets, counting, the end tally and
//! the bonus AI (ai-and-rules §8.3). The AI reuses W2's [`Aligner`].
//!
//! Coin entry is decoded (gaps §7.3, `sin`). Open item O3: the layout, the intro, the coins'
//! leftward motion and how the bonus fly and swatter move (proposal: the world's fly physics and
//! farmer movement on the bonus play field).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, Music, PlayParams, SoundOp, SoundSlot};
use crate::input::{FarmerPad, FlyPad, KeyState, Scancode};
use crate::match_state::{MatchState, Players};
use crate::screen::{MatchFlow, MatchScreen};
use crate::shop::GoodiesShop;
use crate::winner::Winner;
use crate::world::ai::{AiLevel, Aligner, Controller, Target};
use crate::world::flying_goodies::Side;

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
    /// Coin lifetime (25000 ms) and its frame and animation delay (gaps §7.3).
    pub until: Timer,
    pub frame: i32,
    pub anim_ms: i32,
}

/// A bonus-round actor's position and velocity. Open item O3: its motion rule.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BonusMover {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
}

/// shortcut: open item O3. The movers, the scroll speed, the actor boxes and the target rows
/// are guesses (the bonus layout is not decoded); revisit when `FUN_004452a1` is re-read.
pub const SCROLL_PX: f64 = 4.0;
pub const MOVE_PX: f64 = 6.0;
pub const FLY_BOX: (f64, f64) = (60.0, 44.0);
pub const SWATTER_BOX: (f64, f64) = (80.0, 80.0);
pub const TARGET_ROWS: [f64; 5] = [40.0, 110.0, 180.0, 250.0, 320.0];
pub const SPAWN_X: f64 = 810.0;
pub const COIN_LIFETIME_MS: u64 = 25_000;
/// shortcut: the `piece` sheet's frame count is passed in code and not decoded; 16 frames.
pub const COIN_FRAMES: i32 = 16;
const FIELD_H: i32 = 440;
const COIN_H: i32 = 45;
const TARGET_W: f64 = 48.0;
const TARGET_H: f64 = 44.0;

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
        let params = BONUS_AI_PARAMS[level as usize];
        let controller = Controller {
            target: Target::BonusTarget(0),
            ring: TARGET_RING,
            spread: (SWATTER_BOX.0 * params.spread_of_width) as i32,
            ..Controller::default()
        };
        let aligner = Aligner {
            align_tol: params.align_tol,
            react_base: params.react_base,
            react_rand: params.react_rand,
            ..Aligner::default()
        };
        let mut ai = Self {
            params,
            controller,
            aligner,
            frenzy: false,
            frenzy_timer: Timer::UNARMED,
            paused: false,
        };
        ai.arm_off(h_farmer, cx);
        ai
    }

    fn arm_off(&mut self, h: f64, cx: &mut Ctx) {
        let p = &self.params;
        let ms = p.off_base + cx.rng.r(((f64::from(p.off_rand) * h) as i32).max(1));
        self.frenzy_timer.arm(cx.now, ms as u64);
    }

    /// §8.3 step 7 frenzy toggle.
    fn frenzy_step(&mut self, h: f64, cx: &mut Ctx) {
        if !self.frenzy_timer.expired(cx.now) {
            return;
        }
        let p = self.params;
        let chance = p.p_frenzy + (f64::from(p.p_frenzy) * (1.0 - 2.0 * h)) as i32;
        if self.frenzy || cx.rng.r(100) >= chance {
            self.frenzy = false;
            return self.arm_off(h, cx);
        }
        self.frenzy = true;
        let on = p.on_base + cx.rng.r(((f64::from(p.on_rand) * (1.0 - h)) as i32).max(1));
        self.frenzy_timer.arm(cx.now, on as u64);
    }

    /// Aims at the nearest active target by |Δx| and returns the pad, swing included.
    fn pad(&mut self, swatter: &BonusMover, targets: &[Piece], h: f64, cx: &mut Ctx) -> FarmerPad {
        self.frenzy_step(h, cx);
        let nearest = targets
            .iter()
            .enumerate()
            .filter(|(_, t)| t.active)
            .min_by(|a, b| {
                (a.1.x - swatter.x)
                    .abs()
                    .total_cmp(&(b.1.x - swatter.x).abs())
            });
        let Some((i, t)) = nearest else {
            return FarmerPad::default();
        };
        self.controller.target = Target::BonusTarget(i);
        self.controller.refresh(
            (swatter.x as i32, swatter.y as i32),
            (t.x as i32, t.y as i32),
        );
        let aligned = self
            .aligner
            .step(self.controller.dx, self.controller.dy, cx);
        let both = aligned.x_aligned && aligned.y_aligned;
        if both {
            self.controller.reroll_offset(cx);
        }
        FarmerPad {
            swing: both || self.frenzy,
            ..self.aligner.pad()
        }
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
    /// Coin wave amplitude A = trunc((bgH − 2.5 × coinH) / 2) (gaps §7.3).
    pub amplitude: i32,
    pub target_spawns: usize,
    /// The previous tick's swing, for the Tab edge.
    pub swing_held: bool,
}

/// §8.3 step 1: h = 1 − meter, with the two edge values.
fn handicap(meter: f64) -> f64 {
    if meter == 0.0 {
        H_AT_METER_0
    } else if meter == 1.0 {
        H_AT_METER_1
    } else {
        1.0 - meter
    }
}

fn overlaps(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

fn steer(m: &mut BonusMover, left: bool, right: bool, up: bool, down: bool, w: f64, h: f64) {
    let axis = |neg: bool, pos: bool| f64::from(i8::from(pos) - i8::from(neg)) * MOVE_PX;
    m.vx = axis(left, right);
    m.vy = axis(up, down);
    m.x = (m.x + m.vx).clamp(0.0, 800.0 - w);
    m.y = (m.y + m.vy).clamp(0.0, f64::from(FIELD_H) - h);
}

impl BonusRound {
    /// Handicaps from [`crate::economy::Economy::meter`], countdown armed.
    pub fn new(m: &MatchState, cx: &mut Ctx) -> Self {
        let h_fly = handicap(m.economy.meter(Side::Fly));
        let h_farmer = handicap(m.economy.meter(Side::Farmer));
        let bg_h = cx.data.sizes.get("bckbonusfond").map_or(FIELD_H, |s| s.h);
        let coin_rate = f64::from(cx.rng.r(60) + 1) / 100.0;
        let ai = (m.config.players == Players::One)
            .then(|| BonusAi::new(m.config.ai_level, h_farmer, cx));
        cx.emit(Event::Music(Music::Bonus));
        Self {
            phase: BonusPhase::Countdown {
                count: COUNTDOWN_FROM,
                next: Timer::armed(cx.now, COUNTDOWN_STEP_MS),
            },
            h_fly,
            h_farmer,
            coins: [Piece::default(); COINS],
            targets: [Piece::default(); TARGETS],
            coin_timer: Timer::UNARMED,
            target_timer: Timer::UNARMED,
            coin_rate,
            coin_spawns: 0,
            coins_taken: 0,
            hits: 0,
            fly: BonusMover {
                x: 100.0,
                y: 200.0,
                ..BonusMover::default()
            },
            swatter: BonusMover {
                x: 400.0,
                y: 200.0,
                ..BonusMover::default()
            },
            ai,
            amplitude: ((f64::from(bg_h) - 2.5 * f64::from(COIN_H)) / 2.0) as i32,
            target_spawns: 0,
            swing_held: false,
        }
    }

    /// After the result: match over → winner, else → goodies shop.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        match self.phase {
            BonusPhase::Countdown { count, mut next } => {
                if next.expired_rearm(cx.now) {
                    self.countdown(count - 1, next, cx);
                } else {
                    self.phase = BonusPhase::Countdown { count, next };
                }
            }
            BonusPhase::Play { round } => self.play(round, keys, m, cx),
            BonusPhase::End { sound } => {
                if sound.expired(cx.now) {
                    let farmer_ahead = 2 * self.coins_taken < 10 * self.hits;
                    let slot = if farmer_ahead {
                        SoundSlot::CHEER_FARMER
                    } else {
                        SoundSlot::CHEER_FLY
                    };
                    cx.emit(Event::play(slot));
                    self.phase = BonusPhase::Result;
                }
            }
            BonusPhase::Result => {
                if keys.pressed(Scancode::RETURN) {
                    return MatchFlow::Next(after_bonus(m, cx));
                }
            }
        }
        MatchFlow::Stay
    }

    fn countdown(&mut self, count: i32, next: Timer, cx: &mut Ctx) {
        if count >= 0 {
            self.phase = BonusPhase::Countdown { count, next };
            return;
        }
        let target_ms = (self.h_farmer * 300.0 + 200.0) as u64;
        self.phase = BonusPhase::Play {
            round: Timer::armed(cx.now, ROUND_MS),
        };
        self.coin_timer.arm(cx.now, COIN_SPAWN_MS);
        self.target_timer.arm(cx.now, target_ms);
    }

    fn play(&mut self, round: Timer, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) {
        if round.expired(cx.now) {
            return self.end(m, cx);
        }
        let fly = FlyPad::read(keys);
        steer(
            &mut self.fly,
            fly.left,
            fly.right,
            fly.up,
            fly.down,
            FLY_BOX.0,
            FLY_BOX.1,
        );
        let pad = match &mut self.ai {
            Some(ai) => ai.pad(&self.swatter, &self.targets, self.h_farmer, cx),
            None => FarmerPad::read(keys),
        };
        let (w, h) = SWATTER_BOX;
        steer(
            &mut self.swatter,
            pad.left,
            pad.right,
            pad.up,
            pad.down,
            w,
            h,
        );
        let swing = pad.swing && !self.swing_held;
        self.swing_held = pad.swing;
        if self.coin_timer.expired_rearm(cx.now) {
            self.spawn_coin(cx);
        }
        if self.target_timer.expired_rearm(cx.now) {
            self.spawn_target();
        }
        self.scroll(cx.now);
        self.collect(cx);
        if swing {
            self.swat(cx);
        }
        self.phase = BonusPhase::Play { round };
    }

    /// gaps §7.3: the counter always counts; a coin only enters a free slot.
    fn spawn_coin(&mut self, cx: &mut Ctx) {
        self.coin_spawns += 1;
        let Some(slot) = self.coins.iter().position(|c| !c.active) else {
            return;
        };
        let h = self.h_fly;
        if f64::from(cx.rng.r(100)) < 8.0 * h + 4.0 {
            self.coin_rate = wave_rate(h, cx);
        }
        let frame = cx.rng.r(COIN_FRAMES);
        let wave = (f64::from(self.coin_spawns) * self.coin_rate).sin() + 1.0;
        let y = (wave * f64::from(self.amplitude)) as i32 + COIN_H;
        let anim_ms = cx.rng.r(30) + 15;
        self.coins[slot] = Piece {
            active: true,
            x: SPAWN_X,
            y: f64::from(y),
            until: Timer::armed(cx.now, COIN_LIFETIME_MS),
            frame,
            anim_ms,
            ..Piece::default()
        };
    }

    fn spawn_target(&mut self) {
        let Some(slot) = self.targets.iter().position(|t| !t.active) else {
            return;
        };
        let y = TARGET_ROWS[self.target_spawns % TARGET_ROWS.len()];
        self.target_spawns += 1;
        self.targets[slot] = Piece {
            active: true,
            x: SPAWN_X,
            y,
            ..Piece::default()
        };
    }

    fn scroll(&mut self, now: u64) {
        for p in self.coins.iter_mut().chain(self.targets.iter_mut()) {
            p.x -= SCROLL_PX;
            if p.x < -TARGET_W || p.until.expired(now) {
                p.active = false;
            }
        }
    }

    fn collect(&mut self, cx: &mut Ctx) {
        let fly = (self.fly.x, self.fly.y, FLY_BOX.0, FLY_BOX.1);
        let c = f64::from(COIN_H);
        for i in 0..COINS {
            let coin = self.coins[i];
            if !coin.active || !overlaps(fly, (coin.x, coin.y, c, c)) {
                continue;
            }
            self.coins[i].active = false;
            self.coins_taken += 1;
            let slot = SoundSlot(SoundSlot::RING_A.0 + cx.rng.r(2) as u8);
            let freq = 21_500 + cx.rng.r(500) as u32;
            cx.sound(slot, play_at(freq));
            if self.coins_taken % 50 == 0 {
                cx.emit(Event::play(SoundSlot::CHEER_FLY));
            }
        }
    }

    fn swat(&mut self, cx: &mut Ctx) {
        let sw = (self.swatter.x, self.swatter.y, SWATTER_BOX.0, SWATTER_BOX.1);
        for i in 0..TARGETS {
            let t = self.targets[i];
            if !t.active || !overlaps(sw, (t.x, t.y, TARGET_W, TARGET_H)) {
                continue;
            }
            self.targets[i].active = false;
            self.hits += 1;
            cx.emit(Event::play(SoundSlot::FLYING_GOODIE));
            if self.hits % 10 == 0 {
                cx.emit(Event::play(SoundSlot::CHEER_FARMER));
            }
        }
    }

    /// §8.3 step 6.
    fn end(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        if let Some(ai) = &mut self.ai {
            ai.paused = true;
        }
        self.coin_timer.disarm();
        self.target_timer.disarm();
        let e = &mut m.economy;
        e.fly.money += COIN_VALUE * self.coins_taken;
        e.fly.score += COIN_VALUE * self.coins_taken;
        e.farmer.money += HIT_VALUE * self.hits;
        e.farmer.score += HIT_VALUE * self.hits;
        self.phase = BonusPhase::End {
            sound: Timer::armed(cx.now, RESULT_SOUND_DELAY_MS),
        };
    }
}

fn play_at(freq: u32) -> SoundOp {
    SoundOp::Play(PlayParams {
        freq_hz: Some(freq),
        ..PlayParams::default()
    })
}

/// gaps §7.3 (wins over AR §8.3's older form): ω = (R(c) + lo) / 100.
fn wave_rate(h: f64, cx: &mut Ctx) -> f64 {
    let (lo, c) = if h <= 0.5 {
        (1, (100.0 * h + 10.0) as i32)
    } else {
        let lo = ((h - 0.5) / 0.5 * 29.0 + 5.0) as i32;
        (lo, 60 - lo)
    };
    f64::from(cx.rng.r(c) + lo) / 100.0
}

/// game-spec §2.2: the match ends after this bonus, or the shop follows.
fn after_bonus(m: &mut MatchState, cx: &mut Ctx) -> MatchScreen {
    if m.over_after_period() {
        MatchScreen::Winner(Box::new(Winner::new(m, cx)))
    } else {
        MatchScreen::GoodiesShop(Box::new(GoodiesShop::new(m, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_state::MatchConfig;
    use crate::screen::rig::Rig;

    const TICK_CAP: usize = 10_000;

    /// AR §8.3: countdown 3 s, a 35 s round, coins every 50 ms, targets every
    /// trunc(0.5 × 300 + 200) = 350 ms at a tie, then the result sound 2 s after the end.
    #[test]
    fn the_round_runs_35_s_and_sounds_the_result_2_s_later() {
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = rig.cx(|cx| MatchState::new(config, false, cx));
        let mut b = rig.cx(|cx| BonusRound::new(&m, cx));
        assert_eq!((b.h_fly, b.h_farmer), (0.5, 0.5));
        let mut play_at = None;
        let mut end_at = None;
        for _ in 0..TICK_CAP {
            let now = rig.now();
            rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
            match b.phase {
                BonusPhase::Play { .. } if play_at.is_none() => play_at = Some(now),
                BonusPhase::End { .. } if end_at.is_none() => end_at = Some(now),
                BonusPhase::Result => break,
                _ => {}
            }
        }
        let (play, end) = (play_at.unwrap(), end_at.unwrap());
        assert!((4000..4020).contains(&play), "{play}");
        assert!((35_000..35_020).contains(&(end - play)), "{}", end - play);
        // AR §0: a 50 ms timer re-arms from the tick it fires on, so every 4 ticks (61.5 ms).
        assert!((560..=575).contains(&b.coin_spawns), "{}", b.coin_spawns);
        assert!((90..=100).contains(&b.target_spawns), "{}", b.target_spawns);
        let fly_ahead = 2 * b.coins_taken >= 10 * b.hits;
        assert!(fly_ahead);
        assert_eq!(rig.events.last(), Some(&Event::play(SoundSlot::CHEER_FLY)));
        assert_eq!(
            m.economy.fly.score,
            2 * b.coins_taken + starting_score(&config)
        );
    }

    fn starting_score(config: &MatchConfig) -> i32 {
        let mut rig = Rig::new(1);
        rig.cx(|cx| MatchState::new(*config, false, cx))
            .economy
            .fly
            .score
    }

    #[test]
    fn the_farmer_wins_the_result_when_hits_outweigh_coins() {
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = rig.cx(|cx| MatchState::new(config, false, cx));
        let mut b = rig.cx(|cx| BonusRound::new(&m, cx));
        b.coins_taken = 4;
        b.hits = 1;
        b.phase = BonusPhase::End {
            sound: Timer::armed(rig.now(), 0),
        };
        rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
        assert_eq!(
            rig.events.last(),
            Some(&Event::play(SoundSlot::CHEER_FARMER))
        );
    }
}
