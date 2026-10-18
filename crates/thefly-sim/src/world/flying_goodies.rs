//! W2. Flying goodies: luck, spawn, the wander mover, update and catch (fly-and-world §6,
//! ai-and-rules §6.2). Ticks step 16. [`Wander`] is the shared mover (`FUN_004172a8`).

use super::collision::Rect;
use super::goodies::{MYSTERY_MS, Mystery};
use super::{W, World};
use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::data::Size;
use crate::event::{Effect, Event, Pan, PlayParams, SoundOp, SoundSlot};
use crate::match_state::MatchState;

pub const FIRST_CHECK_BASE_MS: u64 = 8000;
pub const FIRST_CHECK_RAND: i32 = 7000;
pub const FLY_LIFETIME_MS: u64 = 8000;
pub const FARMER_LIFETIME_MS: u64 = 12_000;
/// Kind 15 is the mystery goodie; it is drawn when R(100) is below this.
pub const MYSTERY_KIND: i32 = 15;
pub const MYSTERY_PERCENT: i32 = 3;
/// n = the `Goodies` sheet's frame count (5 × 2); kinds are R(trunc(n/2 − 2)) + 3.
pub const SHEET_FRAMES: i32 = 10;
pub const GOODIES_SPRITE: &str = "Goodies";
/// Normal kinds 3, 4, 5: Turbo/Triple, SuperFly/Nuclear, Midges/Boot (FW §6.3).
pub const FIRST_KIND: i32 = 3;
pub const TURBO_KIND: i32 = 3;
pub const SUPERFLY_KIND: i32 = 4;
pub const MIDGES_KIND: i32 = 5;
/// The trailing-side re-roll picks kind 3 or 4.
pub const TRAILING_KINDS: i32 = 2;
/// Re-arm trunc((1 − p) × 8000) + 8000 (no spawn), + 16000 (fly) or + 20000 (farmer).
pub const REARM_SPAN_MS: f64 = 8000.0;
pub const NO_SPAWN_REARM_BASE_MS: u64 = 8000;
pub const FLY_REARM_BASE_MS: u64 = 16_000;
pub const FARMER_REARM_BASE_MS: u64 = 20_000;
/// Launch at (R(800), R(500)); the mover re-rolls every R(500) ms at R(30) + 65 px/s, runs 8
/// (x) and 6 (y), acceleration 5; y stays in [0, 500 − h], x ≤ W + 100 − w.
pub const LAUNCH_W: i32 = 800;
pub const LAUNCH_H: i32 = 500;
pub const SPEED_RAND: i32 = 30;
pub const SPEED_BASE: i32 = 65;
pub const REROLL_RAND_MS: i32 = 500;
pub const RUNS: (i32, i32) = (8, 6);
pub const ACCEL: i32 = 5;
pub const RIGHT_MARGIN: i32 = 100;
/// A mystery catch: R(100) below 50 is the first effect, else Turbo 25 s or Nuclear 20 s.
pub const MYSTERY_FIRST_BELOW: i32 = 50;
pub const MYSTERY_TURBO_MS: u64 = 25_000;
pub const MYSTERY_NUCLEAR_MS: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Fly,
    Farmer,
}

/// The wander mover: per-axis target speed re-rolled every R(reroll) ms, ramped by `accel`.
// shortcut: FW §6.2 gives the mover's parameters, not its internals (`FUN_004172a8`,
// `FUN_004174c0`). The port keeps each axis's direction for `run` re-rolls, then takes a fresh
// sign roll (x before y), and ramps |v| toward the target by `accel` per tick. Revisit with
// those functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wander {
    pub reroll_rand_ms: i32,
    pub speed_x: i32,
    pub speed_y: i32,
    /// Same-direction run lengths per axis.
    pub run_x: i32,
    pub run_y: i32,
    pub accel: i32,
    pub vx: i32,
    pub vy: i32,
    pub target_vx: i32,
    pub target_vy: i32,
    pub timer: Timer,
    /// Remainder state of the FPS-based integrator (`FUN_004180af`).
    pub frames: i32,
    pub rem_x: i32,
    pub rem_y: i32,
    /// Re-rolls left in the current direction run, per axis.
    pub left_x: i32,
    pub left_y: i32,
    /// The velocity the integrator's current run started with.
    pub run_vx: i32,
    pub run_vy: i32,
}

