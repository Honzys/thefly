//! One gameplay period: every actor on the table, advanced by [`World::tick`].
//!
//! [`World::tick`] is the literal tick order of `FUN_0040326c` (fly-and-world §2): one call per
//! step, in order. Workers fill the step bodies in their own files and never reorder or merge
//! steps here; a step that needs another's result gets it as an argument, as `hit` does.
//!
//! Ownership rule: a step writes only its owner's state. Cross-owner effects (the boot splats the
//! fly, a catch adds a goodie) go through the owner's public methods, listed in the plan.

pub mod ai;
pub mod ambient;
pub mod boot;
pub mod camera;
pub mod collision;
pub mod farmer;
pub mod fly;
pub mod flying_goodies;
pub mod goodies;
pub mod lift;
pub mod midges;
pub mod objects;
pub mod traps;

use crate::ctx::Ctx;
use crate::input::{FarmerPad, FlyPad, KeyState};
use crate::match_state::{MatchState, PeriodClock};

/// The world's ring width W in fly-layer pixels: the main scenery layer's width (game-spec §4.1).
/// Open item: not measured from the asset; the arena, `Maison` and `dehors` are all 4000 wide.
pub const W: i32 = 4000;

/// Parallax of the objects, lift and boot layer; its wrap width is trunc(W × 1.25).
pub const OBJECT_PARALLAX: f64 = 1.25;

pub const SCREEN_W: i32 = 800;
pub const SCREEN_H: i32 = 600;

#[derive(Debug, Clone)]
pub struct World {
    pub fly: fly::Fly,
    pub farmer: farmer::Farmer,
    /// Present with one player (data+0x23 == 1); the human is then the fly.
    pub ai: Option<ai::FarmerAi>,
    pub objects: objects::Objects,
    pub lift: lift::Lift,
    pub traps: traps::Traps,
    pub boot: boot::Boot,
    /// The Midges goodie's swarm (300 slots).
    pub midges: midges::Swarm,
    pub ambient_midges: midges::AmbientMidges,
    pub flying_goodies: flying_goodies::FlyingGoodies,
    pub goodies: goodies::Goodies,
    pub camera: camera::Camera,
    pub ambient: ambient::Ambient,
    pub clock: PeriodClock,
    /// game+0x407c8: −1 while Enter is up, 10 after a goodie use or unpause (step 1).
    pub fly_action_memory: i32,
    /// game+0x407cc: the same scheme for the farmer's swing key (step 2).
    pub farmer_swing_memory: i32,
    /// game+0x408ac: every key reads as released.
    pub keys_blocked: bool,
    /// The quit dialog is open. Open item: which steps freeze besides the boot and the AI.
    pub paused: bool,
    /// game+0x407b8: set by step 12 when the table is cleared or the clock runs out.
    pub period_over: bool,
}

/// The swatter's result for this tick (step 6), consumed by the fly update (step 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SwatHit {
    pub damage: i32,
    pub dir_x: fly::KickDir,
    pub dir_y: fly::KickDir,
}

impl World {
    /// Builds the period: the loading work of `FUN_004015c8` (placement, respawn with 3000 tries,
    /// AI creation, trap entities from the shop layout, timers armed at `cx.now`).
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        let _ = (m, cx);
        todo!("W1: period start, fly-and-world §5.2, §3.7; screens §4.1")
    }

    /// One 65 Hz gameplay tick.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) {
        let keys = if self.keys_blocked {
            KeyState::NONE
        } else {
            *keys
        };
        let fly_pad = FlyPad::read(&keys);
        let farmer_pad = FarmerPad::read(&keys);

        // 1. Fly action: Enter held and not blocked since the last goodie use.
        let action = self.fly_action_edge(&fly_pad);
        // 2. Farmer swing edge.
        let swing_edge = self.farmer_swing_edge(&farmer_pad);
        // 3. Fly goodie selector and use; a use cancels this tick's action.
        let action = self.fly_goodie(&fly_pad, action, m, cx);
        // 4. Farmer goodie selector and use (human pad, or the AI's request).
        self.farmer_goodie(&farmer_pad, m, cx);
        // 5. Fly physics, with the keys swapped while inverted controls are active.
        self.fly_physics(&fly_pad, cx);
        // 6. Farmer: human move and hit test, or the AI. Yields this tick's damage.
        let hit = self.farmer_step(&farmer_pad, swing_edge, m, cx);
        // 7. Boot update.
        self.boot_step(cx);
        // 8. Fly update with the un-inverted keys, the damage and the action.
        self.fly_update(&fly_pad, hit, action, m, cx);
        // 9. Respawn placement: re-roll while overlapping an object, then appear.
        self.respawn_placement(cx);
        // 10. Boot collision, only while the fly is alive.
        self.boot_collision(cx);
        // 11. Fly not alive: both selectors and the inverted flag reset.
        self.fly_not_alive_resets();
        // 12. Table objects and the lift; all delivered or the clock ends the period.
        self.objects_step(m, cx);
        // 13. Traps.
        self.traps_step(cx);
        // 14. Ambient midges.
        self.ambient_midges_step(cx);
        // 15. Mystery expiry, both sides.
        self.mystery_expiry(cx);
        // 16. Flying goodies: spawn, update, catch.
        self.flying_goodies_step(m, cx);
        // 17. Ambient sounds, farmer voice, tutorial.
        self.ambient_step(m, cx);

        // Port addition: what the original did in its render pass (`FUN_00403e36`), which ran
        // once per tick after the update. Open item: the order of these two within that pass.
        // 18a. Goodie midges: homing, swatter kills, farmer slowdown (fly-and-world §8.2).
        self.midges_step(cx);
        // 18b. Appear animation done → alive; splat done → kill recorded, respawn requested.
        self.fly_render_driven(m, cx);
    }
}
