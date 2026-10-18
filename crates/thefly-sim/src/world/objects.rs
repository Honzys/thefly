//! W1. Table objects: the type table, placement, motion, stacking, fly contact, grab, carry, drop
//! and delivery (fly-and-world §4.4, §5). Ticks step 12, which also runs the lift (§5.5 step 9)
//! and the period clock.

use super::collision::{Rect, Shape, fly_outline, outlines_overlap};
use super::fly::Fly;
use super::lift::LiftResult;
use super::traps::{Traps, fan_frame, play_fan_hit};
use super::{OBJECT_PARALLAX, W, World};
use crate::clock::DT;
use crate::ctx::Ctx;
use crate::data::{Outline, Size};
use crate::event::{Effect, Event, Pan, PlayParams, SoundOp, SoundSlot};
use crate::match_state::{MatchState, Meal};

/// Placement stops at this many objects (§5.2).
pub const MAX_PLACED: usize = 15;
/// Fan stands become extra non-collectible objects; there are at most 5 fans.
pub const MAX_OBJECTS: usize = MAX_PLACED + 5;
/// Placement gives up after this many failed objects.
pub const PLACEMENT_FAILURES: u32 = 2000;
/// Type draws per object until an eligible type with stock comes up.
pub const PLACEMENT_TYPE_TRIES: u32 = 5001;
/// x draws per budget (answers.md "Extreme placement"): per object in the normal placement,
/// shared by successes in the extreme one.
pub const PLACEMENT_X_TRIES: u32 = 1001;
/// Fly–object separation steps (§4.4 step 5).
pub const SEPARATE_TRIES: u32 = 1000;
/// Grab lock distance from the fly's bottom-centre to the grip point (§5.5 step 5).
pub const LOCK_DISTANCE: f64 = 1000.0;
/// Side-test retries, d = 2..=11, when a falling object meets another (answers.md "Object side
/// test").
pub const FALL_SIDE_RETRIES: u32 = 10;
/// The probe distance after a blocked attach.
const BLOCKED_PROBE: i32 = 5;
/// Fan bounce of a moving object (answers.md "Object fan push"): |vx| and vy grow by
/// R(60) + 90, drop vx by 50 + R(70), then an unused R(100) sign.
const FAN_BOUNCE_RAND: i32 = 60;
const FAN_BOUNCE_BASE: i32 = 90;
const FAN_DROP_BASE: i32 = 50;
const FAN_DROP_RAND: i32 = 70;
const FAN_UNUSED_SIGN_ROLL: i32 = 100;
/// A carried object on the blades kicks the fly back this much per axis.
const CARRIED_FAN_KICK: i32 = 200;
pub const GRAVITY_PER_TICK: i32 = 50;
pub const MAX_VX: i32 = 500;
/// Objects rest at y = 580 − h.
pub const TABLE_Y: i32 = 580;
/// The resolve takes the push-up on this call within one state (answers.md "`FUN_004303e5`
/// push-up, the stuck counter and the held flag").
const STUCK_LIMIT: u32 = 31;
/// The push-up moves 1 px per retest, at most this many times.
const PUSH_UP_STEPS: u32 = 500;
/// Impact sound `FUN_0042dc21`: trunc((1 − weight / 100) × 20000) + R(6000) + 15000 Hz at −150.
const IMPACT_HZ_SPAN: f64 = 20000.0;
const IMPACT_HZ_RAND: i32 = 6000;
const IMPACT_HZ_BASE: i32 = 15000;
const IMPACT_VOLUME_CB: i32 = -150;
/// Each landing dust puff draws R(h), R(w), then 5 more (`FUN_00412fef`, `FUN_00412992`).
const DUST_DRAWS_PER_PUFF: i32 = 7;

/// A row of the type table (§5.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectType {
    /// Sprite and `.bor` name.
    pub name: &'static str,
    /// The point the fly's bottom-centre locks onto, from the sprite's top-left.
    pub grip_x: i32,
    pub grip_y: i32,
    pub weight: i32,
    /// Blocking order in the push test (§4.4 step 3).
    pub rank: i32,
    /// Meal bits: 1 breakfast, 2 lunch, 4 dinner; 7 for the extreme set (any meal).
    pub meal: u8,
    /// Copies per period.
    pub stock: i32,
    /// Base price before fluctuation (data+0x160, or the extreme table).
    pub price: f32,
}

/// A table row: name, [grip x, grip y, weight, rank, meal, stock], price.
const fn t(name: &'static str, c: [i32; 6], price: f32) -> ObjectType {
    let [grip_x, grip_y, weight, rank, meal, stock] = c;
    ObjectType {
        name,
        grip_x,
        grip_y,
        weight,
        rank,
        meal: meal as u8,
        stock,
        price,
    }
}

/// The 22 normal types in type order (`FUN_0040ae51`).
pub const NORMAL_TYPES: [ObjectType; 22] = [
    t("can1", [74, 20, 75, 3, 4, 2], 60.0),
    t("can2", [111, 20, 35, 3, 4, 3], 15.0),
    t("can3", [64, 20, 35, 3, 4, 3], 12.0),
    t("chips1", [75, 20, 15, 2, 4, 3], 5.0),
    t("chips2", [75, 20, 15, 2, 4, 3], 5.0),
    t("clope", [91, 20, 18, 1, 7, 2], 22.0),
    t("saucisson", [150, 40, 100, 5, 4, 1], 90.0),
    t("saucisson1", [70, 20, 20, 2, 6, 3], 20.0),
    t("cacahuete", [60, 20, 15, 1, 4, 6], 10.0),
    t("zippo", [88, 50, 60, 3, 4, 1], 100.0),
    t("lait", [95, 20, 110, 3, 1, 1], 60.0),
    t("mandarine", [85, 20, 45, 3, 1, 5], 21.0),
    t("pomme", [100, 20, 60, 3, 1, 4], 43.0),
    t("sucre", [48, 20, 25, 3, 1, 5], 30.0),
    t("toaster", [234, 20, 130, 3, 1, 1], 100.0),
    t("toast", [117, 20, 30, 3, 1, 6], 18.0),
    t("poulet", [120, 20, 75, 3, 2, 3], 60.0),
    t("poivriere", [58, 20, 60, 3, 2, 2], 10.0),
    t("tomate", [88, 20, 60, 3, 2, 4], 30.0),
    t("muffin", [82, 20, 25, 3, 2, 4], 40.0),
    t("cornichon", [66, 20, 25, 3, 2, 6], 8.0),
    t("mais", [203, 20, 90, 3, 2, 2], 65.0),
];

