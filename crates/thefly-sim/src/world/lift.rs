//! W1. The lift (winch): a hook that meets a carried object and hoists it off the top
//! (fly-and-world §5.6). Driven from step 12 by the objects manager.

use super::objects::{ObjState, Object, Objects};
use super::{OBJECT_PARALLAX, W};
use crate::clock::DT;
use crate::ctx::Ctx;
use crate::event::{Pan, PlayParams, SoundOp, SoundSlot};

/// The three anchor draws must differ pairwise by more than this (shop px, `FUN_0040bf70`).
pub const ANCHOR_GAP: i32 = 36;
/// Anchors are drawn as `rand % (ANCHOR_RANGE + 1)` shop px (ai-and-rules §4.3, fly-and-world
/// §5.6) and stored as trunc(v / 0.152).
pub const ANCHOR_RANGE: i32 = 760;
pub const SHOP_SCALE: f64 = 0.152;
/// UB guard: the original redraws the anchors without a bound. After this many conflicting
/// draws the port accepts the last draw. With a 36 px gap in 760 px a conflict streak this long
/// has probability far below 1e-30, so no real seed reaches it.
pub const WINCH_REROLL_CAP: u32 = 10_000;
pub const RING: usize = 60;
/// The hook trails the object's height by this many ticks.
pub const RING_LAG: usize = 30;
pub const HOIST_VY: i32 = -300;
pub const APPROACH_VY: i32 = 450;
/// The idle bob's first period in ticks (reset, fly-and-world §5.6).
pub const IDLE_PERIOD: i32 = 102;
/// The hook offset `off`, the original's constructor argument. Open item O8: the value is not in
/// the docs (fly-and-world §5.6 only names the field); 0 is a placeholder.
pub const HOOK_OFF: i32 = 0;
/// The object layer's wrap (+0x2c): trunc(W × 1.25) = 5000.
const LIFT_WRAP: i32 = (W as f64 * OBJECT_PARALLAX) as i32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LiftState {
    #[default]
    Idle,
    Tracking,
    Hoisting,
}

