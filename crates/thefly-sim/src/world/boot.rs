//! W2. The friendly boot (fly-and-world §7, ai-and-rules §6.1, answers.md "Boot: ..."). Ticks
//! steps 7 and 10.

use std::cmp::Ordering;

use super::collision::Rect;
use super::{OBJECT_PARALLAX, W, World};
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Effect, Event, SoundOp, SoundSlot};
use crate::mover::FpsRun;

pub const RETARGET_FIRST_MS: u64 = 1000;
/// The boot leaves after this many floor hits (count > 9).
pub const MAX_STOMPS: i32 = 9;
/// Splatting the fly sets the count to this, ending the boot after the stomp.
pub const SPLAT_STOMPS: i32 = 15;
/// Stomp when the horizontal distance to the target is below this.
pub const STOMP_DISTANCE: f64 = 11.0;
/// The boot's own screen wrap (+0x2c), trunc(W × 1.25); the distance ring and the spawn x
/// R(5000 − 900) + 810 are in screen x (FW §7.1).
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
/// speed = trunc(dist × 2000 / BOOT_WRAP + 250).
pub const SPEED_K: i32 = 2000;
pub const SPEED_BASE: i32 = 250;
/// Bursts: a running one ends on R(100) ≥ 91; one starts on R(100) > 97 with R(500) + 500.
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
    /// Screen x (+0xc) on the [`BOOT_WRAP`] ring: what the boot measures, hovers and collides by.
    pub x: i32,
    /// Layer x on the 1.25 layer (+4), which the mover integrates; `x` = trunc(layer × 1.25).
    pub layer_x: i32,
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
    pub run: FpsRun,
}

/// A contact (`FUN_0043b157`): side 0 top, 1 left, 2 below, 3 right. On a tie side1 is the
/// horizontal side and side2 the vertical one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contact {
    pub side1: u8,
    pub side2: Option<u8>,
}

impl Boot {
    /// An inactive boot with the `i_botte` frame size.
    pub fn new(w: i32, h: i32) -> Self {
        Self {
            active: false,
            x: 0,
            layer_x: 0,
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
            run: FpsRun::default(),
        }
    }

    /// `FUN_0043aabe`, from the farmer's Boot goodie: chasing from the start (+0x178 = 1). The
    /// spawn x is a screen x below the wrap, so the setter sets the layer x to trunc(x / 1.25)
    /// (answers.md "Screen-x setter").
    pub fn spawn(&mut self, cx: &mut Ctx) {
        self.x = cx.rng.r(BOOT_WRAP - SPAWN_X_SPAN) + SPAWN_X_MIN;
        self.layer_x = (f64::from(self.x) / OBJECT_PARALLAX) as i32;
        self.y = SPAWN_Y - self.h;
        (self.vx, self.vy, self.accel_y, self.accel, self.offset) = (0, 0, 0, 0, 0);
        self.active = true;
        self.horizontal_on = true;
        self.vertical_on = false;
        self.chasing = true;
        self.stomping = false;
        self.stomps = 0;
        self.retarget = Timer::armed(cx.now, RETARGET_FIRST_MS);
        self.run = FpsRun::default();
    }

    /// The drawn box: across the ring end the screen x is shifted by −BOOT_WRAP (FW §4.4 step 6).
    pub fn rect(&self) -> Rect {
        let x = if self.x >= BOOT_WRAP - self.w {
            self.x - BOOT_WRAP
        } else {
            self.x
        };
        Rect::new(x, self.y, self.w, self.h)
    }

    /// Gone once retired and risen to the top bound (the mover holds y ≥ −h).
    fn gone(&self) -> bool {
        !self.chasing && self.y + self.h < 1
    }

    fn hover_y(&self) -> i32 {
        self.ceiling_y() + (f64::from(self.x / HOVER_X_DIVISOR).sin() * HOVER_AMPLITUDE) as i32
    }

    fn ceiling_y(&self) -> i32 {
        HOVER_Y - self.h
    }