/// Extreme mode uses only these (`FUN_0042bdf8`). The doc gives no meal mask ("–"); 7 encodes
/// "always eligible".
pub const EXTREME_TYPES: [ObjectType; 3] = [
    t("extreme1", [137, 20, 135, 3, 7, 3], 80.0),
    t("extreme2", [107, 20, 100, 2, 7, 5], 50.0),
    t("extreme3", [74, 20, 70, 1, 7, 7], 20.0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjState {
    #[default]
    Rest,
    /// Pushed; friction trunc(weight / 3) per tick.
    Sliding,
    /// Dropped or knocked off: vx += drop vx, vy += 50 per tick.
    Falling,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    /// Index into the active type table, or None for a fan stand.
    pub kind: Option<usize>,
    /// Sprite and outline name.
    pub sprite: &'static str,
    pub w: i32,
    pub h: i32,
    /// Layer x on the 1.25 layer (wraps at the wrap width) and the derived screen x.
    pub layer_x: i32,
    pub screen_x: i32,
    pub y: i32,
    pub rest_y: i32,
    pub prev_x: i32,
    pub prev_y: i32,
    pub vx: i32,
    pub vy: i32,
    pub active: bool,
    /// Read and cleared: the landing dust, weight × 20 / 130 puffs.
    pub landed: bool,
    pub held: bool,
    pub state: ObjState,
    pub drop_vx: i32,
    /// Resolve calls within one uninterrupted state (+0x80); held objects do not count.
    pub stuck: u32,
    /// Objects this one rests on, and objects resting on it.
    pub supported_by: Vec<usize>,
    pub resting_on_top: Vec<usize>,
    pub grip_x: i32,
    pub grip_y: i32,
    pub weight: i32,
    pub collectible: bool,
    pub rank: i32,
    pub on_lift: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Objects {
    pub list: Vec<Object>,
    /// trunc(W × 1.25), screen units.
    pub wrap_width: i32,
    pub carrying: bool,
    /// None is the original's 1000.
    pub carried: Option<usize>,
    pub locked: Option<usize>,
    /// Lift loaded with an object this tick.
    pub lift_loaded: bool,
    pub all_delivered: bool,
}

impl Objects {
    /// No objects on the table: what a world has before [`Objects::place`] runs.
    pub fn empty() -> Self {
        Self {
            list: Vec::new(),
            wrap_width: (f64::from(W) * OBJECT_PARALLAX) as i32,
            carrying: false,
            carried: None,
            locked: None,
            lift_loaded: false,
            all_delivered: false,
        }
    }

    /// `FUN_0042d459` (FW §4.5): a box test of `r` (screen) against each active object.
    pub fn box_hit(&self, r: &Rect) -> bool {
        self.list
            .iter()
            .filter(|o| o.active)
            .any(|o| r.overlaps(&Rect::new(self.screen_left(o), o.y, o.w, o.h)))
    }

    /// The object's screen x, minus the wrap width when it straddles the ring's end (§4.4 step 6).
    fn screen_left(&self, o: &Object) -> i32 {
        if o.screen_x >= self.wrap_width - o.w {
            o.screen_x - self.wrap_width
        } else {
            o.screen_x
        }
    }

    /// Period start (§5.2): resets the per-type counts in `m`, places up to [`MAX_PLACED`]
    /// objects avoiding each other and the traps, then adds the fan stands.
    pub fn place(m: &mut MatchState, meal: Meal, traps: &Traps, cx: &mut Ctx) -> Self {
        let types: &[ObjectType] = if m.config.extreme() {
            &EXTREME_TYPES
        } else {
            &NORMAL_TYPES
        };
        let eligible: Vec<bool> = types.iter().map(|t| t.meal & meal as u8 != 0).collect();
        m.economy.start_period(&eligible);
        let no_traps =
            traps.flypapers.is_empty() && traps.fans.is_empty() && traps.zappers.is_empty();
        let free = |x: i32, w: i32| no_traps || traps.span_free(x, w);
        let mut objects = Self::empty();
        objects.list = if m.config.extreme() {
            place_extreme(free, cx)
        } else {
            place_on_table(types, &eligible, free, cx)
        };
        m.stats.available += objects.list.len() as i32;
        objects.add_fan_stands(traps, cx);
        objects
    }

    /// §5.2 step 4 (`FUN_00427954`, answers.md "Fan stands, fan y and trap x"): one extra,
    /// non-collectible object per fan, in fan order, at the fan's screen x and y with its frame
    /// size; `piedventi_dt` under a fan that blows right.
    fn add_fan_stands(&mut self, traps: &Traps, cx: &Ctx) {
        for fan in &traps.fans {
            let sprite = if fan.facing_right {
                "piedventi_dt"
            } else {
                "piedventi_gc"
            };
            let mut o = Object::blank(sprite, fan_frame(cx));
            o.set_screen_x(fan.screen_x);
            (o.y, o.rest_y, o.prev_y) = (fan.y, fan.y, fan.y);
            self.list.push(o);
        }
    }

    /// §5.5 step 1 in the two passes of `FUN_0042c888` (answers.md "Object fan push", "Object
    /// previous position"): every active object in ascending index gets its landing dust, joins
    /// the pass-2 list when it is moving and collectible, then, when not held, takes the motion
    /// of §5.4; pass 2 walks that list in descending index with the object–object step and the
    /// fan push.
    pub fn step_objects(&mut self, delta: f64, traps: &Traps, cx: &mut Ctx) {
        let mut moving = Vec::with_capacity(self.list.len());
        for i in 0..self.list.len() {
            if !self.list[i].active {
                continue;
            }
            self.landing_dust(i, cx);
            let o = &mut self.list[i];
            if o.collectible && o.state != ObjState::Rest {
                moving.push(i);
            }
            if !o.held {
                o.physics();
                o.motion(DT, delta);
            }
        }
        for &i in moving.iter().rev() {
            self.interact(i, delta, cx);
            self.fan_bounce(i, traps, cx);
        }
    }

    /// `FUN_0042d900`: weight × 20 / 130 puffs of [`DUST_DRAWS_PER_PUFF`] draws each. The puffs
    /// are cosmetic, so the frontend draws its own from [`Effect::Dust`].
    fn landing_dust(&mut self, i: usize, cx: &mut Ctx) {
        let o = &mut self.list[i];
        if !std::mem::take(&mut o.landed) {
            return;
        }
        let puffs = o.weight * 20 / 130;
        cx.emit(Event::Effect(Effect::Dust {
            x: o.screen_x + o.w / 2,
            y: o.y + o.h,
            size: puffs,
        }));
        for _ in 0..puffs * DUST_DRAWS_PER_PUFF {
            let _ = cx.rng.rand();
        }
    }

    /// `FUN_0042754d` mode 0 (answers.md "Object fan push"): an object on the blades of the
    /// first fan it touches bounces away from that fan's centre.
    fn fan_bounce(&mut self, i: usize, traps: &Traps, cx: &mut Ctx) {
        let Some(fan) = traps.blades_hit(&self.list[i].shape(cx), cx) else {
            return;
        };
        let fan_centre = fan.visible_x() + fan.w / 2;
        let o = &mut self.list[i];
        let s = if o.screen_x + o.w / 2 <= fan_centre {
            -1
        } else {
            1
        };
        o.vx = (o.vx.abs() + cx.rng.r(FAN_BOUNCE_RAND) + FAN_BOUNCE_BASE) * s;
        o.drop_vx = (o.drop_vx + FAN_DROP_BASE + cx.rng.r(FAN_DROP_RAND)) * s;
        o.vy = -(o.vy + cx.rng.r(FAN_BOUNCE_RAND) + FAN_BOUNCE_BASE);
        // The original draws a sign here and never uses it.
        let _ = cx.rng.r(FAN_UNUSED_SIGN_ROLL);
        play_fan_hit(cx);
    }

    /// `FUN_0042ec96`, the object–object step (§5.4, answers.md "Object side test", "Object
    /// previous position"). The stuck counter restarts when the step changes the state.
    fn interact(&mut self, i: usize, delta: f64, cx: &mut Ctx) {
        let before = self.list[i].state;
        match before {
            ObjState::Rest => {}
            ObjState::Sliding => self.slide(i, delta, cx),
            ObjState::Falling => self.fall(i, delta, cx),
        }
        if self.list[i].state != before {
            self.list[i].stuck = 0;
        }
    }

    /// A sliding object: on a touch the resolve, the impact sound, a push of k (mode 1) and the
    /// bounce; then, resting on a support, it falls off it or bounces off its side.
    fn slide(&mut self, i: usize, delta: f64, cx: &mut Ctx) {
        let support = self.support(i, cx);
        let list = self.touching(i, cx);
        if !list.is_empty() {
            let k = self.resolve(i, &list, delta, cx);
            let o = &self.list[i];
            let (vx, vy, weight) = (o.vx, o.vy, o.weight);
            impact_sound(weight, cx);
            if self.list[k].collectible {
                self.push(k, PushMode::Slide, vx, vy, weight, cx);
            }
            self.list[i].bounce();
        }
        // No support: no floor test and no fall [C] (answers.md "Sliding object without support").
        let Some((k, still)) = support else {
            return;
        };
        if !still {
            // Only k's rider list; any other support keeps a stale entry [C] (round 5).
            self.list[k].resting_on_top.retain(|&r| r != i);
            self.list[i].supported_by.clear();
            let o = &mut self.list[i];
            o.state = ObjState::Falling;
            o.drop_vx = (f64::from(o.vx) / 50.0) as i32;
            return;
        }
        // Mode 1 probes one side only, so the round-4 "both set: vx = 0" case cannot occur.
        let s = self.probe(i, k, 1, Probe::Heading, cx);
        if s.right || s.left {
            self.list[i].bounce();
        }
    }

    /// A falling object lands on the table, or meets object k: the resolve first, then probes
    /// at the resolved position from d = 1 to 1 + [`FALL_SIDE_RETRIES`] and the impact sound;
    /// below only lands on k, anything else is a side hit (mode 2).
    fn fall(&mut self, i: usize, delta: f64, cx: &mut Ctx) {
        let list = self.touching(i, cx);
        if list.is_empty() {
            self.land_on_table(i, delta, cx);
            return;
        }
        let k = self.resolve(i, &list, delta, cx);
        let mut s = Sides::NONE;
        for d in 1..=1 + FALL_SIDE_RETRIES as i32 {
            s = self.probe(i, k, d, Probe::Heading, cx);
            if s != Sides::NONE {
                break;
            }
        }
        if s == Sides::NONE {
            s.left = true;
        }
        impact_sound(self.list[i].weight, cx);
        if s.below && !s.right && !s.left {
            self.list[k].resting_on_top.push(i);
            let o = &mut self.list[i];
            o.supported_by.push(k);
            o.state = ObjState::Sliding;
            o.vy = 0;
            return;
        }
        let o = &self.list[i];
        let (vx, vy, weight) = (o.vx, o.vy, o.weight);
        if self.list[k].collectible {
            self.push(k, PushMode::Drop, vx, vy, weight, cx);
        }
        let o = &mut self.list[i];
        o.drop_vx = -o.drop_vx;
        o.vx = -o.vx;
        o.vy = 0;
    }

    /// Falling, no touch, at the floor (answers.md "Object previous position"): only vy = 0, so
    /// vx is kept and a second motion moves x again this tick; then the impact sound.
    fn land_on_table(&mut self, i: usize, delta: f64, cx: &mut Ctx) {
        if self.list[i].y < self.list[i].rest_y {
            return;
        }
        self.detach(i);
        let o = &mut self.list[i];
        o.y = o.rest_y;
        (o.drop_vx, o.state, o.vy) = (0, ObjState::Sliding, 0);
        o.motion(DT, delta);
        impact_sound(o.weight, cx);
        self.list[i].landed = true;
    }

    /// `FUN_0042ffc6` (answers.md "Object previous position"): back to the previous position;
    /// then, while v ≠ 0 and below [`STUCK_LIMIT`] calls, path A: the first object in `list`
    /// still touching makes it step 1 px against v and run the motion with dt = 0. Otherwise the
    /// push-up. Returns the object the caller acts on; `list` is the non-empty touching list.
    fn resolve(&mut self, i: usize, list: &[usize], delta: f64, cx: &Ctx) -> usize {
        let head = list[0];
        let o = &mut self.list[i];
        o.restore();
        if !o.held {
            o.stuck += 1;
        }
        let stuck = o.stuck >= STUCK_LIMIT;
        if stuck {
            o.stuck = 0;
        }
        // Paths B and C force v to (0, 15) or vy to 5 for the push-up and restore it after; as
        // called the push-up only moves up, so v is left alone here.
        if stuck || (o.vx, o.vy) == (0, 0) {
            return self.push_up(i, list, cx);
        }
        let Some(k) = list
            .iter()
            .copied()
            .find(|&k| self.touches_other(i, k, 0, cx))
        else {
            return head;
        };
        let o = &mut self.list[i];
        o.step_back();
        o.motion(0.0, delta);
        k
    }

    /// `FUN_004303e5` as the resolve calls it: 1 px up per retest of the whole `list` until
    /// nothing touches, returning the list's tail. A clear test on step [`PUSH_UP_STEPS`] still
    /// fails: back to the previous position, returning the list head. prev is not written.
    fn push_up(&mut self, i: usize, list: &[usize], cx: &Ctx) -> usize {
        for step in 1..=PUSH_UP_STEPS {
            self.list[i].y -= 1;
            let clear = !list.iter().any(|&k| self.touches_other(i, k, 0, cx));
            if clear && step < PUSH_UP_STEPS {
                return list[list.len() - 1];
            }
        }
        self.list[i].restore();
        list[0]
    }

    /// `FUN_0042fe10`'s support (answers.md "Sliding object without support"): the walk runs in
    /// descending index, so it is the lowest-index active support; still when any support
    /// touches `i` 1 px lower.
    fn support(&self, i: usize, cx: &Ctx) -> Option<(usize, bool)> {
        let mut supports = self.list[i]
            .supported_by
            .iter()
            .copied()
            .filter(|&k| self.list[k].active);
        let k = supports.clone().min()?;
        let still = supports.any(|s| self.touches_at(i, s, (0, 1), cx));
        Some((k, still))
    }

    /// `FUN_0042fb1c`: which of `i`'s probes against `k` still touch, with its layer x moved
    /// ±d or its y moved +d. Mode 0 ([`Probe::Both`]) tries both sides; mode 1
    /// ([`Probe::Heading`]) the side vx points to and, while vy ≠ 0, below.
    fn probe(&self, i: usize, k: usize, d: i32, mode: Probe, cx: &Ctx) -> Sides {
        let o = &self.list[i];
        let (right, left, below) = match mode {
            Probe::Both => (true, true, false),
            Probe::Heading => (o.vx > 0, o.vx < 0, o.vy != 0),
        };
        Sides {
            right: right && self.touches_at(i, k, (d, 0), cx),
            left: left && self.touches_at(i, k, (-d, 0), cx),
            below: below && self.touches_at(i, k, (0, d), cx),
        }
    }

    /// Object `i` with its layer x moved `dx` (screen x recomputed) and its y `dy` touches `k`.
    fn touches_at(&self, i: usize, k: usize, (dx, dy): (i32, i32), cx: &Ctx) -> bool {
        let o = &self.list[i];
        let mut a = o.shape(cx);
        if dx != 0 {
            a.x = screen_of((o.layer_x + dx).rem_euclid(W));
        }
        a.y += dy;
        outlines_overlap(&a, &self.list[k].shape(cx), self.wrap_width)
    }

    /// `FUN_0042fe10`'s touching list: the other objects whose outline touches `i`, head first.
    /// The original builds it by prepending, so the head is the last one in index order.
    fn touching(&self, i: usize, cx: &Ctx) -> Vec<usize> {
        (0..self.list.len())
            .rev()
            .filter(|&k| self.touches_other(i, k, 0, cx))
            .collect()
    }

    /// The first object passing `keep` that touches `i` with its screen x moved `dx`.
    fn hit_where(
        &self,
        i: usize,
        dx: i32,
        cx: &Ctx,
        keep: impl Fn(&Object) -> bool,
    ) -> Option<usize> {
        (0..self.list.len()).find(|&k| keep(&self.list[k]) && self.touches_other(i, k, dx, cx))
    }

    /// Another active object `k`, not one of `i`'s supports and not on the lift, touches `i`
    /// with its screen x moved `dx`. Riders count (answers.md "Sliding object without support").
    fn touches_other(&self, i: usize, k: usize, dx: i32, cx: &Ctx) -> bool {
        let (o, b) = (&self.list[i], &self.list[k]);
        let unrelated = k != i && !o.supported_by.contains(&k);
        if !unrelated || !b.active || b.on_lift {
            return false;
        }
        let mut a = o.shape(cx);
        a.x += dx;
        outlines_overlap(&a, &b.shape(cx), self.wrap_width)
    }

    /// Takes object `i` out of every stack list.
    fn detach(&mut self, i: usize) {
        let supports = std::mem::take(&mut self.list[i].supported_by);
        for s in supports {
            self.list[s].resting_on_top.retain(|&r| r != i);
        }
    }

    /// `FUN_0042e28a` (gaps §3): push object `p` with the pusher's vx, vy and weight `m`,
    /// riders first, depth first. Recursion is bounded by the stack depth, at most
    /// [`MAX_OBJECTS`].
    pub fn push(&mut self, p: usize, mode: PushMode, vx: i32, vy: i32, m: i32, cx: &mut Ctx) {
        self.push_at(p, mode, (vx, vy), m, cx, 0);
    }

    fn push_at(
        &mut self,
        p: usize,
        mode: PushMode,
        v: (i32, i32),
        m: i32,
        cx: &mut Ctx,
        depth: usize,
    ) {
        if depth > MAX_OBJECTS {
            return;
        }
        let mut total = self.list[p].weight;
        for q in self.list[p].resting_on_top.clone() {
            total += self.list[q].weight;
            if self.list[q].collectible {
                self.push_at(q, mode, v, m, cx, depth + 1);
            }
        }
        let (vx, vy) = match mode {
            PushMode::Slide => v,
            PushMode::Drop | PushMode::Carried => {
                ((f64::from(v.0) / 1.5) as i32, (f64::from(v.1) / 1.5) as i32)
            }
        };
        let s = (vx.abs() + vy.abs()) / 2;
        let energy = (f64::from(m) * f64::from(s) * f64::from(s)).abs() as i64;
        let q = energy / (2 * i64::from(total.max(1)));
        let speed = (q as f64).sqrt();
        if speed <= 50.0 {
            return;
        }
        let speed = speed as i32;
        let o = &mut self.list[p];
        o.state = ObjState::Sliding;
        o.vx = match vx.signum() {
            1 => speed,
            -1 => -speed,
            _ if mode == PushMode::Drop => {
                if cx.rng.r(10) < 5 {
                    -speed
                } else {
                    speed
                }
            }
            _ => {
                o.state = ObjState::Rest;
                0
            }
        };
    }

    /// §5.5 step 2 (§4.4 for each object): returns the grab candidates, collected only while
    /// not carrying.
    pub fn fly_contact(&mut self, fly: &mut Fly, cx: &mut Ctx) -> Vec<usize> {
        let fly_shape = fly_outline();
        let mut candidates = Vec::new();
        for i in 0..self.list.len() {
            let o = &self.list[i];
            let skip = !o.active || o.state == ObjState::Falling || o.on_lift;
            if skip || self.carried == Some(i) {
                continue;
            }
            self.touch(i, fly, &fly_shape, cx);
            if !self.carrying && self.candidate(i, fly) {
                candidates.push(i);
            }
        }
        candidates
    }

    /// `FUN_0043076b` steps 1–5.
    fn touch(&mut self, i: usize, fly: &mut Fly, fly_shape: &Outline, cx: &Ctx) {
        if !self.touches_fly(i, fly, fly_shape, cx) {
            return;
        }
        let dir = if fly.vy <= 0 { fly.vx.signum() } else { 0 };
        let o = &self.list[i];
        let rank = o.rank;
        let free = dir != 0
            && o.resting_on_top.is_empty()
            && self.hit_where(i, dir, cx, |b| b.rank > rank).is_none();
        let o = &mut self.list[i];
        if free {
            let v = fly.vx / (o.weight / 10).max(1);
            if v.abs() > 99 && o.collectible {
                o.state = ObjState::Sliding;
                o.vx = v;
            }
        }
        o.restore();
        (fly.x, fly.y) = (fly.prev_x, fly.prev_y);
        for _ in 0..SEPARATE_TRIES {
            if !self.touches_fly(i, fly, fly_shape, cx) {
                break;
            }
            fly.x -= fly.vx.signum();
            if !self.touches_fly(i, fly, fly_shape, cx) {
                break;
            }
            fly.y -= fly.vy.signum();
        }
        fly.vx = -(f64::from(fly.vx) / 1.4) as i32;
        fly.vy = -(f64::from(fly.vy) / 1.4) as i32;
    }

    fn touches_fly(&self, i: usize, fly: &Fly, fly_shape: &Outline, cx: &Ctx) -> bool {
        let b = Shape {
            x: fly.x,
            y: fly.y,
            size: fly.size,
            outline: fly_shape,
        };
        outlines_overlap(&self.list[i].shape(cx), &b, self.wrap_width)
    }

    /// §4.4 step 6.
    fn candidate(&self, i: usize, fly: &Fly) -> bool {
        let o = &self.list[i];
        let x = self.screen_left(o);
        (x - fly.size.w..=x + o.w).contains(&fly.x)
            && (o.y - 250..o.y + o.grip_y).contains(&fly.y)
            && o.resting_on_top.is_empty()
            && o.collectible
    }

    /// §5.5 steps 3–8: grab state, unlock, lock, attach or magnet, drop, carry.
    pub fn grab(
        &mut self,
        fly: &mut Fly,
        candidates: &[usize],
        traps: &Traps,
        delta: f64,
        cx: &mut Ctx,
    ) {
        let grab = fly.aspirating && !(self.carrying && !fly.carrying);
        if !grab {
            self.locked = None;
        }
        if grab && self.locked.is_none() && self.carried.is_none() {
            self.lock(fly, candidates);
        }
        if let Some(i) = self.locked.filter(|_| grab && !self.carrying) {
            self.attach(i, fly, cx);
        }
        if !grab && self.carrying {
            self.drop_carried(fly);
        }
        if grab && self.carrying {
            self.carry(fly, traps, delta, cx);
        }
    }

    /// §5.5 step 5: the last candidate within [`LOCK_DISTANCE`] wins.
    fn lock(&mut self, fly: &Fly, candidates: &[usize]) {
        let (fx, fy) = (fly.x + fly.size.w / 2, fly.y + fly.size.h);
        for &i in candidates {
            let o = &self.list[i];
            let dx = f64::from(fx - (self.screen_left(o) + o.grip_x));
            let dy = f64::from(fy - (o.y + o.grip_y));
            if dx.hypot(dy) < LOCK_DISTANCE {
                self.locked = Some(i);
            }
        }
    }

    /// `FUN_0042e704` (§5.5 step 6).
    fn attach(&mut self, i: usize, fly: &mut Fly, cx: &mut Ctx) {
        let o = &self.list[i];
        if !o.collectible {
            return;
        }
        let centre = fly.x + fly.size.w / 2;
        let grip = self.screen_left(o) + o.grip_x;
        let tol = (f64::from(o.w) / 2.0) as i32;
        if (centre - grip).abs() < tol && o.y < fly.y + fly.size.h {
            let saved_y = o.y;
            self.list[i].y = fly.y + fly.size.h - (o.grip_y + 14);
            if let Some(&k) = self.touching(i, cx).first() {
                self.list[i].y = saved_y;
                self.blocked_push(i, k, cx);
                return;
            }
            self.detach(i);
            let o = &mut self.list[i];
            o.held = true;
            o.state = ObjState::Rest;
            self.carrying = true;
            self.carried = Some(i);
            fly.carrying = true;
            return;
        }
        if (centre - grip).abs() <= MAGNET_RANGE {
            let d = (fly.x - grip).abs() + 10;
            fly.vx += if centre < grip { d } else { -d };
            fly.vy += (fly.y - (o.y + o.grip_y)).abs() / 2;
        }
    }

    /// A blocked attach (answers.md "Object side test"): the ±5 probes against k push the
    /// object away from the side still touching, or not at all.
    fn blocked_push(&mut self, i: usize, k: usize, cx: &Ctx) {
        let s = self.probe(i, k, BLOCKED_PROBE, Probe::Both, cx);
        let o = &mut self.list[i];
        let p = o.weight * 100 / 20;
        o.vx = match (s.right, s.left) {
            (true, _) => -p,
            (false, true) => p,
            (false, false) => return,
        };
        o.state = ObjState::Sliding;
    }

    /// §5.5 step 7.
    fn drop_carried(&mut self, fly: &mut Fly) {
        if let Some(i) = self.carried.take() {
            let o = &mut self.list[i];
            o.held = false;
            o.state = ObjState::Falling;
            o.drop_vx = fly.vx / 50;
            (o.vx, o.vy) = (0, 0);
        }
        fly.drop_pop = true;
        fly.y -= 20;
        fly.carrying = false;
        self.carrying = false;
    }

    /// `FUN_0042f3b1` (§5.5 step 8), the fan kick, then fly vy += weight. The previous position
    /// is the pre-placement one (W7 in answers.md "Object previous position").
    fn carry(&mut self, fly: &mut Fly, traps: &Traps, delta: f64, cx: &mut Ctx) {
        let Some(i) = self.carried else {
            return;
        };
        let wrap = self.wrap_width;
        let o = &mut self.list[i];
        (o.prev_x, o.prev_y) = (o.layer_x, o.y);
        let centre = (f64::from(fly.size.w) / 2.0 + f64::from(fly.x)) as i32;
        let off = o.grip_y + 14;
        let sx = centre - o.grip_x;
        o.set_screen_x(if sx <= 0 { wrap - sx.abs() } else { sx });
        o.y = fly.y + fly.size.h - off;
        (o.vx, o.vy) = (fly.vx, fly.vy);
        let weight = o.weight;
        let list = self.touching(i, cx);
        if list.is_empty() {
            self.carry_on_table(i, fly, off);
        } else {
            self.carry_blocked(i, &list, fly, off, delta, cx);
        }
        self.carried_fan_kick(i, fly, traps, cx);
        fly.vy += weight;
    }

    /// answers.md "Carried object collision": the resolve (held, so path A or the push-up), push
    /// k (mode 3), then the fly on the object.
    fn carry_blocked(
        &mut self,
        i: usize,
        list: &[usize],
        fly: &mut Fly,
        off: i32,
        delta: f64,
        cx: &mut Ctx,
    ) {
        let k = self.resolve(i, list, delta, cx);
        let (vx, vy, weight) = (self.list[i].vx, self.list[i].vy, self.list[i].weight);
        if self.list[k].collectible {
            self.push(k, PushMode::Carried, vx, vy, weight, cx);
        }
        let grip = self.screen_left(&self.list[i]) + self.list[i].grip_x;
        fly.x = (f64::from(grip) - f64::from(fly.size.w) / 2.0) as i32;
        fly.y = self.list[i].y - fly.size.h + off;
        fly.vx = -(f64::from(fly.vx) / 1.4) as i32;
        fly.vy = (-(f64::from(fly.vy) / 1.4) - f64::from(weight + 20)) as i32;
    }

    /// `FUN_0042754d` mode 1 (answers.md "Object fan push"): a carried object on the blades
    /// kicks the fly back, vx = −(vx ± 200) with the sign of vx (+ at 0), the same for vy.
    fn carried_fan_kick(&self, i: usize, fly: &mut Fly, traps: &Traps, cx: &mut Ctx) {
        if traps.blades_hit(&self.list[i].shape(cx), cx).is_none() {
            return;
        }
        let kick = |v: i32| {
            -(v + if v < 0 {
                -CARRIED_FAN_KICK
            } else {
                CARRIED_FAN_KICK
            })
        };
        (fly.vx, fly.vy) = (kick(fly.vx), kick(fly.vy));
        play_fan_hit(cx);
    }

    /// §5.5 step 8.4: the carried object touches the table. Its own y is not clamped
    /// (answers.md "Carried object collision").
    fn carry_on_table(&mut self, i: usize, fly: &mut Fly, off: i32) {
        let o = &mut self.list[i];
        if o.y < o.rest_y {
            return;
        }
        o.landed |= (o.y - o.prev_y).abs() > 5;
        fly.y = o.rest_y - fly.size.h + off;
        fly.vy = (-(f64::from(fly.vy) / 1.7) - f64::from(o.weight + 20)) as i32;
    }

    /// §5.5 step 9: what the lift reported. Delivery per ai-and-rules §7.5.
    fn lift_result(&mut self, result: LiftResult, m: &mut MatchState, cx: &mut Ctx) {
        match result {
            LiftResult::Empty => self.lift_loaded = false,
            LiftResult::Hooked(i) => {
                self.lift_loaded = true;
                let o = &mut self.list[i];
                o.state = ObjState::Rest;
                (o.vx, o.vy) = (0, 0);
            }
            LiftResult::Delivered(i) => {
                self.lift_loaded = false;
                cx.emit(Event::play(SoundSlot::MONEY));
                let o = &mut self.list[i];
                o.active = false;
                o.on_lift = false;
                if let Some(kind) = o.kind {
                    m.record_delivery(kind);
                }
                self.all_delivered = !self.list.iter().any(|o| o.active && o.collectible);
            }
        }
    }
}

/// How a stack push was caused (gaps §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushMode {
    /// 1: a sliding object hits another.
    Slide,
    /// 2: a falling object lands on another.
    Drop,
    /// 3: the fly carries an object into another.
    Carried,
}

/// The flags of `FUN_0042fb1c`: still touching with the layer x moved +d, −d, or y moved +d.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Sides {
    right: bool,
    left: bool,
    below: bool,
}

impl Sides {
    const NONE: Self = Self {
        right: false,
        left: false,
        below: false,
    };
}

/// The probe modes of `FUN_0042fb1c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Probe {
    /// Mode 0: right and left.
    Both,
    /// Mode 1: the side vx points to, and below while vy ≠ 0.
    Heading,
}

