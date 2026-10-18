//! W2. The friendly boot (fly-and-world §7, ai-and-rules §6.1). Ticks steps 7 and 10.

use super::collision::Rect;
use super::{OBJECT_PARALLAX, W, World};
use crate::clock::{DT, Timer};
use crate::ctx::Ctx;
use crate::event::{Effect, Event, SoundOp, SoundSlot};

pub const RETARGET_FIRST_MS: u64 = 1000;
/// The boot leaves after this many floor hits (count > 9).
pub const MAX_STOMPS: i32 = 9;
/// Splatting the fly sets the count to this, ending the boot after the stomp.
pub const SPLAT_STOMPS: i32 = 15;
/// Stomp when the horizontal distance to the target is below this.
pub const STOMP_DISTANCE: f64 = 11.0;
/// The boot's screen wrap, trunc(W × 1.25); spawn x = R(5000 − 900) + 810 (FW §7.1).
pub const BOOT_WRAP: i32 = 5000;
pub const SPAWN_X_SPAN: i32 = 900;
pub const SPAWN_X_MIN: i32 = 810;
/// Spawn y = 100 − h; hover and ceiling y = 80 − h; floor y = 570 − h.
pub const SPAWN_Y: i32 = 100;
pub const HOVER_Y: i32 = 80;
pub const FLOOR_Y: i32 = 570;
/// Hover: trunc(base + sin(trunc(screenX / 10)) × 5) (gaps §7.2).
pub const HOVER_X_DIVISOR: i32 = 10;
pub const HOVER_AMPLITUDE: f64 = 5.0;
/// Retarget: target = flyX − (bootW + 50) + offset; re-arm R(1000) + 1500; offset
/// R(bootW + 100 + flyW).
pub const TARGET_GAP: i32 = 50;
pub const RETARGET_BASE_MS: i32 = 1500;
pub const RETARGET_RAND_MS: i32 = 1000;
pub const OFFSET_SPAN: i32 = 100;
/// speed = trunc(dist × 2000 / W + 250).
pub const SPEED_K: i32 = 2000;
pub const SPEED_BASE: i32 = 250;
/// Bursts: end when R(100) ≥ 91; start when R(100) > 97 with R(500) + 500.
pub const BURST_END_FROM: i32 = 91;
pub const BURST_START_ABOVE: i32 = 97;
pub const BURST_RAND: i32 = 500;
pub const BURST_BASE: i32 = 500;
/// Stomp when R(100) > 85, falling with y acceleration 100.
pub const STOMP_ABOVE: i32 = 85;
pub const STOMP_ACCEL: i32 = 100;
/// Floor bounce: vy −1, acceleration 150; retire: vy −80, acceleration 80.
pub const FLOOR_VY: i32 = -1;
pub const FLOOR_ACCEL: i32 = 150;
pub const RETIRE_VY: i32 = -80;
pub const RETIRE_ACCEL: i32 = 80;
/// Sound 6's volume is set to +8000 before the stomp plays it (out of range, FW §7.2 step 9).
pub const BOOM_VOLUME: i32 = 8000;

#[derive(Debug, Clone, PartialEq)]
pub struct Boot {
    /// Not spawned, or gone (`FUN_0043b33b` false).
    pub active: bool,
    /// Layer x on the 1.25 layer, and y.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub vx: i32,
    pub vy: i32,
    pub accel_y: i32,
    pub horizontal_on: bool,
    pub vertical_on: bool,
    pub chasing: bool,
    pub stomping: bool,
    pub stomps: i32,
    /// Burst acceleration (§7.2 step 5); 0 is none.
    pub accel: i32,
    pub offset: i32,
    pub retarget: Timer,
    /// +0x178. Open item O23: what sets it (fly-and-world §7.2 step 7 only reads it).
    pub ready: bool,
}

impl Boot {
    /// An inactive boot with the `i_botte` frame size.
    pub fn new(w: i32, h: i32) -> Self {
        Self {
            active: false,
            x: 0,
            y: 0,
            w,
            h,
            vx: 0,
            vy: 0,
            accel_y: 0,
            horizontal_on: false,
            vertical_on: false,
            chasing: false,
            stomping: false,
            stomps: 0,
            accel: 0,
            offset: 0,
            retarget: Timer::UNARMED,
            ready: false,
        }
    }

