//! W3. The bonus round (0x64): handicaps, countdown, coins, targets, counting, the end tally and
//! the bonus AI (ai-and-rules §8.3). The AI reuses W2's [`Aligner`].
//!
//! Coin entry is decoded (gaps §7.3, `sin`); the bar, the movers, the swing and the boxes follow
//! answers.md "Bonus round movers, boxes and target spawns". shortcut: the swatter-on-fly knock
//! (`FUN_004485ed`), the shockwave and the positional buzz are not modelled; revisit with O3.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, Music, PlayParams, SoundOp, SoundSlot};
use crate::input::{FarmerPad, FlyPad, KeyState, Scancode};
use crate::match_state::{MatchState, Players};
use crate::mover::{FpsRun, Wander};
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
    /// Screen x; a coin's is trunc(`layer_x` × 1.25).
    pub x: f64,
    /// Coins only: the x on the coin's 1.25 layer, moved by the bar step.
    pub layer_x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    /// Coin lifetime (25000 ms) and its frame and animation delay (gaps §7.3).
    pub until: Timer,
    pub frame: i32,
    pub anim_ms: i32,
    /// Targets only; the coins' wander is off.
    pub mover: Wander,
}

/// A bonus-round actor: position and velocity (px/s, whole values), moved by the FPS integrator.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BonusMover {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    run: FpsRun,
}

impl BonusMover {
    /// x += v·dt + the bar step, y += v·dt (answers.md "Bonus round movers"). shortcut: the dt
    /// split is [I]; the FPS integrator stands in for it.
    fn integrate(&mut self, bar_step: i32) {
        let (dx, dy) = self.run.step([self.vx as i32, self.vy as i32]);
        self.x += f64::from(dx + bar_step);
        self.y += f64::from(dy);
    }
}

/// `bckbonusbarre` (1.0 layer) scrolls at −600 px/s; its per-tick step also moves the coins,
/// the fly and the swatter. When the round ends, vx += 2 per tick until it is above −81, then
/// 0; the step keeps its last value (the bar writes it only while vx ≠ 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bar {
    pub vx: i32,
    /// The last per-tick step, in layer px.
    pub step: i32,
    /// The bar's scroll offset, in layer px.
    pub x: i32,
    run: FpsRun,
}

pub const BAR_VX: i32 = -600;
const BAR_BRAKE: i32 = 2;
const BAR_STOP_ABOVE: i32 = -81;

impl Bar {
    // The per-tick split is the FPS integrator's (answers.md "FPS integrator: per-axis runs").
    fn tick(&mut self) {
        if self.vx == 0 {
            return;
        }
        self.step = self.run.step([self.vx, 0]).0;
        self.x += self.step;
    }

    fn brake(&mut self) {
        self.vx += BAR_BRAKE;
        if self.vx > BAR_STOP_ABOVE {
            self.vx = 0;
        }
    }
}

/// The bonus fly (`FUN_004481c2`, `FUN_00448302`): `j_transi_est` 120×94 at (R(300), 150);
/// a held key adds 0.1 × cap (the float 0.1 widened to a double) per tick, caps 890 / 670, a
/// released axis decays by 9; x in [0, 680], y in [bg y − 15, bg y + bg h + 15 − 94].
const FLY_START_X_RAND: i32 = 300;
const FLY_START_Y: f64 = 150.0;
const FLY_CAP: (i32, i32) = (890, 670);
const FLY_DECAY: i32 = 9;
const FLY_MAX_X: f64 = 680.0;
const FLY_Y_MARGIN: f64 = 15.0;
/// The bonus swatter (`FUN_00448802`, `FUN_00448c31`): `j_tapette_1..5`, 181×178, start y 351.
/// A held key adds 400 (x) or 200 (y) per tick, capped at 900 / 400; a released axis stops.
/// x in [0, 800], y in [351, 472].
pub const SWATTER_W: f64 = 181.0;
pub const SWATTER_H: f64 = 178.0;
const SWATTER_ACCEL: (i32, i32) = (400, 200);
const SWATTER_CAP: (i32, i32) = (900, 400);
const SWATTER_MAX_X: f64 = 800.0;
pub const SWATTER_MIN_Y: f64 = 351.0;
pub const SWATTER_MAX_Y: f64 = 472.0;
/// shortcut: the swatter's start x is not given (answers.md "Bonus round movers"); 400.
const SWATTER_START_X: f64 = 400.0;
/// Reach (+0x98) = (1 − (472 − y) / 121) × 375.
const REACH_SPAN: f64 = 121.0;
pub const REACH_MAX: f64 = 375.0;
/// The move sound: slot 22 + R(5), every 31 ticks, or after 21 while vx ≠ 0 (`FUN_0044936a`).
const MOVE_SOUNDS: i32 = 5;
const MOVE_SOUND_TICKS: (i32, i32) = (31, 21);
/// The swing (`FUN_004490be`): frames 1..4, sound 27 at R(15000) + 35000 Hz and a 30 % shout
/// (34) at frame 1, hitting while frame > 2, then a retract and a cooldown that must pass 20.
pub const SWING_FRAMES: i32 = 4;
const SWING_HIT_ABOVE: i32 = 2;
const SWING_COOLDOWN: i32 = 20;
const SWING_FREQ: (u32, i32) = (35_000, 15_000);
const SHOUT_ROLL: i32 = 100;
const SHOUT_BELOW: i32 = 30;