/// §5.5 step 6.4: the magnet reaches this far from the grip x, inclusive.
const MAGNET_RANGE: i32 = 100;

impl Object {
    /// An inactive-free object of `size` at layer x 0, resting on the table.
    fn blank(sprite: &'static str, size: Size) -> Self {
        let y = TABLE_Y - size.h;
        Self {
            kind: None,
            sprite,
            w: size.w,
            h: size.h,
            layer_x: 0,
            screen_x: 0,
            y,
            rest_y: y,
            prev_x: 0,
            prev_y: y,
            vx: 0,
            vy: 0,
            active: true,
            landed: false,
            held: false,
            state: ObjState::Rest,
            drop_vx: 0,
            stuck: 0,
            supported_by: Vec::new(),
            resting_on_top: Vec::new(),
            grip_x: 0,
            grip_y: 0,
            weight: 0,
            collectible: false,
            rank: 0,
            on_lift: false,
        }
    }

    /// A placed, collectible object of type `kind` (§5.2 step 4).
    pub fn of_type(kind: usize, t: &ObjectType, size: Size) -> Self {
        Self {
            kind: Some(kind),
            grip_x: t.grip_x,
            grip_y: t.grip_y,
            weight: t.weight,
            rank: t.rank,
            collectible: true,
            ..Self::blank(t.name, size)
        }
    }