    /// `FUN_0043aabe`, from the farmer's Boot goodie.
    // shortcut: O23, nothing in the docs sets the ready flag (+0x178) that the stomp reads; the
    // port sets it at spawn so the boot can stomp. Revisit when FUN_0043aabe's callers are read.
    pub fn spawn(&mut self, cx: &mut Ctx) {
        self.x = wrap_layer(cx.rng.r(BOOT_WRAP - SPAWN_X_SPAN) + SPAWN_X_MIN);
        self.y = SPAWN_Y - self.h;
        (self.vx, self.vy, self.accel_y, self.accel, self.offset) = (0, 0, 0, 0, 0);
        self.active = true;
        self.horizontal_on = true;
        self.vertical_on = false;
        self.chasing = true;
        self.stomping = false;
        self.stomps = 0;
        self.retarget = Timer::armed(cx.now, RETARGET_FIRST_MS);
        self.ready = true;
    }

    /// trunc(layerX × 1.25) wrapped into the 5000-px screen ring (FW §4.1).
    pub fn screen_x(&self) -> i32 {
        ((f64::from(self.x) * OBJECT_PARALLAX) as i32).rem_euclid(BOOT_WRAP)
    }

    /// The drawn box: across the ring end the screen x is shifted by −BOOT_WRAP (FW §4.4 step 6).
    pub fn rect(&self) -> Rect {
        let x = self.screen_x();
        let x = if x >= BOOT_WRAP - self.w {
            x - BOOT_WRAP
        } else {
            x
        };
        Rect::new(x, self.y, self.w, self.h)
    }

    /// Gone once it has risen above the top bound (y < −h).
    fn gone(&self) -> bool {
        self.y < -self.h
    }

    fn hover_y(&self) -> i32 {
        self.ceiling_y()
            + (f64::from(self.screen_x() / HOVER_X_DIVISOR).sin() * HOVER_AMPLITUDE) as i32
    }

    fn ceiling_y(&self) -> i32 {
        HOVER_Y - self.h
    }

    /// FW §7.2 for one tick. `fly_x`/`fly_w` place the target; `fly_splatted` is read on the floor.
    // shortcut: FW §7.2 does not say whether bootX in steps 3-7 is the layer or the screen x; the
    // port uses the layer x, which wraps at W = 4000 like the distance ring (gaps §7.2), and the
    // fly's screen x as the target. Revisit with FUN_0043abc7.
    pub fn update(&mut self, fly_x: i32, fly_w: i32, fly_splatted: bool, delta: f64, cx: &mut Ctx) {
        if self.stomps > MAX_STOMPS && self.chasing {
            self.retire();
        }
        let target = fly_x - (self.w + TARGET_GAP) + self.offset;
        if self.retarget.expired(cx.now) {
            let ms = cx.rng.r(RETARGET_RAND_MS) + RETARGET_BASE_MS;
            self.retarget.arm(cx.now, u64::try_from(ms).unwrap_or(0));
            self.offset = cx.rng.r(self.w + OFFSET_SPAN + fly_w);
        }
        let (q, dist) = ring_distance(self.x, target);
        let speed = (dist * f64::from(SPEED_K) / f64::from(W) + f64::from(SPEED_BASE)) as i32;
        if !self.stomping {
            self.burst(cx);
            self.steer(q, speed + self.accel);
        }
        if dist < STOMP_DISTANCE && !self.stomping && self.chasing && self.ready {
            self.vx = 0;
            if cx.rng.r(100) > STOMP_ABOVE {
                self.stomping = true;
                (self.accel_y, self.vy, self.vertical_on) = (STOMP_ACCEL, 0, true);
            }
        }
        self.integrate(delta);
        if !self.stomping && self.chasing {
            self.y = self.hover_y();
        }
        self.floor(fly_splatted, cx);
        if self.chasing && self.y < self.ceiling_y() {
            self.y = self.ceiling_y();
            (self.vy, self.accel_y, self.stomping, self.vertical_on) = (0, 0, false, false);
        }
    }

    /// Step 1.
    fn retire(&mut self) {
        self.chasing = false;
        (self.vy, self.accel_y, self.vertical_on) = (RETIRE_VY, RETIRE_ACCEL, true);
        (self.vx, self.horizontal_on) = (0, false);
    }

    /// Step 5.
    fn burst(&mut self, cx: &mut Ctx) {
        if self.accel >= 1 {
            if cx.rng.r(100) >= BURST_END_FROM {
                self.accel = 0;
            }
            return;
        }
        if cx.rng.r(100) > BURST_START_ABOVE {
            self.accel = cx.rng.r(BURST_RAND) + BURST_BASE;
        }
    }

