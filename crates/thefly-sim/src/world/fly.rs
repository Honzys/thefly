//! W1. The fly: init, physics, update, damage, death, respawn, Turbo and SuperFly
//! (fly-and-world §3). Ticks steps 1, 5, 8, 9, 11 and 18b.

use super::collision::Rect;
use super::{SCREEN_H, SCREEN_W, SwatHit, World};
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::data::Size;
use crate::input::FlyPad;
use crate::match_state::MatchState;

/// Turbo length when the goodie passes −1 (§3.8).
pub const TURBO_MS: u64 = 1500;
/// Turbo from a mystery catch; cut at [`super::goodies::MYSTERY_MS`] by step 15.
pub const MYSTERY_TURBO_MS: u64 = 25_000;
pub const SUPERFLY_MS: u64 = 8000;
/// Dying: crash animation done → this timer → kill recorded and respawn requested (§3.6 D).
pub const RESPAWN_DELAY_MS: u64 = 1500;
/// Respawn placement re-rolls (§3.7); [`RESPAWN_TRIES_PERIOD_START`] at period start.
pub const RESPAWN_TRIES: u32 = 1000;
pub const RESPAWN_TRIES_PERIOD_START: u32 = 3000;
/// The ricochet counter's end value: 23 deceleration ticks after the kick sets it to 1.
pub const RICOCHET_END: i32 = 25;
/// Friction per tick toward 0 when no key on that axis is held.
pub const FRICTION: i32 = 9;
/// A damage total above this kills; at or below it stuns.
pub const DEATH_TOTAL: i32 = 100;
/// The grab animation's loop counter limit before turning out (§3.5).
pub const GRAB_LOOP_TICKS: i32 = 50;
/// The single-precision 0.1 the original multiplies by (fly-and-world §1: 0.100000001490116).
pub const TENTH_F32: f64 = 0.1_f32 as f64;

/// Heading, from the keys held (§3.5). Visual only, except that FACE gates the idle jitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Heading {
    #[default]
    Face,
    E,
    W,
    N,
    S,
    NE,
    SE,
    NW,
    SW,
}

/// How the swatter's knockback applies on one axis (ai-and-rules §1.4 hints 0/1/2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KickDir {
    /// 0: v −= kick.
    #[default]
    Minus,
    /// 1: v += kick.
    Plus,
    /// 2: a sign roll picks the sign.
    Random,
}

/// `FUN_0043885d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    Alive,
    DyingOrAppearing,
    Splatted,
}