/// The swatter's reach from its y: 0 at y 351, 375 at y 472.
pub fn reach(y: f64) -> f64 {
    (1.0 - (SWATTER_MAX_Y - y) / REACH_SPAN) * REACH_MAX
}

/// The swing state: `frame` climbs 1..=4 one per tick, then retracts one per tick to 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swing {
    pub frame: i32,
    pub retract: bool,
    /// Idle ticks since the last swing ended.
    pub cooldown: i32,
}

impl Default for Swing {
    fn default() -> Self {
        Self {
            frame: 0,
            retract: false,
            cooldown: SWING_COOLDOWN + 1,
        }
    }
}

impl Swing {
    /// The hitting state (`FUN_00448110`). shortcut: whether the retract also hits is [I]; here
    /// it does not.
    pub fn hitting(&self) -> bool {
        self.frame > SWING_HIT_ABOVE && !self.retract
    }

    /// One tick; `start` is the swing edge. Returns true on the tick a swing starts.
    // shortcut: the shockwave (`FUN_00449428`, at x − 5 + 65) is not modelled, and when the
    // cooldown starts counting (taken as the first idle tick) is [I].
    fn tick(&mut self, start: bool) -> bool {
        if self.frame == 0 {
            self.cooldown += 1;
            if !start || self.cooldown <= SWING_COOLDOWN {
                return false;
            }
            *self = Self {
                frame: 1,
                retract: false,
                cooldown: 0,
            };
            return true;
        }
        if self.retract {
            self.frame -= 1;
            self.retract = self.frame > 0;
            return false;
        }
        self.frame += 1;
        if self.frame > SWING_FRAMES {
            self.frame = SWING_FRAMES;
            self.retract = true;
        }
        false
    }
}

/// A held key adds `accel` per tick (truncated), capped at ±cap; a released axis decays by
/// `decay` toward 0 (`None`: it stops at once). Opposite keys cancel.
fn axis(v: f64, neg: bool, pos: bool, accel: f64, cap: i32, decay: Option<i32>) -> f64 {
    let dir = i32::from(pos) - i32::from(neg);
    let v = v as i32;
    if dir != 0 {
        let v = (f64::from(v) + f64::from(dir) * accel).trunc() as i32;
        return f64::from(v.clamp(-cap, cap));
    }
    let Some(d) = decay else { return 0.0 };
    f64::from(v - v.signum() * v.abs().min(d))
}

/// 0.1 × cap, with the float 0.1 widened to a double: 89.0000013 for 890.
fn tenth_of(cap: i32) -> f64 {
    f64::from(cap) * f64::from(0.1_f32)
}
/// The swatter's hit box is (x + 30, reach, w − 60, 154) (`FUN_00431e58`).
const SWAT_INSET: f64 = 30.0;
const SWAT_H: f64 = 154.0;
/// The fly's coin box is (x + 50, y + 40, w − 100, h − 80) on its `Face` frame (120 × 94).
const FLY_INSET: (i32, i32) = (50, 40);
const FLY_SIZE: (i32, i32) = (120, 94);
/// Coins and targets launch at x 810; a coin's is its 1.25-layer x.
pub const SPAWN_X: f64 = 810.0;
const COIN_LAYER: f64 = 1.25;
/// A target enters at x 810 + R(150).
const TARGET_SPAWN_RAND: i32 = 150;
/// Target wander (answers.md "Bonus round movers"): reroll 500 ms, x speed range 1, y range
/// trunc(500 × h_farmer + 1), runs 6 and 3, accel 5; y stays 15 px inside the field.
const TARGET_REROLL_MS: i32 = 500;
const TARGET_X_SPEED: i32 = 1;
const TARGET_Y_SPEED_SPAN: f64 = 500.0;
const TARGET_RUNS: (i32, i32) = (6, 3);
const TARGET_ACCEL: (i32, i32) = (5, 5);
const TARGET_BAND: f64 = 15.0;
/// The targets' 1.0 layer wraps at 4000; the right bound is wrap + 100, so x + 48 ≤ 4100.
const TARGET_MAX_X: f64 = 4100.0 - TARGET_W;
pub const LIFETIME_MS: u64 = 25_000;
/// Port decision: the `piece` sheet's frame count is passed in code; the port uses 16.
pub const COIN_FRAMES: i32 = 16;
const FIELD_H: i32 = 440;
const COIN_H: i32 = 45;
const TARGET_W: f64 = 48.0;
const TARGET_H: f64 = 44.0;
/// The wings (`ailesgoodies`, cut at 100 × 44: 3 frames) follow at (x − 24, y − 12).
const WING_DX: f64 = -24.0;
const WING_W: f64 = 100.0;
/// Setup: 99 stardust delays R(100) + 700 and one wing delay R(10) + 10 per target slot.
pub const STARDUST: usize = 99;

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
            spread: (SWATTER_W * params.spread_of_width) as i32,
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
    // shortcut: AR §8.3 does not say which swatter y the AI aligns; it aligns the hit box's top,
    // trunc(reach), which y moves monotonically.
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
            (swatter.x as i32, reach(swatter.y) as i32),
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
    pub swing: Swing,
    /// Ticks since the swatter's last move sound.
    pub move_ticks: i32,
    pub bar: Bar,
    pub ai: Option<BonusAi>,
    /// Coin wave amplitude A = trunc((bgH − 2.5 × coinH) / 2) (gaps §7.3).
    pub amplitude: i32,
    pub target_spawns: usize,
    /// The previous tick's swing, for the Tab edge.
    pub swing_held: bool,
    /// Stardust animation delays, one per particle.
    pub stardust_ms: Vec<i32>,
    /// Wing animation delays, one per target slot.
    pub wing_ms: [i32; TARGETS],
    /// `bckbonusfond`'s height; the field's y is 0.
    pub bg_h: i32,
    /// The fly's `Face` frame size.
    pub fly_size: (i32, i32),
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

