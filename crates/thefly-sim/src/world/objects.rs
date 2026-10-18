//! W1. Table objects: the type table, placement, motion, stacking, fly contact, grab, carry, drop
//! and delivery (fly-and-world §4.4, §5). Ticks step 12, which also runs the lift (§5.5 step 9)
//! and the period clock.

use super::collision::Rect;
use super::traps::Traps;
use super::{OBJECT_PARALLAX, W, World};
use crate::ctx::Ctx;
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
    pub fn place(m: &mut MatchState, meal: Meal, traps: &Traps, cx: &mut Ctx) -> Self {
        let _ = (m, meal, traps, cx);
        todo!("W1 fly-and-world §5.2; trap check via traps::span_free")
    }
}

impl World {
    /// Step 12 (`FUN_0042c888`): per-object motion and interaction, fly contact, grab, attach,
    /// magnet, drop, carry, the lift, then delivery and the period-over flag.
    pub fn objects_step(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let _ = (m, cx);
        todo!("W1 fly-and-world §4.4, §5.4, §5.5; period clock ai-and-rules §8.6")
    }
}