    /// Step 6. q in [1, 2) or [3, 4) goes right, [0, 1) or [2, 3) goes left; else vx is kept.
    // shortcut: FW §7.2 step 5 does not say how the burst acceleration applies; the port adds it
    // to the speed. Revisit with FUN_0043abc7.
    fn steer(&mut self, q: f64, speed: i32) {
        if !self.horizontal_on || !(0.0..4.0).contains(&q) {
            return;
        }
        self.vx = if (1.0..2.0).contains(&q) || q >= 3.0 {
            speed
        } else {
            -speed
        };
    }

    /// The time-based integrator (FW §4.1); the y acceleration grows |vy| each tick along the
    /// sign of vy (0 counts as down).
    fn integrate(&mut self, delta: f64) {
        let vx = if self.horizontal_on { self.vx } else { 0 };
        self.x = wrap_layer(self.x + (f64::from(vx) * DT - delta) as i32);
        if !self.vertical_on {
            return;
        }
        self.vy += if self.vy < 0 {
            -self.accel_y
        } else {
            self.accel_y
        };
        self.y = (self.y + (f64::from(self.vy) * DT) as i32).min(FLOOR_Y - self.h);
    }

    /// Step 9.
    fn floor(&mut self, fly_splatted: bool, cx: &mut Ctx) {
        if !self.stomping || self.vy <= 0 || self.y != FLOOR_Y - self.h {
            return;
        }
        (self.vy, self.accel_y) = (FLOOR_VY, FLOOR_ACCEL);
        if fly_splatted {
            self.stomps = SPLAT_STOMPS;
        }
        self.stomps += 1;
        cx.emit(Event::Effect(Effect::Stomp {
            x: self.rect().x + self.w / 2,
            y: FLOOR_Y,
        }));
        cx.sound(SoundSlot::BOOM, SoundOp::SetVolume(BOOM_VOLUME));
        cx.emit(Event::play(SoundSlot::BOOM));
    }

    /// `FUN_0043b157`: None without contact, else side1 (0 top, 1 left, 2 below, 3 right).
    // shortcut: FW §7.3 says a tie "reports both"; the port picks the vertical side. Revisit with
    // FUN_0043b157.
    pub fn contact_side(&self, fly_box: Rect) -> Option<u8> {
        let b = self.rect();
        let pen_x = (b.x + b.w).min(fly_box.x + fly_box.w) - b.x.max(fly_box.x);
        let pen_y = (b.y + b.h).min(fly_box.y + fly_box.h) - b.y.max(fly_box.y);
        if pen_x <= 0 || pen_y <= 0 {
            return None;
        }
        if pen_x < pen_y {
            return Some(if b.x < fly_box.x { 3 } else { 1 });
        }
        Some(if b.y < fly_box.y { 2 } else { 0 })
    }
}

/// Step 3: q = (bootX − target + W) / (W / 2) and the wrap-aware distance, W = 4000 (gaps §7.2).
fn ring_distance(x: i32, target: i32) -> (f64, f64) {
    let (x, t, w) = (f64::from(x), f64::from(target), f64::from(W));
    let q = (x - t + w) / (w / 2.0);
    let dist = if (0.0..1.0).contains(&q) {
        x + w - t
    } else if (3.0..4.0).contains(&q) {
        t + w - x
    } else {
        (x - t).abs()
    };
    (q, dist)
}

fn wrap_layer(x: i32) -> i32 {
    x.rem_euclid(W)
}

impl World {
    /// Step 7: clear the active flag when gone; otherwise update unless paused.
    pub fn boot_step(&mut self, cx: &mut Ctx) {
        if !self.boot.active {
            return;
        }
        if self.boot.gone() {
            self.boot.active = false;
            return;
        }
        if self.paused {
            return;
        }
        let (fly_x, fly_w, splatted) = (self.fly.x, self.fly.size.w, self.fly.splatted);
        self.boot
            .update(fly_x, fly_w, splatted, self.camera.delta, cx);
    }