/// `FUN_00431e58`: the piece's top-left lies in the box grown up and left by the piece's size,
/// inclusive.
fn caught(b: (f64, f64, f64, f64), piece: &Piece, size: (f64, f64)) -> bool {
    (b.0 - size.0..=b.0 + b.2).contains(&piece.x) && (b.1 - size.1..=b.1 + b.3).contains(&piece.y)
}

/// `FUN_00448302`: right is +x, up is −y.
fn fly_step(m: &mut BonusMover, pad: &FlyPad, bar_step: i32, bg_h: f64, h: f64) {
    let (cx, cy) = FLY_CAP;
    m.vx = axis(m.vx, pad.left, pad.right, tenth_of(cx), cx, Some(FLY_DECAY));
    m.vy = axis(m.vy, pad.up, pad.down, tenth_of(cy), cy, Some(FLY_DECAY));
    m.integrate(bar_step);
    m.x = if m.x < 1.0 { 0.0 } else { m.x.min(FLY_MAX_X) };
    m.y = m.y.clamp(-FLY_Y_MARGIN, bg_h + FLY_Y_MARGIN - h);
}

/// `FUN_00448c31`.
fn swatter_step(m: &mut BonusMover, pad: &FarmerPad, bar_step: i32) {
    let (ax, ay) = SWATTER_ACCEL;
    let (cx, cy) = SWATTER_CAP;
    m.vx = axis(m.vx, pad.left, pad.right, f64::from(ax), cx, None);
    m.vy = axis(m.vy, pad.up, pad.down, f64::from(ay), cy, None);
    m.integrate(bar_step);
    m.x = m.x.clamp(0.0, SWATTER_MAX_X);
    m.y = m.y.clamp(SWATTER_MIN_Y, SWATTER_MAX_Y);
}

