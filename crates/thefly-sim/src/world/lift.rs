//! W1. The lift (winch): a hook that meets a carried object and hoists it off the top
//! (fly-and-world §5.6). Driven from step 12 by the objects manager.

use super::objects::Objects;
use crate::ctx::Ctx;

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
    pub x: i32,
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
        let anchor = cx.rng.r(3) as usize;
        let vy = cx.rng.r(150) + 50;
        let y = -body.h;
        Self {
            anchors,
            anchor,
            state: LiftState::Idle,
            waiting: None,
            hooked: None,
            approaching: false,
            moved: false,
            counter: 0,
            period: IDLE_PERIOD,
            x: anchors[anchor],
            y,
            vy,
            ring: [0; RING],
            read: 0,
            write: 0,
            max_y: rope.h,
            off: HOOK_OFF,
            w: body.w,
            h: body.h,
            rope_y: y - rope.h,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::rng::MsvcRand;

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
}
