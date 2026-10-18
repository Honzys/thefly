//! W1. Table objects: the type table, placement, motion, stacking, fly contact, grab, carry, drop
//! and delivery (fly-and-world §4.4, §5). Ticks step 12, which also runs the lift (§5.5 step 9)
//! and the period clock.

use super::collision::{Rect, Shape, fly_outline, outlines_overlap};
use super::fly::Fly;
use super::lift::LiftResult;
use super::traps::Traps;
use super::{OBJECT_PARALLAX, W, World};
use crate::clock::DT;
use crate::ctx::Ctx;
use crate::data::{Outline, Size};
use crate::event::{Effect, Event, SoundSlot};
use crate::match_state::{MatchState, Meal};

/// Placement stops at this many objects (§5.2).
pub const MAX_PLACED: usize = 15;
/// Fan stands become extra non-collectible objects; there are at most 5 fans.
pub const MAX_OBJECTS: usize = MAX_PLACED + 5;
/// Placement gives up after this many failed objects.
pub const PLACEMENT_FAILURES: u32 = 2000;
/// Type draws per object until an eligible type with stock comes up.
pub const PLACEMENT_TYPE_TRIES: u32 = 5001;
/// x draws per object.
pub const PLACEMENT_X_TRIES: u32 = 1000;
/// Fly–object separation steps (§4.4 step 5).
pub const SEPARATE_TRIES: u32 = 1000;
/// Grab lock distance from the fly's bottom-centre to the grip point (§5.5 step 5).
pub const LOCK_DISTANCE: f64 = 1000.0;
/// Side-test retries when a falling object meets another (§5.4).
pub const FALL_SIDE_RETRIES: u32 = 10;
pub const GRAVITY_PER_TICK: i32 = 50;
pub const MAX_VX: i32 = 500;
/// Objects rest at y = 580 − h.
pub const TABLE_Y: i32 = 580;

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
    /// Read and cleared: dust of size weight × 20 / 130 and landing sound 6.
    pub landed: bool,
    pub held: bool,
    pub state: ObjState,
    pub drop_vx: i32,
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
    ///
    /// shortcut: extreme placement (`FUN_0042bdf8`) is undocumented beyond its type table; it
    /// runs the §5.2 loop over the three extreme types with R(3). Revisit when it is read.
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
        objects.list = place_on_table(types, &eligible, free, cx);
        m.stats.available += objects.list.len() as i32;
        objects.add_fan_stands(traps, cx);
        objects
    }

    /// §5.2 step 4 (`FUN_0042a85a`): each fan stand is an extra, non-collectible object.
    ///
    /// shortcut: §5.2 names only the piedventi outline. The stand takes the fan's x as its layer
    /// x (`Fan::x` is documented as 1.25-layer x, unlike the lift, which reads the same 0.152
    /// store as a screen-ring x), `_dt` for a right-facing fan, its size from the outline's
    /// extent, y = 580 − h, weight and rank 0. Revisit when `FUN_0042a85a` is read.
    fn add_fan_stands(&mut self, traps: &Traps, cx: &mut Ctx) {
        for fan in &traps.fans {
            let sprite = if fan.facing_right {
                "piedventi_dt"
            } else {
                "piedventi_gc"
            };
            let size = outline_extent(cx.data.outline(sprite));
            let mut o = Object::blank(sprite, size);
            o.layer_x = fan.x.rem_euclid(W);
            o.screen_x = o.screen_from_layer();
            o.prev_x = o.layer_x;
            self.list.push(o);
        }
    }

    /// §5.5 step 1 for every active object: landing dust, the object–object interaction
    /// (collectible, weight > 0), then the motion of §5.4 when not held.
    ///
    /// shortcut: the fan push of objects (`FUN_0042754d`) is not documented beyond "sideways"
    /// and needs the fan strength (open item O9); it is not applied. Revisit with O9.
    pub fn step_objects(&mut self, delta: f64, cx: &mut Ctx) {
        for i in 0..self.list.len() {
            if !self.list[i].active {
                continue;
            }
            self.landing_dust(i, cx);
            let o = &self.list[i];
            if o.collectible && o.weight > 0 && !o.held && !o.on_lift {
                self.interact(i, cx);
            }
            if !self.list[i].held {
                self.list[i].motion(delta);
            }
        }
    }

    fn landing_dust(&mut self, i: usize, cx: &mut Ctx) {
        let o = &mut self.list[i];
        if !std::mem::take(&mut o.landed) {
            return;
        }
        cx.emit(Event::Effect(Effect::Dust {
            x: o.screen_x + o.w / 2,
            y: o.y + o.h,
            size: o.weight * 20 / 130,
        }));
        cx.emit(Event::play(SoundSlot::BOOM));
    }

    /// `FUN_0042ec96`, the object–object step (§5.4).
    fn interact(&mut self, i: usize, cx: &mut Ctx) {
        match self.list[i].state {
            ObjState::Rest => {}
            ObjState::Sliding => self.slide(i, cx),
            ObjState::Falling => match self.hit_other(i, 0, cx) {
                None => self.list[i].land_on_table(),
                Some(k) => self.fall_into(i, k, cx),
            },
        }
    }

    /// A sliding object: push what it slides into, bounce, then fall when unsupported.
    ///
    /// shortcut: §5.4's side test (`FUN_0042fb1c`) is not documented, so after the bounce
    /// (gaps §3: ±10 × trunc(weight / 3) − vx, 0 unless the sign flips) v is left as is.
    /// Revisit when `FUN_0042fb1c` is read.
    fn slide(&mut self, i: usize, cx: &mut Ctx) {
        if let Some(k) = self.hit_other(i, 0, cx) {
            let o = &self.list[i];
            let (vx, vy, weight) = (o.vx, o.vy, o.weight);
            if self.list[k].collectible {
                self.push(k, PushMode::Slide, vx, vy, weight, cx);
            }
            let rebound = vx.signum() * 10 * (f64::from(weight) / 3.0) as i32 - vx;
            self.list[i].vx = if rebound.signum() == -vx.signum() {
                rebound
            } else {
                0
            };
        }
        if !self.supported(i) {
            self.detach(i);
            let o = &mut self.list[i];
            o.state = ObjState::Falling;
            o.drop_vx = (f64::from(o.vx) / 50.0) as i32;
        }
    }

    /// On the table, or above a supporter that still spans its x.
    fn supported(&self, i: usize) -> bool {
        let o = &self.list[i];
        o.y >= o.rest_y
            || o.supported_by.iter().any(|&s| {
                let b = &self.list[s];
                o.screen_x < b.screen_x + b.w && b.screen_x < o.screen_x + o.w
            })
    }

    /// A falling object meets object `k`.
    ///
    /// shortcut: §5.4 gives "side tests with up to 10 retries" without the test. Here the
    /// object lands on top when its bottom is in the upper half of `k` (stepping up 1 px while
    /// the outlines still touch, at most [`FALL_SIDE_RETRIES`]); otherwise it is a side hit.
    /// Revisit when `FUN_0042fb1c` is read.
    fn fall_into(&mut self, i: usize, k: usize, cx: &mut Ctx) {
        let (o, other) = (&self.list[i], &self.list[k]);
        if o.y + o.h - other.y < other.h / 2 {
            for _ in 0..FALL_SIDE_RETRIES {
                if self.hit_other(i, 0, cx) != Some(k) {
                    break;
                }
                self.list[i].y -= 1;
            }
            self.list[k].resting_on_top.push(i);
            let o = &mut self.list[i];
            o.supported_by.push(k);
            o.state = ObjState::Sliding;
            o.vy = 0;
            return;
        }
        let (vx, vy, weight) = (o.vx, o.vy, o.weight);
        if other.collectible {
            self.push(k, PushMode::Drop, vx, vy, weight, cx);
        }
        let o = &mut self.list[i];
        o.drop_vx = -o.drop_vx;
        o.vx = -o.vx;
        o.vy = 0;
    }

    /// The first other active object whose outline touches object `i` shifted `dx` px, leaving
    /// out its own stack and objects on the lift.
    fn hit_other(&self, i: usize, dx: i32, cx: &Ctx) -> Option<usize> {
        self.hit_where(i, dx, cx, |_| true)
    }

    fn hit_where(
        &self,
        i: usize,
        dx: i32,
        cx: &Ctx,
        keep: impl Fn(&Object) -> bool,
    ) -> Option<usize> {
        let o = &self.list[i];
        let mut a = o.shape(cx);
        a.x += dx;
        (0..self.list.len()).find(|&k| {
            let b = &self.list[k];
            let unrelated =
                k != i && !o.supported_by.contains(&k) && !o.resting_on_top.contains(&k);
            unrelated
                && b.active
                && !b.on_lift
                && keep(b)
                && outlines_overlap(&a, &b.shape(cx), self.wrap_width)
        })
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
        o.layer_x = o.prev_x;
        o.y = o.prev_y;
        o.screen_x = o.screen_from_layer();
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
    pub fn grab(&mut self, fly: &mut Fly, candidates: &[usize], cx: &mut Ctx) {
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
            self.carry(fly, cx);
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
    ///
    /// shortcut: the side test after a blocked attach is undocumented; the object is pushed
    /// away from the object it hit. Revisit when `FUN_0042fb1c` is read.
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
            if let Some(k) = self.hit_other(i, 0, cx) {
                let away = (self.list[i].screen_x - self.list[k].screen_x).signum();
                let o = &mut self.list[i];
                o.y = saved_y;
                o.vx = away * o.weight * 100 / 20;
                o.state = ObjState::Sliding;
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

    /// `FUN_0042f3b1` (§5.5 step 8), then fly vy += weight.
    ///
    /// shortcut: on a collision the object goes back to its previous position before the fly
    /// is placed on it (§5.5 step 8.5 does not say). Revisit when `FUN_0042f3b1` is read. The
    /// fan push between the carry and vy += weight is skipped with the object fan push (O9).
    fn carry(&mut self, fly: &mut Fly, cx: &mut Ctx) {
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
        match self.hit_other(i, 0, cx) {
            None => self.carry_on_table(i, fly, off),
            Some(k) => {
                if self.list[k].collectible {
                    self.push(k, PushMode::Carried, fly.vx, fly.vy, weight, cx);
                }
                let o = &mut self.list[i];
                (o.layer_x, o.y) = (o.prev_x, o.prev_y);
                o.screen_x = o.screen_from_layer();
                let grip = self.screen_left(&self.list[i]) + self.list[i].grip_x;
                fly.x = (f64::from(grip) - f64::from(fly.size.w) / 2.0) as i32;
                fly.y = self.list[i].y - fly.size.h + off;
                fly.vx = -(f64::from(fly.vx) / 1.4) as i32;
                fly.vy = (-(f64::from(fly.vy) / 1.4) - f64::from(weight + 20)) as i32;
            }
        }
        fly.vy += weight;
    }

    /// §5.5 step 8.4: the carried object touches the table.
    fn carry_on_table(&mut self, i: usize, fly: &mut Fly, off: i32) {
        let o = &mut self.list[i];
        if o.y < o.rest_y {
            return;
        }
        o.landed |= (o.y - o.prev_y).abs() > 5;
        o.y = o.rest_y;
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

    /// Sets the screen x directly and the layer x from it.
    ///
    /// shortcut: §5.2 and §5.5 step 8.2 write the screen x; the layer x is taken back as
    /// trunc(x / 1.25), which the next integration turns into trunc(layer × 1.25). Revisit if
    /// the sprite setter is read.
    pub fn set_screen_x(&mut self, x: i32) {
        self.screen_x = x;
        self.layer_x = (f64::from(x) / OBJECT_PARALLAX) as i32;
        self.prev_x = self.layer_x;
    }

    fn screen_from_layer(&self) -> i32 {
        (f64::from(self.layer_x) * OBJECT_PARALLAX) as i32
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

    /// §5.4 steps 1–5 (`FUN_0042eb13`, `FUN_0042ea74`).
    fn motion(&mut self, delta: f64) {
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
        if !self.collectible {
            (self.vx, self.vy) = (0, 0);
        }
        self.layer_x += (f64::from(self.vx) * DT - delta) as i32;
        self.layer_x = self.layer_x.rem_euclid(W);
        self.y += (f64::from(self.vy) * DT) as i32;
        self.screen_x = self.screen_from_layer();
        if self.state == ObjState::Rest {
            (self.prev_x, self.prev_y) = (self.layer_x, self.y);
        }
        self.vx = self.vx.clamp(-MAX_VX, MAX_VX);
    }

    /// §5.4 "Falling, no collision".
    fn land_on_table(&mut self) {
        if self.y < self.rest_y {
            return;
        }
        self.y = self.rest_y;
        self.state = ObjState::Sliding;
        (self.vx, self.vy) = (0, 0);
        self.landed = true;
    }
}

/// `v` moved `d` toward 0 without overshooting.
fn toward_zero(v: i32, d: i32) -> i32 {
    if v > 0 {
        (v - d).max(0)
    } else {
        (v + d).min(0)
    }
}

/// The frame an outline covers: its row count by twice its widest value.
fn outline_extent(o: &Outline) -> Size {
    let widest = o.rows.iter().flatten().copied().max().unwrap_or(0);
    Size {
        w: (i32::from(widest) + 1) * 2,
        h: i32::try_from(o.rows.len()).unwrap_or(0),
    }
}

/// §5.2 step 3 without the economy: up to [`MAX_PLACED`] objects, each a type draw then up to
/// [`PLACEMENT_X_TRIES`] x draws in 0..5000, with landing sound 6 per placed object.
/// `span_free` gets that screen-unit x (§5.2 step 3.3), not a layer x (open item O22).
pub fn place_on_table(
    types: &[ObjectType],
    eligible: &[bool],
    span_free: impl Fn(i32, i32) -> bool,
    cx: &mut Ctx,
) -> Vec<Object> {
    let wrap = (f64::from(W) * OBJECT_PARALLAX) as i32;
    let mut stock: Vec<i32> = types.iter().map(|t| t.stock).collect();
    let mut placed: Vec<Object> = Vec::new();
    let mut failures = 0;
    while placed.len() < MAX_PLACED && failures < PLACEMENT_FAILURES {
        let Some(kind) = pick_type(eligible, &stock, cx) else {
            failures += 1;
            continue;
        };
        let t = &types[kind];
        let size = cx.data.size(t.name);
        let fits = |x: i32| {
            let clear = placed
                .iter()
                .all(|p| x + size.w < p.screen_x || p.screen_x + p.w < x - size.w);
            clear && x + size.w < wrap && span_free(x, size.w)
        };
        let x = (0..PLACEMENT_X_TRIES).find_map(|_| Some(cx.rng.r(wrap)).filter(|&x| fits(x)));
        let Some(x) = x else {
            failures += 1;
            continue;
        };
        let mut o = Object::of_type(kind, t, size);
        o.set_screen_x(x);
        placed.push(o);
        stock[kind] -= 1;
        cx.emit(Event::play(SoundSlot::BOOM));
    }
    placed
}

/// §5.2 step 3.1: R(types) until an eligible type with stock comes up.
///
/// UB guard: after [`PLACEMENT_TYPE_TRIES`] draws the original would carry on with the last,
/// unsuitable type; the port counts a placement failure instead. No meal reaches it (every
/// meal has more than 15 copies; extreme stops at exactly 15).
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
        self.objects.step_objects(delta, cx);
        let candidates = self.objects.fly_contact(&mut self.fly, cx);
        self.objects.grab(&mut self.fly, &candidates, cx);
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
    use crate::data::GameData;
    use crate::event::{Event, SoundSlot};
    use crate::rng::MsvcRand;
    use crate::world::fly::Fly;
    use crate::world::lift::LiftState;

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
                let clash = o.screen_x - o.w <= p.screen_x + p.w && p.screen_x <= o.screen_x + o.w;
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
        let all = [true; 3];
        let placed = b.with(|cx| place_on_table(&EXTREME_TYPES, &all, |_, _| true, cx));
        for (k, t) in EXTREME_TYPES.iter().enumerate() {
            assert!(placed.iter().filter(|o| o.kind == Some(k)).count() as i32 <= t.stock);
        }
    }

    /// Acceptance W1 8.
    #[test]
    fn a_dropped_object_falls_at_50_per_tick_to_the_table() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(0, 300);
        (o.y, o.state, o.drop_vx) = (100, ObjState::Falling, 12);
        objs.list.push(o);
        let mut vys = Vec::new();
        for _ in 0..60 {
            b.with(|cx| objs.step_objects(0.0, cx));
            let o = &objs.list[0];
            assert!(o.vx.abs() <= MAX_VX);
            if o.state != ObjState::Falling {
                break;
            }
            vys.push(o.vy);
        }
        assert_eq!(vys[..4], [50, 100, 150, 200]);
        let o = &objs.list[0];
        assert_eq!((o.y, o.vx, o.vy), (TABLE_Y - o.h, 0, 0));
        assert!(o.landed);
        b.with(|cx| objs.step_objects(0.0, cx));
        assert!(!objs.list[0].landed);
        assert!(b.events.contains(&Event::play(SoundSlot::BOOM)));
    }

    #[test]
    fn a_fast_drop_is_capped_at_500_px_s_sideways() {
        let mut b = Bench::new(1);
        let mut objs = Objects::empty();
        let mut o = b.object(0, 300);
        (o.y, o.state, o.drop_vx) = (-4000, ObjState::Falling, 40);
        objs.list.push(o);
        for _ in 0..30 {
            b.with(|cx| objs.step_objects(0.0, cx));
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
        b.with(|cx| objs.grab(&mut fly, &[0], cx));
        assert!(fly.carrying && objs.carrying);
        assert_eq!(objs.carried, Some(0));
        assert!(objs.list[0].held);
        // Carrying puts the object under the fly: y = flyY + flyH − (grip y + 14).
        b.with(|cx| objs.grab(&mut fly, &[], cx));
        assert_eq!(objs.list[0].y, fly.y + 94 - (20 + 14));
        fly.aspirating = false;
        fly.vx = 400;
        b.with(|cx| objs.grab(&mut fly, &[], cx));
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
        b.with(|cx| objs.grab(&mut fly, &[0], cx));
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
}