    /// The screen-x setter `FUN_00417fab` (answers.md "Screen-x setter"): the screen x as given
    /// and the layer x trunc(x / 1.25), until the next motion step sets trunc(layer × 1.25). It
    /// leaves the previous position alone.
    pub fn set_screen_x(&mut self, x: i32) {
        self.screen_x = x;
        self.layer_x = (f64::from(x) / OBJECT_PARALLAX) as i32;
    }

    fn screen_from_layer(&self) -> i32 {
        screen_of(self.layer_x)
    }

    /// Back to the previous position, screen x recomputed (`FUN_00417edf`).
    fn restore(&mut self) {
        (self.layer_x, self.y) = (self.prev_x, self.prev_y);
        self.screen_x = self.screen_from_layer();
    }

    /// One px against v (path A of the resolve). The dt = 0 motion that follows saves it as the
    /// previous position (W3, W4).
    fn step_back(&mut self) {
        self.layer_x = (self.layer_x - self.vx.signum()).rem_euclid(W);
        self.y -= self.vy.signum();
        self.screen_x = self.screen_from_layer();
    }

    /// The slide bounce (gaps §3, answers.md "Object side test"), w = trunc(weight / 3): vx < 0
    /// becomes −10w − vx, stopping below 1; vx > 0 becomes 10w − vx, stopping at ≥ 0.
    fn bounce(&mut self) {
        let w10 = 10 * (f64::from(self.weight) / 3.0) as i32;
        self.vx = match self.vx {
            v if v < 0 && -w10 - v >= 1 => -w10 - v,
            v if v > 0 && w10 - v < 0 => w10 - v,
            _ => 0,
        };
    }

    fn shape<'a>(&self, cx: &Ctx<'a>) -> Shape<'a> {
        Shape {
            x: self.screen_x,
            y: self.y,
            size: Size {
                w: self.w,
                h: self.h,
            },
            outline: cx.data.outline(self.sprite),
        }
    }

    /// §5.4 step 1 (`FUN_0042eb13`).
    fn physics(&mut self) {
        match self.state {
            ObjState::Rest => self.vy = 0,
            ObjState::Sliding => {
                self.vy = 0;
                let friction = (f64::from(self.weight) / 3.0) as i32;
                self.vx = toward_zero(self.vx, friction);
                if self.vx == 0 {
                    self.state = ObjState::Rest;
                }
            }
            ObjState::Falling => {
                self.vx += self.drop_vx;
                self.vy += GRAVITY_PER_TICK;
            }
        }
    }

    /// `FUN_0042ea74` (answers.md "Object previous position", W1 and W2): v = 0 when not
    /// collectible; the previous position is saved before the move in every state, and again
    /// after it in state 0; vx is clamped to ±[`MAX_VX`].
    fn motion(&mut self, dt: f64, delta: f64) {
        if !self.collectible {
            (self.vx, self.vy) = (0, 0);
        }
        (self.prev_x, self.prev_y) = (self.layer_x, self.y);
        self.layer_x += (f64::from(self.vx) * dt - delta) as i32;
        self.layer_x = self.layer_x.rem_euclid(W);
        self.y += (f64::from(self.vy) * dt) as i32;
        self.screen_x = self.screen_from_layer();
        if self.state == ObjState::Rest {
            (self.prev_x, self.prev_y) = (self.layer_x, self.y);
        }
        self.vx = self.vx.clamp(-MAX_VX, MAX_VX);
    }
}

/// `FUN_0042dc21` on every slide touch, floor landing and fall touch: slot 6 at
/// trunc((1 − weight / 100) × 20000) + R(6000) + 15000 Hz, volume −150, centred.
fn impact_sound(weight: i32, cx: &mut Ctx) {
    let base = ((1.0 - f64::from(weight) / 100.0) * IMPACT_HZ_SPAN) as i32;
    let freq = base + cx.rng.r(IMPACT_HZ_RAND) + IMPACT_HZ_BASE;
    let params = PlayParams {
        volume_cb: IMPACT_VOLUME_CB,
        freq_hz: u32::try_from(freq).ok(),
        pan: Pan::Centre,
    };
    cx.sound(SoundSlot::BOOM, SoundOp::Play(params));
}

/// The object screen x of a layer x: trunc(layer × 1.25).
fn screen_of(layer_x: i32) -> i32 {
    (f64::from(layer_x) * OBJECT_PARALLAX) as i32
}

/// `v` moved `d` toward 0 without overshooting.
fn toward_zero(v: i32, d: i32) -> i32 {
    if v > 0 {
        (v - d).max(0)
    } else {
        (v + d).min(0)
    }
}

/// §5.2 step 3 without the economy (`FUN_0042ac27`): up to [`MAX_PLACED`] objects, each a type
/// draw then its own [`PLACEMENT_X_TRIES`] x draws in 0..5000, with landing sound 6 per placed
/// object. `span_free` gets that screen-unit x (§5.2 step 3.3), not a layer x (answers.md O22).
pub fn place_on_table(
    types: &[ObjectType],
    eligible: &[bool],
    span_free: impl Fn(i32, i32) -> bool,
    cx: &mut Ctx,
) -> Vec<Object> {
    let mut stock: Vec<i32> = types.iter().map(|t| t.stock).collect();
    let mut placed: Vec<Object> = Vec::new();
    let mut failures = 0;
    while placed.len() < MAX_PLACED && failures < PLACEMENT_FAILURES {
        let Some(kind) = pick_type(eligible, &stock, cx) else {
            failures += 1;
            continue;
        };
        let size = cx.data.size(types[kind].name);
        let mut tries = 0;
        let Some(x) = draw_x(&mut tries, |x| fits(&placed, size, &span_free, x), cx) else {
            failures += 1;
            continue;
        };
        placed.push(put(kind, &types[kind], size, x, cx));
        stock[kind] -= 1;
    }
    leave_out_last_on_failure_cap(placed, failures)
}

/// Extreme placement (`FUN_0042bdf8`, answers.md "Extreme placement"): the types in order, 0 ×3,
/// 1 ×5, 2 ×7, with no type draw, at y = 580 − h and with the §5.2 x checks. Successes share one
/// x budget; only a failure resets it.
pub fn place_extreme(span_free: impl Fn(i32, i32) -> bool, cx: &mut Ctx) -> Vec<Object> {
    let mut stock = EXTREME_TYPES.map(|t| t.stock);
    let mut placed: Vec<Object> = Vec::new();
    let (mut failures, mut tries) = (0, 0);
    while placed.len() < MAX_PLACED && failures < PLACEMENT_FAILURES {
        let Some(kind) = (0..stock.len()).find(|&k| stock[k] >= 1) else {
            break;
        };
        let t = &EXTREME_TYPES[kind];
        let size = cx.data.size(t.name);
        let Some(x) = draw_x(&mut tries, |x| fits(&placed, size, &span_free, x), cx) else {
            (failures, tries) = (failures + 1, 0);
            continue;
        };
        placed.push(put(kind, t, size, x, cx));
        stock[kind] -= 1;
    }
    leave_out_last_on_failure_cap(placed, failures)
}

/// answers.md "Extreme placement" step 4: the 2000th failure decrements the placed index before
/// the loop exits, so the count leaves out the last placed object. With none placed the
/// original count would be −1; the port returns none.
fn leave_out_last_on_failure_cap(mut placed: Vec<Object>, failures: u32) -> Vec<Object> {
    if failures >= PLACEMENT_FAILURES {
        placed.pop();
    }
    placed
}

/// The placement wrap: trunc(W × 1.25).
fn placement_wrap() -> i32 {
    (f64::from(W) * OBJECT_PARALLAX) as i32
}

/// §5.2 step 3.3: clear of every placed object (answers.md "Placement overlap test": refused when
/// xj − wNew ≤ x ≤ xj + wj), inside the wrap and on a free trap span.
fn fits(placed: &[Object], size: Size, span_free: impl Fn(i32, i32) -> bool, x: i32) -> bool {
    let clear = placed
        .iter()
        .all(|p| x + size.w < p.screen_x || p.screen_x + p.w < x);
    clear && x + size.w < placement_wrap() && span_free(x, size.w)
}

/// R(wrap) until `fits`, while the counter `tries` is below [`PLACEMENT_X_TRIES`].
fn draw_x(tries: &mut u32, fits: impl Fn(i32) -> bool, cx: &mut Ctx) -> Option<i32> {
    while *tries < PLACEMENT_X_TRIES {
        *tries += 1;
        let x = cx.rng.r(placement_wrap());
        if fits(x) {
            return Some(x);
        }
    }
    None
}

