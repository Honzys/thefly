//! W2. The Midges goodie's swarm (fly-and-world §8, ai-and-rules §6.3) and the ambient midges
//! (tick step 14). Ticks steps 14 and 18a.

use super::collision::Rect;
use super::farmer::{Farmer, SPRITE_H, SPRITE_W};
use super::traps::Traps;
use super::{W, World};
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
/// |vx| = R(200) + 200; size R(5) + 30; y = R(600); a timer R(500) + 500.
pub const SPAWN_X_MARGIN: i32 = 810;
pub const SPAWN_X_BASE: i32 = 800;
pub const SIGN_ROLL: i32 = 10;
pub const SIGN_BELOW: i32 = 5;
pub const SPAWN_Y_RAND: i32 = 600;
pub const SPAWN_VX_RAND: i32 = 200;
pub const SPAWN_VX_BASE: i32 = 200;
pub const SIZE_RAND: i32 = 5;
pub const SIZE_BASE: i32 = 30;
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
/// gaps §8.1 contact routine: a 700 ms stun; a zapped midge falls with y acceleration 30; the
/// zapper test is at (x, y + 7); a respawn draws x = R(500) + 1000, y = R(H − h) + 20.
pub const STUN_MS: u64 = 700;
pub const FALL_ACCEL: i32 = 30;
pub const ARC_TEST_DY: i32 = 7;
pub const RESPAWN_X_RAND: i32 = 500;
pub const RESPAWN_X_BASE: i32 = 1000;
pub const RESPAWN_Y_BASE: i32 = 20;
/// answers.md "Midges: spawn x, ..., ambient update and constructor": slot 0 at x 900, vx +60;
/// then count = R(15) and slots 1..count − 1 at x = R(W), |vx| = R(30) + 45; each with size
/// R(5) + 30, y = R(600) and a timer R(500) + 800. Parameters 100 / 10 / 50 / 50 / 30 / 0 and
/// bounds x −1000..9000, y 0..600.
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
pub const AMBIENT_X_BOUNDS: (i32, i32) = (-1000, 9000);
pub const AMBIENT_Y_BOUNDS: (i32, i32) = (0, 600);
/// The jitter, once per expiry of the midge's timer: R(100) > 70, then m = R(15) and an R(10)
/// sign, vx = sign × m − vx.
pub const JITTER_ROLL: i32 = 100;
pub const JITTER_ABOVE: i32 = 70;
pub const JITTER_RAND: i32 = 15;
/// A pushed v steps this much per tick back toward the v saved at the push.
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
    pub x: i32,
    pub y: i32,
    /// R(5) + 30 px.
    pub size: i32,
    /// The velocity is `mover.v`.
    pub(crate) mover: Wander,
    /// The spawn vx (v0); the homing speeds are 0 until the first R(100) > 50 re-roll.
    pub v0: i32,
    pub speed_x: i32,
    pub speed_y: i32,
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
    /// The v saved by the last fan push; the update steps v back toward it.
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
    /// tolerance the first.
    // shortcut: answers.md "Midges: spawn x, ..." sets accel flag x, so x never re-rolls under
    // Wander; `mover::Wander` has no such flag, so x still re-rolls (three draws per expiry).
    // Revisit when the mover gains per-axis accel flags.
    fn spawn(cx: &mut Ctx) -> Self {
        let x = cx.rng.r(W - SPAWN_X_MARGIN) + SPAWN_X_BASE;
        let vx = sign_roll(cx) * (cx.rng.r(SPAWN_VX_RAND) + SPAWN_VX_BASE);
        let size = cx.rng.r(SIZE_RAND) + SIZE_BASE;
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
        );
        mover.v[0] = vx;
        Self {
            active: true,
            x,
            y,
            size,
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

    /// One ambient midge of the constructor: size, y and timer after the caller's x and vx.
    fn ambient(x: i32, vx: i32, cx: &mut Ctx) -> Self {
        let size = cx.rng.r(SIZE_RAND) + SIZE_BASE;
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
            y,
            size,
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

    /// `FUN_00442f95`, after the contact step (answers.md "Midges: spawn x, homing gates, ...").
    /// A stunned or falling midge does nothing. Else maybe new speeds, then x steers toward
    /// farmer x + home x and y toward farmer y + home y outside ±tol_y (inside, y wanders). The x
    /// tolerance is not used.
    // shortcut: answers.md gates the x steer by the "ring q as decoded in round 1", which no
    // round defines beyond "q in [0, 4)"; the port always steers x. Revisit with FUN_00442f95.
    fn home(&mut self, farmer: &Farmer, cx: &mut Ctx) {
        if self.stunned || self.falling {
            return;
        }
        if cx.rng.r(HOMING_ROLL) > HOMING_ABOVE {
            self.speed_x = cx.rng.r(HOMING_X_RAND) + self.v0.abs();
            self.speed_y = cx.rng.r(HOMING_Y_RAND) + (self.v0 - HOMING_Y_OFFSET).abs();
        }
        let dx = farmer.x + self.home_dx - self.x;
        self.mover.v[0] = dx.signum() * self.speed_x;
        let dy = farmer.y + self.home_dy - self.y;
        if dy.abs() > self.tol_y {
            self.mover.v[1] = dy.signum() * self.speed_y;
        }
    }

    /// `FUN_00442960` for a goodie midge: a stunned midge holds still, a zapped one drops, else
    /// the push decays and the wander mover moves it.
    fn update(&mut self, cx: &mut Ctx) {
        if self.stunned && !self.falling {
            return;
        }
        let (dx, dy) = if self.falling {
            self.fall()
        } else {
            self.decay();
            self.mover.step(cx)
        };
        self.x += dx;
        self.y += dy;
    }

    /// One push decay step: v moves [`PUSH_DECAY`] toward the saved v, and the push ends there.
    fn decay(&mut self) {
        let Some(saved) = self.saved_v else {
            return;
        };
        for (v, s) in self.mover.v.iter_mut().zip(saved) {
            *v = s.clamp(*v - PUSH_DECAY, *v + PUSH_DECAY);
        }
        if self.mover.v == saved {
            self.saved_v = None;
        }
    }

    fn fall(&mut self) -> (i32, i32) {
        self.mover.v = [0, self.mover.v[1] + FALL_ACCEL];
        self.mover.integrate()
    }

    /// `FUN_00442960` for an ambient midge: the goodie update with the jitter first, and the
    /// position kept inside the ambient bounds while flying.
    // shortcut: answers.md "Midges: ..., ambient update and constructor" gives the parameters
    // 100 / 10 / 50 / 50 / 30 / 0 without naming them; the port reads them as the wander mover's
    // (re-roll, speeds, runs, accel) like FW §8.1's goodie midge. It also does not say where the
    // jitter sits in the update or whether the timer re-arms at its period (the port: first, and
    // yes). Revisit when FUN_00442960 is read.
    fn ambient_update(&mut self, cx: &mut Ctx) {
        if self.stunned || self.falling {
            self.update(cx);
            return;
        }
        self.jitter(cx);
        self.update(cx);
        self.x = self.x.clamp(AMBIENT_X_BOUNDS.0, AMBIENT_X_BOUNDS.1);
        self.y = self.y.clamp(AMBIENT_Y_BOUNDS.0, AMBIENT_Y_BOUNDS.1);
    }

    /// On each expiry of the midge's timer: R(100) > 70, then vx = sign × R(15) − vx.
    fn jitter(&mut self, cx: &mut Ctx) {
        if !self.timer.expired_rearm(cx.now) || cx.rng.r(JITTER_ROLL) <= JITTER_ABOVE {
            return;
        }
        let m = cx.rng.r(JITTER_RAND);
        self.mover.v[0] = sign_roll(cx) * m - self.mover.v[0];
    }

    /// gaps §8.1 step 1: the respawn condition.
    fn due_respawn(&self, cx: &Ctx) -> bool {
        let stun_over = self.stunned && self.stun_timer.expired(cx.now) && !self.falling;
        stun_over || (self.falling && self.y > WORLD_H)
    }

    /// gaps §8.1 step 1: back to flight at a fresh spot.
    fn respawn(&mut self, cx: &mut Ctx) {
        let h = cx.data.size(MIDGE_SPRITE).h;
        (self.stunned, self.falling) = (false, false);
        self.stun_timer.disarm();
        self.x = cx.rng.r(RESPAWN_X_RAND) + RESPAWN_X_BASE;
        self.y = cx.rng.r(WORLD_H - h) + RESPAWN_Y_BASE;
    }

    /// gaps §8.1 `FUN_00442bb3`: respawn, else swatted, else zapped. Returns the hit-state flag
    /// (0 after a respawn) and whether the midge respawned.
    fn contact(&mut self, swatter: Option<Rect>, zapped: bool, cx: &mut Ctx) -> (bool, bool) {
        if self.due_respawn(cx) {
            self.respawn(cx);
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
        let swatter = self.swatter_gate();
        for m in &mut self.ambient_midges.slots {
            let zapped = self.traps.arcs_at(m.arc_target(cx), cx) > 0;
            m.contact(swatter, zapped, cx);
            m.ambient_update(cx);
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
        let swatter = self.swatter_gate();
        let f = &self.farmer;
        let slow_box = Rect::new(f.x, f.y, SPRITE_W, SPRITE_H);
        let (mut k, mut in_box) = (0, 0);
        for m in self.midges.slots.iter_mut().filter(|m| m.active) {
            if k < SWAT_KILLS {
                let zapped = self.traps.arcs_at(m.arc_target(cx), cx) > 0;
                let (hit, respawned) = m.contact(swatter, zapped, cx);
                k += usize::from(zapped) + usize::from(hit);
                m.dead |= respawned;
            }
            m.home(&self.farmer, cx);
            if retarget {
                m.retarget(cx);
            }
            in_box += i32::from(slow_box.catches(&m.rect(cx)));
            m.fan_push(&self.traps, cx);
            m.update(cx);
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
        assert_eq!((m.x, m.y, m.mover.v[0], m.size), (841, 569, 334, 30));
        assert_eq!((m.home_dx, m.home_dy, m.tol_x, m.tol_y), (29, 58, 18, 18));
        assert_eq!((m.speed_x, m.speed_y), (0, 0));
        assert!(!m.timer.expired(724) && m.timer.expired(725));
        assert_eq!((n.x, n.y, n.mover.v[0], n.size), (3315, 361, 281, 32));
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

    /// v0 200, seed 1: R(100) = 41 keeps the speeds (200, 20); then 18467 % 100 = 67 > 50 draws
    /// sx = 6334 % 200 + 200 = 334 and sy = 26500 % 60 + |200 − 180| = 60. x always steers;
    /// y steers only outside ±tol_y.
    #[test]
    fn homing_rerolls_speeds_over_50_and_steers_y_outside_the_tolerance() {
        let mut rig = Rig::new(1);
        let w = world(&mut rig);
        let (fx, fy) = (w.farmer.x, w.farmer.y);
        let mut m = Midge {
            active: true,
            x: fx + 100,
            y: fy + 5,
            v0: 200,
            speed_x: 200,
            speed_y: 20,
            tol_y: 10,
            ..Midge::default()
        };
        m.mover.v = [0, 7];
        let mut rig = Rig::new(1);
        m.home(&w.farmer, &mut rig.cx(0));
        assert_eq!(m.mover.v, [-200, 7]);
        m.y = fy + 50;
        m.home(&w.farmer, &mut rig.cx(0));
        assert_eq!(m.mover.v, [-334, -60]);
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
        assert_eq!((m0.x, m0.y, m0.size, m0.mover.v), (900, 467, 31, [60, 0]));
        assert!(!m0.timer.expired(1134) && m0.timer.expired(1135));
        assert_eq!((m1.x, m1.y, m1.size, m1.mover.v), (3169, 562, 33, [-63, 0]));
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
        m.jitter(&mut rig.cx(1134));
        assert_eq!((m.mover.v, &rig.rng), ([60, 0], &before));
        m.jitter(&mut rig.cx(1135));
        assert_eq!(m.mover.v, [-63, 0]);
        let before = rig.rng.clone();
        m.jitter(&mut rig.cx(1135 + 1134));
        assert_eq!(&rig.rng, &before);
    }

    /// The fan push saves v and adds +180 when v > 0, else −180 (zero included); the update then
    /// steps v 40 toward the saved v, so the push and the wander act on the same v.
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
        m.decay();
        assert_eq!((m.mover.v, m.saved_v), ([0, 50], None));
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
