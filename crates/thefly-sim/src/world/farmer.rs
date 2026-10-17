//! W2. The farmer and swatter: movement, swing machine, hit test, Triple, Nuclear, voices and
//! footsteps (ai-and-rules §1). Ticks steps 2 and 6.

use super::collision::Rect;
use super::{SCREEN_W, SwatHit, World};
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::FarmerPad;
use crate::match_state::MatchState;
use crate::stats::Stats;

pub const MAX_LEVEL: i32 = 5;
pub const HITBOX_W: i32 = 130;
pub const HITBOX_H: i32 = 154;
pub const Y_TOP: i32 = 351;
pub const Y_BOTTOM: i32 = 472;
pub const Y_SPAN: i32 = 121;
pub const REACH_MAX: i32 = 375;
/// The farmer's x ring, farmer+0x70.
pub const RING: i32 = 4000;
pub const VX_STEP: i32 = 400;
pub const VY_STEP: i32 = 200;
pub const VY_MAX: i32 = 400;
pub const VOICE_FIRST_MS: u64 = 10_000;
/// Nuclear length when the goodie passes −1; the mystery passes 20000.
pub const NUCLEAR_MS: u64 = 5000;
pub const MYSTERY_NUCLEAR_MS: u64 = 20_000;
/// Triple ends once more than this many bursts have completed.
pub const TRIPLE_MAX_BURSTS: i32 = 6;
pub const TRIPLE_BURST_SWINGS: i32 = 3;
pub const STEP_COUNTER_LIMIT: i32 = 20;

/// `maxSpeedX` = 650 + 60·L.
pub const fn max_speed_x(level: i32) -> i32 {
    650 + 60 * level
}

