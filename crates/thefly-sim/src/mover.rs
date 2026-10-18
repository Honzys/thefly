//! The shared movers: the FPS integrator `FUN_004180af` (FW §4.1) and the wander mover
//! `FUN_004174c0` built on it (answers.md "Wander mover internals").

use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;

/// The run state of `FUN_004180af`: exactly |v| px over each run of FPS frames; the run restarts
/// when v changes or the frame count reaches FPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FpsRun {
    frames: i32,
    rem: [i32; 2],
    /// The velocity the current run started with.
    run_v: [i32; 2],
}

impl FpsRun {
    /// One frame at v px/s; returns the (dx, dy) px moved.
    pub fn step(&mut self, v: [i32; 2]) -> (i32, i32) {
        if v != self.run_v || self.frames >= FPS {
            *self = Self {
                run_v: v,
                ..Self::default()
            };
        }
        self.frames += 1;
        (
            fps_share(v[0], &mut self.rem[0]),
            fps_share(v[1], &mut self.rem[1]),
        )
    }
}

/// This frame's share of |v| px over FPS frames: the quotient, plus 1 when the remainder carries.
fn fps_share(v: i32, rem: &mut i32) -> i32 {
    let (q, r) = (v.abs() / FPS, v.abs() % FPS);
    *rem += r;
    let carry = if *rem >= FPS {
        *rem -= FPS;
        1
    } else {
        0
    };
    v.signum() * (q + carry)
}

/// The wander mover: `FUN_004174c0`, moved by the shared mover `FUN_004172a8` (answers.md
/// "Wander mover internals").
///
/// [`Wander::new`]`(reroll_ms, speed, run_max, accel)` builds it at rest, both axis timers
/// unarmed. Each [`Wander::step`] does, per axis, x first, when that axis's timer has expired (an
/// unarmed one has): re-arm it with R(reroll_ms); d = R(10) picks the direction from the axis's
/// run count, its `run_max` and the sign of v; then v = (R(speed) + 65) × direction, set directly.
/// Every step then adds sign(v) × accel to each axis and moves by v px/s through the FPS
/// integrator, returning the (dx, dy) px moved. A caller that steers writes [`Wander::v`]
/// between steps; one that only needs the integrator calls [`Wander::integrate`]. Bounds belong
/// to the caller: clamp the position and leave the velocity, there is no bounce.
///
/// [`Wander::with_gravity`] is true gravity (+0x130): v += accel instead of sign(v) × accel. Not
/// modelled, as no user sets them: the accel flags (+0xa8, +0xac; set, an axis never re-rolls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wander {
    reroll_ms: i32,
    speed: [i32; 2],
    run_max: [i32; 2],
    accel: [i32; 2],
    run: [i32; 2],
    timers: [Timer; 2],
    /// (vx, vy) in px/s.
    pub v: [i32; 2],
    gravity: bool,
    fps: FpsRun,
}

/// The re-rolled speed is R(speed) + this (answers.md "Wander mover internals").
pub const WANDER_SPEED_BASE: i32 = 65;
/// d = R(10) below this takes the first branch of the direction table.
pub const WANDER_DIR_ROLL: i32 = 10;
pub const WANDER_DIR_BELOW: i32 = 5;
/// UB guard: `rand % 0` for a mover built with no re-roll or speed spread.
pub const MIN_WANDER_RAND: i32 = 1;

impl Wander {
    pub fn new(reroll_ms: i32, speed: (i32, i32), run_max: (i32, i32), accel: (i32, i32)) -> Self {
        Self {
            reroll_ms,
            speed: [speed.0, speed.1],
            run_max: [run_max.0, run_max.1],
            accel: [accel.0, accel.1],
            ..Self::default()
        }
    }

    pub fn set_accel(&mut self, accel: (i32, i32)) {
        self.accel = [accel.0, accel.1];
    }

    pub fn with_gravity(self) -> Self {
        Self {
            gravity: true,
            ..self
        }
    }

    /// One tick: re-roll the expired axes, accelerate, integrate; returns (dx, dy) in px.
    pub fn step(&mut self, cx: &mut Ctx) -> (i32, i32) {
        for a in 0..2 {
            if self.timers[a].expired(cx.now) {
                self.reroll(a, cx);
            }
            let dir = if self.gravity { 1 } else { self.v[a].signum() };
            self.v[a] += dir * self.accel[a];
        }
        self.integrate()
    }

    fn reroll(&mut self, a: usize, cx: &mut Ctx) {
        let ms = cx.rng.r(self.reroll_ms.max(MIN_WANDER_RAND));
        self.timers[a].arm(cx.now, u64::try_from(ms).unwrap_or(0));
        let low = cx.rng.r(WANDER_DIR_ROLL) < WANDER_DIR_BELOW;
        let (v, run) = (self.v[a], self.run[a]);
        let sign_ok = if low { v < 0 } else { v >= 0 };
        let (run, dir) = if sign_ok && run <= self.run_max[a] {
            (run + 1, 1)
        } else {
            (1, -1)
        };
        self.run[a] = run;
        self.v[a] = (cx.rng.r(self.speed[a].max(MIN_WANDER_RAND)) + WANDER_SPEED_BASE) * dir;
    }

    /// The FPS integrator alone, at the current v.
    pub fn integrate(&mut self) -> (i32, i32) {
        self.fps.step(self.v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::farmer::rig::Rig;

    /// `FUN_004180af`: exactly |v| px per 65 frames, with the remainder spread over the run.
    #[test]
    fn the_integrator_moves_exactly_v_px_per_second() {
        for v in [69, -130, 5, 0] {
            let mut rem = 0;
            let total: i32 = (0..FPS).map(|_| fps_share(v, &mut rem)).sum();
            assert_eq!(total, v);
        }
    }

    /// Seed 1, re-roll 0 ms and speed spread 1 (v = ±65), caps 1, no accel. Draws per axis:
    /// timer, d, speed. x: d 7, 8, 7, 6; y: d 9, 5, 5, 2. Tick 2 passes the cap (run 2 > 1) and
    /// flips both to −1; tick 3 keeps x at −1 (d ≥ 5 with v < 0) and turns y to +1 (d < 5 with
    /// v < 0 and run 1 ≤ 1).
    #[test]
    fn the_direction_table_follows_the_run_cap_and_the_roll() {
        let mut rig = Rig::new(1);
        let mut w = Wander::new(0, (1, 1), (1, 1), (0, 0));
        let v: Vec<[i32; 2]> = (0..4)
            .map(|tick| {
                w.step(&mut rig.at(tick));
                w.v
            })
            .collect();
        assert_eq!(v, [[65, 65], [65, 65], [-65, -65], [-65, 65]]);
    }

    /// True gravity (+0x130) adds the accel whatever the sign of v. Seed 1, re-roll 0, spread 1,
    /// caps 1, accel 5: d is x 7, 8, 7 and y 9, 5, 5, so both axes run +1 for two ticks (65 + 5)
    /// and pass the cap on tick 2: −65 + 5 = −60, where sign(v) × accel would give −70.
    #[test]
    fn true_gravity_always_adds_the_accel() {
        let mut rig = Rig::new(1);
        let mut w = Wander::new(0, (1, 1), (1, 1), (5, 5)).with_gravity();
        let v: Vec<[i32; 2]> = (0..3)
            .map(|tick| {
                w.step(&mut rig.at(tick));
                w.v
            })
            .collect();
        assert_eq!(v, [[70, 70], [70, 70], [-60, -60]]);
    }
}