    /// FW §7.2 (`FUN_0043abc7`) for one tick, in the order of answers.md "Boot: ... step order":
    /// retire, retarget, distance, burst, steer, stomp trigger, hover, floor, ceiling, then the
    /// mover. `fly_x`/`fly_w` place the target; `fly_splatted` is read on the floor.
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
        let speed =
            (dist * f64::from(SPEED_K) / f64::from(BOOT_WRAP) + f64::from(SPEED_BASE)) as i32;
        self.burst(cx);
        if !self.stomping {
            self.steer(q, speed + self.accel);
        }
        if dist < STOMP_DISTANCE && !self.stomping && self.chasing {
            self.vx = 0;
            if cx.rng.r(100) > STOMP_ABOVE {
                self.stomping = true;
                (self.accel_y, self.vy, self.vertical_on) = (STOMP_ACCEL, 0, true);
            }
        }
        if !self.stomping && self.chasing {
            self.y = self.hover_y();
        }
        self.floor(fly_splatted, cx);
        if self.stomping && self.chasing && self.y < self.ceiling_y() {
            self.y = self.ceiling_y();
            (self.vy, self.accel_y, self.accel) = (0, 0, 0);
            (self.stomping, self.vertical_on) = (false, false);
        }
        self.integrate(delta);
    }

    /// Step 1.
    fn retire(&mut self) {
        self.chasing = false;
        (self.vy, self.accel_y, self.vertical_on) = (RETIRE_VY, RETIRE_ACCEL, true);
        (self.vx, self.horizontal_on) = (0, false);
    }

    /// Step 5 (answers.md "Boot burst draws"): both rolls are drawn even while stomping, and a
    /// running burst that survives its first roll is rolled again.
    fn burst(&mut self, cx: &mut Ctx) {
        let rolls_on = self.accel < 1 || cx.rng.r(100) < BURST_END_FROM || self.stomping;
        if !rolls_on {
            self.accel = 0;
            return;
        }
        if cx.rng.r(100) > BURST_START_ABOVE && !self.stomping {
            self.accel = cx.rng.r(BURST_RAND) + BURST_BASE;
        }
    }

    /// Step 6. q in [1, 2) or [3, 4) goes right, [0, 1) or [2, 3) goes left; else vx is kept.
    /// The horizontal-on flag is not checked, so a retired boot still steers. The burst adds to
    /// the speed: the mover adds sign × accel once per tick.
    fn steer(&mut self, q: f64, speed: i32) {
        if !(0.0..4.0).contains(&q) {
            return;
        }
        self.vx = if (1.0..2.0).contains(&q) || q >= 3.0 {
            speed
        } else {
            -speed
        };
    }

    /// The shared FPS mover (`FUN_004180af`, FW §4.1) on the layer x, then the camera shift; the
    /// y acceleration grows |vy| each tick along the sign of vy (0 counts as down). The mover
    /// bounds hold y in [−h, 570 − h].
    fn integrate(&mut self, delta: f64) {
        if self.vertical_on {
            self.vy += if self.vy < 0 {
                -self.accel_y
            } else {
                self.accel_y
            };
        }
        let vx = if self.horizontal_on { self.vx } else { 0 };
        let vy = if self.vertical_on { self.vy } else { 0 };
        let (dx, dy) = self.run.step([vx, vy]);
        self.layer_x = (self.layer_x + dx + (-delta) as i32).rem_euclid(W);
        self.x = ((f64::from(self.layer_x) * OBJECT_PARALLAX) as i32).rem_euclid(BOOT_WRAP);
        self.y = (self.y + dy).clamp(-self.h, FLOOR_Y - self.h);
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

    /// `FUN_0043b157` (answers.md "Boot contact tie"): inclusive overlap of the boot's frame and
    /// the fly box, and per side the distance between the facing edges.
    pub fn contact(&self, fly_box: Rect) -> Option<Contact> {
        let (b, f) = (self.rect(), fly_box);
        let overlap = b.x <= f.x + f.w && f.x <= b.x + b.w && b.y <= f.y + f.h && f.y <= b.y + b.h;
        if !overlap {
            return None;
        }
        let (h_side, h_pen) = if b.x < f.x {
            (3, (b.x + b.w - f.x).abs())
        } else {
            (1, (b.x - (f.x + f.w)).abs())
        };
        let (v_side, v_pen) = if b.y < f.y {
            (2, (b.y + b.h - f.y).abs())
        } else {
            (0, (b.y - (f.y + f.h)).abs())
        };
        let (side1, side2) = match h_pen.cmp(&v_pen) {
            Ordering::Less => (h_side, None),
            Ordering::Greater => (v_side, None),
            Ordering::Equal => (h_side, Some(v_side)),
        };
        Some(Contact { side1, side2 })
    }
}