impl BonusRound {
    /// Handicaps from [`crate::economy::Economy::meter`], countdown armed.
    pub fn new(m: &MatchState, cx: &mut Ctx) -> Self {
        let h_fly = handicap(m.economy.meter(Side::Fly));
        let h_farmer = handicap(m.economy.meter(Side::Farmer));
        let bg_h = cx.data.sizes.get("bckbonusfond").map_or(FIELD_H, |s| s.h);
        // `FUN_004452a1`: ω, the stardust and wing delays, then the ω re-roll, before the AI.
        let _initial_rate = cx.rng.r(60);
        let stardust_ms = (0..STARDUST).map(|_| cx.rng.r(100) + 700).collect();
        let wing_ms = std::array::from_fn(|_| cx.rng.r(10) + 10);
        let coin_rate = wave_rate(h_fly, cx);
        // shortcut: where the fly setup's R(300) falls in this order is not given (answers.md
        // "Bonus round movers"); taken after the ω re-roll, before the AI.
        let fly_x = f64::from(cx.rng.r(FLY_START_X_RAND));
        let fly_size = cx.data.sizes.get("Face").map_or(FLY_SIZE, |s| (s.w, s.h));
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
                x: fly_x,
                y: FLY_START_Y,
                ..BonusMover::default()
            },
            swatter: BonusMover {
                x: SWATTER_START_X,
                y: SWATTER_MIN_Y,
                ..BonusMover::default()
            },
            swing: Swing::default(),
            move_ticks: 0,
            bar: Bar {
                vx: BAR_VX,
                ..Bar::default()
            },
            ai,
            amplitude: ((f64::from(bg_h) - 2.5 * f64::from(COIN_H)) / 2.0) as i32,
            target_spawns: 0,
            swing_held: false,
            stardust_ms,
            wing_ms,
            bg_h,
            fly_size,
        }
    }

    /// After the result: match over → winner, else → goodies shop.
    ///
    /// answers.md "Bonus round: target right bound and the tick order": the update pass
    /// (`FUN_00446030`: the end-of-round check, the bar scroll, the fly, the swatter), then the
    /// items pass outside the countdown (`FUN_004472df`: spawns, targets, coins).
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        if matches!(self.phase, BonusPhase::End { .. } | BonusPhase::Result) {
            self.bar.brake();
        }
        if let BonusPhase::Play { round } = self.phase
            && round.expired(cx.now)
        {
            self.end(m, cx);
        }
        self.bar.tick();
        let playing = matches!(self.phase, BonusPhase::Play { .. });
        if playing {
            self.movers(keys, cx);
        }
        if !matches!(self.phase, BonusPhase::Countdown { .. }) {
            self.items(playing, cx);
        }
        match self.phase {
            BonusPhase::Countdown { count, mut next } => {
                if next.expired_rearm(cx.now) {
                    self.countdown(count - 1, next, cx);
                } else {
                    self.phase = BonusPhase::Countdown { count, next };
                }
            }
            BonusPhase::Play { .. } => {}
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

    /// The fly, then the swatter mover, or the swing while swinging. The human swings on the Tab
    /// press edge (gaps §2, bonus keys); the AI's request is a level, so in frenzy it swings
    /// whenever the cooldown allows (AR §8.3 step 7).
    fn movers(&mut self, keys: &KeyState, cx: &mut Ctx) {
        let step = self.bar.step;
        let (bg_h, fly_h) = (f64::from(self.bg_h), f64::from(self.fly_size.1));
        fly_step(&mut self.fly, &FlyPad::read(keys), step, bg_h, fly_h);
        let (pad, start) = match &mut self.ai {
            Some(ai) => {
                let pad = ai.pad(&self.swatter, &self.targets, self.h_farmer, cx);
                (pad, pad.swing)
            }
            None => {
                let pad = FarmerPad::read(keys);
                (pad, pad.swing && !self.swing_held)
            }
        };
        self.swing_held = pad.swing;
        if self.swing.frame == 0 {
            swatter_step(&mut self.swatter, &pad, step);
            self.move_sound(cx);
        }
        if self.swing.tick(start) {
            swing_sounds(cx);
        }
    }

    /// `FUN_004472df`: the coin spawn, the target spawn, then each target and each coin in slot
    /// order. Hits and pickups count only during play (AR §8.3 step 5).
    fn items(&mut self, counting: bool, cx: &mut Ctx) {
        if counting && self.coin_timer.expired_rearm(cx.now) {
            self.spawn_coin(cx);
        }
        if counting && self.target_timer.expired_rearm(cx.now) {
            self.spawn_target(cx);
        }
        for i in 0..TARGETS {
            self.target_step(i, counting, cx);
        }
        for i in 0..COINS {
            self.coin_step(i, counting, cx);
        }
    }

    /// `FUN_0044936a`: slot 22 + R(5) once the tick count passes 31, or 21 while vx ≠ 0.
    // shortcut: "every 31 ticks, or after 21" is read as count ≥ 31 (≥ 21 moving), then 0 [I].
    fn move_sound(&mut self, cx: &mut Ctx) {
        self.move_ticks += 1;
        let (idle, moving) = MOVE_SOUND_TICKS;
        let due = if self.swatter.vx == 0.0 { idle } else { moving };
        if self.move_ticks < due {
            return;
        }
        self.move_ticks = 0;
        let slot = SoundSlot::STEP_FIRST.0 + cx.rng.r(MOVE_SOUNDS) as u8;
        cx.emit(Event::play(SoundSlot(slot)));
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
        // shortcut: the launch x 810 is taken as the layer x (screen x 1012) [I].
        self.coins[slot] = Piece {
            active: true,
            layer_x: SPAWN_X,
            x: (SPAWN_X * COIN_LAYER).trunc(),
            y: f64::from(y),
            until: Timer::armed(cx.now, LIFETIME_MS),
            frame,
            anim_ms,
            ..Piece::default()
        };
    }

    /// Draws y = R(bg h), then x = 810 + R(150) (the field's y is 0); lifetime 25000 ms, and the
    /// target wander.
    fn spawn_target(&mut self, cx: &mut Ctx) {
        let Some(slot) = self.targets.iter().position(|t| !t.active) else {
            return;
        };
        let y = cx.rng.r(self.bg_h.max(1));
        let x = SPAWN_X + f64::from(cx.rng.r(TARGET_SPAWN_RAND));
        self.target_spawns += 1;
        self.targets[slot] = Piece {
            active: true,
            x,
            y: f64::from(y),
            until: Timer::armed(cx.now, LIFETIME_MS),
            mover: target_mover(self.h_farmer),
            ..Piece::default()
        };
    }

    /// One target: the lifetime check, Wander then the FPS integrator with the bar step as the
    /// camera delta, the bounds (x ≤ 4100 − 48 on the 1.0 layer, wrap 4000; y 15 px inside the
    /// field; the left bound is never applied), the wing removal (x − 24 + 100 < 0), then the
    /// swatter hit test while hitting.
    fn target_step(&mut self, i: usize, counting: bool, cx: &mut Ctx) {
        let bottom = f64::from(self.bg_h) - TARGET_BAND - TARGET_H;
        let (step, now) = (f64::from(self.bar.step), cx.now);
        let t = &mut self.targets[i];
        if !t.active {
            return;
        }
        if t.until.expired(now) {
            t.active = false;
            return;
        }
        let (dx, dy) = t.mover.step(cx);
        t.x = (t.x + f64::from(dx) + step).min(TARGET_MAX_X);
        t.y = (t.y + f64::from(dy)).clamp(TARGET_BAND, bottom);
        if t.x + WING_DX + WING_W < 0.0 {
            t.active = false;
            return;
        }
        if counting && self.swing.hitting() {
            self.swat(i, cx);
        }
    }

    /// One coin: its layer x moves by the bar step and its screen x = trunc(layer x × 1.25)
    /// (`FUN_004180af` with the bar's camera delta); it leaves once x + w < 0 or at the end of
    /// its lifetime; then the pickup.
    fn coin_step(&mut self, i: usize, counting: bool, cx: &mut Ctx) {
        let (step, now) = (f64::from(self.bar.step), cx.now);
        let c = &mut self.coins[i];
        if !c.active {
            return;
        }
        c.layer_x += step;
        c.x = (c.layer_x * COIN_LAYER).trunc();
        if c.x + f64::from(COIN_H) < 0.0 || c.until.expired(now) {
            c.active = false;
            return;
        }
        if counting {
            self.collect(i, cx);
        }
    }

    fn collect(&mut self, i: usize, cx: &mut Ctx) {
        let (w, h) = self.fly_size;
        let (dx, dy) = FLY_INSET;
        let fly = (
            self.fly.x + f64::from(dx),
            self.fly.y + f64::from(dy),
            f64::from(w - 2 * dx),
            f64::from(h - 2 * dy),
        );
        let c = f64::from(COIN_H);
        if !caught(fly, &self.coins[i], (c, c)) {
            return;
        }
        self.coins[i].active = false;
        self.coins_taken += 1;
        // Port decision: 50 plays when 49 is busy and 50 free (`FUN_0040cf55`); no slot state.
        let freq = 21_500 + cx.rng.r(500) as u32;
        cx.sound(SoundSlot::RING_A, play_at(freq));
        if self.coins_taken % 50 == 0 {
            cx.emit(Event::play(SoundSlot::CHEER_FLY));
        }
    }

    /// The box (x + 30, trunc(reach), 121, 154) against target `i`.
    fn swat(&mut self, i: usize, cx: &mut Ctx) {
        let s = &self.swatter;
        let sw = (
            s.x + SWAT_INSET,
            reach(s.y).trunc(),
            SWATTER_W - 2.0 * SWAT_INSET,
            SWAT_H,
        );
        if !caught(sw, &self.targets[i], (TARGET_W, TARGET_H)) {
            return;
        }
        self.targets[i].active = false;
        self.hits += 1;
        cx.emit(Event::play(SoundSlot::FLYING_GOODIE));
        if self.hits % 10 == 0 {
            cx.emit(Event::play(SoundSlot::CHEER_FARMER));
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

fn target_mover(h_farmer: f64) -> Wander {
    let y_speed = (TARGET_Y_SPEED_SPAN * h_farmer + 1.0) as i32;
    Wander::new(
        TARGET_REROLL_MS,
        (TARGET_X_SPEED, y_speed),
        TARGET_RUNS,
        TARGET_ACCEL,
    )
}

/// Frame 1 of a swing: sound 27 at R(15000) + 35000 Hz, then R(100) < 30 adds sound 34.
fn swing_sounds(cx: &mut Ctx) {
    let (base, spread) = SWING_FREQ;
    let freq = base + cx.rng.r(spread) as u32;
    cx.sound(SoundSlot::SWING, play_at(freq));
    if cx.rng.r(SHOUT_ROLL) < SHOUT_BELOW {
        cx.emit(Event::play(SoundSlot::VOICE_SHOUT));
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
    use crate::rng::MsvcRand;
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
        // Four 1000 ms steps, each due on the first tick strictly past its deadline: 1015 (tick
        // 66), 2030 (132; 2015 is not past), 3046 (198), 4061 (264; 4046 is not past).
        assert_eq!(play, 4061);
        assert!((35_000..35_020).contains(&(end - play)), "{}", end - play);
        // AR §0: a 50 ms timer re-arms from the tick it fires on, so every 4 ticks (61.5 ms).
        assert!((560..=575).contains(&b.coin_spawns), "{}", b.coin_spawns);
        // ≈ 99 firings at 23 ticks; a firing with all 10 slots taken spawns nothing.
        assert!((80..=100).contains(&b.target_spawns), "{}", b.target_spawns);
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
        // A 0 ms timer is due on the tick after it was armed (strictly before now).
        for _ in 0..2 {
            rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
        }
        assert_eq!(
            rig.events.last(),
            Some(&Event::play(SoundSlot::CHEER_FARMER))
        );
    }

    fn two_player_round(rig: &mut Rig) -> (MatchState, BonusRound) {
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let m = rig.cx(|cx| MatchState::new(config, false, cx));
        rig.rng = MsvcRand::new(1);
        let b = rig.cx(|cx| BonusRound::new(&m, cx));
        (m, b)
    }

    /// answers.md "Bonus round movers": ω R(60), 99 stardust R(100) + 700, 10 wings R(10) + 10,
    /// then the ω re-roll (h 0.5: (R(60) + 1) / 100), then the fly's start x R(300). Seed 1:
    /// the first stardust is 18467 % 100 + 700 = 767.
    #[test]
    fn the_setup_draws_the_stardust_and_wings_before_the_rate() {
        let mut rig = Rig::new(1);
        let (_, b) = two_player_round(&mut rig);
        let mut probe = MsvcRand::new(1);
        let draws: Vec<i32> = (0..112).map(|_| probe.rand()).collect();
        assert_eq!(b.stardust_ms.len(), 99);
        assert_eq!(b.stardust_ms[0], 767);
        assert_eq!(b.stardust_ms[98], draws[99] % 100 + 700);
        assert_eq!(b.wing_ms[0], draws[100] % 10 + 10);
        assert_eq!(b.wing_ms[9], draws[109] % 10 + 10);
        assert_eq!(b.coin_rate, f64::from(draws[110] % 60 + 1) / 100.0);
        assert_eq!((b.fly.x, b.fly.y), (f64::from(draws[111] % 300), 150.0));
        assert_eq!((b.swatter.x, b.swatter.y), (400.0, 351.0));
        assert_eq!(rig.rng, probe);
    }

    /// The coin's top-left must lie in [fx + 50 − 45, fx + 50 + 20] × [fy + 40 − 45, fy + 40 +
    /// 14], inclusive; the catch plays 49 at 21500 + R(500) (seed 1: 21541), one draw.
    #[test]
    fn a_coin_is_caught_by_its_top_left_in_the_grown_fly_box() {
        let mut rig = Rig::new(1);
        let (_, mut b) = two_player_round(&mut rig);
        (b.fly.x, b.fly.y) = (100.0, 200.0);
        let at = |x: f64, y: f64| Piece {
            active: true,
            x,
            y,
            ..Piece::default()
        };
        b.coins[0] = at(105.0, 195.0);
        b.coins[1] = at(170.0, 254.0);
        b.coins[2] = at(104.0, 195.0);
        b.coins[3] = at(171.0, 254.0);
        b.coins[4] = at(170.0, 255.0);
        rig.rng = MsvcRand::new(1);
        let before = rig.events.len();
        rig.cx(|cx| (0..5).for_each(|i| b.collect(i, cx)));
        let taken: Vec<bool> = b.coins[..5].iter().map(|c| !c.active).collect();
        assert_eq!(taken, [true, true, false, false, false]);
        let ring = Event::Sound {
            slot: SoundSlot::RING_A,
            op: play_at(21_541),
        };
        assert_eq!(rig.events[before], ring);
        let mut probe = MsvcRand::new(1);
        probe.rand();
        probe.rand();
        assert_eq!(rig.rng, probe);
    }

    /// Seed 1: y = 41 % 440 = 41, then x = 810 + 18467 % 150 = 827.
    #[test]
    fn a_target_spawns_at_a_random_height_past_the_right_edge() {
        let mut rig = Rig::new(1);
        let (_, mut b) = two_player_round(&mut rig);
        rig.rng = MsvcRand::new(1);
        let now = rig.now();
        rig.cx(|cx| b.spawn_target(cx));
        let t = b.targets[0];
        assert_eq!((t.active, t.x, t.y), (true, 827.0, 41.0));
        assert_eq!(t.until.remaining_ms(now), LIFETIME_MS);
    }

    /// answers.md "Bonus round movers": after the spawn above (seed 1), the first drift draws x:
    /// timer 6334, d 26500 % 10 = 0 (v not < 0: −1), 19169 % 1 → −65 − 5 = −70; y: timer 15724,
    /// d 8 (+1), 29358 % trunc(500 × 0.5 + 1) = 242 → 307 + 5 = 312. The FPS step is (−1, 4).
    #[test]
    fn a_target_drifts_under_the_wander_rule() {
        let mut rig = Rig::new(1);
        let (_, mut b) = two_player_round(&mut rig);
        rig.rng = MsvcRand::new(1);
        rig.cx(|cx| b.spawn_target(cx));
        rig.cx(|cx| b.target_step(0, false, cx));
        let t = b.targets[0];
        assert_eq!((t.mover.v, t.x, t.y), ([-70, 312], 826.0, 45.0));
        assert_eq!(rig.rng.rand(), 26962);
    }

    /// Bounds top 15 and bottom 440 − 15 − 44 = 381. Seed 1, both at rest: target 0 x d 7 → 70,
    /// y d 9, 15724 % 251 = 162 → 232 (3 px, 13 → 15); target 1 x d 8 → 70, y d 5,
    /// 28145 % 251 = 33 → 103 (1 px, 401 → 381).
    #[test]
    fn targets_are_clamped_to_the_field_bands() {
        let mut rig = Rig::new(1);
        let (_, mut b) = two_player_round(&mut rig);
        rig.rng = MsvcRand::new(1);
        for (i, y) in [(0, 10.0), (1, 400.0)] {
            b.targets[i] = Piece {
                active: true,
                x: 500.0,
                y,
                until: Timer::armed(0, LIFETIME_MS),
                mover: target_mover(b.h_farmer),
                ..Piece::default()
            };
        }
        rig.cx(|cx| (0..2).for_each(|i| b.target_step(i, false, cx)));
        let got: Vec<_> = b.targets[..2].iter().map(|t| (t.x, t.y)).collect();
        assert_eq!(got, [(501.0, 15.0), (501.0, 381.0)]);
    }

    /// The swatter at (400, 472) has reach 375 and hits through (430, 375, 121, 154) grown by
    /// the 48 × 44 target: x 382..=551, y 331..=529.
    #[test]
    fn a_target_is_hit_by_its_top_left_in_the_grown_swatter_box() {
        let mut rig = Rig::new(1);
        let (_, mut b) = two_player_round(&mut rig);
        (b.swatter.x, b.swatter.y) = (400.0, 472.0);
        let at = |x: f64, y: f64| Piece {
            active: true,
            x,
            y,
            ..Piece::default()
        };
        b.targets[0] = at(382.0, 331.0);
        b.targets[1] = at(551.0, 529.0);
        b.targets[2] = at(381.0, 331.0);
        b.targets[3] = at(552.0, 529.0);
        b.targets[4] = at(551.0, 530.0);
        rig.cx(|cx| (0..5).for_each(|i| b.swat(i, cx)));
        let hit: Vec<bool> = b.targets[..5].iter().map(|t| !t.active).collect();
        assert_eq!(hit, [true, true, false, false, false]);
        assert_eq!(b.hits, 2);
    }

    /// answers.md "Bonus round movers": reach 0 at y 351, 375 at y 472; y 412 gives
    /// 61 / 121 × 375 = 189.05.
    #[test]
    fn the_reach_runs_0_to_375_with_the_swatter_y() {
        assert_eq!(reach(SWATTER_MIN_Y), 0.0);
        assert_eq!(reach(SWATTER_MAX_Y), 375.0);
        assert_eq!(reach(412.0).trunc(), 189.0);
    }

    /// The fly: +trunc(v + 89.0000013) per held tick to the 890 cap (ten ticks), +90 from −100,
    /// a released axis decays by 9. y adds 67.000001 to 670.
    #[test]
    fn the_fly_accelerates_by_a_tenth_of_its_cap_and_decays_by_9() {
        let a = tenth_of(890);
        let held: Vec<f64> = (0..10)
            .scan(0.0, |v, _| {
                *v = axis(*v, false, true, a, 890, Some(9));
                Some(*v)
            })
            .collect();
        assert_eq!(held[..3], [89.0, 178.0, 267.0]);
        assert_eq!(held[9], 890.0);
        assert_eq!(axis(890.0, false, true, a, 890, Some(9)), 890.0);
        assert_eq!(axis(-100.0, false, true, a, 890, Some(9)), -10.0);
        assert_eq!(axis(-100.0, true, false, a, 890, Some(9)), -189.0);
        assert_eq!(axis(890.0, false, false, a, 890, Some(9)), 881.0);
        assert_eq!(axis(-5.0, false, false, a, 890, Some(9)), 0.0);
        assert_eq!(axis(0.0, true, false, tenth_of(670), 670, Some(9)), -67.0);
    }

    /// The swatter: ±400 / ±200 per held tick, capped at 900 / 400, zeroed on release.
    #[test]
    fn the_swatter_accelerates_in_steps_and_stops_on_release() {
        let x: Vec<f64> = (0..3)
            .scan(0.0, |v, _| {
                *v = axis(*v, false, true, 400.0, 900, None);
                Some(*v)
            })
            .collect();
        assert_eq!(x, [400.0, 800.0, 900.0]);
        assert_eq!(axis(900.0, false, false, 400.0, 900, None), 0.0);
        assert_eq!(axis(-200.0, true, false, 200.0, 400, None), -400.0);
    }

    /// −600 px/s over 65 ticks: 9 px with a 15 carry, so the first step is −9. The brake adds 2
    /// per tick: −600 + 2 × 259 = −82, and the 260th gives −80 > −81, so 0. The step then stays.
    #[test]
    fn the_bar_steps_brakes_to_0_and_keeps_its_last_step() {
        let mut bar = Bar {
            vx: BAR_VX,
            ..Bar::default()
        };
        bar.tick();
        assert_eq!((bar.step, bar.x), (-9, -9));
        for _ in 0..259 {
            bar.brake();
        }
        assert_eq!(bar.vx, -82);
        bar.brake();
        assert_eq!(bar.vx, 0);
        let step = bar.step;
        bar.tick();
        assert_eq!((bar.step, bar.x), (step, -9));
    }

    /// The fly at x 100 with vx 89 (1 px this tick) and the bar step −9 lands on 92; x stays in
    /// [0, 680] and y in [−15, 440 + 15 − 94].
    #[test]
    fn the_fly_moves_with_the_bar_step_inside_its_bounds() {
        let pad = FlyPad {
            right: true,
            ..FlyPad::default()
        };
        let mut fly = BonusMover {
            x: 100.0,
            y: 150.0,
            ..BonusMover::default()
        };
        fly_step(&mut fly, &pad, -9, 440.0, 94.0);
        assert_eq!((fly.x, fly.y, fly.vx), (92.0, 150.0, 89.0));
        let mut edge = BonusMover {
            x: 5.0,
            y: 360.0,
            vy: 670.0,
            ..BonusMover::default()
        };
        fly_step(&mut edge, &FlyPad::default(), -9, 440.0, 94.0);
        assert_eq!((edge.x, edge.y), (0.0, 361.0));
    }

    /// Swing from rest (seed 1): frames 1, 2, 3, 4, 4 (retract), 3, 2, 1, 0; it hits on 3 and 4
    /// only. The start plays 27 at 35000 + 41 % 15000 = 35041 Hz; 18467 % 100 = 67 ≥ 30, so no
    /// shout. The next swing needs 21 idle ticks: ticks 9..=28 are refused, 29 starts.
    #[test]
    fn a_swing_climbs_hits_on_frames_3_and_4_retracts_and_cools_down() {
        let mut s = Swing::default();
        let mut frames = Vec::new();
        let mut hits = Vec::new();
        let mut starts = Vec::new();
        for tick in 0..30 {
            if s.tick(true) {
                starts.push(tick);
            }
            frames.push(s.frame);
            hits.push(s.hitting());
        }
        assert_eq!(frames[..9], [1, 2, 3, 4, 4, 3, 2, 1, 0]);
        let hit_ticks: Vec<usize> = (0..9).filter(|&t| hits[t]).collect();
        assert_eq!(hit_ticks, [2, 3]);
        assert_eq!(starts, [0, 29]);
        let mut rig = Rig::new(1);
        rig.cx(swing_sounds);
        let swing = Event::Sound {
            slot: SoundSlot::SWING,
            op: play_at(35_041),
        };
        assert_eq!(rig.events, [swing]);
    }

    /// A coin's layer x moves by the bar step: 810 − 9 = 801, screen x trunc(801 × 1.25) = 1001.
    /// A target (1.0 layer) moves by its wander step plus the bar step: seed 1 from rest gives
    /// vx 70 (x d 7, +1, 65 + 5), 1 px, so 500 + 1 − 9 = 492.
    #[test]
    fn coins_and_targets_move_by_the_bar_step() {
        let mut rig = Rig::new(1);
        let (_, mut b) = two_player_round(&mut rig);
        b.coins[0] = Piece {
            active: true,
            layer_x: SPAWN_X,
            x: 1012.0,
            until: Timer::armed(0, LIFETIME_MS),
            ..Piece::default()
        };
        b.targets[0] = Piece {
            active: true,
            x: 500.0,
            y: 100.0,
            until: Timer::armed(0, LIFETIME_MS),
            mover: target_mover(b.h_farmer),
            ..Piece::default()
        };
        b.bar.tick();
        rig.rng = MsvcRand::new(1);
        rig.cx(|cx| {
            b.coin_step(0, false, cx);
            b.target_step(0, false, cx);
        });
        assert_eq!((b.coins[0].layer_x, b.coins[0].x), (801.0, 1001.0));
        assert_eq!(b.targets[0].x, 492.0);
    }

    /// answers.md "Bonus round: target right bound": x ≤ 4100 − 48 = 4052. Seed 1 from rest the
    /// target moves +1 px (as above) with no bar step, so 4052 stays 4052.
    #[test]
    fn a_target_stops_at_the_right_bound() {
        let mut rig = Rig::new(1);
        let (_, mut b) = two_player_round(&mut rig);
        b.targets[0] = Piece {
            active: true,
            x: 4052.0,
            y: 100.0,
            until: Timer::armed(0, LIFETIME_MS),
            mover: target_mover(b.h_farmer),
            ..Piece::default()
        };
        rig.rng = MsvcRand::new(1);
        rig.cx(|cx| b.target_step(0, false, cx));
        assert_eq!((b.targets[0].x, b.targets[0].y), (4052.0, 103.0));
    }

    /// A two-player round in play, both spawners far off.
    fn playing(rig: &mut Rig) -> (MatchState, BonusRound) {
        let (m, mut b) = two_player_round(rig);
        let now = rig.now();
        b.phase = BonusPhase::Play {
            round: Timer::armed(now, ROUND_MS),
        };
        b.coin_timer = Timer::armed(now, 1_000_000);
        b.target_timer = Timer::armed(now, 1_000_000);
        (m, b)
    }

    /// The spawns run after the bar scroll and before the coin movers, so a new coin moves by
    /// the bar step on its spawn tick: layer x 810 − 9.
    #[test]
    fn a_new_coin_moves_on_its_spawn_tick() {
        let mut rig = Rig::new(1);
        let (mut m, mut b) = playing(&mut rig);
        b.coin_timer = Timer::armed(rig.now(), 0);
        for _ in 0..2 {
            rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
        }
        assert_eq!(b.coin_spawns, 1);
        assert_eq!(b.bar.step, -9);
        assert_eq!(b.coins[0].layer_x, 801.0);
    }

    /// "the swatter mover, or the swing while swinging": mid-swing the swatter does not move,
    /// not even by the bar step.
    #[test]
    fn the_swatter_holds_still_while_swinging() {
        let mut rig = Rig::new(1);
        let (mut m, mut b) = playing(&mut rig);
        b.swing = Swing {
            frame: 2,
            retract: false,
            cooldown: 0,
        };
        rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
        assert_eq!((b.swatter.x, b.swing.frame), (400.0, 3));
        rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
        b.swing = Swing::default();
        rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
        assert!(b.swatter.x < 400.0, "{}", b.swatter.x);
    }

    /// AR §8.3: in frenzy the AI swings every tick, so its request is a level, not an edge: a
    /// swing starts on tick 0 and again on tick 29, as soon as the cooldown passes 20.
    #[test]
    fn the_frenzy_ai_swings_again_after_the_cooldown() {
        let mut rig = Rig::new(1);
        let (mut m, mut b) = playing(&mut rig);
        let now = rig.now();
        let mut ai = rig.cx(|cx| BonusAi::new(AiLevel::Normal, b.h_farmer, cx));
        (ai.frenzy, ai.frenzy_timer) = (true, Timer::armed(now, 1_000_000));
        b.ai = Some(ai);
        b.targets[0] = Piece {
            active: true,
            x: 3000.0,
            y: 100.0,
            until: Timer::armed(now, LIFETIME_MS),
            mover: target_mover(b.h_farmer),
            ..Piece::default()
        };
        let starts: Vec<usize> = (0..40)
            .filter(|_| {
                let idle = b.swing.frame == 0;
                rig.step(&[], |k, cx| b.tick(k, &mut m, cx));
                idle && b.swing.frame == 1
            })
            .collect();
        assert_eq!(starts, [0, 29]);
    }
}
