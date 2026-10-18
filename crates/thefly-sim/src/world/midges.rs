//! W2. The Midges goodie's swarm (fly-and-world §8, ai-and-rules §6.3) and the ambient midges
//! (tick step 14). Ticks steps 14 and 18a.

use super::collision::Rect;
use super::farmer::{Farmer, SPRITE_H, SPRITE_W};
use super::traps::Traps;
use super::{OBJECT_PARALLAX, W, World};
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::mover::Wander;

pub const SWARM_SLOTS: usize = 300;
/// Each use adds min(this, 300 − active) midges.
pub const SWARM_SPAWN: usize = 100;
/// A swatter strike kills at most this many touched midges.
pub const SWAT_KILLS: usize = 20;
/// A fan pushes a touching midge by this on each axis: + above 0, − at or below it.
pub const TRAP_PUSH: i32 = 180;
/// Per-tick chance (%) to re-roll a midge's homing point.
pub const RETARGET_PERCENT: i32 = 30;
/// The retarget roll is R(10) < 3 (gaps §8.1).
pub const RETARGET_ROLL: i32 = 10;
pub const RETARGET_BELOW: i32 = RETARGET_PERCENT / 10;
/// FW §8.1 spawn: mover R(150) ms, speeds 200/200, runs 5/3; acceleration 0 (answers.md
/// "Midges: spawn x, homing gates, ambient update and constructor").
pub const MOVER_REROLL_MS: i32 = 150;
pub const MOVER_SPEED: i32 = 200;
pub const MOVER_RUNS: (i32, i32) = (5, 3);
pub const MOVER_ACCEL: (i32, i32) = (0, 0);
/// answers.md "Midges: spawn x, ...": x = R(W − 810) + 800; the sign is R(10) < 5 → −1;
/// |vx| = R(200) + 200; frame period R(5) + 30 ms; y = R(600); a timer R(500) + 500.
pub const SPAWN_X_MARGIN: i32 = 810;
pub const SPAWN_X_BASE: i32 = 800;
pub const SIGN_ROLL: i32 = 10;
pub const SIGN_BELOW: i32 = 5;
pub const SPAWN_Y_RAND: i32 = 600;
pub const SPAWN_VX_RAND: i32 = 200;
pub const SPAWN_VX_BASE: i32 = 200;
/// R(5) + 30 is the animation frame period in ms, not a size (answers.md "Midge wrap, bounds and
/// frame period (round 5)").
pub const FRAME_MS_RAND: i32 = 5;
pub const FRAME_MS_BASE: i32 = 30;
pub const SPAWN_TIMER_RAND: i32 = 500;
pub const SPAWN_TIMER_BASE: u64 = 500;
/// Homing offsets R(181) and R(farmerH − 60); tolerances R(30).
pub const HOME_Y_MARGIN: i32 = 60;
pub const TOLERANCE_RAND: i32 = 30;
/// answers.md "Midge homing": when R(100) > 50, sx = R(200) + |v0| and sy = R(60) + |v0 − 180|.
pub const HOMING_ROLL: i32 = 100;
pub const HOMING_ABOVE: i32 = 50;
pub const HOMING_X_RAND: i32 = 200;
pub const HOMING_Y_RAND: i32 = 60;
pub const HOMING_Y_OFFSET: i32 = 180;
/// answers.md "Midge homing: ring bands (round 4)": the ring is the midge's wrap (+0x2c); a band
/// is half of it. Only goodie midges home, and their 1.0 layer gives +0x2c = 4000 (answers.md
/// "Midge wrap, bounds and frame period (round 5)").
pub const HOMING_RING: i32 = W;
pub const HOMING_BAND: i32 = HOMING_RING / 2;
/// gaps §8.1 contact routine: a 700 ms stun; a zapped midge falls with y acceleration 30; the
/// zapper test is at (x, y + 7); a respawn draws x = R(500) + 1000, y = R(H − h) + 20.
pub const STUN_MS: u64 = 700;
pub const FALL_ACCEL: i32 = 30;
pub const ARC_TEST_DY: i32 = 7;
pub const RESPAWN_X_RAND: i32 = 500;
pub const RESPAWN_X_BASE: i32 = 1000;
pub const RESPAWN_Y_BASE: i32 = 20;
/// answers.md "Midges: spawn x, ..., ambient update and constructor": slot 0 at x 900, vx +60;
/// then count = R(15) and slots 1..count − 1 at x = R(W), |vx| = R(30) + 45; each with frame
/// period R(5) + 30, y = R(600) and a timer R(500) + 800. Parameters 100 / 10 / 50 / 50 / 30 / 0:
/// re-roll R(100) ms, speeds R(10) + 65 and R(50) + 65, runs 50 and 30, accel 0 (answers.md
/// "Ambient midge update `FUN_00442960` (round 5)").
pub const AMBIENT_FIRST_X: i32 = 900;
pub const AMBIENT_FIRST_VX: i32 = 60;
pub const AMBIENT_COUNT_RAND: i32 = 15;
pub const AMBIENT_VX_RAND: i32 = 30;
pub const AMBIENT_VX_BASE: i32 = 45;
pub const AMBIENT_TIMER_BASE: u64 = 800;
pub const AMBIENT_REROLL_MS: i32 = 100;
pub const AMBIENT_SPEED: (i32, i32) = (10, 50);
pub const AMBIENT_RUNS: (i32, i32) = (50, 30);
pub const AMBIENT_ACCEL: (i32, i32) = (0, 0);
/// Ambient midges sit on the 1.25 layer: x is a screen x, the motion runs in layer px.
pub const AMBIENT_LAYER: f64 = OBJECT_PARALLAX;
/// The 27 × 27 frame (`FUN_00413ce0(0x1b, 0x1b)`).
pub const MIDGE_FRAME: i32 = 27;
/// `FUN_004171bd(-1000, 9000, 0, 600)` as coded bounds y to 0..600 − 27 and never x (answers.md
/// "Midge wrap, bounds and frame period (round 5)").
pub const AMBIENT_Y_BOUNDS: (i32, i32) = (0, WORLD_H - MIDGE_FRAME);
/// The jitter, once per expiry of the midge's timer: R(100) > 70, then m = R(15) and an R(10)
/// sign, vx = sign × m − vx.
pub const JITTER_ROLL: i32 = 100;
pub const JITTER_ABOVE: i32 = 70;
pub const JITTER_RAND: i32 = 15;
/// A pushed v steps this much per tick toward zero, then snaps to the saved v once inside it.
pub const PUSH_DECAY: i32 = 40;
/// The world is one 600 px screen tall (game-spec §3).
pub const WORLD_H: i32 = 600;
pub const MIDGE_SPRITE: &str = "moucheron";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Midge {
    pub active: bool,
    /// Caught this tick: it runs the rest of the tick and is dropped at the next tick's
    /// compaction (answers.md "Goodie-midge removal").
    pub dead: bool,
    /// Screen x (+0xc), in [0, trunc(4000 × f)) once the mover has run; the collision tests and
    /// the frontend read it.
    pub x: i32,
    /// Layer x, wrapped into [0, 4000); the motion and the camera delta move it.
    pub layer_x: i32,
    pub y: i32,
    /// The animation frame period, R(5) + 30 ms.
    pub frame_ms: i32,
    /// The facing frame set: +0x186 (left) when the jitter leaves vx < 0, else +0x184.
    pub facing_left: bool,
    /// The velocity is `mover.v`.
    pub(crate) mover: Wander,
    /// The spawn vx (v0), the base of the homing speeds.
    pub v0: i32,
    /// Homing point on the farmer: offsets R(farmerW), R(farmerH − 60); tolerances R(30).
    pub home_dx: i32,
    pub home_dy: i32,
    pub tol_x: i32,
    pub tol_y: i32,
    /// The hit state: swatted, motionless until the 700 ms timer ends.
    pub stunned: bool,
    pub stun_timer: Timer,
    /// Zapped (+0x204): drops with y acceleration 30 until below the world.
    pub falling: bool,
    /// The v saved by the last fan push; the decay snaps v back to it.
    pub saved_v: Option<[i32; 2]>,
    /// The constructor's timer (R(500) + 500, ambient R(500) + 800); ambient midges jitter on
    /// each expiry.
    pub timer: Timer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Swarm {
    /// game+0x407d0.
    pub on: bool,
    pub slots: Vec<Midge>,
    /// Midges spawned in this swarm (the slowdown's denominator).
    pub total: i32,
}

impl Swarm {
    pub fn new() -> Self {
        Self {
            on: false,
            slots: vec![Midge::default(); SWARM_SLOTS],
            total: 0,
        }
    }

    /// `FUN_00404caf`: add min(100, 300 − active) midges.
    pub fn spawn(&mut self, cx: &mut Ctx) {
        self.on = true;
        let free = self.slots.iter().filter(|m| !m.active).count();
        let n = SWARM_SPAWN.min(free);
        for m in self.slots.iter_mut().filter(|m| !m.active).take(n) {
            *m = Midge::spawn(cx);
        }
        self.total += i32::try_from(n).unwrap_or(0);
    }
}

impl Default for Swarm {
    fn default() -> Self {
        Self::new()
    }
}

impl Midge {
    /// `FUN_00404caf`'s per-midge draws, in rand order (answers.md "Midge spawn draw order",
    /// amended by "Midges: spawn x, ..."). The x tolerance is the second R(30), the y
    /// tolerance the first. Accel flag x is set, so x never re-rolls under Wander.
    fn spawn(cx: &mut Ctx) -> Self {
        let x = cx.rng.r(W - SPAWN_X_MARGIN) + SPAWN_X_BASE;
        let vx = sign_roll(cx) * (cx.rng.r(SPAWN_VX_RAND) + SPAWN_VX_BASE);
        let frame_ms = cx.rng.r(FRAME_MS_RAND) + FRAME_MS_BASE;
        let y = cx.rng.r(SPAWN_Y_RAND);
        let timer = midge_timer(SPAWN_TIMER_BASE, cx);
        let tol_y = cx.rng.r(TOLERANCE_RAND);
        let tol_x = cx.rng.r(TOLERANCE_RAND);
        let home_dy = cx.rng.r(SPRITE_H - HOME_Y_MARGIN);
        let home_dx = cx.rng.r(SPRITE_W);
        let mut mover = Wander::new(
            MOVER_REROLL_MS,
            (MOVER_SPEED, MOVER_SPEED),
            MOVER_RUNS,
            MOVER_ACCEL,
        )
        .with_accel_flags(true, false);
        mover.v[0] = vx;
        Self {
            active: true,
            x,
            layer_x: x,
            y,
            frame_ms,
            mover,
            v0: vx,
            home_dx,
            home_dy,
            tol_x,
            tol_y,
            timer,
            ..Self::default()
        }
    }

    /// One ambient midge of the constructor: frame period, y and timer after the caller's x and
    /// vx. x is a screen x, set through the screen-x setter (answers.md "Midge wrap, ...").
    fn ambient(x: i32, vx: i32, cx: &mut Ctx) -> Self {
        let frame_ms = cx.rng.r(FRAME_MS_RAND) + FRAME_MS_BASE;
        let y = cx.rng.r(SPAWN_Y_RAND);
        let timer = midge_timer(AMBIENT_TIMER_BASE, cx);
        let mut mover = Wander::new(
            AMBIENT_REROLL_MS,
            AMBIENT_SPEED,
            AMBIENT_RUNS,
            AMBIENT_ACCEL,
        );
        mover.v[0] = vx;
        Self {
            active: true,
            x,
            layer_x: layer_of(x, AMBIENT_LAYER),
            y,
            frame_ms,
            mover,
            v0: vx,
            timer,
            ..Self::default()
        }
    }

    fn rect(&self, cx: &Ctx) -> Rect {
        let s = cx.data.size(MIDGE_SPRITE);
        Rect::new(self.x, self.y, s.w, s.h)
    }

    /// The zapper tests a midge at (x, y + 7) (gaps §8.1).
    fn arc_target(&self, cx: &Ctx) -> Rect {
        let r = self.rect(cx);
        Rect {
            y: r.y + ARC_TEST_DY,
            ..r
        }
    }

    /// `FUN_00442f95`, after the contact step (answers.md "Midge homing: ring bands (round 4)").
    /// A stunned or falling midge does nothing. Else R(100) > 50 rolls this tick's speeds; x
    /// steers by its ring band, y outside ±tol_y. v is written only on a rolled tick; the accel
    /// flags, set on every tick, keep a steered axis from re-rolling under Wander.
    fn home(&mut self, farmer: &Farmer, cx: &mut Ctx) {
        if self.stunned || self.falling {
            return;
        }
        let rolled = cx.rng.r(HOMING_ROLL) > HOMING_ABOVE;
        let (sx, sy) = if rolled {
            let sx = cx.rng.r(HOMING_X_RAND) + self.v0.abs();
            (
                sx,
                cx.rng.r(HOMING_Y_RAND) + (self.v0 - HOMING_Y_OFFSET).abs(),
            )
        } else {
            (0, 0)
        };
        // q = (d + ring) / (ring / 2) with band edges 0..4; integer division gives the band.
        let d = self.x - (farmer.x + self.home_dx);
        let x_dir = match (d + HOMING_RING).div_euclid(HOMING_BAND) {
            0 | 2 => Some(-1),
            1 | 3 => Some(1),
            _ => None,
        };
        let target_y = farmer.y + self.home_dy;
        let y_dir = if self.y < target_y - self.tol_y {
            Some(1)
        } else if self.y > target_y + self.tol_y {
            Some(-1)
        } else {
            None
        };
        if let Some(dir) = x_dir.filter(|_| sx != 0) {
            self.mover.v[0] = dir * sx;
        }
        if let Some(dir) = y_dir.filter(|_| sy != 0) {
            self.mover.v[1] = dir * sy;
        }
        self.mover = self
            .mover
            .with_accel_flags(x_dir.is_some(), y_dir.is_some());
    }

    /// `FUN_00442960` steps 3–4 on a layer of parallax `factor`: a stunned midge holds still, a
    /// zapped one drops, else the push decays and the wander mover moves it. The mover
    /// (`FUN_004180af`) adds the camera delta to the layer x and wraps it into [0, 4000); screen
    /// x = trunc(layer x × factor) (answers.md "Screen-x setter", "Midge wrap, ... (round 5)").
    // shortcut: answers.md does not say whether a stunned or falling midge's layer mover runs;
    // the port scrolls it with the camera like every layer sprite. Revisit if that is decoded.
    fn update(&mut self, factor: f64, camera_delta: f64, cx: &mut Ctx) {
        let (dx, dy) = self.motion(cx);
        self.layer_x = (self.layer_x + dx + (-camera_delta) as i32).rem_euclid(W);
        self.x = (f64::from(self.layer_x) * factor) as i32;
        self.y += dy;
    }

    fn motion(&mut self, cx: &mut Ctx) -> (i32, i32) {
        if self.falling {
            return self.fall();
        }
        if self.stunned {
            return (0, 0);
        }
        self.decay();
        self.mover.step(cx)
    }

    /// `FUN_00442960` step 3, per axis: v steps [`PUSH_DECAY`] toward zero, then snaps to the
    /// saved v once |v| ≤ |saved|. The push ends when both axes are back; with a saved 0 it can
    /// oscillate until the jitter ends it.
    fn decay(&mut self) {
        let Some(saved) = self.saved_v else {
            return;
        };
        for (v, s) in self.mover.v.iter_mut().zip(saved) {
            *v += if *v < 0 { PUSH_DECAY } else { -PUSH_DECAY };
            if v.abs() <= s.abs() {
                *v = s;
            }
        }
        if self.mover.v == saved {
            self.saved_v = None;
        }
    }

    fn fall(&mut self) -> (i32, i32) {
        self.mover.v = [0, self.mover.v[1] + FALL_ACCEL];
        self.mover.integrate()
    }

    /// `FUN_00442960` for an ambient midge (answers.md "Ambient midge update `FUN_00442960`
    /// (round 5)"): the jitter timer, then the goodie update on the 1.25 layer, then the y bound
    /// while not falling.
    fn ambient_update(&mut self, camera_delta: f64, cx: &mut Ctx) {
        self.jitter_tick(cx);
        self.update(AMBIENT_LAYER, camera_delta, cx);
        if !self.falling {
            self.y = self.y.clamp(AMBIENT_Y_BOUNDS.0, AMBIENT_Y_BOUNDS.1);
        }
    }

    /// Steps 1–2: the timer is checked and re-armed on every tick; only on an expiry while free
    /// R(100) > 70 ends an active push, then vx = sign × R(15) − vx and the facing follows vx.
    fn jitter_tick(&mut self, cx: &mut Ctx) {
        let fired = self.timer.expired_rearm(cx.now);
        if !fired || self.stunned || self.falling {
            return;
        }
        if cx.rng.r(JITTER_ROLL) <= JITTER_ABOVE {
            return;
        }
        if let Some(saved) = self.saved_v.take() {
            self.mover.v = saved;
        }
        let m = cx.rng.r(JITTER_RAND);
        self.mover.v[0] = sign_roll(cx) * m - self.mover.v[0];
        self.facing_left = self.mover.v[0] < 0;
    }

    /// gaps §8.1 step 1: the respawn condition.
    fn due_respawn(&self, cx: &Ctx) -> bool {
        let stun_over = self.stunned && self.stun_timer.expired(cx.now) && !self.falling;
        stun_over || (self.falling && self.y > WORLD_H)
    }

    /// gaps §8.1 step 1: back to flight at a fresh spot.
    // shortcut: answers.md names the screen-x setter for the spawn x, not the respawn's; the port
    // sets the respawn x the same way. Revisit if FUN_00442bb3's setter call is documented.
    fn respawn(&mut self, factor: f64, cx: &mut Ctx) {
        let h = cx.data.size(MIDGE_SPRITE).h;
        (self.stunned, self.falling) = (false, false);
        self.stun_timer.disarm();
        self.x = cx.rng.r(RESPAWN_X_RAND) + RESPAWN_X_BASE;
        self.layer_x = layer_of(self.x, factor);
        self.y = cx.rng.r(WORLD_H - h) + RESPAWN_Y_BASE;
    }

    /// gaps §8.1 `FUN_00442bb3`: respawn, else swatted, else zapped. Returns the hit-state flag
    /// (0 after a respawn) and whether the midge respawned.
    fn contact(
        &mut self,
        swatter: Option<Rect>,
        zapped: bool,
        factor: f64,
        cx: &mut Ctx,
    ) -> (bool, bool) {
        if self.due_respawn(cx) {
            self.respawn(factor, cx);
            return (false, true);
        }
        let rect = self.rect(cx);
        if swatter.is_some_and(|b| b.catches(&rect)) {
            self.stunned = true;
            self.mover.v = [0, 0];
            self.stun_timer.arm(cx.now, STUN_MS);
        } else if zapped {
            self.falling = true;
        }
        (self.stunned, false)
    }

    /// gaps §8.1 step 2.3: four draws, tolerance A, tolerance B, y offset, x offset.
    fn retarget(&mut self, cx: &mut Ctx) {
        let a = cx.rng.r(TOLERANCE_RAND);
        let b = cx.rng.r(TOLERANCE_RAND);
        self.home_dy = cx.rng.r(SPRITE_H - HOME_Y_MARGIN);
        self.home_dx = cx.rng.r(SPRITE_W);
        (self.tol_x, self.tol_y) = (b, a);
    }

    /// `FUN_00427885` (FW §8.2 step 3): a midge in a fan is pushed.
    fn fan_push(&mut self, traps: &Traps, cx: &Ctx) {
        if traps.fan_touches(self.x, self.y, cx) {
            self.push();
        }
    }

    /// Save v, then add +180 to a component above 0 and −180 to the others.
    fn push(&mut self) {
        self.saved_v = Some(self.mover.v);
        self.mover.v = self
            .mover
            .v
            .map(|v| if v > 0 { v + TRAP_PUSH } else { v - TRAP_PUSH });
    }
}

/// R(10) < 5 → −1, else +1.
fn sign_roll(cx: &mut Ctx) -> i32 {
    if cx.rng.r(SIGN_ROLL) < SIGN_BELOW {
        -1
    } else {
        1
    }
}

/// The screen-x setter `FUN_00417fab`: layer x = trunc(screen x / factor) for 0 ≤ x < wrap.
fn layer_of(x: i32, factor: f64) -> i32 {
    (f64::from(x) / factor) as i32
}

/// A midge's constructor timer, R(500) + `base` ms.
fn midge_timer(base: u64, cx: &mut Ctx) -> Timer {
    let ms = u64::try_from(cx.rng.r(SPAWN_TIMER_RAND)).unwrap_or(0);
    Timer::armed(cx.now, ms + base)
}

/// The cosmetic population at game+0x15a20 that reacts to the swatter and fans. It never homes
/// and is never removed (answers.md "Ambient midge population (O2)").
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AmbientMidges {
    pub slots: Vec<Midge>,
}

impl AmbientMidges {
    /// Period load: slot 0, then slots 1..R(15) − 1, so max(count, 1) midges, all moving.
    pub fn spawn(cx: &mut Ctx) -> Self {
        let mut slots = vec![Midge::ambient(AMBIENT_FIRST_X, AMBIENT_FIRST_VX, cx)];
        let count = cx.rng.r(AMBIENT_COUNT_RAND);
        for _ in 1..count {
            let x = cx.rng.r(W);
            let vx = sign_roll(cx) * (cx.rng.r(AMBIENT_VX_RAND) + AMBIENT_VX_BASE);
            slots.push(Midge::ambient(x, vx, cx));
        }
        Self { slots }
    }
}

impl World {
    /// Step 14, per ambient midge: arc test, contact, update, fan push.
    pub fn ambient_midges_step(&mut self, cx: &mut Ctx) {
        let (swatter, delta) = (self.swatter_gate(), self.camera.delta);
        for m in &mut self.ambient_midges.slots {
            let zapped = self.traps.arcs_at(m.arc_target(cx), cx) > 0;
            m.contact(swatter, zapped, AMBIENT_LAYER, cx);
            m.ambient_update(delta, cx);
            m.fan_push(&self.traps, cx);
        }
    }

    /// Step 18a (`FUN_00404ef3`, render pass in the original): last tick's caught midges are
    /// dropped, then per midge the zapper test and contact (while k < 20), homing, retarget, the
    /// slowdown box, the fan push and the update; then the farmer slowdown pulse and the swarm
    /// end (gaps §8.1, answers.md "Goodie-midge removal").
    pub fn midges_step(&mut self, cx: &mut Ctx) {
        if !self.midges.on {
            return;
        }
        for m in self.midges.slots.iter_mut().filter(|m| m.dead) {
            (m.active, m.dead) = (false, false);
        }
        let retarget = cx.rng.r(RETARGET_ROLL) < RETARGET_BELOW;
        let (swatter, delta) = (self.swatter_gate(), self.camera.delta);
        let f = &self.farmer;
        let slow_box = Rect::new(f.x, f.y, SPRITE_W, SPRITE_H);
        let (mut k, mut in_box) = (0, 0);
        for m in self.midges.slots.iter_mut().filter(|m| m.active) {
            if k < SWAT_KILLS {
                let zapped = self.traps.arcs_at(m.arc_target(cx), cx) > 0;
                let (hit, respawned) = m.contact(swatter, zapped, 1.0, cx);
                k += usize::from(zapped) + usize::from(hit);
                m.dead |= respawned;
            }
            m.home(&self.farmer, cx);
            if retarget {
                m.retarget(cx);
            }
            in_box += i32::from(slow_box.catches(&m.rect(cx)));
            m.fan_push(&self.traps, cx);
            m.update(1.0, delta, cx);
        }
        let total = self.midges.total;
        self.farmer.midges_pulse(in_box, total, cx);
        self.farmer.midges_on = true;
        if self.midges.slots.iter().any(|m| m.active) {
            return;
        }
        self.midges.on = false;
        self.midges.total = 0;
        self.farmer.midges_on = false;
        self.farmer.slowed = false;
    }

    /// The swatter box while the gate is open (gaps §5.1).
    fn swatter_gate(&self) -> Option<Rect> {
        self.farmer.catch_open().then(|| self.farmer.catch_box())
    }
}

#[cfg(test)]
mod tests {
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::match_state::{MatchConfig, MatchState, Players};
    use crate::rng::MsvcRand;

    fn world(rig: &mut Rig) -> World {
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let m = MatchState::new(config, false, &mut rig.cx(0));
        World::bare(&m, &mut rig.cx(0))
    }

    /// Seed 1 in answers.md order ("Midges: spawn x, ..."). First midge: x 41 % 3190 + 800 = 841,
    /// sign 18467 % 10 = 7 (+), |vx| 6334 % 200 + 200 = 334, size 26500 % 5 + 30 = 30, y 19169 %
    /// 600 = 569, timer 15724 % 500 + 500 = 724, tolerances A 11478 % 30 = 18 and B 29358 % 30 =
    /// 18, home y 26962 % 118 = 58, home x 24464 % 181 = 29. Second: x 5705 % 3190 + 800 = 3315,
    /// sign 28145 % 10 = 5 (+), |vx| 23281 % 200 + 200 = 281, size 16827 % 5 + 30 = 32, y 9961 %
    /// 600 = 361, timer 491 + 500 = 991, A 2995 % 30 = 25, B 11942 % 30 = 2 (x tolerance B, y
    /// tolerance A), home y 4827 % 118 = 107, home x 5436 % 181 = 6. No homing speed is seeded.
    #[test]
    fn a_use_adds_up_to_100_midges_and_caps_at_300() {
        let mut rig = Rig::new(1);
        let mut s = Swarm::new();
        s.spawn(&mut rig.cx(0));
        let (m, n) = (s.slots[0], s.slots[1]);
        assert!(s.on);
        assert_eq!(
            (m.x, m.layer_x, m.y, m.mover.v[0], m.frame_ms),
            (841, 841, 569, 334, 30)
        );
        assert_eq!((m.home_dx, m.home_dy, m.tol_x, m.tol_y), (29, 58, 18, 18));
        // Accel flag x is set at the spawn, so a step re-rolls only y: R(150), R(10), R(200).
        let (mut moved, before) = (m, rig.rng.clone());
        moved.mover.step(&mut rig.cx(0));
        assert_eq!(
            (draws_between(&before, &rig.rng), moved.mover.v[0]),
            (3, 334)
        );
        assert!(!m.timer.expired(724) && m.timer.expired(725));
        assert_eq!((n.x, n.y, n.mover.v[0], n.frame_ms), (3315, 361, 281, 32));
        assert_eq!((n.home_dx, n.home_dy, n.tol_x, n.tol_y), (6, 107, 2, 25));
        let active = |s: &Swarm| s.slots.iter().filter(|m| m.active).count();
        assert_eq!((active(&s), s.total), (100, 100));
        s.slots[5].active = false;
        for _ in 0..3 {
            s.spawn(&mut rig.cx(0));
        }
        // total counts spawns, so the refilled slot 5 makes it 301.
        assert_eq!((active(&s), s.total), (300, 301));
    }

    /// No midge left: the effect ends and the farmer is released (FW §8.2 step 6, AR §6.3).
    #[test]
    fn the_swarm_ends_when_no_midge_remains() {
        let mut rig = Rig::new(1);
        let mut w = world(&mut rig);
        (w.midges.on, w.midges.total) = (true, 100);
        (w.farmer.midges_on, w.farmer.slowed) = (true, true);
        w.midges_step(&mut rig.at(1));
        assert!(!w.midges.on && !w.farmer.midges_on && !w.farmer.slowed);
        assert_eq!(w.midges.total, 0);
    }

    fn swarm_at(w: &mut World, b: Rect) {
        w.midges.on = true;
        w.midges.total = 30;
        for m in w.midges.slots.iter_mut().take(30) {
            *m = Midge {
                active: true,
                x: b.x + 10,
                y: b.y + 10,
                mover: Wander::new(150, (200, 200), (5, 3), (30, 30)),
                tol_x: 1000,
                tol_y: 1000,
                ..Midge::default()
            };
        }
    }

    /// Thirty midges under an open swatter: 20 are stunned. 700 ms later they respawn (the two
    /// draws), are marked dead and still run the tick; the next tick drops them.
    #[test]
    fn a_swat_kills_at_most_20() {
        let mut rig = Rig::new(1);
        let mut w = world(&mut rig);
        let b = w.farmer.catch_box();
        swarm_at(&mut w, b);
        w.farmer.swing.frame = 3;
        w.midges_step(&mut rig.at(1));
        let stunned = w.midges.slots.iter().filter(|m| m.stunned).count();
        assert_eq!(stunned, 20);
        w.farmer.swing.frame = 0;
        let active = |w: &World| w.midges.slots.iter().filter(|m| m.active).count();
        w.midges_step(&mut rig.at(1 + 46));
        let dead = w.midges.slots.iter().filter(|m| m.dead).count();
        assert_eq!((active(&w), dead), (30, 20));
        w.midges_step(&mut rig.at(1 + 47));
        assert_eq!(active(&w), 10);
    }

    /// How many rand() calls `after` is past `before`.
    fn draws_between(before: &MsvcRand, after: &MsvcRand) -> u32 {
        let mut r = before.clone();
        for n in 0..10_000 {
            if r == *after {
                return n;
            }
            r.rand();
        }
        panic!("rng not reached within 10000 draws");
    }

    /// A homing midge `d` px right of its target, with v0 200, tol_y 10 and v = (5, 7).
    fn homer(w: &World, d: i32, dy: i32) -> Midge {
        let mut m = Midge {
            active: true,
            x: w.farmer.x + d,
            y: w.farmer.y + dy,
            v0: 200,
            tol_y: 10,
            ..Midge::default()
        };
        m.mover.v = [5, 7];
        m
    }

    /// answers.md "Midge homing: ring bands (round 4)", seed 1: R(100) = 41 rolls nothing, so v
    /// is kept; then 18467 % 100 = 67 > 50 rolls sx = 6334 % 200 + 200 = 334 and sy = 26500 % 60
    /// + |200 − 180| = 60. y steers only outside ±tol_y.
    #[test]
    fn homing_writes_v_only_on_ticks_that_roll_the_speeds() {
        let mut rig = Rig::new(1);
        let w = world(&mut rig);
        let mut m = homer(&w, 100, 50);
        let mut rig = Rig::new(1);
        m.home(&w.farmer, &mut rig.cx(0));
        assert_eq!(m.mover.v, [5, 7]);
        m.home(&w.farmer, &mut rig.cx(0));
        assert_eq!(m.mover.v, [-334, -60]);
        let mut m = homer(&w, 100, 5);
        let mut rig = Rig::new(1);
        rig.rng.rand();
        m.home(&w.farmer, &mut rig.cx(0));
        assert_eq!(m.mover.v, [-334, 7], "inside the tolerance y wanders");
    }

    /// q = 2 + 2d / 4000 for d = midge x − target x: [0, 1) and [2, 3) steer left, [1, 2) and
    /// [3, 4) right (the shortest way round the ring), and outside [0, 4) x is not steered.
    /// Seed 1 after one skipped draw rolls sx = 334.
    #[test]
    fn homing_steers_x_the_short_way_round_the_ring() {
        let mut rig = Rig::new(1);
        let w = world(&mut rig);
        let cases = [
            (-3000, -334), // q 0.5
            (-1000, 334),  // q 1.5
            (0, -334),     // q 2
            (1000, -334),  // q 2.5
            (2500, 334),   // q 3.25
            (-4500, 5),    // q −0.25
            (4100, 5),     // q 4.05
        ];
        for (d, vx) in cases {
            let mut m = homer(&w, d, 0);
            let mut rig = Rig::new(1);
            rig.rng.rand();
            m.home(&w.farmer, &mut rig.cx(0));
            assert_eq!(m.mover.v[0], vx, "d {d}");
        }
    }

    /// The accel flags: in a band x is flagged and holds its v, so a step re-rolls only y (inside
    /// the tolerance its flag is 0): 3 draws. Outside the bands both axes re-roll: 6.
    #[test]
    fn homing_sets_the_accel_flags_so_the_wander_rerolls_only_free_axes() {
        let mut rig = Rig::new(1);
        let w = world(&mut rig);
        for (d, draws) in [(100, 3), (4100, 6)] {
            let mut m = homer(&w, d, 0);
            m.home(&w.farmer, &mut rig.cx(0));
            let before = rig.rng.clone();
            m.mover.step(&mut rig.cx(0));
            assert_eq!(draws_between(&before, &rig.rng), draws, "d {d}");
        }
    }

    /// A stunned or falling midge makes no homing draws and keeps its velocity.
    #[test]
    fn a_stunned_or_falling_midge_does_not_home() {
        let mut rig = Rig::new(1);
        let w = world(&mut rig);
        for (stunned, falling) in [(true, false), (false, true)] {
            let mut m = Midge {
                active: true,
                stunned,
                falling,
                ..Midge::default()
            };
            m.mover.v = [5, 7];
            let before = rig.rng.clone();
            m.home(&w.farmer, &mut rig.cx(0));
            assert_eq!((m.mover.v, &rig.rng), ([5, 7], &before));
        }
    }

    /// Seed 1, answers.md "Midges: ... ambient ... constructor": slot 0 at x 900, vx +60, size 41
    /// % 5 + 30 = 31, y 18467 % 600 = 467, timer 6334 % 500 + 800 = 1134; count 26500 % 15 = 10;
    /// slot 1 x 19169 % 4000 = 3169, sign 15724 % 10 = 4 (−), |vx| 11478 % 30 + 45 = 63, size
    /// 29358 % 5 + 30 = 33, y 26962 % 600 = 562, timer 24464 % 500 + 800 = 1264. 3 + 1 + 9 × 6 =
    /// 58 draws, so the next is draw 58 (27529).
    #[test]
    fn period_load_builds_slot_0_then_r15_moving_ambient_midges() {
        let mut rig = Rig::new(1);
        let a = AmbientMidges::spawn(&mut rig.cx(0));
        let (m0, m1) = (a.slots[0], a.slots[1]);
        assert_eq!(a.slots.len(), 10);
        assert_eq!(
            (m0.x, m0.y, m0.frame_ms, m0.mover.v),
            (900, 467, 31, [60, 0])
        );
        // Screen x as set; layer x trunc(900 / 1.25) = 720 and trunc(3169 / 1.25) = 2535.
        assert_eq!((m0.layer_x, a.slots[1].layer_x), (720, 2535));
        assert!(!m0.timer.expired(1134) && m0.timer.expired(1135));
        assert_eq!(
            (m1.x, m1.y, m1.frame_ms, m1.mover.v),
            (3169, 562, 33, [-63, 0])
        );
        assert!(!m1.timer.expired(1264) && m1.timer.expired(1265));
        assert!(a.slots.iter().all(|m| m.active && m.mover.v[0] != 0));
        assert_eq!(rig.rng.rand(), 27529);
    }

    /// A count of 0 or 1 still builds slot 0 alone, max(count, 1): the first seed whose fourth
    /// draw (the count) is R(15) ≤ 1.
    #[test]
    fn an_r15_of_0_or_1_leaves_only_slot_0() {
        let seed = (1..2000)
            .find(|&s| {
                let mut r = crate::rng::MsvcRand::new(s);
                r.rand();
                r.rand();
                r.rand();
                r.rand() % 15 <= 1
            })
            .expect("a seed in range");
        let mut rig = Rig::new(seed);
        assert_eq!(AmbientMidges::spawn(&mut rig.cx(0)).slots.len(), 1);
    }

    /// The jitter runs once per expiry of the midge's timer. Seed 12: R(100) = 77 > 70, m = 5628
    /// % 15 = 3, sign 6232 % 10 = 2 (−): vx = −3 − 60 = −63. Before the next expiry, no draw.
    #[test]
    fn the_ambient_jitter_runs_once_per_timer_expiry() {
        let mut rig = Rig::new(12);
        let mut m = Midge {
            active: true,
            timer: Timer::armed(0, 1134),
            ..Midge::default()
        };
        m.mover.v = [60, 0];
        let before = rig.rng.clone();
        m.jitter_tick(&mut rig.cx(1134));
        assert_eq!((m.mover.v, &rig.rng), ([60, 0], &before));
        m.jitter_tick(&mut rig.cx(1135));
        assert_eq!(m.mover.v, [-63, 0]);
        let before = rig.rng.clone();
        m.jitter_tick(&mut rig.cx(1135 + 1134));
        assert_eq!(&rig.rng, &before);
    }

    /// The fan push saves v and adds +180 when v > 0, else −180 (zero included); the update then
    /// steps v 40 toward zero, so the push and the wander act on the same v.
    #[test]
    fn the_fan_push_saves_v_and_decays_back_by_40() {
        let mut m = Midge::default();
        m.mover.v = [0, 50];
        m.push();
        assert_eq!((m.mover.v, m.saved_v), ([-180, 230], Some([0, 50])));
        for _ in 0..4 {
            m.decay();
        }
        assert_eq!(m.mover.v, [-20, 70]);
        // x: −20 + 40 = 20, |20| > 0 so no snap; y: 70 − 40 = 30 ≤ 50 snaps to 50.
        m.decay();
        assert_eq!((m.mover.v, m.saved_v), ([20, 50], Some([0, 50])));
        let mut m = Midge::default();
        m.mover.v = [-100, 230];
        m.saved_v = Some([-80, 50]);
        // x: −60 ≤ 80 snaps to −80; y: 190, 150, 110, 70, then 30 → 50, and the push ends.
        for _ in 0..5 {
            m.decay();
        }
        assert_eq!((m.mover.v, m.saved_v), ([-80, 50], None));
    }

    /// A midge at rest: both accel flags set, so a mover step neither re-rolls nor moves it.
    fn still(layer_x: i32, x: i32, y: i32) -> Midge {
        Midge {
            active: true,
            layer_x,
            x,
            y,
            mover: Wander::new(100, (10, 50), (50, 30), (0, 0)).with_accel_flags(true, true),
            ..Midge::default()
        }
    }

    /// answers.md "Midge wrap, bounds and frame period (round 5)": layer x wraps into [0, 4000)
    /// with the camera delta added (answers.md "Screen-x setter"); screen x = trunc(layer x × f).
    /// 10 − 20 → 3990: goodie screen 3990, ambient trunc(3990 × 1.25) = 4987.
    #[test]
    fn midge_layer_x_wraps_with_the_camera() {
        let mut rig = Rig::new(1);
        let mut g = still(10, 10, 100);
        g.update(1.0, 20.0, &mut rig.cx(0));
        assert_eq!((g.layer_x, g.x), (3990, 3990));
        let mut a = still(10, 12, 100);
        a.update(AMBIENT_LAYER, 20.0, &mut rig.cx(0));
        assert_eq!((a.layer_x, a.x), (3990, 4987));
        let mut g = still(3995, 3995, 100);
        g.update(1.0, -10.0, &mut rig.cx(0));
        assert_eq!((g.layer_x, g.x), (5, 5));
    }

    /// 650 px/s is 10 layer px a tick at 65 Hz: a goodie moves 10 on screen, an ambient midge
    /// trunc(10 × 1.25) = 12.
    #[test]
    fn an_ambient_midge_moves_1_25_times_on_screen() {
        let mut rig = Rig::new(1);
        for (factor, x) in [(1.0, 10), (AMBIENT_LAYER, 12)] {
            let mut m = still(0, 0, 100);
            m.mover.v = [650, 0];
            m.update(factor, 0.0, &mut rig.cx(0));
            assert_eq!((m.layer_x, m.x), (10, x), "factor {factor}");
        }
    }

    /// Ambient bounds: y 0..573 (600 − the 27 px frame), no x bound; a falling midge is not
    /// bounded (the contact routine clears +0x138).
    #[test]
    fn ambient_midges_are_bounded_only_in_y() {
        let mut rig = Rig::new(1);
        for (y, want) in [(590, 573), (-5, 0), (300, 300)] {
            let mut m = still(3990, 4987, y);
            m.timer = Timer::armed(0, 10_000);
            m.ambient_update(0.0, &mut rig.cx(0));
            assert_eq!((m.x, m.y), (4987, want), "y {y}");
        }
        let mut m = still(0, 0, 590);
        (m.falling, m.timer, m.mover.v) = (true, Timer::armed(0, 10_000), [0, 650]);
        m.ambient_update(0.0, &mut rig.cx(0));
        assert!(m.y > 590);
    }

    /// The respawn's x (seed 1: 41 + 1000 = 1041) is a screen x: on the 1.25 layer the layer x
    /// is trunc(1041 / 1.25) = 832.
    #[test]
    fn a_respawn_sets_the_screen_x() {
        for (factor, layer_x) in [(1.0, 1041), (AMBIENT_LAYER, 832)] {
            let mut rig = Rig::new(1);
            let mut m = still(0, 0, 0);
            (m.stunned, m.stun_timer) = (true, Timer::armed(0, 0));
            let (_, respawned) = m.contact(None, false, factor, &mut rig.cx(1));
            assert!(respawned);
            assert_eq!((m.x, m.layer_x), (1041, layer_x));
        }
    }

    /// The timer is checked every tick: one expiring during a stun re-arms on its grid, so the
    /// first free tick draws nothing (answers.md "Ambient midge update (round 5)" step 1).
    #[test]
    fn the_jitter_timer_rearms_while_stunned() {
        let mut rig = Rig::new(12);
        let mut m = still(0, 0, 0);
        (m.stunned, m.timer) = (true, Timer::armed(0, 1134));
        let before = rig.rng.clone();
        m.jitter_tick(&mut rig.cx(1135));
        m.stunned = false;
        m.jitter_tick(&mut rig.cx(1136));
        assert_eq!(&rig.rng, &before);
        assert!(!m.timer.expired(1135 + 1134) && m.timer.expired(1135 + 1135));
    }

    /// The jitter ends an active push first (v = saved v), then vx = sign × m − vx and the
    /// facing frame follows vx's sign. Seed 12: 77 > 70, m 3, sign − → −3 − 60 = −63.
    #[test]
    fn the_jitter_ends_a_push_and_sets_the_facing() {
        let mut rig = Rig::new(12);
        let mut m = still(0, 0, 0);
        m.timer = Timer::armed(0, 1134);
        m.mover.v = [240, 185];
        m.saved_v = Some([60, 5]);
        m.jitter_tick(&mut rig.cx(1135));
        assert_eq!(
            (m.mover.v, m.saved_v, m.facing_left),
            ([-63, 5], None, true)
        );
    }

    /// The decay steps toward zero and snaps to the saved v once |v| ≤ |saved|; with saved 0 it
    /// oscillates (30 → −10 → 30) and the push never ends.
    #[test]
    fn the_push_decay_steps_toward_zero_and_can_oscillate() {
        let mut m = Midge::default();
        m.mover.v = [30, 50];
        m.saved_v = Some([0, 50]);
        m.decay();
        assert_eq!((m.mover.v, m.saved_v), ([-10, 50], Some([0, 50])));
        m.decay();
        assert_eq!((m.mover.v, m.saved_v), ([30, 50], Some([0, 50])));
    }

    /// Midges in the farmer's box (x, y, 181, 178; gaps §8.1 step 2.4) slow him: r = 30/30 caps the speed at 0 and stops vy.
    #[test]
    fn the_farmer_slows_while_midges_touch_him() {
        let mut rig = Rig::new(1);
        let mut w = world(&mut rig);
        let body = Rect::new(w.farmer.x, w.farmer.y, SPRITE_W, SPRITE_H);
        swarm_at(&mut w, body);
        w.midges_step(&mut rig.at(1));
        assert!(w.farmer.midges_on && w.farmer.slowed);
        assert_eq!(w.farmer.slow_speed, 0);
    }
}