/// Steps 3–4 on the boot's screen ring: q = (bootX − target + Wb) / (Wb / 2.0) and the
/// wrap-aware distance, Wb = [`BOOT_WRAP`].
fn ring_distance(x: i32, target: i32) -> (f64, f64) {
    let (x, t, w) = (f64::from(x), f64::from(target), f64::from(BOOT_WRAP));
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
    /// (`FUN_0043734c`), which may splat the fly. On a tie side1 is horizontal, so the push
    /// cannot splat.
    pub fn boot_collision(&mut self, cx: &mut Ctx) {
        if !self.boot.active || !self.fly.alive() {
            return;
        }
        let Some(contact) = self.boot.contact(self.fly.boot_box()) else {
            return;
        };
        let boot = self.boot.rect();
        self.fly
            .push_by_boot(contact.side1, contact.side2, boot, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::match_state::{MatchConfig, MatchState, Players};
    use crate::rng::MsvcRand;

    const FLY_W: i32 = 120;

    /// A spawned 270 × 575 boot at screen x `x` (layer x × 1.25 exact), rand reset to `seed`.
    fn boot_at(x: i32, seed: u32) -> (Boot, Rig) {
        let mut rig = Rig::new(1);
        let mut boot = Boot::new(270, 575);
        boot.spawn(&mut rig.cx(0));
        (boot.layer_x, boot.x) = ((f64::from(x) / OBJECT_PARALLAX) as i32, x);
        rig.rng = MsvcRand::new(seed);
        (boot, rig)
    }

    /// FW §4.3 step 1 and §4.4 step 6: a boot at screen x 4950 and 270 wide straddles the 5000
    /// ring end and covers x −50..220, so it touches a fly box at x 0.
    #[test]
    fn a_boot_across_the_ring_end_touches_a_fly_at_the_left_edge() {
        let (mut boot, _) = boot_at(4950, 1);
        boot.y = 0;
        assert_eq!(
            boot.contact(Rect::new(0, 100, 100, 300)).map(|c| c.side1),
            Some(3)
        );
        assert_eq!(boot.contact(Rect::new(300, 100, 100, 300)), None);
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

    /// answers.md "Boot: ... spawn x": seed 1 puts the screen x at R(5000 − 900) + 810 = 851,
    /// the layer x at trunc(851 / 1.25) = 680; y = 100 − 575.
    #[test]
    fn spawn_places_the_boot_off_screen_and_chasing() {
        let mut rig = Rig::new(1);
        let mut boot = Boot::new(270, 575);
        boot.spawn(&mut rig.cx(0));
        assert_eq!((boot.x, boot.layer_x, boot.y), (851, 680, -475));
        assert!(boot.active && boot.chasing && boot.horizontal_on && boot.stomps == 0);
        assert!(boot.retarget.expired(1001) && !boot.retarget.expired(1000));
    }

    /// answers.md "Boot: distance metric, spawn x and step order". Screen x 4895, target
    /// 420 − 320 = 100 on the 5000 ring: q = 9795 / 2500 ∈ [3, 4), dist = 100 + 5000 − 4895 =
    /// 205, speed = trunc(205 · 2000 / 5000 + 250) = 332, to the right. Seed 1's 41 is no burst.
    /// The hover runs before the move, on x / 10 = 489: y = 80 − 575 + trunc(sin 489 · 5) =
    /// −499. Then the FPS mover steps trunc(332 / 65) = 5 layer px: 3916 → 3921, screen 4901.
    #[test]
    fn one_update_measures_on_the_screen_ring_and_hovers_before_moving() {
        let (mut boot, mut rig) = boot_at(4895, 1);
        boot.update(420, FLY_W, false, 0.0, &mut rig.cx(0));
        assert_eq!((boot.vx, boot.y), (332, -499));
        assert_eq!((boot.layer_x, boot.x), (3921, 4901));
        assert_eq!(rig.rng.rand(), 18467);
    }

    /// answers.md "Boot burst draws", with (accel, stomping, seed) → (accel, next rand):
    /// 0 free on seed 49: 198 % 100 = 98 > 97, accel = 10103 % 500 + 500 = 603;
    /// 0 stomping on seed 49: the R(100) is still drawn, nothing set;
    /// 600 free on seed 1: 41 < 91 re-rolls, 18467 % 100 = 67 keeps 600;
    /// 600 free on seed 17: 94 ≥ 91 ends it with one draw;
    /// 600 stomping on seed 17: 94 ≥ 91 but stomping, so 26602 is drawn and 600 stays.
    #[test]
    fn the_burst_draws_follow_the_nested_rolls() {
        let cases = [
            (0, false, 49, 603, 11846),
            (0, true, 49, 0, 10103),
            (600, false, 1, 600, 6334),
            (600, false, 17, 0, 26602),
            (600, true, 17, 600, 30017),
        ];
        for (accel, stomping, seed, want, next) in cases {
            let (mut boot, mut rig) = boot_at(1000, seed);
            (boot.accel, boot.stomping) = (accel, stomping);
            boot.burst(&mut rig.cx(0));
            assert_eq!(
                (boot.accel, rig.rng.rand()),
                (want, next),
                "{accel} {stomping}"
            );
        }
    }

    /// The ceiling reset needs stomping and chasing, and also ends the burst.
    #[test]
    fn the_ceiling_stops_a_rising_stomp_and_its_burst() {
        let (mut boot, mut rig) = boot_at(1000, 1);
        let ceiling = HOVER_Y - boot.h;
        (boot.y, boot.vy, boot.accel_y) = (ceiling - 7, -300, 150);
        (boot.stomping, boot.vertical_on, boot.accel) = (true, true, 600);
        boot.update(1300, FLY_W, false, 0.0, &mut rig.cx(0));
        assert!(!boot.stomping && boot.accel == 0 && boot.vy == 0);
        assert_eq!(boot.y, ceiling);
    }

    /// A retired boot still steers, but with horizontal motion off it does not move.
    #[test]
    fn a_retired_boot_steers_in_place() {
        let (mut boot, mut rig) = boot_at(1000, 1);
        boot.stomps = MAX_STOMPS + 1;
        boot.update(2000, FLY_W, false, 0.0, &mut rig.cx(0));
        assert!(!boot.chasing && !boot.horizontal_on && boot.vx > 0);
        assert_eq!(boot.x, 1000);
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

    /// A retired boot rises until the mover holds it at y = −h; step 7 then clears the active
    /// flag (y + h < 1).
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
            assert!(w.boot.y >= -w.boot.h);
            !w.boot.active
        });
        assert!(gone.is_some_and(|t| t < 100), "{gone:?}");
    }

    /// answers.md "Boot contact tie": inclusive overlap, penetration from the facing edges. The
    /// boot spans x 500..770, y −5..570.
    #[test]
    fn contact_picks_the_side_with_the_shallower_edge() {
        let (mut boot, _) = boot_at(500, 1);
        boot.y = -5;
        let side = |r: Rect| boot.contact(r).map(|c| (c.side1, c.side2));
        // Below: |570 − 560| = 10 against |770 − 520| = 250.
        assert_eq!(side(Rect::new(520, 560, 90, 69)), Some((2, None)));
        assert_eq!(side(Rect::new(765, 100, 90, 69)), Some((3, None)));
        assert_eq!(side(Rect::new(420, 100, 90, 69)), Some((1, None)));
        // Inside horizontally: |770 − 510| = 260 beats |570 − 100| = 470 (min/max would say 2).
        assert_eq!(side(Rect::new(510, 100, 90, 69)), Some((3, None)));
        // Touching edges overlap.
        assert_eq!(side(Rect::new(770, 100, 90, 69)), Some((3, None)));
        assert_eq!(side(Rect::new(0, 0, 90, 69)), None);
        // A tie (10 and 10): horizontal first, vertical second.
        assert_eq!(side(Rect::new(760, 560, 90, 69)), Some((3, Some(2))));
    }

    /// On a tie the push places both axes and never splats: x = 770 − 25, y = 570 − 20 capped
    /// at 600 − 94.
    #[test]
    fn a_tie_pushes_both_ways_without_a_splat() {
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let m = MatchState::new(config, false, &mut rig.cx(0));
        let mut w = World::bare(&m, &mut rig.cx(0));
        w.boot = Boot::new(270, 575);
        w.boot.spawn(&mut rig.cx(0));
        (w.boot.layer_x, w.boot.x, w.boot.y) = (400, 500, -5);
        (w.fly.x, w.fly.y) = (730, 535);
        assert!(w.fly.alive() && w.fly.size.w == 120 && w.fly.size.h == 94);
        w.boot_collision(&mut rig.cx(0));
        assert_eq!((w.fly.x, w.fly.y), (745, 506));
        assert!(!w.fly.splatted);
    }
}