/// UB guard: `rand % 0` for a mover built with no re-roll spread.
pub const MIN_REROLL_RAND: i32 = 1;

impl Wander {
    pub fn new(reroll_rand_ms: i32, speed: (i32, i32), run: (i32, i32), accel: i32) -> Self {
        Self {
            reroll_rand_ms,
            speed_x: speed.0,
            speed_y: speed.1,
            run_x: run.0,
            run_y: run.1,
            accel,
            ..Self::default()
        }
    }

    /// One tick: maybe re-roll, ramp, integrate; returns (dx, dy) in px.
    pub fn step(&mut self, cx: &mut Ctx) -> (i32, i32) {
        self.step_toward([None, None], cx)
    }

    /// Like [`Wander::step`], but an axis given as `Some(d)` (signed distance to a homing point,
    /// already outside its tolerance) heads toward it at that axis's speed.
    pub fn step_toward(&mut self, home: [Option<i32>; 2], cx: &mut Ctx) -> (i32, i32) {
        if !self.timer.is_armed() || self.timer.expired(cx.now) {
            self.reroll(cx);
        }
        if let Some(d) = home[0] {
            self.target_vx = d.signum() * self.speed_x;
        }
        if let Some(d) = home[1] {
            self.target_vy = d.signum() * self.speed_y;
        }
        self.vx = ramp(self.vx, self.target_vx, self.accel);
        self.vy = ramp(self.vy, self.target_vy, self.accel);
        self.integrate()
    }

    fn reroll(&mut self, cx: &mut Ctx) {
        let ms = cx.rng.r(self.reroll_rand_ms.max(MIN_REROLL_RAND));
        self.timer.arm(cx.now, u64::try_from(ms).unwrap_or(0));
        let dx = run_direction(&mut self.left_x, self.run_x, self.target_vx.signum(), cx);
        let dy = run_direction(&mut self.left_y, self.run_y, self.target_vy.signum(), cx);
        self.target_vx = dx * self.speed_x;
        self.target_vy = dy * self.speed_y;
    }

    /// `FUN_004180af`: exactly |v| px per run of FPS frames; the run restarts when v changes or
    /// the frame count reaches FPS.
    pub(super) fn integrate(&mut self) -> (i32, i32) {
        if self.vx != self.run_vx || self.vy != self.run_vy || self.frames >= FPS {
            (self.frames, self.rem_x, self.rem_y) = (0, 0, 0);
            (self.run_vx, self.run_vy) = (self.vx, self.vy);
        }
        self.frames += 1;
        (
            fps_share(self.vx, &mut self.rem_x),
            fps_share(self.vy, &mut self.rem_y),
        )
    }
}

fn run_direction(left: &mut i32, run: i32, current: i32, cx: &mut Ctx) -> i32 {
    if *left > 0 && current != 0 {
        *left -= 1;
        return current;
    }
    *left = run - 1;
    cx.rng.sign()
}