/// What the lift did this tick (`FUN_00431115` result).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftResult {
    /// −1: hoisted off the top; the object is delivered.
    Delivered(usize),
    /// 1000: nothing on the hook.
    Empty,
    /// The hooked object index.
    Hooked(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lift {
    /// Stored units (shop px / 0.152), shared with the traps validator. From [`draw_anchors`].
    pub anchors: [i32; 3],
    pub anchor: usize,
    pub state: LiftState,
    pub waiting: Option<usize>,
    pub hooked: Option<usize>,
    pub approaching: bool,
    pub moved: bool,
    pub counter: i32,
    pub period: i32,
    /// Screen-ring x (0..5000), set from the anchor; `layer_x` is the object-layer x it
    /// follows the camera with.
    pub x: i32,
    pub layer_x: i32,
    pub y: i32,
    pub vy: i32,
    pub ring: [i32; RING],
    pub read: usize,
    pub write: usize,
    /// Rope bitmap height (`treuil_corde`).
    pub max_y: i32,
    /// Hook offset, the constructor argument ([`HOOK_OFF`]).
    pub off: i32,
    /// `treuil` frame size.
    pub w: i32,
    pub h: i32,
    pub rope_y: i32,
    /// Sound 47 is looping.
    pub winching: bool,
}

/// `FUN_0040bf70(760, gap, 0.152)`: three draws of `rand % 761` (0..=760), each more than `gap`
/// from every earlier one (a conflict redraws that slot), stored as trunc(v / 0.152). Bounded by
/// [`WINCH_REROLL_CAP`]. fly-and-world §5.6 and ai-and-rules §4.3 agree on `% 761`. The anchors
/// persist in [`crate::match_state::MatchState`].
pub fn draw_anchors(gap: i32, cx: &mut Ctx) -> [i32; 3] {
    let mut drawn = [0_i32; 3];
    for i in 0..drawn.len() {
        for _ in 0..=WINCH_REROLL_CAP {
            let v = cx.rng.r(ANCHOR_RANGE + 1);
            drawn[i] = v;
            if drawn[..i].iter().all(|&p| (v - p).abs() > gap) {
                break;
            }
        }
    }
    drawn.map(|v| (f64::from(v) / SHOP_SCALE) as i32)
}

impl Lift {
    /// Init (`FUN_00430e48`) then reset (`FUN_00430fe7`, fly-and-world §5.6): a random anchor,
    /// parked above the screen, idle, with a random drift.
    pub fn new(anchors: [i32; 3], cx: &mut Ctx) -> Self {
        let (body, rope) = (cx.data.size("treuil"), cx.data.size("treuil_corde"));
        let mut lift = Self {
            anchors,
            anchor: 0,
            state: LiftState::Idle,
            waiting: None,
            hooked: None,
            approaching: false,
            moved: false,
            counter: 0,
            period: IDLE_PERIOD,
            x: 0,
            layer_x: 0,
            y: 0,
            vy: 0,
            ring: [0; RING],
            read: 0,
            write: 0,
            max_y: rope.h,
            off: HOOK_OFF,
            w: body.w,
            h: body.h,
            rope_y: 0,
            winching: false,
        };
        lift.reset(cx);
        lift
    }

    /// `FUN_00430fe7`. The anchor goes through the screen-x setter (answers.md "Screen-x
    /// setter"): the layer x becomes trunc(x / 1.25) only for 0 ≤ x < wrap, and the next follow
    /// sets x = trunc(layer x × 1.25), so x can move by 1 px (1618 → 1617).
    fn reset(&mut self, cx: &mut Ctx) {
        self.anchor = usize::try_from(cx.rng.r(3)).unwrap_or(0);
        self.x = self.anchors[self.anchor];
        if (0..LIFT_WRAP).contains(&self.x) {
            self.layer_x = (f64::from(self.x) / OBJECT_PARALLAX) as i32;
        }
        self.y = -self.h;
        self.vy = cx.rng.r(150) + 50;
        (self.ring, self.read, self.write) = ([0; RING], 0, 0);
        (self.state, self.counter, self.period) = (LiftState::Idle, 0, IDLE_PERIOD);
        self.rope_y = self.y - self.max_y;
    }

    /// `FUN_00431115` then follow (`FUN_00431931`), with the winch sound 47. Every hoisting tick
    /// returns the hooked index until the winch is above the top (answers.md "Winch return while
    /// hoisting").
    pub fn update(
        &mut self,
        objects: &mut Objects,
        fly_carrying: bool,
        camera_delta: f64,
        cx: &mut Ctx,
    ) -> LiftResult {
        self.transitions(objects, fly_carrying, cx);
        let result = match self.state {
            LiftState::Idle => {
                self.bob(cx);
                LiftResult::Empty
            }
            LiftState::Tracking => self.track(objects),
            LiftState::Hoisting => self.hoist(objects),
        };
        self.follow(camera_delta, cx);
        result
    }

    /// The object manager's +0x1a ≠ 1000: from the hook tick until the delivery tick, which
    /// clears `hooked` (answers.md "Farmer voice 33").
    pub fn hoisting(&self) -> bool {
        self.state == LiftState::Hoisting && self.hooked.is_some()
    }

    fn transitions(&mut self, objects: &Objects, fly_carrying: bool, cx: &mut Ctx) {
        if !fly_carrying && self.state != LiftState::Hoisting {
            self.approaching = false;
            if let Some(w) = self.waiting.take() {
                self.hooked = Some(w);
            }
            if let Some(h) = self.hooked {
                let o = &objects.list[h];
                // Effective state ≥ 2: falling (2) or held (counts as 3).
                let in_air = o.held || o.state == ObjState::Falling;
                self.state = if in_air {
                    LiftState::Tracking
                } else {
                    LiftState::Idle
                };
                self.hooked = self.hooked.filter(|_| in_air);
            }
        }
        if fly_carrying {
            if self.waiting.is_none() && self.hooked.is_none() && objects.carried.is_some() {
                self.waiting = objects.carried;
                (self.state, self.approaching, self.counter) = (LiftState::Tracking, true, 0);
            } else if self.waiting.is_some() {
                self.state = LiftState::Tracking;
            }
        }
        if self.state == LiftState::Hoisting && self.hooked.is_none() {
            self.reset(cx);
        }
    }

    /// State 1.
    fn bob(&mut self, cx: &mut Ctx) {
        self.counter += 1;
        if self.counter > self.period {
            self.period = cx.rng.r(51) * 2;
            self.vy = cx.rng.r(250) + 50;
            if self.vy < 100 {
                self.vy = 0;
            }
            self.counter = 0;
            if cx.rng.r(10) < 5 {
                self.vy = -self.vy;
            }
        }
        if self.vy > 0 {
            self.vy += 7;
        } else if self.vy < 0 {
            self.vy = (self.vy + 3).min(0);
        }
        self.step();
    }

    /// State 2: feed the ring, approach the object, then the hook test.
    fn track(&mut self, objects: &mut Objects) -> LiftResult {
        let Some(i) = self.hooked.or(self.waiting) else {
            return LiftResult::Empty;
        };
        let bottom = objects.list[i].y + objects.list[i].h;
        self.ring[self.write] = (bottom - self.off + 30).clamp(0, self.max_y);
        self.write = (self.write + 1) % RING;
        if self.approaching {
            self.approach(bottom);
        } else {
            self.counter = self.period + 5;
        }
        let Some(h) = self.hooked else {
            return LiftResult::Empty;
        };
        let o = &objects.list[h];
        let (hook, bottom) = (self.y + self.off, o.y + o.h);
        let under = o.screen_x >= self.x - (o.w - 35) && o.screen_x + 35 <= self.x + self.w;
        if !(under && bottom < hook && hook < bottom + 30) {
            return LiftResult::Empty;
        }
        (self.state, self.vy) = (LiftState::Hoisting, HOIST_VY);
        self.step();
        self.carry(&mut objects.list[h]);
        LiftResult::Hooked(h)
    }

    fn approach(&mut self, bottom: i32) {
        let hook = self.y + self.off;
        if hook < bottom {
            self.vy = APPROACH_VY;
        } else if hook >= bottom + 50 {
            self.vy = -APPROACH_VY;
        }
        self.step();
        let hook = self.y + self.off;
        if bottom < hook && hook < bottom + 70 {
            (self.approaching, self.moved) = (false, false);
            self.read = (self.write + RING - 1) % RING;
        }
    }

    /// State 3.
    fn hoist(&mut self, objects: &mut Objects) -> LiftResult {
        self.vy = HOIST_VY;
        self.step();
        let Some(h) = self.hooked else {
            return LiftResult::Empty;
        };
        self.carry(&mut objects.list[h]);
        if self.y + self.h >= 0 {
            return LiftResult::Hooked(h);
        }
        self.hooked = None;
        LiftResult::Delivered(h)
    }

    fn carry(&self, o: &mut Object) {
        o.y = self.y + self.off - o.h + 10;
        o.on_lift = true;
    }

    /// Move (`FUN_00431826`).
    ///
    /// shortcut: §5.6 does not say what sets `moved`; Move sets it and only the end of the
    /// approach clears it. Revisit when `FUN_00431826` is re-read.
    fn step(&mut self) {
        self.y += (f64::from(self.vy) * DT) as i32;
        if self.y >= self.max_y {
            self.y = self.max_y;
            self.vy = -(f64::from(self.vy) / 1.5) as i32;
        }
        if self.state == LiftState::Idle && self.y < 1 {
            self.y = 0;
        }
        self.moved = true;
    }

    /// Follow (`FUN_00431931`).
    fn follow(&mut self, camera_delta: f64, cx: &mut Ctx) {
        self.layer_x = (self.layer_x + (-camera_delta) as i32).rem_euclid(W);
        self.x = (f64::from(self.layer_x) * OBJECT_PARALLAX) as i32;
        let lag = (self.write as i64 - self.read as i64).abs();
        if self.state == LiftState::Tracking && !self.approaching && lag == RING_LAG as i64 {
            self.read = (self.read + 1) % RING;
        }
        if !self.moved {
            self.y = self.ring[self.read];
        }
        self.rope_y = self.y - self.max_y;
        self.winch_sound(cx);
    }

    /// Sound 47: stopped while vy is 0, else positional at trunc(vy / 400 × 3000) + 18000 Hz.
    ///
    /// shortcut: §5.6 does not say how the loop starts; it is looped the first tick vy ≠ 0
    /// after a stop, then moved and retuned (as the buzz, open item O14). Revisit with O14.
    fn winch_sound(&mut self, cx: &mut Ctx) {
        if self.vy == 0 {
            if std::mem::take(&mut self.winching) {
                cx.sound(SoundSlot::WINCH, SoundOp::Stop);
            }
            return;
        }
        let hz = (f64::from(self.vy) / 400.0 * 3000.0) as i32 + 18000;
        let freq = u32::try_from(hz).unwrap_or(0);
        let pan = Pan::ScreenX {
            x: self.x,
            w: self.w,
        };
        if !std::mem::replace(&mut self.winching, true) {
            let params = PlayParams {
                freq_hz: Some(freq),
                pan,
                ..PlayParams::default()
            };
            cx.sound(SoundSlot::WINCH, SoundOp::Loop(params));
            return;
        }
        cx.sound(SoundSlot::WINCH, SoundOp::Move(pan));
        cx.sound(SoundSlot::WINCH, SoundOp::SetFrequency(freq));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::event::{Event, Pan, PlayParams, SoundOp, SoundSlot};
    use crate::rng::MsvcRand;
    use crate::world::objects::{NORMAL_TYPES, ObjState, Object};

    /// Seed 1 draws 41, 18467, 6334 (fly-and-world §1); mod 761 they are 41, 203 and 246, which
    /// are more than 36 apart, so no slot is redrawn. Stored as trunc(v / 0.152).
    #[test]
    fn seed_one_anchors_are_the_first_three_draws_mod_761() {
        let (mut rng, data, mut events) = (MsvcRand::new(1), GameData::synthetic(), Vec::new());
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        assert_eq!(draw_anchors(ANCHOR_GAP, &mut cx), [269, 1335, 1618]);
        let mut probe = MsvcRand::new(1);
        (0..3).for_each(|_| _ = probe.rand());
        assert_eq!(rng, probe);
    }

    #[test]
    fn anchors_are_more_than_the_gap_apart_for_any_seed() {
        let (data, mut events) = (GameData::synthetic(), Vec::new());
        for seed in 0..500 {
            let mut rng = MsvcRand::new(seed);
            let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
            let a = draw_anchors(ANCHOR_GAP, &mut cx).map(|s| (f64::from(s) * SHOP_SCALE).round());
            let gap = f64::from(ANCHOR_GAP);
            assert!(
                (a[0] - a[1]).abs() > gap && (a[0] - a[2]).abs() > gap && (a[1] - a[2]).abs() > gap,
                "seed {seed}: {a:?}"
            );
        }
    }

    fn bench(seed: u32) -> (MsvcRand, GameData, Vec<Event>) {
        (MsvcRand::new(seed), GameData::synthetic(), Vec::new())
    }

    const ANCHORS: [i32; 3] = [269, 1335, 1618];

    /// answers.md "Screen-x setter": the layer x follows the screen x only for 0 ≤ x < wrap. An
    /// anchor of 760 shop px is trunc(760 / 0.152) = 5000, the wrap, so the layer x is kept.
    #[test]
    fn an_anchor_at_the_wrap_keeps_the_layer_x() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new([5000; 3], &mut cx);
        assert_eq!((lift.x, lift.layer_x), (5000, 0));
        lift.layer_x = 700;
        lift.reset(&mut cx);
        assert_eq!((lift.x, lift.layer_x), (5000, 700));
        lift.anchors = ANCHORS;
        lift.reset(&mut cx);
        assert_eq!(lift.layer_x, (f64::from(lift.x) / OBJECT_PARALLAX) as i32);
    }

    /// Seed 1: anchor = 41 % 3 = 2, vy = 18467 % 150 + 50 = 67.
    #[test]
    fn reset_picks_an_anchor_and_a_drift_above_the_screen() {
        let (mut rng, data, mut events) = bench(1);
        let lift = Lift::new(ANCHORS, &mut Ctx::new(0, &mut rng, &data, &mut events));
        assert_eq!((lift.anchor, lift.x, lift.y, lift.vy), (2, 1618, -151, 67));
        assert_eq!(
            (lift.state, lift.counter, lift.period),
            (LiftState::Idle, 0, IDLE_PERIOD)
        );
    }

    /// Idle: vy 67 + 7 = 74, y = −151 + trunc(74 / 65) = −150, then y < 1 snaps to 0. The winch
    /// loops at trunc(74 / 400 × 3000) + 18000 = 18555 Hz.
    #[test]
    fn the_idle_hook_accelerates_and_snaps_to_the_top() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new(ANCHORS, &mut cx);
        let r = lift.update(&mut Objects::empty(), false, 0.0, &mut cx);
        assert_eq!(r, LiftResult::Empty);
        assert_eq!(
            (lift.counter, lift.vy, lift.y, lift.rope_y),
            (1, 74, 0, -380)
        );
        let params = PlayParams {
            freq_hz: Some(18555),
            // The anchor goes through the layer x (answers.md "Screen-x setter"): 1617.
            pan: Pan::ScreenX { x: lift.x, w: 235 },
            ..PlayParams::default()
        };
        let op = SoundOp::Loop(params);
        assert_eq!(
            events,
            [Event::Sound {
                slot: SoundSlot::WINCH,
                op
            }]
        );
    }

    /// Past the period: draws 6334, 26500, 19169 give period (6334 % 51) × 2 = 20, vy =
    /// 26500 % 250 + 50 = 50 → below 100 so 0, and 19169 % 10 = 9 keeps the sign.
    #[test]
    fn the_idle_hook_rerolls_its_drift_after_the_period() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new(ANCHORS, &mut cx);
        (lift.counter, lift.y) = (IDLE_PERIOD, 100);
        lift.update(&mut Objects::empty(), false, 0.0, &mut cx);
        assert_eq!(
            (lift.counter, lift.period, lift.vy, lift.y),
            (0, 20, 0, 100)
        );
        let mut probe = MsvcRand::new(1);
        (0..5).for_each(|_| _ = probe.rand());
        assert_eq!(rng, probe);
        // Never started, so vy 0 sends no stop.
        assert!(events.is_empty());
    }

    fn one_object(y: i32) -> Objects {
        let mut objects = Objects::empty();
        let t = &NORMAL_TYPES[0];
        let mut o = Object::of_type(0, t, GameData::synthetic().size(t.name));
        o.set_screen_x(1618);
        o.y = y;
        objects.list.push(o);
        objects
    }

    #[test]
    fn a_carried_object_calls_the_hook_down() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new(ANCHORS, &mut cx);
        let mut objects = one_object(300);
        objects.carried = Some(0);
        objects.list[0].held = true;
        lift.update(&mut objects, true, 0.0, &mut cx);
        assert_eq!(
            (lift.state, lift.waiting, lift.approaching),
            (LiftState::Tracking, Some(0), true)
        );
        // Hook y + off below the bottom (400): vy = 450, y = −151 + trunc(450 / 65) = −145.
        assert_eq!((lift.vy, lift.y), (APPROACH_VY, -145));
        assert_eq!(lift.ring[0], 380.min(400 - HOOK_OFF + 30));
        // Let go of a held object: it is hooked and tracking goes on.
        objects.carried = None;
        lift.update(&mut objects, false, 0.0, &mut cx);
        assert_eq!(
            (lift.waiting, lift.hooked, lift.state),
            (None, Some(0), LiftState::Tracking)
        );
        assert!(!lift.approaching);
    }

    #[test]
    fn an_object_dropped_short_releases_the_hook() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new(ANCHORS, &mut cx);
        let mut objects = one_object(300);
        (lift.state, lift.waiting) = (LiftState::Tracking, Some(0));
        objects.list[0].state = ObjState::Sliding;
        lift.update(&mut objects, false, 0.0, &mut cx);
        assert_eq!((lift.hooked, lift.state), (None, LiftState::Idle));
    }

    /// Hook window: bottom (y + 100) < lift y + off < bottom + 30. Hoisting moves trunc(−300 / 65)
    /// = −4 per tick and carries the object at y + off − h + 10 until the lift is off the top.
    #[test]
    fn a_falling_object_in_the_window_is_hooked_hoisted_and_delivered() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new(ANCHORS, &mut cx);
        let mut objects = one_object(200);
        objects.list[0].state = ObjState::Falling;
        (lift.state, lift.hooked, lift.y, lift.moved) = (LiftState::Tracking, Some(0), 310, true);
        let r = lift.update(&mut objects, false, 0.0, &mut cx);
        assert_eq!(r, LiftResult::Hooked(0));
        assert_eq!(
            (lift.state, lift.vy, lift.y),
            (LiftState::Hoisting, HOIST_VY, 306)
        );
        assert_eq!(objects.list[0].y, 306 + HOOK_OFF - 100 + 10);
        assert!(objects.list[0].on_lift);
        let mut ticks = 0;
        let delivered = loop {
            ticks += 1;
            match lift.update(&mut objects, false, 0.0, &mut cx) {
                LiftResult::Hooked(0) if ticks < 1000 => continue,
                other => break other,
            }
        };
        // 306 − 4n + 151 < 0 first at n = 115.
        assert_eq!((delivered, ticks), (LiftResult::Delivered(0), 115));
        assert_eq!(lift.hooked, None);
        // The next tick resets: two more draws (R(3), R(150)).
        let before = cx.rng.state();
        lift.update(&mut objects, false, 0.0, &mut cx);
        assert_eq!(lift.state, LiftState::Idle);
        assert_ne!(cx.rng.state(), before);
    }

    #[test]
    fn an_object_outside_the_column_is_not_hooked() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new(ANCHORS, &mut cx);
        let mut objects = one_object(200);
        objects.list[0].set_screen_x(1618 + 235 - 34);
        objects.list[0].state = ObjState::Falling;
        (lift.state, lift.hooked, lift.y, lift.moved) = (LiftState::Tracking, Some(0), 310, true);
        assert_eq!(
            lift.update(&mut objects, false, 0.0, &mut cx),
            LiftResult::Empty
        );
    }

    /// Follow: the lift rides the object layer; a camera delta of 4.0 moves its layer x by −4,
    /// its screen x by trunc(layer × 1.25).
    #[test]
    fn the_lift_follows_the_camera() {
        let (mut rng, data, mut events) = bench(1);
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let mut lift = Lift::new(ANCHORS, &mut cx);
        // layer = trunc(1618 / 1.25) = 1294; 1290 × 1.25 = 1612.
        lift.update(&mut Objects::empty(), false, 4.0, &mut cx);
        assert_eq!(lift.x, 1612);
    }
}
