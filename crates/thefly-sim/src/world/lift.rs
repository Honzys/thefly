//! W1. The lift (winch): a hook that meets a carried object and hoists it off the top
//! (fly-and-world §5.6). Driven from step 12 by the objects manager.

use super::objects::Objects;
use crate::ctx::Ctx;

/// The three anchor draws must differ pairwise by more than this (shop px, `FUN_0040bf70`).
pub const ANCHOR_GAP: i32 = 36;
/// Anchors are drawn from 0..760 shop px and stored as trunc(v / 0.152).
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
    Delivered,
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
    pub x: i32,
    pub y: i32,
    pub vy: i32,
    pub ring: [i32; RING],
    pub read: usize,
    pub write: usize,
    /// Rope bitmap height (`treuil_corde`).
    pub max_y: i32,
    /// Hook offset, the constructor argument. Open item: its value is not in the docs.
    pub off: i32,
    /// `treuil` frame size.
    pub w: i32,
    pub h: i32,
    pub rope_y: i32,
}

/// `FUN_0040bf70(760, gap, 0.152)`: three draws pairwise more than `gap` apart, stored as
/// trunc(v / 0.152). Bounded by [`WINCH_REROLL_CAP`]. Open item: fly-and-world §5.6 says R(760),
/// ai-and-rules §4.3 says `rand % 761`. The anchors persist in [`crate::match_state::MatchState`].
pub fn draw_anchors(gap: i32, cx: &mut Ctx) -> [i32; 3] {
    let _ = (gap, cx);
    todo!("W1 fly-and-world §5.6 Anchors, ai-and-rules §4.3 Winch slots")
}

impl Lift {
    /// Init (`FUN_00430e48`) then reset.
    pub fn new(anchors: [i32; 3], cx: &mut Ctx) -> Self {
        let _ = (anchors, cx);
        todo!("W1 fly-and-world §5.6 Reset")
    }

    /// `FUN_00431115` then follow (`FUN_00431931`), with the winch sound 47.
    pub fn update(
        &mut self,
        objects: &mut Objects,
        fly_carrying: bool,
        camera_delta: f64,
        cx: &mut Ctx,
    ) -> LiftResult {
        let _ = (objects, fly_carrying, camera_delta, cx);
        todo!("W1 fly-and-world §5.6 Update, Follow")
    }
}