/// Animation phase of a heading or grab strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Turn {
    #[default]
    In,
    Loop,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Turbo {
    pub active: bool,
    /// After the timer: boosts shrink by 5% of the base maxima per tick.
    pub decaying: bool,
    pub timer: Timer,
    /// 16-bit fields in the original.
    pub boost_x: i16,
    pub boost_y: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fly {
    /// Frame size, 120 × 94 [I, from the `Face` asset].
    pub size: Size,
    /// Screen position of the frame's top-left: 0 ≤ x ≤ 800 − w, 0 ≤ y ≤ 600 − h.
    pub x: i32,
    pub y: i32,
    /// Saved before each move; restored by the object separation (§4.4).
    pub prev_x: i32,
    pub prev_y: i32,
    /// Integer px/s.
    pub vx: i32,
    pub vy: i32,
    /// 550 + 60 × speed level.
    pub max_x: i32,
    pub max_y: i32,
    /// Camera scroll speed from the previous tick's edge scroll, px/s.
    pub scroll: i32,
    pub turbo: Turbo,
    pub superfly: bool,
    pub superfly_timer: Timer,
    /// 0.2 × strength level, a float.
    pub strength_f: f32,
    pub dying: bool,
    pub stunned: bool,
    pub splatted: bool,
    pub hit_in_progress: bool,
    /// Stun or dying phase, 0 or 1.
    pub phase: u8,
    /// 0..=3 during the stun.
    pub hit_frame: i32,
    pub damage_total: i32,
    pub hits_this_life: i32,
    /// damage / 50 of the hit in progress.
    pub hit_k: i32,
    /// The swatter's direction hints for the hit in progress.
    pub hit_dir: (KickDir, KickDir),
    /// Heading saved at the hit, restored when the stun ends.
    pub saved_heading: Heading,
    /// fly+0x314, set when a stun ends or by fan damage at or below the death total.
    pub post_stun: bool,
    pub ricochet: i32,
    pub kick_x: i32,
    pub kick_y: i32,
    pub aspirating: bool,
    pub prev_action: bool,
    pub carrying: bool,
    pub grab_lock: bool,
    pub grab_frame: i32,
    pub grab_turn: Turn,
    pub grab_counter: i32,
    pub heading: Heading,
    pub heading_target: Heading,
    pub heading_frame: i32,
    pub heading_turn: Turn,
    pub drop_pop: bool,
    pub respawn_request: bool,
    pub appearing: bool,
    pub respawn_timer: Timer,
    /// Port: the crash animation's end, which the original detected in render.
    pub crash_timer: Timer,
    /// Port: the appear animation's end (render-driven in the original, §2).
    pub appear_timer: Timer,
    /// Port: the splat particles' end (render-driven; duration is an open item).
    pub splat_timer: Timer,
    /// Survival statistic: when this life started, ms.
    pub life_started_ms: Option<u64>,
}

impl Fly {
    /// `FUN_00436df6(strength, speed)` (§3.3), alive at once. Draws one rand for x.
    pub fn new(strength: i32, speed: i32, size: Size, cx: &mut Ctx) -> Self {
        let max = 550 + 60 * speed;
        Self {
            size,
            x: cx.rng.r(SCREEN_W - 2 * size.w) + size.w,
            y: (SCREEN_H - size.h) / 2,
            prev_x: 0,
            prev_y: 0,
            vx: 0,
            vy: 0,
            max_x: max,
            max_y: max,
            scroll: 0,
            turbo: Turbo::default(),
            superfly: false,
            superfly_timer: Timer::UNARMED,
            strength_f: strength as f32 * 0.2,
            dying: false,
            stunned: false,
            splatted: false,
            hit_in_progress: false,
            phase: 0,
            hit_frame: 0,
            damage_total: 0,
            hits_this_life: 0,
            hit_k: 0,
            hit_dir: (KickDir::Minus, KickDir::Minus),
            saved_heading: Heading::Face,
            post_stun: false,
            ricochet: 0,
            kick_x: 0,
            kick_y: 0,
            aspirating: false,
            prev_action: false,
            carrying: false,
            grab_lock: false,
            grab_frame: 0,
            grab_turn: Turn::In,
            grab_counter: 0,
            heading: Heading::Face,
            heading_target: Heading::Face,
            heading_frame: 0,
            heading_turn: Turn::In,
            drop_pop: false,
            respawn_request: false,
            appearing: false,
            respawn_timer: Timer::UNARMED,
            crash_timer: Timer::UNARMED,
            appear_timer: Timer::UNARMED,
            splat_timer: Timer::UNARMED,
            life_started_ms: Some(cx.now),
        }
    }

    pub fn life(&self) -> Life {
        todo!("W1 fly-and-world §3.7 FUN_0043885d")
    }

    pub fn alive(&self) -> bool {
        self.life() == Life::Alive
    }

    /// Centre in screen pixels (the swatter hit test's reference, ai-and-rules §1.4).
    pub fn centre(&self) -> (i32, i32) {
        (self.x + self.size.w / 2, self.y + self.size.h / 2)
    }

    /// (x + 50, y + 40, w − 100, h − 80): respawn overlap and flying-goodie catches.
    pub fn inner_box(&self) -> Rect {
        Rect::new(
            self.x + 50,
            self.y + 40,
            self.size.w - 100,
            self.size.h - 80,
        )
    }

    /// (x + 30, y + 25, w − 30, h − 25): the boot contact box (step 10).
    pub fn boot_box(&self) -> Rect {
        Rect::new(self.x + 30, self.y + 25, self.size.w - 30, self.size.h - 25)
    }

    /// `FUN_00436fc1(ms)`; None is the goodie's −1 (1500 ms). Plays slot 35.
    pub fn start_turbo(&mut self, ms: Option<u64>, cx: &mut Ctx) {
        let _ = (ms, cx);
        todo!("W1 fly-and-world §3.8")
    }

    /// Turbo off at once (death, splat, mystery expiry with flag 2).
    pub fn end_turbo(&mut self) {
        todo!("W1 fly-and-world §3.6 A4, §3.8")
    }

    /// `FUN_0043728f`: 8000 ms, looping slot 36.
    pub fn start_superfly(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W1 fly-and-world §3.8")
    }

    /// `FUN_004397c0`, from the boot or a fan: instant death into particles. `dir` is the fan's
    /// push direction (0 for the boot).
    pub fn splat(&mut self, dir: i32, cx: &mut Ctx) {
        let _ = (dir, cx);
        todo!("W1 fly-and-world §3.6 Splat")
    }

    /// `FUN_00439f29`, the zapper's grill: instant death. Open item: whether it takes the dying
    /// path (crash, 1500 ms) or the splat path.
    pub fn grill(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W1 game-spec §4.6 zapper, open item")
    }

    /// Damage added straight to the total, bypassing the stun (fan §7.4, zapper arc 1/tick).
    /// At or below 100 sets the post-stun flag; above it, a fan splats (`dir`) and an arc kills.
    pub fn add_raw_damage(&mut self, damage: i32, source: RawDamage, cx: &mut Ctx) {
        let _ = (damage, source, cx);
        todo!("W1 fly-and-world §7.4, game-spec §4.6")
    }

    /// `FUN_0043734c` mode 2: place the fly against the boot on `side1`, cap y at 600 − h, and
    /// splat it when pushed down below y 249 (§7.3).
    pub fn push_by_boot(&mut self, side1: u8, boot: Rect, cx: &mut Ctx) {
        let _ = (side1, boot, cx);
        todo!("W1 fly-and-world §7.3")
    }

    /// `FUN_004385c6`: respawn request with a fresh x and the fixed y (§3.7.1).
    pub fn request_respawn(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W1 fly-and-world §3.7")
    }

    /// `FUN_00438643(appear)`: clear hit, stun, damage, splat, grab, carrying, Turbo, SuperFly.
    pub fn reset(&mut self, appear: bool, cx: &mut Ctx) {
        let _ = (appear, cx);
        todo!("W1 fly-and-world §3.7.3")
    }
}

/// Who adds raw damage to the fly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawDamage {
    /// A fan, with its push direction for the splat particles.
    Fan {
        dir: i32,
    },
    ZapperArc,
}