/// `cooldownLimit` = 35 − 5·L ticks.
pub const fn cooldown_limit(level: i32) -> i32 {
    35 - 5 * level
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Swing {
    /// 0..=4 (AR §1.3).
    pub frame: i32,
    pub swinging: bool,
    pub retracting: bool,
    pub cooldown: i32,
    /// The hit is active this tick: frame > 2 after the step, so ticks 3, 4 and 5 of a swing.
    pub active: bool,
    /// The farmer's mystery "cannot hit" (result 0x0b, farmer+0x154).
    pub blocked: bool,
    /// Ticks without a hit.
    pub miss_streak: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Triple {
    pub on: bool,
    pub bursts: i32,
    pub burst_swing: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Nuclear {
    pub on: bool,
    pub timer: Timer,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Farmer {
    pub level: i32,
    pub max_speed_x: i32,
    pub cooldown_limit: i32,
    /// On the [`RING`]; relative to the screen, shifted by the camera delta.
    pub x: i32,
    /// Clamped to [`Y_TOP`, `Y_BOTTOM`].
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    /// (1 − (472 − y)/121) · 375.
    pub reach: f64,
    pub ignore_scroll_once: bool,
    /// Midges slowdown (ai-and-rules §6.3, farmer+0x118/0x11c/0x120).
    pub midges_on: bool,
    pub slowed: bool,
    pub slow_speed: i32,
    pub slow_timer: Timer,
    pub swing: Swing,
    pub triple: Triple,
    pub nuclear: Nuclear,
    pub voice_timer: Timer,
    pub step_counter: i32,
    /// Rising-edge memories for voices 33 and 30.
    pub prev_objects_left: bool,
    pub prev_fly_dead: bool,
}

impl Farmer {
    /// `FUN_0043c5f2`: x = rand % 800, y = 351, voice timer 10000 ms. The rising-edge memories
    /// start false; the original's start value is not in the docs.
    pub fn new(level: i32, cx: &mut Ctx) -> Self {
        Self {
            level,
            max_speed_x: max_speed_x(level),
            cooldown_limit: cooldown_limit(level),
            x: cx.rng.r(SCREEN_W),
            y: Y_TOP,
            vx: 0,
            vy: 0,
            reach: 0.0,
            ignore_scroll_once: false,
            midges_on: false,
            slowed: false,
            slow_speed: 0,
            slow_timer: Timer::UNARMED,
            swing: Swing::default(),
            triple: Triple::default(),
            nuclear: Nuclear::default(),
            voice_timer: Timer::armed(cx.now, VOICE_FIRST_MS),
            step_counter: 0,
            prev_objects_left: false,
            prev_fly_dead: false,
        }
    }

    /// The hit box (x, y, 130, 154) in screen space.
    pub fn hit_box(&self) -> Rect {
        Rect::new(self.x, self.y, HITBOX_W, HITBOX_H)
    }

    /// `FUN_0043cea5` with the four direction flags (shared by the human and the AI).
    pub fn move_by(&mut self, pad: &FarmerPad, camera_delta: f64) {
        let _ = (pad, camera_delta);
        todo!("W2 ai-and-rules §1.2")
    }

    /// `FUN_0043d1a0`/`FUN_0043d510`: the swing machine, then the hit test on active ticks.
    pub fn swing_and_hit(
        &mut self,
        edge: bool,
        fly_box: Rect,
        stats: &mut Stats,
        cx: &mut Ctx,
    ) -> SwatHit {
        let _ = (edge, fly_box, stats, cx);
        todo!("W2 ai-and-rules §1.3, §1.4; stats swing counter on frame 1")
    }

    /// Triple goodie (`FUN_0043c8e2`): Nuclear off; on, or refresh the burst counter.
    pub fn start_triple(&mut self) {
        todo!("W2 ai-and-rules §1.6")
    }

    /// Nuclear (`FUN_0043cb71`); None is the goodie's −1 (5000 ms).
    pub fn start_nuclear(&mut self, ms: Option<u64>, cx: &mut Ctx) {
        let _ = (ms, cx);
        todo!("W2 ai-and-rules §1.7")
    }

    pub fn end_nuclear(&mut self) {
        todo!("W2 ai-and-rules §1.7 step 3")
    }

    /// `FUN_0043cd23`: the midges slowdown pulse.
    pub fn midges_pulse(&mut self, stuck: i32, total: i32, cx: &mut Ctx) {
        let _ = (stuck, total, cx);
        todo!("W2 ai-and-rules §6.3, fly-and-world §8.2")
    }

    /// `FUN_0043c730`: voices 31/32/33/30 and footsteps.
    pub fn voices(&mut self, objects_left: bool, fly_dead: bool, cx: &mut Ctx) {
        let _ = (objects_left, fly_dead, cx);
        todo!("W2 ai-and-rules §1.5")
    }
}

impl World {
    /// Step 2: the swing key counts on a press edge (memory < 0 before, key ≥ 1 now).
    pub fn farmer_swing_edge(&mut self, pad: &FarmerPad) -> bool {
        let _ = pad;
        todo!("W2 fly-and-world §2 step 2, ai-and-rules §1.3 edge input")
    }

    /// Step 6: human move then hit test, or the AI (which moves and swings itself).
    pub fn farmer_step(
        &mut self,
        pad: &FarmerPad,
        swing_edge: bool,
        m: &mut MatchState,
        cx: &mut Ctx,
    ) -> SwatHit {
        let _ = (pad, swing_edge, m, cx);
        todo!("W2 fly-and-world §2 step 6, ai-and-rules §1.2–§1.4, §2.4")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::rng::MsvcRand;

    /// ai-and-rules §1.1: level 2 gives 770 px/s and a 25-tick cooldown; seed 1 puts x at
    /// 41 % 800 = 41; y starts at the far edge, where the reach is 0.
    #[test]
    fn init_follows_the_constants_table() {
        let (mut rng, data, mut events) = (MsvcRand::new(1), GameData::synthetic(), Vec::new());
        let mut cx = Ctx::new(0, &mut rng, &data, &mut events);
        let farmer = Farmer::new(2, &mut cx);
        assert_eq!((farmer.max_speed_x, farmer.cooldown_limit), (770, 25));
        assert_eq!((farmer.x, farmer.y, farmer.reach), (41, 351, 0.0));
        assert!(farmer.voice_timer.expired(VOICE_FIRST_MS) && !farmer.voice_timer.expired(9999));
    }
}