    /// Step 10: only while the fly is alive, the contact test (`FUN_0043b157`) and the push
    /// (`Fly::push_by_boot`), which may splat the fly.
    pub fn boot_collision(&mut self, cx: &mut Ctx) {
        if !self.boot.active || !self.fly.alive() {
            return;
        }
        let Some(side1) = self.boot.contact_side(self.fly.boot_box()) else {
            return;
        };
        self.fly.push_by_boot(side1, self.boot.rect(), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::match_state::{MatchConfig, MatchState, Players};

    const FLY_W: i32 = 120;

    /// FW §4.3 step 1 and §4.4 step 6: a boot at screen x 4950 (layer 3960) and 270 wide straddles
    /// the 5000 ring end and covers x −50..220, so it touches a fly box at x 0.
    #[test]
    fn a_boot_across_the_ring_end_touches_a_fly_at_the_left_edge() {
        let mut boot = Boot::new(270, 575);
        (boot.x, boot.y) = (3960, 0);
        assert_eq!(boot.screen_x(), 4950);
        assert_eq!(boot.contact_side(Rect::new(0, 100, 100, 300)), Some(3));
        assert_eq!(boot.contact_side(Rect::new(300, 100, 100, 300)), None);
    }

    /// Runs the boot over a still fly until it retires or `cap` ticks pass; returns the ticks.
    fn run(boot: &mut Boot, rig: &mut Rig, splatted: bool, cap: u64) -> u64 {
        let fly_x = 300;
        (1..=cap)
            .find(|&t| {
                boot.update(fly_x, FLY_W, splatted, 0.0, &mut rig.at(t));
                !boot.chasing
            })
            .unwrap_or(cap)
    }

    /// FW §7.1 on seed 1: x = R(4100) + 810 = 41 + 810, y = 100 − 575.
    #[test]
    fn spawn_places_the_boot_off_screen_and_chasing() {
        let mut rig = Rig::new(1);
        let mut boot = Boot::new(270, 575);
        boot.spawn(&mut rig.cx(0));
        assert_eq!((boot.x, boot.y), (851, -475));
        assert!(boot.active && boot.chasing && boot.horizontal_on && boot.stomps == 0);
        assert!(boot.retarget.expired(1000) && !boot.retarget.expired(999));
    }

    /// More than 9 floor hits: the boot retires on the update after its tenth stomp.
    #[test]
    fn the_boot_leaves_after_ten_stomps() {
        let mut rig = Rig::new(5);
        let mut boot = Boot::new(270, 575);
        boot.spawn(&mut rig.cx(0));
        run(&mut boot, &mut rig, false, 65 * 600);
        assert_eq!(boot.stomps, MAX_STOMPS + 1);
        assert!(!boot.chasing && boot.vy < 0 && !boot.horizontal_on);
        let booms = rig
            .events
            .iter()
            .filter(|e| **e == Event::play(SoundSlot::BOOM))
            .count();
        assert_eq!(booms, 10);
    }

    /// A stomp landing while the fly is splatted sets the count to 15 (+1), ending the boot.
    #[test]
    fn a_splat_ends_the_boot_after_that_stomp() {
        let mut rig = Rig::new(5);
        let mut boot = Boot::new(270, 575);
        boot.spawn(&mut rig.cx(0));
        run(&mut boot, &mut rig, true, 65 * 600);
        assert_eq!(boot.stomps, SPLAT_STOMPS + 1);
        let booms = rig
            .events
            .iter()
            .filter(|e| **e == Event::play(SoundSlot::BOOM))
            .count();
        assert_eq!(booms, 1);
    }

    /// A retired boot rises out of the top bound; step 7 then clears the active flag.
    #[test]
    fn a_retired_boot_goes_inactive() {
        let mut rig = Rig::new(5);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let m = MatchState::new(config, false, &mut rig.cx(0));
        let mut w = World::bare(&m, &mut rig.cx(0));
        w.boot.spawn(&mut rig.cx(0));
        w.boot.stomps = SPLAT_STOMPS + 1;
        let gone = (1..=650).find(|&t| {
            w.boot_step(&mut rig.at(t));
            !w.boot.active
        });
        assert!(gone.is_some_and(|t| t < 100), "{gone:?}");
    }

    /// FW §7.3: side1 is the axis with the smaller penetration.
    #[test]
    fn contact_picks_the_shallower_axis() {
        let mut boot = Boot::new(270, 575);
        (boot.x, boot.y) = (400, -5);
        // Screen x 500; the boot spans x 500..770, y −5..570.
        let below = Rect::new(520, 560, 90, 69);
        assert_eq!(boot.contact_side(below), Some(2));
        let right = Rect::new(765, 100, 90, 69);
        assert_eq!(boot.contact_side(right), Some(3));
        let left = Rect::new(420, 100, 90, 69);
        assert_eq!(boot.contact_side(left), Some(1));
        assert_eq!(boot.contact_side(Rect::new(0, 0, 90, 69)), None);
    }
}