/// A placed object at screen x `x`, with landing sound 6.
fn put(kind: usize, t: &ObjectType, size: Size, x: i32, cx: &mut Ctx) -> Object {
    let mut o = Object::of_type(kind, t, size);
    o.set_screen_x(x);
    cx.emit(Event::play(SoundSlot::BOOM));
    o
}

/// §5.2 step 3.1: R(types) until an eligible type with stock comes up.
///
/// UB guard: after [`PLACEMENT_TYPE_TRIES`] draws the original would carry on with the last,
/// unsuitable type; the port counts a placement failure instead. No meal reaches it: every
/// meal has more than 15 copies.
fn pick_type(eligible: &[bool], stock: &[i32], cx: &mut Ctx) -> Option<usize> {
    let n = i32::try_from(eligible.len()).unwrap_or(i32::MAX);
    (0..PLACEMENT_TYPE_TRIES).find_map(|_| {
        let t = usize::try_from(cx.rng.r(n)).unwrap_or(0);
        (eligible[t] && stock[t] >= 1).then_some(t)
    })
}

impl World {
    /// Step 12 (`FUN_0042c888`): per-object motion and interaction, fly contact, grab, attach,
    /// magnet, drop, carry, the lift, then delivery and the period-over flag.
    pub fn objects_step(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let delta = self.camera.delta;
        self.objects.step_objects(delta, &self.traps, cx);
        let candidates = self.objects.fly_contact(&mut self.fly, cx);
        self.objects
            .grab(&mut self.fly, &candidates, &self.traps, delta, cx);
        let result = self
            .lift
            .update(&mut self.objects, self.fly.carrying, delta, cx);
        self.objects.lift_result(result, m, cx);
        if self.objects.all_delivered || self.clock.expired(cx.now) {
            self.period_over = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Timer;
    use crate::data::GameData;
    use crate::event::{Event, SoundOp, SoundSlot};
    use crate::rng::MsvcRand;
    use crate::world::fly::Fly;
    use crate::world::lift::LiftState;
    use crate::world::traps::{FAN_STRENGTH, Fan};

    /// Enough ticks for a drop from y 200 to settle.
    const SETTLE_TICKS: usize = 200;

    struct Bench {
        rng: MsvcRand,
        data: GameData,
        events: Vec<Event>,
    }

    impl Bench {
        fn new(seed: u32) -> Self {
            Self {
                rng: MsvcRand::new(seed),
                data: GameData::synthetic(),
                events: Vec::new(),
            }
        }

        fn with<T>(&mut self, f: impl FnOnce(&mut Ctx) -> T) -> T {
            let mut cx = Ctx::new(0, &mut self.rng, &self.data, &mut self.events);
            f(&mut cx)
        }

        fn object(&self, kind: usize, screen_x: i32) -> Object {
            let t = &NORMAL_TYPES[kind];
            let mut o = Object::of_type(kind, t, self.data.size(t.name));
            o.set_screen_x(screen_x);
            o
        }
    }

    fn eligible(meal: Meal) -> Vec<bool> {
        NORMAL_TYPES
            .iter()
            .map(|t| t.meal & meal as u8 != 0)
            .collect()
    }

    /// Acceptance W1 7. Seed 1 by hand: types R(22) = 41 % 22 = 19 (muffin, lunch: no), then
    /// 18467 % 22 = 9 (zippo); x = 6334 % 5000 = 1334. Next 26500 % 22 = 12 (pomme, no),
    /// 19169 % 22 = 7 (saucisson1); x = 15724 % 5000 = 724, clear of zippo's [1334, 1510].
    #[test]
    fn placement_is_bounded_disjoint_within_stock_and_deterministic() {
        let dinner = eligible(Meal::Dinner);
        let run = || {
            let mut b = Bench::new(1);
            let placed = b.with(|cx| place_on_table(&NORMAL_TYPES, &dinner, |_, _| true, cx));
            (placed, b.events, b.rng.state())
        };
        let (placed, events, _) = run();
        assert_eq!(run().0, placed);
        let first: Vec<_> = placed
            .iter()
            .take(2)
            .map(|o| (o.kind, o.screen_x))
            .collect();
        assert_eq!(first, [(Some(9), 1334), (Some(7), 724)]);
        assert!(!placed.is_empty() && placed.len() <= MAX_PLACED);
        let booms = events
            .iter()
            .filter(|e| **e == Event::play(SoundSlot::BOOM));
        assert_eq!(booms.count(), placed.len());
        for (i, o) in placed.iter().enumerate() {
            let kind = o.kind.unwrap_or(usize::MAX);
            assert!(dinner[kind] && o.collectible);
            assert_eq!(o.y, TABLE_Y - o.h);
            assert!(o.screen_x + o.w < 5000);
            for p in &placed[..i] {
                let clash = p.screen_x - o.w <= o.screen_x && o.screen_x <= p.screen_x + p.w;
                assert!(!clash, "{i} overlaps an earlier object");
            }
            let copies = placed.iter().filter(|p| p.kind == o.kind).count();
            assert!(copies as i32 <= NORMAL_TYPES[kind].stock);
        }
    }

    #[test]
    fn placement_avoids_refused_trap_spans_and_stops_on_stock() {
        let mut b = Bench::new(3);
        let lunch = eligible(Meal::Lunch);
        let free = |x: i32, w: i32| x >= 2000 || x + w < 1000;
        let placed = b.with(|cx| place_on_table(&NORMAL_TYPES, &lunch, free, cx));
        assert!(placed.iter().all(|o| free(o.screen_x, o.w)));
        // Extreme: 3 + 5 + 7 = 15 copies in all, so stock is the limit as well.
        let mut b = Bench::new(5);
        let placed = b.with(|cx| place_extreme(|_, _| true, cx));
        for (k, t) in EXTREME_TYPES.iter().enumerate() {
            assert!(placed.iter().filter(|o| o.kind == Some(k)).count() as i32 <= t.stock);
        }
    }

    /// The impact sounds (`FUN_0042dc21`) on slot 6: (frequency, volume).
    fn impacts(events: &[Event]) -> Vec<(Option<u32>, i32)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Sound {
                    slot,
                    op: SoundOp::Play(p),
                } if *slot == SoundSlot::BOOM => Some((p.freq_hz, p.volume_cb)),
                _ => None,
            })
            .collect()
    }

    /// Acceptance W1 8. can1 (weight 75) falls with drop vx 12, so vx grows by 12 per tick and
    /// the landing keeps it (answers.md "Object previous position"). The landing plays the impact
    /// sound: trunc(0.25 × 20000) + R(6000) + 15000, with seed 1's first draw 41.
    #[test]
    fn a_dropped_object_falls_at_50_per_tick_to_the_table() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(0, 300);
        (o.y, o.state, o.drop_vx) = (100, ObjState::Falling, 12);
        objs.list.push(o);
        let mut vys = Vec::new();
        for _ in 0..60 {
            b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
            let o = &objs.list[0];
            assert!(o.vx.abs() <= MAX_VX);
            if o.state != ObjState::Falling {
                break;
            }
            vys.push(o.vy);
        }
        assert_eq!(vys[..4], [50, 100, 150, 200]);
        let o = &objs.list[0];
        let ticks = vys.len() as i32 + 1;
        assert_eq!((o.y, o.vx, o.vy), (TABLE_Y - o.h, 12 * ticks, 0));
        assert!(o.landed);
        assert_eq!(impacts(&b.events), [(Some(20_041), -150)]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        assert!(!objs.list[0].landed);
    }

    /// answers.md "Object previous position": on the floor landing only vy = 0; vx is kept and
    /// a second motion moves x again. chips1 at layer 240, y 479, vx 130, vy 600: the motion saves
    /// (240, 479) and moves to layer 242, y 489 (vy 650 / 65); the landing sets y 480, then the
    /// second motion saves (242, 480) and moves to layer 244. Impact: 17000 + 41 + 15000.
    #[test]
    fn a_floor_landing_keeps_vx_and_moves_x_a_second_time() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 300), 479);
        (o.state, o.vx, o.vy) = (ObjState::Falling, 130, 600);
        objs.list.push(o);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!((o.layer_x, o.y, o.vx, o.vy), (244, 480, 130, 0));
        assert_eq!((o.prev_x, o.prev_y), (242, 480));
        assert_eq!((o.state, o.drop_vx, o.landed), (ObjState::Sliding, 0, true));
        assert_eq!(impacts(&b.events), [(Some(32_041), -150)]);
    }

    /// W1: the motion saves the previous position before it moves, in every state. A falling
    /// chips1 at y 100 with vy 600: y 110 (650 / 65) with prev 100, then y 120 with prev 110.
    #[test]
    fn the_motion_saves_the_previous_position_before_every_move() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 300), 100);
        (o.state, o.vy, o.prev_y) = (ObjState::Falling, 600, 0);
        objs.list.push(o);
        for (prev_y, y) in [(100, 110), (110, 120)] {
            b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
            let o = &objs.list[0];
            assert_eq!((o.prev_x, o.prev_y, o.y), (240, prev_y, y));
        }
    }

    /// answers.md "Object previous position": the landing dust spawns trunc(15 × 20 / 130) = 2
    /// puffs of 7 draws each, and plays no sound (the impact sound belongs to the landing).
    #[test]
    fn the_landing_dust_draws_7_per_puff_and_plays_no_sound() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(3, 300);
        o.landed = true;
        objs.list.push(o);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let mut probe = MsvcRand::new(1);
        skip(&mut probe, 14);
        assert_eq!(b.rng, probe);
        assert!(b.events.iter().all(|e| !matches!(e, Event::Sound { .. })));
        assert!(matches!(
            b.events[..],
            [Event::Effect(Effect::Dust { size: 2, .. })]
        ));
    }

    #[test]
    fn a_fast_drop_is_capped_at_500_px_s_sideways() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(0, 300);
        (o.y, o.state, o.drop_vx) = (-4000, ObjState::Falling, 40);
        objs.list.push(o);
        for _ in 0..30 {
            b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        }
        assert_eq!(objs.list[0].vx, MAX_VX);
    }

    /// A pusher of weight 60 at vx 300 meets an object of weight 30 carrying a 20-weight rider.
    fn stack(b: &Bench) -> Objects {
        let mut objs = Objects::empty();
        let mut base = b.object(17, 1000);
        let mut rider = b.object(19, 1000);
        (base.weight, rider.weight) = (30, 20);
        base.resting_on_top.push(1);
        rider.supported_by.push(0);
        objs.list = vec![base, rider];
        objs
    }

    /// Acceptance W1 11 (gaps §3). s = 300 / 2 = 150, E = 60 × 150² = 1 350 000.
    /// Rider: q = E / 40 = 33 750, √ = 183.7. Base: M = 50, q = 13 500, √ = 116.2.
    #[test]
    fn the_stack_push_shares_the_energy_by_summed_weight() {
        let mut b = Bench::new(1);
        let mut objs = stack(&b);
        b.with(|cx| objs.push(0, PushMode::Slide, 300, 0, 60, cx));
        assert_eq!(
            (objs.list[1].vx, objs.list[1].state),
            (183, ObjState::Sliding)
        );
        assert_eq!(
            (objs.list[0].vx, objs.list[0].state),
            (116, ObjState::Sliding)
        );
        let mut objs = stack(&b);
        b.with(|cx| objs.push(0, PushMode::Slide, -300, 0, 60, cx));
        assert_eq!((objs.list[1].vx, objs.list[0].vx), (-183, -116));
    }

    #[test]
    fn a_push_of_50_or_less_changes_nothing() {
        let mut b = Bench::new(1);
        let mut objs = stack(&b);
        // s = 60, E = 3 × 3600 = 10 800; base q = 108, √ = 10.4.
        b.with(|cx| objs.push(0, PushMode::Slide, 120, 0, 3, cx));
        assert_eq!(objs.list, stack(&b).list);
    }

    /// Mode 2, vx 1 → trunc(1 / 1.5) = 0: s = (0 + 400) / 2 = 200, E = 2 400 000. Rider √60 000 =
    /// 244, base √24 000 = 154. Seed 1 draws 41 % 10 = 1 (< 5, left) for the rider first, then
    /// 18467 % 10 = 7 (right) for the base.
    #[test]
    fn a_vertical_drop_draws_one_side_roll_per_pushed_object_depth_first() {
        let mut b = Bench::new(1);
        let mut objs = stack(&b);
        b.with(|cx| objs.push(0, PushMode::Drop, 1, 600, 60, cx));
        assert_eq!((objs.list[1].vx, objs.list[0].vx), (-244, 154));
        // Mode 3 with vx 0 stops the objects instead of rolling.
        let mut objs = stack(&b);
        let before = b.rng.state();
        b.with(|cx| objs.push(0, PushMode::Carried, 0, 600, 60, cx));
        assert_eq!((objs.list[0].vx, objs.list[0].state), (0, ObjState::Rest));
        assert_eq!(b.rng.state(), before);
    }

    fn fly_at(b: &mut Bench, x: i32, y: i32) -> Fly {
        let mut fly = b.with(|cx| Fly::new(0, 0, cx.data.size("Face"), cx));
        (fly.x, fly.y, fly.prev_x, fly.prev_y) = (x, y, x, y);
        fly
    }

    /// FW §4.4: a fly moving right into a light object pushes it (v = 500 / max(1, 15 / 10))
    /// and bounces back at −trunc(500 / 1.4).
    #[test]
    fn the_fly_pushes_a_light_object_and_bounces_off() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let chips = b.object(3, 200);
        let (oy, oh) = (chips.y, chips.h);
        objs.list.push(chips);
        // Fly right edge (x + 90 px solid) meets the chips' left edge at 200.
        let mut fly = fly_at(&mut b, 200 - 90, oy + oh / 2 - 50);
        (fly.vx, fly.vy) = (500, 0);
        let candidates = b.with(|cx| objs.fly_contact(&mut fly, cx));
        assert_eq!(
            (objs.list[0].state, objs.list[0].vx),
            (ObjState::Sliding, 500)
        );
        assert_eq!((fly.vx, fly.vy), (-357, 0));
        assert!(candidates.is_empty() || candidates == [0]);
    }

    #[test]
    fn a_heavy_object_does_not_move_but_still_stops_the_fly() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let toaster = b.object(14, 200);
        let oy = toaster.y;
        objs.list.push(toaster);
        let mut fly = fly_at(&mut b, 200 - 90, oy + 10);
        (fly.vx, fly.vy) = (500, -100);
        b.with(|cx| objs.fly_contact(&mut fly, cx));
        // 500 / (130 / 10) = 38, not above 99.
        assert_eq!(objs.list[0].state, ObjState::Rest);
        assert_eq!((fly.vx, fly.vy), (-357, 71));
    }

    /// §5.5 steps 5–7: lock, attach over the grip, carry, then drop.
    #[test]
    fn the_fly_grabs_carries_and_drops_an_object() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let can = b.object(1, 300);
        let (grip_abs, top) = (300 + can.grip_x, can.y);
        objs.list.push(can);
        // Bottom-centre on the grip x, fly bottom just under the object top.
        let mut fly = fly_at(&mut b, grip_abs - 60, top - 94 + 5);
        fly.aspirating = true;
        b.with(|cx| objs.grab(&mut fly, &[0], &Traps::default(), 0.0, cx));
        assert!(fly.carrying && objs.carrying);
        assert_eq!(objs.carried, Some(0));
        assert!(objs.list[0].held);
        // Carrying puts the object under the fly: y = flyY + flyH − (grip y + 14).
        b.with(|cx| objs.grab(&mut fly, &[], &Traps::default(), 0.0, cx));
        assert_eq!(objs.list[0].y, fly.y + 94 - (20 + 14));
        fly.aspirating = false;
        fly.vx = 400;
        b.with(|cx| objs.grab(&mut fly, &[], &Traps::default(), 0.0, cx));
        let o = &objs.list[0];
        assert!(!o.held && o.state == ObjState::Falling && o.drop_vx == 8);
        assert!(fly.drop_pop && !fly.carrying && objs.carried.is_none());
    }

    #[test]
    fn the_magnet_pulls_a_near_miss_toward_the_grip() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let can = b.object(2, 300);
        let (grip_abs, top) = (300 + can.grip_x, can.y);
        objs.list.push(can);
        // Centre 80 px left of the grip: outside tol (64) but within 100.
        let mut fly = fly_at(&mut b, grip_abs - 80 - 60, top - 200);
        fly.aspirating = true;
        b.with(|cx| objs.grab(&mut fly, &[0], &Traps::default(), 0.0, cx));
        assert!(!fly.carrying);
        // d = |flyX − grip| + 10 = 150; vy += |flyY − (objY + 20)| / 2 = 110.
        assert_eq!((fly.vx, fly.vy), (150, 110));
    }

    fn two_player_world(b: &mut Bench) -> (MatchState, World) {
        use crate::match_state::{MatchConfig, Players};
        b.with(|cx| {
            let config = MatchConfig {
                players: Players::Two,
                ..MatchConfig::default()
            };
            let mut m = MatchState::new(config, false, cx);
            m.economy.start_period(&[true; 22]);
            let w = World::bare(&m, cx);
            (m, w)
        })
    }

    /// Acceptance W1 9 (AR §7.5): the hoist that takes the lift off the top delivers the
    /// object: sound 11, saved and delivered + 1, price total + trunc(price), and the period
    /// ends with nothing left on the table.
    #[test]
    fn a_hoisted_object_is_delivered_and_an_empty_table_ends_the_period() {
        let mut b = Bench::new(1);
        let (mut m, mut w) = two_player_world(&mut b);
        let mut o = b.object(1, 1000);
        o.on_lift = true;
        w.objects.list.push(o);
        (w.fly.x, w.fly.y) = (100, 100);
        let lift = &mut w.lift;
        (lift.state, lift.hooked, lift.y) = (LiftState::Hoisting, Some(0), -lift.h + 2);
        let price = m.economy.food_prices[1];
        b.with(|cx| w.objects_step(&mut m, cx));
        assert!(b.events.contains(&Event::play(SoundSlot::MONEY)));
        assert_eq!(m.economy.saved[1], Some(1));
        assert_eq!((m.stats.delivered, m.stats.price_total), (1, price as i32));
        assert!(!w.objects.list[0].active && !w.objects.lift_loaded);
        assert!(w.objects.all_delivered && w.period_over);
    }

    #[test]
    fn the_period_goes_on_while_a_collectible_object_is_left() {
        let mut b = Bench::new(1);
        let (mut m, mut w) = two_player_world(&mut b);
        let (mut o, other) = (b.object(1, 1000), b.object(2, 3000));
        o.on_lift = true;
        w.objects.list.extend([o, other]);
        (w.fly.x, w.fly.y) = (100, 100);
        let lift = &mut w.lift;
        (lift.state, lift.hooked, lift.y) = (LiftState::Hoisting, Some(0), -lift.h + 2);
        b.with(|cx| w.objects_step(&mut m, cx));
        assert_eq!(m.stats.delivered, 1);
        assert!(!w.objects.all_delivered && !w.period_over);
    }

    /// The object at `y`, with its previous position there too.
    fn placed(mut o: Object, y: i32) -> Object {
        (o.y, o.prev_y) = (y, y);
        o
    }

    /// A left fan at screen 500, y 270: synthetic blades 100 × 100 at half-x 250..299, rows
    /// 270..369; its centre is 500 + 222 / 2 = 611.
    fn one_fan() -> Traps {
        Traps {
            fans: vec![Fan {
                facing_right: false,
                x: 400,
                screen_x: 500,
                y: 270,
                w: 222,
                strength: FAN_STRENGTH,
                rehit: Timer::UNARMED,
            }],
            ..Traps::default()
        }
    }

    fn fan_hits_hz(events: &[Event]) -> Vec<u32> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Sound {
                    slot,
                    op: SoundOp::Play(p),
                } if *slot == SoundSlot::FAN_HIT => p.freq_hz,
                _ => None,
            })
            .collect()
    }

    /// answers.md "Object fan push": pass 2 runs in descending index, so object 1 draws first.
    /// Both fall (vy 50 after the motion) left of the fan's centre (420 + 75 ≤ 611, s = −1),
    /// one over the other so they do not touch. Seed 1: object 1 takes R(60) 41, R(70) 57,
    /// R(60) 34, R(100) 0, R(6000) 1169; object 0 takes 15724 % 60 = 4, 11478 % 70 = 68,
    /// 29358 % 60 = 18, 26962 % 100, 24464 % 6000 = 464.
    #[test]
    fn falling_objects_bounce_off_the_blades_in_descending_order() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        for y in [300, 200] {
            let mut o = placed(b.object(3, 420), y);
            o.state = ObjState::Falling;
            objs.list.push(o);
        }
        b.with(|cx| objs.step_objects(0.0, &one_fan(), cx));
        let v = |o: &Object| (o.vx, o.drop_vx, o.vy, o.state);
        assert_eq!(v(&objs.list[1]), (-131, -107, -174, ObjState::Falling));
        assert_eq!(v(&objs.list[0]), (-94, -118, -158, ObjState::Falling));
        assert_eq!(fan_hits_hz(&b.events), [21_169, 20_464]);
        assert_eq!(b.rng.rand(), 5705);
    }

    /// A resting object on the blades is never pushed and draws nothing.
    #[test]
    fn a_resting_object_is_not_pushed_by_a_fan() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        objs.list.push(placed(b.object(3, 420), 300));
        b.with(|cx| objs.step_objects(0.0, &one_fan(), cx));
        assert_eq!((objs.list[0].vx, objs.list[0].vy), (0, 0));
        assert_eq!(b.rng.rand(), 41);
    }

    /// The carried can2 (222 wide, grip 111) sits at (420, 300) on the blades: fly vx 100 →
    /// −(100 + 200), vy −50 → −(−50 − 200) = 250, then + weight 35. Fly::new takes 41; the kick
    /// draws 18467 % 6000 = 467 → 20467 Hz.
    #[test]
    fn a_carried_object_on_the_blades_kicks_the_fly_200_back() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut can = b.object(1, 420);
        can.held = true;
        objs.list.push(can);
        (objs.carrying, objs.carried) = (true, Some(0));
        let mut fly = fly_at(&mut b, 471, 240);
        (fly.aspirating, fly.carrying, fly.vx, fly.vy) = (true, true, 100, -50);
        b.with(|cx| objs.grab(&mut fly, &[], &one_fan(), 0.0, cx));
        assert_eq!((objs.list[0].screen_x, objs.list[0].y), (420, 300));
        assert_eq!((fly.vx, fly.vy), (-300, 285));
        assert_eq!(fan_hits_hz(&b.events), [20_467]);
        assert_eq!(b.rng.rand(), 6334);
    }

    /// Blocked attach (answers.md "Object side test"): chips1 at (300, 480) with the fly over
    /// its grip at y 400 is lifted to y 460, where chips2 at (200, k_y) touches it (row 460:
    /// chips1's top row 150..224 holds chips2's right side 174). Back at y 480 it probes chips2
    /// at layer x ± 5.
    fn blocked_attach(k_y: i32) -> Object {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        objs.list.push(b.object(3, 300));
        objs.list.push(placed(b.object(4, 200), k_y));
        let mut fly = fly_at(&mut b, 315, 400);
        fly.aspirating = true;
        b.with(|cx| objs.grab(&mut fly, &[0], &Traps::default(), 0.0, cx));
        assert!(!fly.carrying && !objs.list[0].held);
        objs.list.swap_remove(0)
    }

    /// k at y 430 shares rows with the object at y 480; layer 245 (screen 306, half-x 153..227)
    /// still holds 174, so right is set: vx = −15 × 100 / 20, although k is on the left.
    #[test]
    fn a_blocked_attach_with_the_right_probe_set_pushes_left() {
        let o = blocked_attach(430);
        assert_eq!((o.y, o.vx, o.state), (480, -75, ObjState::Sliding));
    }

    /// k at y 380 (rows 380..479) shares no row with the object back at y 480: no push.
    #[test]
    fn a_blocked_attach_with_no_probe_set_pushes_nothing() {
        let o = blocked_attach(380);
        assert_eq!((o.y, o.vx, o.state), (480, 0, ObjState::Rest));
    }

    /// chips1 (weight 15, w = 5) slides right at 300: friction leaves 295, the motion saves layer
    /// 240 (screen 300) and moves 4 layer px to screen 305, where its top row (half-x 152..226)
    /// meets chips2's (at layer 361, screen 451: 225..299). The resolve goes back to 240, which
    /// touches nothing, so there is no step. Impact 17000 + 41 + 15000; it pushes chips2 (s =
    /// 147, E = 15 × 147², q = E / 30 = 10804, √ = 103) and bounces: 10 × 5 − 295 = −245.
    #[test]
    fn a_sliding_object_that_hits_another_returns_to_its_previous_position() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(3, 300);
        (o.state, o.vx) = (ObjState::Sliding, 300);
        objs.list.push(o);
        objs.list.push(b.object(4, 452));
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!((o.layer_x, o.screen_x, o.vx), (240, 300, -245));
        assert_eq!(
            (objs.list[1].vx, objs.list[1].state),
            (103, ObjState::Sliding)
        );
        assert_eq!(impacts(&b.events), [(Some(32_041), -150)]);
    }

    /// answers.md "Object previous position": the touching list is built by prepending, so the
    /// resolve returns the last touching object in index order. chips1 (index 2) at screen 305
    /// touches chips2 at (451, 480) and another at (451, 470); back at 300 it touches neither, so
    /// the list head, index 1, is pushed (103) and index 0 is not.
    #[test]
    fn the_resolve_picks_the_last_touching_object_in_index_order() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(3, 300);
        (o.state, o.vx) = (ObjState::Sliding, 300);
        objs.list
            .extend([b.object(4, 452), placed(b.object(4, 452), 470), o]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let v = |k: usize| (objs.list[k].vx, objs.list[k].state);
        assert_eq!(
            [v(0), v(1)],
            [(0, ObjState::Rest), (103, ObjState::Sliding)]
        );
        assert_eq!(objs.list[2].vx, -245);
    }

    /// chips1 rests one row into chips2 (y 381, bottom row 480 = chips2's top row) and slides
    /// right at 300 → 295, layer 260. Still supported (row 481 holds chips2's side 224); the
    /// right probe at layer 261 meets chips2's top row, so it bounces to 10 × 5 − 295 = −245.
    #[test]
    fn a_sliding_object_bounces_off_the_side_of_its_support() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 320), 381);
        (o.state, o.vx) = (ObjState::Sliding, 300);
        o.supported_by.push(1);
        let mut k = b.object(4, 300);
        k.resting_on_top.push(0);
        objs.list.extend([o, k]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!(
            (o.layer_x, o.y, o.vx, o.state),
            (260, 381, -245, ObjState::Sliding)
        );
        assert_eq!(o.supported_by, [1]);
    }

    /// answers.md "Sliding object without support" (round 5): the support is the lowest-index
    /// one and "still" holds when any support overlaps 1 px lower. As in the bounce test chips1
    /// rests one row into chips2 (index 2), but it also lists a far chips2 (index 0, at 3000)
    /// as a support. Index 0 is picked, chips2 makes it still, and the right probe against the
    /// far one touches nothing: no bounce, vx stays 295.
    #[test]
    fn a_sliding_object_probes_its_lowest_index_support_and_is_still_on_any() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 320), 381);
        (o.state, o.vx) = (ObjState::Sliding, 300);
        o.supported_by.extend([2, 0]);
        let mut far = b.object(4, 3000);
        far.resting_on_top.push(1);
        let mut k = b.object(4, 300);
        k.resting_on_top.push(1);
        objs.list.extend([far, o, k]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[1];
        assert_eq!(
            (o.layer_x, o.y, o.vx, o.state),
            (260, 381, 295, ObjState::Sliding)
        );
    }

    /// The slide-off (round 5) leaves only the picked support's rider list (index 0, the lowest)
    /// and clears its own list; index 2 keeps a stale rider entry.
    #[test]
    fn a_slide_off_leaves_only_the_picked_supports_rider_list() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 1000), 380);
        (o.state, o.vx) = (ObjState::Sliding, 300);
        o.supported_by.extend([2, 0]);
        let mut low = b.object(4, 300);
        low.resting_on_top.push(1);
        let mut high = b.object(4, 3000);
        high.resting_on_top.push(1);
        objs.list.extend([low, o, high]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        assert_eq!(objs.list[1].state, ObjState::Falling);
        assert!(objs.list[1].supported_by.is_empty());
        assert!(objs.list[0].resting_on_top.is_empty());
        assert_eq!(objs.list[2].resting_on_top, [1]);
    }

    /// Riders are touches too (round 5): the hit of the previous-position test, with chips2
    /// listed as chips1's rider, still resolves, pushes chips2 (103) and bounces (−245).
    #[test]
    fn a_rider_that_overlaps_counts_as_a_touch() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(3, 300);
        (o.state, o.vx) = (ObjState::Sliding, 300);
        o.resting_on_top.push(1);
        objs.list.extend([o, b.object(4, 452)]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        assert_eq!((objs.list[0].screen_x, objs.list[0].vx), (300, -245));
        assert_eq!(objs.list[1].vx, 103);
    }

    /// Off its support (no overlap 1 px lower) a sliding object falls: drop vx trunc(295 / 50).
    #[test]
    fn a_sliding_object_off_its_support_falls() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 1000), 380);
        (o.state, o.vx) = (ObjState::Sliding, 300);
        o.supported_by.push(1);
        let mut k = b.object(4, 300);
        k.resting_on_top.push(0);
        objs.list.extend([o, k]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!((o.state, o.drop_vx), (ObjState::Falling, 5));
        assert!(o.supported_by.is_empty() && objs.list[1].resting_on_top.is_empty());
    }

    /// chips1 falls straight down (vx 0, vy 50, y 381 with prev 381 from the motion) into
    /// chips2's top row. The restored position still touches, so it steps 1 px up to 380 (path
    /// A, the stuck counter 1 < 31); the probe there sets only below (y 381), so it lands on
    /// chips2 at 380. The state changed, so the counter is back to 0.
    #[test]
    fn a_falling_object_touching_only_below_lands_on_the_other() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 320), 381);
        o.state = ObjState::Falling;
        objs.list.extend([o, b.object(4, 300)]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!(
            (o.y, o.vy, o.state, o.stuck),
            (380, 0, ObjState::Sliding, 0)
        );
        assert_eq!((o.prev_x, o.prev_y), (256, 380));
        assert_eq!(
            (
                o.supported_by.as_slice(),
                objs.list[1].resting_on_top.as_slice()
            ),
            (&[1][..], &[0][..])
        );
        assert_eq!(impacts(&b.events), [(Some(32_041), -150)]);
    }

    /// chips1 at y 385 overlaps chips2's top row by 5 rows (its row 480 holds 160, chips2's top
    /// row 150..224). Path A steps only 1 px, to 384, and the probe below lands it there.
    #[test]
    fn a_falling_object_steps_back_only_1_px_while_its_stuck_counter_is_below_31() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 320), 385);
        o.state = ObjState::Falling;
        objs.list.extend([o, b.object(4, 300)]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!((o.y, o.state), (384, ObjState::Sliding));
    }

    /// answers.md "`FUN_004303e5` push-up": the 31st resolve within one state (path B) resets
    /// the counter and pushes the object up 1 px per retest with v = (0, 15): 384..381 still
    /// touch, 380 is clear. v is restored, the probe below lands it at 380.
    #[test]
    fn the_31st_resolve_in_one_state_pushes_the_object_up_out_of_the_overlap() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 320), 385);
        (o.state, o.stuck) = (ObjState::Falling, 30);
        objs.list.extend([o, b.object(4, 300)]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!((o.y, o.state, o.stuck), (380, ObjState::Sliding, 0));
        assert_eq!(o.supported_by, [1]);
    }

    /// chips1 falls right at 300 (layer 240 → 244, screen 305) into chips2's top row at 451. The
    /// resolve goes back to layer 240, which touches nothing; the probe there at d = 1 (layer
    /// 241, screen 301, the same half-x) sets nothing, at d = 2 (screen 302) right: a side hit.
    /// Impact 32041, then chips2 is pushed in mode 2 (200 and 33: s = 116, q = 15 × 116² / 30 =
    /// 6728, √ = 82), and chips1 reverses. Still falling, so the stuck counter stays at 1.
    #[test]
    fn a_falling_object_with_a_side_flag_reverses_and_pushes() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 300), 430);
        (o.state, o.vx) = (ObjState::Falling, 300);
        objs.list.extend([o, b.object(4, 452)]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!(
            (o.layer_x, o.y, o.vx, o.vy, o.stuck),
            (240, 430, -300, 0, 1)
        );
        assert_eq!(
            (objs.list[1].vx, objs.list[1].state),
            (82, ObjState::Sliding)
        );
        assert!(objs.list[1].resting_on_top.is_empty());
        assert_eq!(impacts(&b.events), [(Some(32_041), -150)]);
    }

    /// With vx 0 and vy 0 after the motion the resolve takes path C: vy = 5, the push-up to 380
    /// (clear at once), vy = 0. Nothing is probed at any d, so the fallback is a side hit: it does
    /// not land on the other. The only draw is the impact sound's R(6000).
    #[test]
    fn a_falling_object_with_no_probe_set_takes_the_side_hit() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 320), 381);
        (o.state, o.vy) = (ObjState::Falling, -50);
        objs.list.extend([o, b.object(4, 300)]);
        b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        let o = &objs.list[0];
        assert_eq!((o.y, o.vx, o.vy, o.state), (380, 0, 0, ObjState::Falling));
        assert!(objs.list[1].resting_on_top.is_empty());
        assert_eq!(b.rng.rand(), 18467);
    }

    /// answers.md "Extreme placement": the types run in order, 0 ×3, 1 ×5, 2 ×7, with no type
    /// draw. Seed 1, x = R(5000) with the synthetic widths 274 and 214: 41, 3467, 1334 (type 0),
    /// then 1500 clashes with 1334..1608, so type 1 takes 4169 and then 724.
    #[test]
    fn extreme_placement_takes_the_types_in_order_with_no_type_draw() {
        let mut b = Bench::new(1);
        let placed = b.with(|cx| place_extreme(|_, _| true, cx));
        let first: Vec<_> = placed
            .iter()
            .take(5)
            .map(|o| (o.kind, o.screen_x))
            .collect();
        let want = [
            (Some(0), 41),
            (Some(0), 3467),
            (Some(0), 1334),
            (Some(1), 4169),
            (Some(1), 724),
        ];
        assert_eq!(first, want);
        let kinds: Vec<_> = placed.iter().filter_map(|o| o.kind).collect();
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        assert_eq!(kinds, sorted);
        assert!(placed.iter().all(|o| o.y == TABLE_Y - o.h));
    }

    /// The x search: 1001 draws per budget. A success leaves the budget used, so the next
    /// search with the same counter stops after the 401 draws left; a fresh one gets 1001.
    #[test]
    fn the_x_search_draws_from_a_1001_budget_that_a_success_does_not_reset() {
        let mut b = Bench::new(1);
        let calls = std::cell::Cell::new(0);
        let on_600th = |_| {
            calls.set(calls.get() + 1);
            calls.get() == 600
        };
        let mut tries = 0;
        assert!(b.with(|cx| draw_x(&mut tries, on_600th, cx)).is_some());
        assert_eq!(tries, 600);
        assert_eq!(b.with(|cx| draw_x(&mut tries, |_| false, cx)), None);
        let mut probe = MsvcRand::new(1);
        skip(&mut probe, 1001);
        assert_eq!(b.rng, probe);
        let mut tries = 0;
        assert_eq!(b.with(|cx| draw_x(&mut tries, |_| false, cx)), None);
        skip(&mut probe, 1001);
        assert_eq!(b.rng, probe);
    }

    /// The shared budget in extreme placement: the first object takes draw 1, so the second
    /// fails after 1000 draws, then 1999 failures of 1001: 2 002 000 draws in all, one fewer than
    /// with a budget per object. The 2000th failure leaves the one placed object out.
    #[test]
    fn extreme_placement_shares_the_x_budget_until_a_failure() {
        let mut b = Bench::new(1);
        let calls = std::cell::Cell::new(0);
        let first_only = |_, _| {
            calls.set(calls.get() + 1);
            calls.get() == 1
        };
        let placed = b.with(|cx| place_extreme(first_only, cx));
        assert!(placed.is_empty());
        let mut probe = MsvcRand::new(1);
        skip(&mut probe, 1 + 1000 + 1999 * 1001);
        assert_eq!(b.rng, probe);
    }

    /// answers.md "Extreme placement": the normal placement resets its counter per object and
    /// draws 1001 x per failed object. One eligible type: R(1), then 1001 refused draws, 2000
    /// times.
    #[test]
    fn a_normal_placement_that_never_fits_draws_1001_x_per_failure() {
        let mut b = Bench::new(1);
        let placed = b.with(|cx| place_on_table(&NORMAL_TYPES[..1], &[true], |_, _| false, cx));
        assert!(placed.is_empty());
        let mut probe = MsvcRand::new(1);
        skip(&mut probe, 2000 * (1 + 1001));
        assert_eq!(b.rng, probe);
    }

    /// answers.md "Placement overlap test": x is refused when xj − wNew ≤ x ≤ xj + wj. chips1
    /// at 1000 is 150 wide, the new object 100: 900..=1150 is refused, 1151 is not.
    #[test]
    fn placement_refuses_x_from_the_new_width_left_of_an_object_to_its_right_edge() {
        let b = Bench::new(1);
        let placed = [b.object(3, 1000)];
        let size = Size { w: 100, h: 100 };
        let ok = |x| fits(&placed, size, |_, _| true, x);
        assert_eq!([899, 900, 1150, 1151].map(ok), [true, false, false, true]);
    }

    /// answers.md "Extreme placement" step 4: the 2000th failure decrements the placed index, so
    /// the count leaves out the last placed object. One type: R(1) and x on the first draw, then
    /// 2000 failures of R(1) and 1001 refused x.
    #[test]
    fn a_placement_ending_on_its_2000th_failure_leaves_out_the_last_object() {
        let mut b = Bench::new(1);
        let calls = std::cell::Cell::new(0);
        let first_only = |_, _| {
            calls.set(calls.get() + 1);
            calls.get() == 1
        };
        let placed = b.with(|cx| place_on_table(&NORMAL_TYPES[..1], &[true], first_only, cx));
        assert!(placed.is_empty());
        let mut probe = MsvcRand::new(1);
        skip(&mut probe, 2 + 2000 * (1 + 1001));
        assert_eq!(b.rng, probe);
    }
    fn skip(rng: &mut MsvcRand, n: u32) {
        for _ in 0..n {
            rng.rand();
        }
    }

    /// can2 (222 × 100, grip 111, 20, weight 35) carried at screen 300, y 300 is placed at
    /// (400, 420) under a fly at (451, 360), where its right side (half-x 310) meets chips2's
    /// top row (y 480, 300..374). It goes back to screen 300 and y 300, which touch nothing, so
    /// there is no step. The fly: x = 300 + 111 − 60 = 351, y = 300 − 94 + 34 = 240,
    /// vx −trunc(100 / 1.4) = −71, vy trunc(−50 / 1.4 − 55) = −90, then + 35.
    #[test]
    fn a_carried_object_that_hits_another_goes_back_in_x_and_y() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut can = placed(b.object(1, 300), 300);
        can.held = true;
        objs.list.extend([can, b.object(4, 600)]);
        (objs.carrying, objs.carried) = (true, Some(0));
        let mut fly = fly_at(&mut b, 451, 360);
        (fly.aspirating, fly.carrying, fly.vx, fly.vy) = (true, true, 100, 50);
        b.with(|cx| objs.grab(&mut fly, &[], &Traps::default(), 0.0, cx));
        let o = &objs.list[0];
        assert_eq!((o.layer_x, o.screen_x, o.y), (240, 300, 300));
        assert_eq!((fly.x, fly.y, fly.vx, fly.vy), (351, 240, -71, -55));
    }

    /// The restored can2 (300, 300) still touches chips2 at (500, 380): its right side (half-x
    /// 260) is on chips2's top row (250..324). It steps 1 px against v (100, 50) to layer 239 and
    /// y 299, then the dt = 0 motion saves that and scrolls x by −trunc(2.0): layer 237 (screen
    /// 296); in state 0 prev is the post-move position. The fly is placed on that: x = 296 + 111
    /// − 60 = 347, y = 299 − 60 = 239. Held, the stuck counter is not bumped (30 stays 30, so
    /// it is path A, not the push-up).
    #[test]
    fn a_carried_object_still_touching_after_the_restore_steps_back_one_px() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut can = placed(b.object(1, 300), 300);
        (can.held, can.stuck) = (true, 30);
        objs.list.extend([can, placed(b.object(4, 500), 380)]);
        (objs.carrying, objs.carried) = (true, Some(0));
        let mut fly = fly_at(&mut b, 371, 250);
        (fly.aspirating, fly.carrying, fly.vx, fly.vy) = (true, true, 100, 50);
        b.with(|cx| objs.grab(&mut fly, &[], &Traps::default(), 2.0, cx));
        let o = &objs.list[0];
        assert_eq!((o.layer_x, o.screen_x, o.y, o.stuck), (237, 296, 299, 30));
        assert_eq!((o.prev_x, o.prev_y), (237, 299));
        assert_eq!((fly.x, fly.y), (347, 239));
    }

    /// answers.md "`FUN_004303e5` push-up": a carried object with v = 0 (path C) placed at (400,
    /// 420) touches chips2 at (500, 380). Restored to (300, 300), its bottom rows still meet
    /// chips2's top row; it moves up 1 px per retest until y 280 is clear (box 280..380). prev is
    /// not written. The fly: x = 351, y = 280 − 60 = 220, vy = trunc(−0 − 55) + 35.
    #[test]
    fn a_carried_object_with_no_speed_is_pushed_up_out_of_the_overlap() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut can = placed(b.object(1, 300), 300);
        can.held = true;
        objs.list.extend([can, placed(b.object(4, 500), 380)]);
        (objs.carrying, objs.carried) = (true, Some(0));
        let mut fly = fly_at(&mut b, 451, 360);
        (fly.aspirating, fly.carrying, fly.vx, fly.vy) = (true, true, 0, 0);
        b.with(|cx| objs.grab(&mut fly, &[], &Traps::default(), 0.0, cx));
        let o = &objs.list[0];
        assert_eq!((o.screen_x, o.y, o.prev_x, o.prev_y), (300, 280, 240, 300));
        assert_eq!((fly.x, fly.y, fly.vx, fly.vy), (351, 220, 0, -20));
    }

    /// The push-up gives up after 500 steps, and a clear test on the 500th counts as a failure.
    /// can2 (100 tall, restored at y 300) beside a 700-tall object with the same left edge: with
    /// its top at −99 the box clears at y −199, step 499; at −100 only at step 500, so the can
    /// goes back to its previous position.
    #[test]
    fn the_push_up_gives_up_after_500_steps_even_when_the_500th_is_clear() {
        for (top, want_y) in [(-99, -199), (-100, 300)] {
            let mut b = Bench::new(1);
            let size = Size { w: 150, h: 700 };
            b.data.sizes.insert("tall".into(), size);
            b.data.outlines.insert("tall".into(), Outline::rect(size));
            let mut tall = Object::blank("tall", size);
            tall.set_screen_x(300);
            tall.y = top;
            let mut objs = Objects::empty();
            let mut can = placed(b.object(1, 300), 300);
            can.held = true;
            objs.list.extend([can, tall]);
            (objs.carrying, objs.carried) = (true, Some(0));
            let mut fly = fly_at(&mut b, 351, 326);
            (fly.aspirating, fly.carrying, fly.vx, fly.vy) = (true, true, 0, 0);
            b.with(|cx| objs.grab(&mut fly, &[], &Traps::default(), 0.0, cx));
            assert_eq!((objs.list[0].screen_x, objs.list[0].y), (300, want_y));
            assert_eq!(fly.y, want_y - 60);
        }
    }

    /// answers.md "Carried object collision": on the table the fly is set on the rest y
    /// (480 − 94 + 34 = 420), but the object keeps its placed y (430 + 60 = 490) this tick.
    /// It moved 190 from its previous y 300, so it is landed.
    #[test]
    fn a_carried_object_on_the_table_is_not_clamped_to_its_rest_y() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut can = placed(b.object(1, 300), 300);
        can.held = true;
        objs.list.push(can);
        (objs.carrying, objs.carried) = (true, Some(0));
        let mut fly = fly_at(&mut b, 351, 430);
        (fly.aspirating, fly.carrying, fly.vy) = (true, true, 0);
        b.with(|cx| objs.grab(&mut fly, &[], &Traps::default(), 0.0, cx));
        let o = &objs.list[0];
        assert_eq!((o.y, o.landed), (490, true));
        assert_eq!((fly.y, fly.vy), (420, -55 + 35));
    }

    /// answers.md "Object previous position", the open `objects.rs:1829` case: chips1 dropped
    /// at y 200 over chips2 (y 480). W1 makes every resolve go back one tick of fall:
    /// - at y 368, vy 1100 moves it to 384; the probes from 368 reach only y 379, so the forced
    ///   side hit sets vy = 0 and pushes chips2 in mode 2: trunc(1100 / 1.5) = 733, s = 366,
    ///   √(15 × 366² / 30) = 258, side roll 18467 % 10 = 7 (after the impact's 41): right.
    /// - chips1 falls again; at y 377 (moved to 381) chips2, first in pass 2, slides into it and
    ///   pushes it (mode 1), so chips1 is sliding before its own fall step runs.
    /// - Sliding with no support it never falls again: it stops 3 px above chips2's top row,
    ///   unsupported, while chips2 slides on to the right.
    #[test]
    fn a_dropped_object_knocks_the_one_below_away_and_stops_above_it() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = placed(b.object(3, 300), 200);
        o.state = ObjState::Falling;
        objs.list.extend([o, b.object(4, 300)]);
        for _ in 0..SETTLE_TICKS {
            b.with(|cx| objs.step_objects(0.0, &Traps::default(), cx));
        }
        let (o, k) = (&objs.list[0], &objs.list[1]);
        assert_eq!((o.screen_x, o.y, o.state), (300, 377, ObjState::Rest));
        assert!(o.supported_by.is_empty() && k.resting_on_top.is_empty());
        assert!(k.screen_x > 300);
    }
}