/// |v| ramps toward |target| by `accel`; the sign follows the target.
fn ramp(v: i32, target: i32, accel: i32) -> i32 {
    if v < target {
        (v + accel).min(target)
    } else {
        (v - accel).max(target)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flyer {
    pub active: bool,
    /// 3, 4, 5, or [`MYSTERY_KIND`].
    pub kind: i32,
    /// Sheet frame for the renderer.
    pub frame: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub mover: Wander,
    pub lifetime: Timer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlyingGoodies {
    pub fly: Flyer,
    pub farmer: Flyer,
    pub fly_check: Timer,
    pub farmer_check: Timer,
}

/// `FUN_0040c086(side)`: the balance meter p in [0, 1]; the trailing side gets more than 0.5.
/// W3 owns the formula ([`crate::economy::Economy::meter`]); this is the call site's name.
pub fn luck(m: &MatchState, side: Side) -> f64 {
    m.economy.meter(side)
}

impl FlyingGoodies {
    /// Both check timers at R(7000) + 8000 ms, farmer first.
    pub fn new(cx: &mut Ctx) -> Self {
        let mut first_check = || {
            let ms = FIRST_CHECK_BASE_MS + cx.rng.r(FIRST_CHECK_RAND) as u64;
            Timer::armed(cx.now, ms)
        };
        let farmer_check = first_check();
        let fly_check = first_check();
        Self {
            farmer_check,
            fly_check,
            ..Self::default()
        }
    }
}

impl World {
    /// Step 16: spawn (`FUN_00432684`), update (`FUN_00432dfb`), catch (`FUN_0043309b`).
    /// The farmer catches only while the swing frame is 3 or more and the "cannot hit" flag is
    /// clear, against the box (x, trunc(reach), 131, 154), bounds inclusive (gaps §5.1). The
    /// trailing-side re-roll is `R(100) <= 2q` for the fly and `< 2q` for the farmer (gaps §5.2).
    // shortcut: the order of the two sides' spawn checks within a tick is not documented; the
    // port checks the farmer first, as the timers are armed (FW §6.2). Revisit with FUN_00432684.
    pub fn flying_goodies_step(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let size = cx.data.size(GOODIES_SPRITE);
        for side in [Side::Farmer, Side::Fly] {
            self.flying_goodies.check(side, || luck(m, side), size, cx);
        }
        let delta = self.camera.delta;
        self.flying_goodies.fly.update(delta, cx);
        self.flying_goodies.farmer.update(delta, cx);
        self.fly_catch(m, cx);
        self.farmer_catch(m, cx);
    }

    fn fly_catch(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let g = &mut self.flying_goodies.fly;
        if !g.active || !self.fly.inner_box().catches(&g.rect()) {
            return;
        }
        g.active = false;
        g.play(cx);
        let inv = &mut m.economy.fly;
        match g.kind {
            TURBO_KIND => inv.turbo += 1,
            SUPERFLY_KIND => inv.superfly += 1,
            MIDGES_KIND => inv.midges += 1,
            _ => self.fly_mystery(cx),
        }
    }

    fn farmer_catch(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let g = &mut self.flying_goodies.farmer;
        let open = self.farmer.catch_open();
        if !g.active || !open || !self.farmer.catch_box().catches(&g.rect()) {
            return;
        }
        g.active = false;
        g.play(cx);
        let inv = &mut m.economy.farmer;
        match g.kind {
            TURBO_KIND => inv.triple += 1,
            SUPERFLY_KIND => inv.nuclear += 1,
            MIDGES_KIND => inv.boot += 1,
            _ => self.farmer_mystery(cx),
        }
    }

    fn fly_mystery(&mut self, cx: &mut Ctx) {
        let g = &mut self.goodies;
        g.fly_mystery_timer.arm(cx.now, MYSTERY_MS);
        if cx.rng.r(100) < MYSTERY_FIRST_BELOW {
            g.fly_mystery = Mystery::First;
            return;
        }
        g.fly_mystery = Mystery::Second;
        self.fly.start_turbo(Some(MYSTERY_TURBO_MS), cx);
    }

    fn farmer_mystery(&mut self, cx: &mut Ctx) {
        let g = &mut self.goodies;
        g.farmer_mystery_timer.arm(cx.now, MYSTERY_MS);
        if cx.rng.r(100) < MYSTERY_FIRST_BELOW {
            g.farmer_mystery = Mystery::First;
            self.farmer.swing.blocked = true;
            return;
        }
        g.farmer_mystery = Mystery::Second;
        self.farmer.start_nuclear(Some(MYSTERY_NUCLEAR_MS), cx);
    }
}

impl FlyingGoodies {
    /// FW §6.2: when the side's timer fires, read the meter, maybe launch a goodie and re-arm.
    fn check(&mut self, side: Side, luck: impl FnOnce() -> f64, size: Size, cx: &mut Ctx) {
        let (timer, flyer, rearm_base) = match side {
            Side::Fly => (&mut self.fly_check, &mut self.fly, FLY_REARM_BASE_MS),
            Side::Farmer => (
                &mut self.farmer_check,
                &mut self.farmer,
                FARMER_REARM_BASE_MS,
            ),
        };
        if !timer.expired(cx.now) {
            return;
        }
        let p = luck();
        let behind = ((1.0 - p) * REARM_SPAN_MS) as u64;
        let roll = cx.rng.r(100);
        if p * 100.0 <= f64::from(roll) {
            timer.arm(cx.now, behind + NO_SPAWN_REARM_BASE_MS);
            return;
        }
        let (kind, frame) = pick_kind(side, p, cx);
        *flyer = Flyer::launch(side, kind, frame, size, cx);
        timer.arm(cx.now, behind + rearm_base);
    }
}

/// FW §6.2 steps 3–4: mystery, or a normal kind with the trailing-side re-roll (gaps §5.2).
fn pick_kind(side: Side, p: f64, cx: &mut Ctx) -> (i32, i32) {
    if cx.rng.r(100) < MYSTERY_PERCENT {
        let frame = match side {
            Side::Fly => 1,
            Side::Farmer => 2,
        };
        return (MYSTERY_KIND, frame);
    }
    let normal_kinds = SHEET_FRAMES / 2 - 2;
    let mut kind = cx.rng.r(normal_kinds) + FIRST_KIND;
    let q = (p * 100.0 - 50.0) as i32;
    if q > 0 && trailing_reroll(side, cx.rng.r(100), q) {
        kind = cx.rng.r(TRAILING_KINDS) + FIRST_KIND;
    }
    let frame = match side {
        Side::Fly => kind,
        Side::Farmer => SHEET_FRAMES / 2 + kind,
    };
    (kind, frame)
}

fn trailing_reroll(side: Side, roll: i32, q: i32) -> bool {
    let threshold = q * 100 / 50;
    match side {
        Side::Fly => roll <= threshold,
        Side::Farmer => roll < threshold,
    }
}

impl Flyer {
    // shortcut: FW §6.2 gives one "target speed (R(30) + 65)" for both axes; the port draws it
    // once, after x and y. Revisit with FUN_00431c64.
    fn launch(side: Side, kind: i32, frame: i32, size: Size, cx: &mut Ctx) -> Self {
        let x = cx.rng.r(LAUNCH_W);
        let y = cx.rng.r(LAUNCH_H);
        let speed = cx.rng.r(SPEED_RAND) + SPEED_BASE;
        let lifetime = match side {
            Side::Fly => FLY_LIFETIME_MS,
            Side::Farmer => FARMER_LIFETIME_MS,
        };
        let g = Self {
            active: true,
            kind,
            frame,
            x,
            y,
            w: size.w,
            h: size.h,
            mover: Wander::new(REROLL_RAND_MS, (speed, speed), RUNS, ACCEL),
            lifetime: Timer::armed(cx.now, lifetime),
        };
        cx.emit(Event::Effect(Effect::GoodieSpawn { x, y }));
        g.play(cx);
        g
    }

    /// FW §6.3: wander, scroll with the fly layer, clamp to the bounds (the left one never
    /// applies), expire.
    fn update(&mut self, delta: f64, cx: &mut Ctx) {
        if !self.active {
            return;
        }
        if self.lifetime.expired(cx.now) {
            self.active = false;
            return;
        }
        let (dx, dy) = self.mover.step(cx);
        self.x += dx + (-delta) as i32;
        self.y += dy;
        self.x = self.x.min(W + RIGHT_MARGIN - self.w);
        self.y = self.y.clamp(0, LAUNCH_H - self.h);
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.w, self.h)
    }

    fn play(&self, cx: &mut Ctx) {
        let pan = Pan::ScreenX {
            x: self.x,
            w: self.w,
        };
        let params = PlayParams {
            pan,
            ..PlayParams::default()
        };
        cx.sound(SoundSlot::FLYING_GOODIE, SoundOp::Play(params));
    }
}

#[cfg(test)]
mod tests {
    use super::super::farmer::rig::Rig;
    use super::*;

    const SIZE: Size = Size { w: 51, h: 50 };

    /// Seed 1: farmer R(7000) = 41 → 8041 ms, then fly 18467 % 7000 = 4467 → 12467 ms.
    #[test]
    fn first_checks_are_farmer_then_fly() {
        let mut rig = Rig::new(1);
        let g = FlyingGoodies::new(&mut rig.cx(0));
        assert_eq!(g.farmer_check.remaining_ms(0), 8041);
        assert_eq!(g.fly_check.remaining_ms(0), 12467);
    }

    /// p = 0.5, seed 1: roll 41 < 50 spawns; 18467 % 100 = 67 is not a mystery; kind
    /// 6334 % 3 + 3 = 4; x 26500 % 800 = 100, y 19169 % 500 = 169, speed 15724 % 30 + 65 = 69;
    /// re-arm 4000 + 16000.
    #[test]
    fn a_spawn_draws_kind_position_and_speed_in_order() {
        let mut rig = Rig::new(1);
        let mut g = FlyingGoodies {
            fly_check: Timer::armed(0, 0),
            ..FlyingGoodies::default()
        };
        g.check(Side::Fly, || 0.5, SIZE, &mut rig.cx(0));
        let f = g.fly;
        assert!(f.active);
        assert_eq!((f.kind, f.frame, f.x, f.y), (4, 4, 100, 169));
        assert_eq!((f.mover.speed_x, f.mover.speed_y), (69, 69));
        assert_eq!(f.lifetime.remaining_ms(0), FLY_LIFETIME_MS);
        assert_eq!(g.fly_check.remaining_ms(0), 20_000);
        assert!(
            rig.events
                .contains(&Event::Effect(Effect::GoodieSpawn { x: 100, y: 169 }))
        );
        assert!(
            rig.events.iter().any(
                |e| matches!(e, Event::Sound { slot, .. } if *slot == SoundSlot::FLYING_GOODIE)
            )
        );
    }

    /// p = 0.5 after one draw on seed 1: roll 18467 % 100 = 67 ≥ 50, no spawn, 4000 + 8000.
    #[test]
    fn a_failed_roll_rearms_without_spawning() {
        let mut rig = Rig::new(1);
        rig.rng.rand();
        let mut g = FlyingGoodies {
            farmer_check: Timer::armed(0, 0),
            ..FlyingGoodies::default()
        };
        g.check(Side::Farmer, || 0.5, SIZE, &mut rig.cx(0));
        assert!(!g.farmer.active);
        assert_eq!(g.farmer_check.remaining_ms(0), 12_000);
        assert_eq!(rig.rng.rand(), 6334);
    }

    /// Seed 51 draws 205 then 31600: 31600 % 100 = 0 < 3 is the mystery, farmer frame 2.
    #[test]
    fn a_low_roll_is_the_mystery_goodie() {
        let mut rig = Rig::new(51);
        rig.rng.rand();
        assert_eq!(
            pick_kind(Side::Farmer, 0.5, &mut rig.cx(0)),
            (MYSTERY_KIND, 2)
        );
    }

    /// q = 30: the fly re-rolls at R(100) ≤ 60, the farmer only below 60 (gaps §5.2).
    #[test]
    fn the_trailing_reroll_is_strict_only_for_the_farmer() {
        assert!(trailing_reroll(Side::Fly, 60, 30));
        assert!(!trailing_reroll(Side::Farmer, 60, 30));
        assert!(trailing_reroll(Side::Farmer, 59, 30));
    }

    /// `FUN_004180af`: exactly |v| px per 65 frames, with the remainder spread over the run.
    #[test]
    fn the_integrator_moves_exactly_v_px_per_second() {
        for v in [69, -130, 5, 0] {
            let mut rem = 0;
            let total: i32 = (0..FPS).map(|_| fps_share(v, &mut rem)).sum();
            assert_eq!(total, v);
        }
    }

    /// The speed ramps by `accel` per tick toward the target and holds there.
    #[test]
    fn the_mover_ramps_toward_its_target_speed() {
        let mut rig = Rig::new(1);
        let mut w = Wander::new(500, (69, 69), (8, 6), 5);
        w.step(&mut rig.at(0));
        assert_eq!(w.vx.abs(), 5);
        for tick in 1..20 {
            w.step(&mut rig.at(tick));
        }
        assert_eq!((w.vx.abs(), w.vy.abs()), (69, 69));
    }

    /// The fly's goodie lives 8000 ms (tick 520), clamps to y ≤ 450 and scrolls by −delta.
    #[test]
    fn a_goodie_expires_and_scrolls_with_the_layer() {
        let mut rig = Rig::new(1);
        let mut f = Flyer {
            active: true,
            kind: 3,
            y: 449,
            w: 51,
            h: 50,
            x: 400,
            lifetime: Timer::armed(0, FLY_LIFETIME_MS),
            ..Flyer::default()
        };
        f.update(10.6, &mut rig.at(1));
        assert_eq!((f.x, f.y), (390, 449));
        for tick in 2..520 {
            f.update(0.0, &mut rig.at(tick));
            assert!((0..=450).contains(&f.y));
        }
        assert!(f.active);
        f.update(0.0, &mut rig.at(520));
        assert!(!f.active);
    }

    /// Ten simulated minutes per side: the trailing side (p = 0.8) spawns more than the leading
    /// one (p = 0.2) and gets more Turbo/SuperFly (kinds 3, 4) per spawn.
    #[test]
    fn the_trailing_side_spawns_more_often() {
        const MINUTES_MS: u64 = 600_000;
        let run = |p: f64| {
            let mut rig = Rig::new(7);
            let mut g = FlyingGoodies::new(&mut rig.cx(0));
            let (mut spawns, mut low_kinds, mut now) = (0, 0, 0);
            while now < MINUTES_MS {
                let was = g.fly.lifetime;
                g.check(Side::Fly, || p, SIZE, &mut rig.cx(now));
                if g.fly.lifetime != was {
                    spawns += 1;
                    low_kinds += i32::from(g.fly.kind < MIDGES_KIND);
                }
                now += 1;
            }
            (spawns, f64::from(low_kinds) / f64::from(spawns.max(1)))
        };
        let (behind, behind_low) = run(0.8);
        let (ahead, ahead_low) = run(0.2);
        assert!(behind > 2 * ahead, "{behind} vs {ahead}");
        assert!(behind_low > ahead_low, "{behind_low} vs {ahead_low}");
    }

    /// The farmer catches only from swing frame 3, against (x, trunc(reach), 131, 154).
    #[test]
    fn the_farmer_catches_only_in_the_hit_window() {
        use crate::match_state::{MatchConfig, MatchState, Players};
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = MatchState::new(config, false, &mut rig.cx(0));
        let mut w = World::bare(&m, &mut rig.cx(0));
        let b = w.farmer.catch_box();
        w.flying_goodies.farmer = Flyer {
            active: true,
            kind: SUPERFLY_KIND,
            x: b.x + b.w,
            y: b.y,
            w: 51,
            h: 50,
            lifetime: Timer::armed(0, FARMER_LIFETIME_MS),
            ..Flyer::default()
        };
        let before = m.economy.farmer.nuclear;
        w.farmer.swing.frame = 2;
        w.flying_goodies_step(&mut m, &mut rig.at(1));
        assert!(w.flying_goodies.farmer.active);
        w.farmer.swing.frame = 3;
        w.flying_goodies_step(&mut m, &mut rig.at(2));
        assert!(!w.flying_goodies.farmer.active);
        assert_eq!(m.economy.farmer.nuclear, before + 1);
    }
}
