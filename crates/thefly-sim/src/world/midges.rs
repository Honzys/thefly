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
/// Traps push a touching midge by this on each axis, keeping the sign.
pub const TRAP_PUSH: i32 = 180;
/// Per-tick chance (%) to re-roll a midge's homing point.
pub const RETARGET_PERCENT: i32 = 30;
/// The retarget roll is R(10) < 3 (gaps §8.1).
pub const RETARGET_ROLL: i32 = 10;
pub const RETARGET_BELOW: i32 = RETARGET_PERCENT / 10;
/// FW §8.1 spawn: mover R(150) ms, speeds 200/200, runs 5/3, acceleration 30.
pub const MOVER_REROLL_MS: i32 = 150;
pub const MOVER_SPEED: i32 = 200;
pub const MOVER_RUNS: (i32, i32) = (5, 3);
pub const MOVER_ACCEL: (i32, i32) = (30, 30);
/// answers.md "Midge spawn draw order": x = raw rand, wrapped; the sign is R(10) < 5 → −1;
/// |vx| = R(200) + 200; size R(5) + 30; y = R(600); a timer R(500) + 500.
pub const SIGN_ROLL: i32 = 10;
pub const SIGN_BELOW: i32 = 5;
pub const SPAWN_Y_RAND: i32 = 600;
pub const SPAWN_VX_RAND: i32 = 200;
pub const SPAWN_VX_BASE: i32 = 200;
pub const SIZE_RAND: i32 = 5;
pub const SIZE_BASE: i32 = 30;
pub const SPAWN_TIMER_RAND: i32 = 500;
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
/// answers.md "Ambient midge population (O2)": R(15) midges at period load; per tick a jitter
/// (R(100) > 70, then m = R(15) and an R(10) sign: vx = sign × m − vx); the fan push decays by
/// 40 per tick.
pub const AMBIENT_COUNT_RAND: i32 = 15;
pub const JITTER_ROLL: i32 = 100;
pub const JITTER_ABOVE: i32 = 70;
pub const JITTER_RAND: i32 = 15;
pub const AMBIENT_PUSH_DECAY: i32 = 40;
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
    /// The spawn vx (v0) and the homing speeds it seeds.
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
    /// Ambient midges: the fan push, on top of the jitter, decaying toward 0.
    pub push: [i32; 2],
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
    /// `FUN_00404caf`'s per-midge draws, in rand order (answers.md "Midge spawn draw order").
    /// The x tolerance is the second R(30), the y tolerance the first.
    // shortcut: answers.md says x is the raw rand "then wrapped" without the width; the port
    // wraps at W like the fly layer. Revisit if midges spawn on the wrong side of the screen.
    fn spawn(cx: &mut Ctx) -> Self {
        let x = cx.rng.rand().rem_euclid(W);
        let sign = if cx.rng.r(SIGN_ROLL) < SIGN_BELOW {
            -1
        } else {
            1
        };
        let vx = sign * (cx.rng.r(SPAWN_VX_RAND) + SPAWN_VX_BASE);
        let size = cx.rng.r(SIZE_RAND) + SIZE_BASE;
        let y = cx.rng.r(SPAWN_Y_RAND);
        // The timer `FUN_00442806` arms; nothing documented reads it.
        cx.rng.r(SPAWN_TIMER_RAND);
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
            speed_x: vx.abs(),
            speed_y: (vx - HOMING_Y_OFFSET).abs(),
            home_dx,
            home_dy,
            tol_x,
            tol_y,
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

    /// `FUN_00442f95`, after the contact step (answers.md "Midge homing"): maybe new speeds,
    /// then x steers toward farmer x + home x and y toward farmer y + home y outside ±tol_y
    /// (inside, y wanders). The x tolerance is not used.
    // shortcut: answers.md does not define the "ring q" that gates the x steer (always on for
    // q in [0, 4)), the speeds before the first re-roll (the port seeds R = 0 of the formula) or
    // how homing meets the hit and fall states (the port draws, then leaves a stunned or falling
    // midge's velocity alone). Revisit with FUN_00442f95.
    fn home(&mut self, farmer: &Farmer, cx: &mut Ctx) {
        if cx.rng.r(HOMING_ROLL) > HOMING_ABOVE {
            self.speed_x = cx.rng.r(HOMING_X_RAND) + self.v0.abs();
            self.speed_y = cx.rng.r(HOMING_Y_RAND) + (self.v0 - HOMING_Y_OFFSET).abs();
        }
        if self.stunned || self.falling {
            return;
        }
        let dx = farmer.x + self.home_dx - self.x;
        self.mover.v[0] = dx.signum() * self.speed_x;
        let dy = farmer.y + self.home_dy - self.y;
        if dy.abs() > self.tol_y {
            self.mover.v[1] = dy.signum() * self.speed_y;
        }
    }

    /// The goodie midge's update: the wander mover; a stunned midge holds still, a zapped one
    /// drops.
    fn update(&mut self, cx: &mut Ctx) {
        if self.stunned && !self.falling {
            return;
        }
        let (dx, dy) = if self.falling {
            self.fall()
        } else {
            self.mover.step(cx)
        };
        self.x += dx;
        self.y += dy;
    }

    fn fall(&mut self) -> (i32, i32) {
        self.mover.v = [0, self.mover.v[1] + FALL_ACCEL];
        self.mover.integrate()
    }

    /// The ambient update (answers.md "Ambient midge population"): the jitter, then the fan push
    /// decays by 40; the midge moves by jitter + push.
    // shortcut: answers.md gives the jitter and the decay, not how the push combines with the
    // jitter; the port keeps the push apart and moves by their sum through the FPS integrator.
    // Revisit when the ambient update is read.
    fn ambient_update(&mut self, cx: &mut Ctx) {
        if self.stunned && !self.falling {
            return;
        }
        if self.falling {
            let (dx, dy) = self.fall();
            (self.x, self.y) = (self.x + dx, self.y + dy);
            return;
        }
        if cx.rng.r(JITTER_ROLL) > JITTER_ABOVE {
            let m = cx.rng.r(JITTER_RAND);
            let sign = if cx.rng.r(SIGN_ROLL) < SIGN_BELOW {
                -1
            } else {
                1
            };
            self.mover.v[0] = sign * m - self.mover.v[0];
        }
        self.push = self.push.map(|p| toward_zero(p, AMBIENT_PUSH_DECAY));
        let jitter = self.mover.v;
        self.mover.v = [jitter[0] + self.push[0], jitter[1] + self.push[1]];
        let (dx, dy) = self.mover.integrate();
        self.mover.v = jitter;
        (self.x, self.y) = (self.x + dx, self.y + dy);
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

    /// FW §8.2 step 3: a midge in a fan gets each component pushed 180 further from zero.
    fn fan_push(&mut self, traps: &Traps, cx: &Ctx) {
        if traps.fan_touches(self.x, self.y, cx) {
            self.mover.v = self.mover.v.map(|v| v + away_from_zero(v));
        }
    }

    /// The ambient fan push lands on the decaying push, away from the midge's velocity.
    fn ambient_fan_push(&mut self, traps: &Traps, cx: &Ctx) {
        if !traps.fan_touches(self.x, self.y, cx) {
            return;
        }
        for a in 0..2 {
            self.push[a] += away_from_zero(self.mover.v[a] + self.push[a]);
        }
    }
}

/// +180 for a positive (or zero) component, −180 for a negative one.
fn away_from_zero(v: i32) -> i32 {
    if v < 0 { -TRAP_PUSH } else { TRAP_PUSH }
}

/// `v` moved `d` toward 0 without overshooting.
fn toward_zero(v: i32, d: i32) -> i32 {
    if v > 0 {
        (v - d).max(0)
    } else {
        (v + d).min(0)
    }
}

/// The cosmetic population at game+0x15a20 that reacts to the swatter and fans. It never homes
/// and is never removed (answers.md "Ambient midge population").
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AmbientMidges {
    pub slots: Vec<Midge>,
}

impl AmbientMidges {
    /// Period load: R(15) midges.
    // shortcut: answers.md gives the count, not the per-midge draws; each midge is placed by the
    // contact routine's respawn draws (x = R(500) + 1000, y = R(H − h) + 20) and starts at rest.
    // Revisit when the ambient constructor is read.
    pub fn spawn(cx: &mut Ctx) -> Self {
        let n = cx.rng.r(AMBIENT_COUNT_RAND);
        let slots = (0..n)
            .map(|_| {
                let mut m = Midge {
                    active: true,
                    ..Midge::default()
                };
                m.respawn(cx);
                m
            })
            .collect();
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
            m.ambient_fan_push(&self.traps, cx);
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

    /// Seed 1 in answers.md order. First midge: x 41, sign 18467 % 10 = 7 (+), |vx| 6334 % 200 +
    /// 200 = 334, size 26500 % 5 + 30 = 30, y 19169 % 600 = 569, timer 15724, tolerances A
    /// 11478 % 30 = 18 and B 29358 % 30 = 18, home y 26962 % 118 = 58, home x 24464 % 181 = 29.
    /// Second: x 5705 % 4000 = 1705, sign 28145 % 10 = 5 (+), |vx| 23281 % 200 + 200 = 281, size
    /// 16827 % 5 + 30 = 32, y 9961 % 600 = 361, timer 491, A 2995 % 30 = 25, B 11942 % 30 = 2
    /// (x tolerance B, y tolerance A), home y 4827 % 118 = 107, home x 5436 % 181 = 6.
    #[test]
    fn a_use_adds_up_to_100_midges_and_caps_at_300() {
        let mut rig = Rig::new(1);
        let mut s = Swarm::new();
        s.spawn(&mut rig.cx(0));
        let (m, n) = (s.slots[0], s.slots[1]);
        assert!(s.on);
        assert_eq!((m.x, m.y, m.mover.v[0], m.size), (41, 569, 334, 30));
        assert_eq!((m.home_dx, m.home_dy, m.tol_x, m.tol_y), (29, 58, 18, 18));
        assert_eq!((n.x, n.y, n.mover.v[0], n.size), (1705, 361, 281, 32));
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

    /// Seed 1: R(15) = 41 % 15 = 11 ambient midges; the first respawn-draws x 18467 % 500 + 1000
    /// = 1467 and y 6334 % 573 + 20 = 51.
    #[test]
    fn period_load_spawns_r15_ambient_midges() {
        let mut rig = Rig::new(1);
        let a = AmbientMidges::spawn(&mut rig.cx(0));
        assert_eq!(a.slots.len(), 11);
        assert_eq!((a.slots[0].x, a.slots[0].y), (1467, 51));
    }

    /// One ambient midge at rest, seed 1, no traps: R(100) draws 41, 67, 34, 0, 69, 24 (≤ 70, no
    /// jitter), then 78 > 70: m = 29358 % 15 = 3, sign 26962 % 10 = 2 (−), vx = −3 − 0. It never
    /// homes on the farmer.
    #[test]
    fn an_ambient_midge_jitters_and_never_homes() {
        let mut rig = Rig::new(1);
        let mut w = world(&mut rig);
        w.ambient_midges.slots = vec![Midge {
            active: true,
            x: 300,
            y: 300,
            ..Midge::default()
        }];
        let mut rig = Rig::new(1);
        for tick in 1..=7 {
            w.ambient_midges_step(&mut rig.at(tick));
        }
        let m = w.ambient_midges.slots[0];
        assert_eq!((m.mover.v, m.y), ([-3, 0], 300));
        assert_eq!(rig.rng.rand(), 24464);
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