impl World {
    /// Step 1. Returns the action flag; updates [`World::fly_action_memory`].
    pub fn fly_action_edge(&mut self, pad: &FlyPad) -> bool {
        let _ = pad;
        todo!("W1 fly-and-world §2 step 1")
    }

    /// Step 5 (`FUN_00437547`), skipped while appearing. Writes the camera delta.
    pub fn fly_physics(&mut self, pad: &FlyPad, cx: &mut Ctx) {
        let _ = (pad, cx);
        todo!("W1 fly-and-world §3.4")
    }

    /// Step 8 (`FUN_00433ddd`): death timer, damage machine, grab toggle and animations.
    pub fn fly_update(
        &mut self,
        pad: &FlyPad,
        hit: SwatHit,
        action: bool,
        m: &mut MatchState,
        cx: &mut Ctx,
    ) {
        let _ = (pad, hit, action, m, cx);
        todo!("W1 fly-and-world §3.5, §3.6; stats via MatchState")
    }

    /// Step 9: re-roll while overlapping an object (≤ [`RESPAWN_TRIES`]), then appear.
    pub fn respawn_placement(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W1 fly-and-world §2 step 9, §4.5")
    }

    /// Step 11: fly not alive → both goodie selectors 0, inverted controls off.
    pub fn fly_not_alive_resets(&mut self) {
        todo!("W1 fly-and-world §2 step 11")
    }

    /// Step 18b: appear done → alive (`FUN_004387d5`, slot 38, survival timer); splat done →
    /// kill recorded and respawn requested.
    pub fn fly_render_driven(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let _ = (m, cx);
        todo!("W1 fly-and-world §2 render-driven state, §3.6 Splat, §3.7.4")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::rng::MsvcRand;

    /// fly-and-world §3.3 with seed 1 (first draw 41 % 560 = 41) and the 120 × 94 Face frame.
    #[test]
    fn init_places_the_fly_from_the_first_draw() {
        let (mut rng, data, mut events) = (MsvcRand::new(1), GameData::synthetic(), Vec::new());
        let mut cx = Ctx::new(1000, &mut rng, &data, &mut events);
        let fly = Fly::new(0, 0, data.size("Face"), &mut cx);
        assert_eq!((fly.x, fly.y), (41 + 120, (600 - 94) / 2));
        assert_eq!((fly.max_x, fly.max_y, fly.strength_f), (550, 550, 0.0));
        assert_eq!(fly.life_started_ms, Some(1000));
        let fast = Fly::new(3, 5, data.size("Face"), &mut cx);
        assert_eq!((fast.max_x, fast.max_y), (850, 850));
        assert_eq!(fast.strength_f, 0.6);
    }
}
