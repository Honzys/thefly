//! W2. The Midges goodie's swarm (fly-and-world §8, ai-and-rules §6.3) and the ambient midges
//! (tick step 14). Ticks steps 14 and 18a.

use super::collision::Rect;
use super::farmer::{Farmer, SPRITE_H, SPRITE_W};
use super::flying_goodies::Wander;
use super::traps::Traps;
use super::{W, World};
use crate::clock::Timer;
use crate::ctx::Ctx;

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
pub const MOVER_ACCEL: i32 = 30;
/// x = R(W − 810) + 800, y = R(600), vx = ±(R(200) + 200), size R(5) + 30.
pub const SPAWN_X_MIN: i32 = 800;
pub const SPAWN_X_MARGIN: i32 = 810;
pub const SPAWN_Y_RAND: i32 = 600;
pub const SPAWN_VX_RAND: i32 = 200;
pub const SPAWN_VX_BASE: i32 = 200;
pub const SIZE_RAND: i32 = 5;
pub const SIZE_BASE: i32 = 30;
/// Homing offsets R(181) and R(farmerH − 60); tolerances R(30).
pub const HOME_Y_MARGIN: i32 = 60;
pub const TOLERANCE_RAND: i32 = 30;
/// gaps §8.1 contact routine: a 700 ms stun; a zapped midge falls with y acceleration 30; the
/// zapper test is at (x, y + 7); a respawn draws x = R(500) + 1000, y = R(H − h) + 20.
pub const STUN_MS: u64 = 700;
pub const FALL_ACCEL: i32 = 30;
pub const ARC_TEST_DY: i32 = 7;
pub const RESPAWN_X_RAND: i32 = 500;
pub const RESPAWN_X_BASE: i32 = 1000;
pub const RESPAWN_Y_BASE: i32 = 20;
/// The world is one 600 px screen tall (game-spec §3).
pub const WORLD_H: i32 = 600;
pub const MIDGE_SPRITE: &str = "moucheron";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Midge {
    pub active: bool,
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    /// R(5) + 30 px.
    pub size: i32,
    pub mover: Wander,
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
    // shortcut: FW §8.1 lists the fields, not the draw order; the port draws in the listed order
    // (x, y, |vx|, sign, size, home x, home y, tolerances). Revisit with FUN_00404caf.
    fn spawn(cx: &mut Ctx) -> Self {
        let mover = Wander::new(
            MOVER_REROLL_MS,
            (MOVER_SPEED, MOVER_SPEED),
            MOVER_RUNS,
            MOVER_ACCEL,
        );
        let x = cx.rng.r(W - SPAWN_X_MARGIN) + SPAWN_X_MIN;
        let y = cx.rng.r(SPAWN_Y_RAND);
        let speed = cx.rng.r(SPAWN_VX_RAND) + SPAWN_VX_BASE;
        let vx = cx.rng.sign() * speed;
        let size = cx.rng.r(SIZE_RAND) + SIZE_BASE;
        let home_dx = cx.rng.r(SPRITE_W);
        let home_dy = cx.rng.r(SPRITE_H - HOME_Y_MARGIN);
        let tol_x = cx.rng.r(TOLERANCE_RAND);
        let tol_y = cx.rng.r(TOLERANCE_RAND);
        Self {
            active: true,
            x,
            y,
            vx,
            size,
            mover: Wander { vx, ..mover },
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

    /// `FUN_00442f95`: fly toward the homing point on the farmer; stunned midges hold still,
    /// zapped ones drop.
    // shortcut: FW §8.2 names the homing function only; the port steers each axis at the mover's
    // speed while outside its tolerance and wanders inside it. Revisit with FUN_00442f95.
    fn fly(&mut self, farmer: &Farmer, cx: &mut Ctx) {
        if self.stunned && !self.falling {
            return;
        }
        if self.falling {
            self.mover.vx = 0;
            self.mover.vy += FALL_ACCEL;
            let (_, dy) = self.mover.integrate();
            self.y += dy;
            self.vy = self.mover.vy;
            return;
        }
        let dx = farmer.x + self.home_dx - self.x;
        let dy = farmer.y + self.home_dy - self.y;
        let home = [
            (dx.abs() > self.tol_x).then_some(dx),
            (dy.abs() > self.tol_y).then_some(dy),
        ];
        let (mx, my) = self.mover.step_toward(home, cx);
        self.x += mx;
        self.y += my;
        (self.vx, self.vy) = (self.mover.vx, self.mover.vy);
    }

    /// gaps §8.1 step 1: the respawn condition.
    fn due_respawn(&self, cx: &Ctx) -> bool {
        let stun_over = self.stunned && self.stun_timer.expired(cx.now) && !self.falling;
        stun_over || (self.falling && self.y > WORLD_H)
    }

    /// gaps §8.1 step 1 for the ambient population: back to flight at a fresh spot.
    fn respawn(&mut self, cx: &mut Ctx) {
        let h = cx.data.size(MIDGE_SPRITE).h;
        (self.stunned, self.falling) = (false, false);
        self.stun_timer.disarm();
        self.x = cx.rng.r(RESPAWN_X_RAND) + RESPAWN_X_BASE;
        self.y = cx.rng.r(WORLD_H - h) + RESPAWN_Y_BASE;
    }

    /// gaps §8.1 steps 2–4: swatted, else zapped; returns the hit-state flag.
    fn contact(&mut self, swatter: Option<Rect>, zapped: bool, cx: &Ctx) -> bool {
        let rect = self.rect(cx);
        if swatter.is_some_and(|b| b.catches(&rect)) {
            self.stunned = true;
            (self.mover.vx, self.mover.vy, self.vx, self.vy) = (0, 0, 0, 0);
            self.stun_timer.arm(cx.now, STUN_MS);
        } else if zapped {
            self.falling = true;
        }
        self.stunned
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
        if !traps.fan_touches(self.x, self.y, cx) {
            return;
        }
        self.mover.vx += away_from_zero(self.mover.vx);
        self.mover.vy += away_from_zero(self.mover.vy);
        (self.vx, self.vy) = (self.mover.vx, self.mover.vy);
    }
}

/// +180 for a positive (or zero) component, −180 for a negative one.
fn away_from_zero(v: i32) -> i32 {
    if v < 0 { -TRAP_PUSH } else { TRAP_PUSH }
}

/// The cosmetic population at game+0x15a20 that reacts to the swatter and fans. Open item O2: its
/// count and spawn rule are not in the docs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AmbientMidges {
    pub slots: Vec<Midge>,
}

impl World {
    /// Step 14.
    // shortcut: O2, the ambient population's count and spawn are not documented, so the port
    // starts it empty; this runs the shared contact routine (with a real respawn) on whatever
    // slots exist. Revisit when O2 is closed.
    pub fn ambient_midges_step(&mut self, cx: &mut Ctx) {
        let swatter = self.swatter_gate();
        for m in &mut self.ambient_midges.slots {
            m.fly(&self.farmer, cx);
            if m.due_respawn(cx) {
                m.respawn(cx);
                continue;
            }
            let zapped = self.traps.arc_touches(m.x, m.y + ARC_TEST_DY, cx);
            m.contact(swatter, zapped, cx);
            m.fan_push(&self.traps, cx);
        }
    }

    /// Step 18a (`FUN_00404ef3`, render pass in the original): homing, swatter kills, trap push,
    /// the farmer slowdown pulse, swarm end.
    // shortcut: gaps §8.1 has a goodie midge "respawn" like an ambient one, which would leave the
    // swarm alive forever against FW §8.2 steps 2 and 6 ("kills", "ends when no midges
    // remain"); the port retires a goodie midge on the respawn condition instead, without the
    // two respawn draws. Revisit with FUN_00442bb3's caller.
    pub fn midges_step(&mut self, cx: &mut Ctx) {
        if !self.midges.on {
            return;
        }
        let retarget = cx.rng.r(RETARGET_ROLL) < RETARGET_BELOW;
        let swatter = self.swatter_gate();
        let f = &self.farmer;
        let slow_box = Rect::new(f.x, f.y, SPRITE_W, SPRITE_H);
        let (mut k, mut in_box) = (0, 0);
        for m in self.midges.slots.iter_mut().filter(|m| m.active) {
            m.fly(&self.farmer, cx);
            if k < SWAT_KILLS && m.due_respawn(cx) {
                m.active = false;
                continue;
            }
            if k < SWAT_KILLS {
                let zapped = self.traps.arc_touches(m.x, m.y + ARC_TEST_DY, cx);
                k += usize::from(zapped);
                k += usize::from(m.contact(swatter, zapped, cx));
            }
            if retarget {
                m.retarget(cx);
            }
            in_box += i32::from(slow_box.catches(&m.rect(cx)));
            m.fan_push(&self.traps, cx);
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

    /// Seed 1, first midge: x 41 + 800, y 18467 % 600 = 467, |vx| 6334 % 200 + 200 = 334 with
    /// sign roll 26500 (> 16383, +), size 19169 % 5 + 30 = 34, home 15724 % 181 = 158 and
    /// 11478 % 118 = 32, tolerances 29358 % 30 = 18 and 26962 % 30 = 22.
    #[test]
    fn a_use_adds_up_to_100_midges_and_caps_at_300() {
        let mut rig = Rig::new(1);
        let mut s = Swarm::new();
        s.spawn(&mut rig.cx(0));
        let m = s.slots[0];
        assert!(s.on);
        assert_eq!((m.x, m.y, m.vx, m.size), (841, 467, 334, 34));
        assert_eq!((m.home_dx, m.home_dy, m.tol_x, m.tol_y), (158, 32, 18, 22));
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
                mover: Wander::new(150, (200, 200), (5, 3), 30),
                tol_x: 1000,
                tol_y: 1000,
                ..Midge::default()
            };
        }
    }

    /// Thirty midges under an open swatter: 20 are stunned, then retired 700 ms later.
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
        w.midges_step(&mut rig.at(1 + 46));
        let active = w.midges.slots.iter().filter(|m| m.active).count();
        assert_eq!(active, 10);
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
