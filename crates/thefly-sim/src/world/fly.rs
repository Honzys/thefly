//! W1. The fly: init, physics, update, damage, death, respawn, Turbo and SuperFly
//! (fly-and-world §3). Ticks steps 1, 5, 8, 9, 11 and 18b.

use super::collision::Rect;
use super::goodies::Mystery;
use super::{SCREEN_H, SCREEN_W, SwatHit, World};
use crate::clock::{DT, FPS, Timer, first_tick_at};
use crate::ctx::Ctx;
use crate::data::Size;
use crate::event::{Effect, Event, Pan, PlayParams, SoundOp, SoundSlot};
use crate::input::FlyPad;
use crate::match_state::MatchState;

/// Turbo length when the goodie passes −1 (§3.8).
pub const TURBO_MS: u64 = 1500;
/// Turbo from a mystery catch; cut at [`super::goodies::MYSTERY_MS`] by step 15.
pub const MYSTERY_TURBO_MS: u64 = 25_000;
pub const SUPERFLY_MS: u64 = 8000;
/// SuperFly's sparkle timer (+0x9a50) and the sparkles per firing and at the start.
const SUPERFLY_SPARKLE_MS: u64 = 40;
const SUPERFLY_SPARKLES: u32 = 5;
/// reset(0)'s forced burst, `FUN_00439450(0, 45)`.
const RESET_SPARKLES: u32 = 45;
/// The sparkle pool's slots (+0x18f0) and the cap on one request (answers.md "SuperFly
/// sparkles: call site, lifetime and pool").
pub const SPARKLE_SLOTS: usize = 90;
const SPARKLE_REQUEST_CAP: u32 = 89;
/// A sparkle's alpha starts at 255 and falls by trunc(255 / (fps × 200 / 1000.0)) per render
/// (`FUN_0041787c`); below 1 the slot is free again.
const SPARKLE_ALPHA: i32 = 255;
const SPARKLE_FADE: i32 = (SPARKLE_ALPHA as f64 / (FPS as f64 * 200.0 / 1000.0)) as i32;
/// A sparkle's rolls: animation delay R(60) + 20, position inside the fly less 20 px, speed
/// R(160) per axis, each with an R(10) sign roll (−1 below 5).
const SPARKLE_DELAY_ROLL: i32 = 60;
const SPARKLE_DELAY_BASE: i32 = 20;
const SPARKLE_SIGN_ROLL: i32 = 10;
const SPARKLE_SIGN_BELOW: i32 = 5;
const SPARKLE_INSET: i32 = 20;
const SPARKLE_SPEED_ROLL: i32 = 160;
/// Dying: crash animation done → this timer → kill recorded and respawn requested (§3.6 D).
pub const RESPAWN_DELAY_MS: u64 = 1500;
/// Respawn placement re-rolls (§3.7); [`RESPAWN_TRIES_PERIOD_START`] at period start.
pub const RESPAWN_TRIES: u32 = 1000;
pub const RESPAWN_TRIES_PERIOD_START: u32 = 3000;
/// The ricochet counter's end value: 23 deceleration ticks after the kick sets it to 1.
pub const RICOCHET_END: i32 = 25;
/// Friction per tick toward 0 when no key on that axis is held.
pub const FRICTION: i32 = 9;
/// The appear animation: scale tween 3.0 to 0.1 over 500 ms. The fly is alive when the scale
/// tween ends, tick 33 at 65 Hz (gaps §2.3, §11.7 [I]); the alpha tween would end at tick 37.
pub const APPEAR_MS: u64 = 500;
/// [`APPEAR_MS`] in ticks: 33. The tween steps once per tick, so the port counts ticks.
pub const APPEAR_TICKS: u32 = first_tick_at(APPEAR_MS) as u32;
/// The crash sprite's scale tween 0.3 → 1.0 (answers.md "Crash animation length"). It steps in
/// the render pass and ends on the frame the scale passes 1.0.
/// shortcut: the exact end tick is [I] (answers.md "Crash animation length"); the port ends it on
/// the first fly update after 200 ms. Revisit if a capture times the respawn.
pub const CRASH_MS: u64 = 200;
/// The splat's blood particles and body pieces (gaps §2.1).
pub const SPLAT_BLOOD: usize = 60;
pub const SPLAT_PIECES: usize = 11;
/// A zapper death adds 40 fading blood particles to the 60 (gaps §2.1).
pub const ZAPPER_EXTRA_BLOOD: usize = 40;
/// DirectSound volume passed for slot 29 before the crash plays; out of range (FW §9).
const CRASH_VOLUME: i32 = 5000;
/// `FUN_0040d005(x, vx, vy, w, −700)` (answers.md "Fly buzz: pitch, pan and volume"): frequency
/// 10000 + R(1000) + vy × 1500 / 700 + |vx| × 2500 / 700; a pan of ±2000 at the screen edges;
/// volume −700 less the px off screen.
const BUZZ_FREQ_BASE: i32 = 10_000;
const BUZZ_FREQ_ROLL: i32 = 1000;
const BUZZ_VY_HZ: i32 = 1500;
const BUZZ_VX_HZ: i32 = 2500;
const BUZZ_SPEED_DIV: i32 = 700;
const BUZZ_VOLUME_CB: i32 = -700;
const BUZZ_PAN_CB: f64 = 2000.0;
/// The pan's centre, half the 800 px screen.
const BUZZ_PAN_MID: i32 = SCREEN_W / 2;
/// The hit frame the stun turns around at (§3.6 B).
const HIT_FRAME_TOP: i32 = 3;
/// Grab and heading strips: frames 0..=9, the loop runs 4..=9 (§3.5).
const TURN_IN_END: i32 = 4;
/// A steady heading facing the opposite key turns out from this frame.
const TURN_OUT_FRAME: i32 = 3;
const LOOP_LAST: i32 = 9;
/// Frames of the FACE idle cycle.
const FACE_FRAMES: i32 = 10;
/// Frames of the `blood` sheet, 3 × 2 (screens-and-rendering §4.6); the frame roll is R(n − 3).
const BLOOD_FRAMES: i32 = 6;
/// Fade length roll of each blood particle: R(100) + 850 ms (gaps §2.2), re-rolled for the 60
/// common ones when the crash respawn timer expires.
const BLOOD_FADE_ROLL: i32 = 100;
/// The zapper extras' acceleration 13 − k + R(k), k = max(1, trunc(13 / 2.0)) = 6.
const ZAPPER_ACCEL_ROLL: i32 = 6;
/// `FUN_0043a158`: the crash's blood spray (answers.md "Crash animation length").
pub const CRASH_SPRAY: usize = 60;
/// A zapper splat ends when its 100 blood particles have faded: 64 ticks (gaps §2.2). Boot and
/// fan splats end when 11 body pieces are below [`SPLAT_FLOOR_Y`]; simulate the pieces.
pub const ZAPPER_SPLAT_TICKS: u32 = 64;
pub const SPLAT_FLOOR_Y: i32 = 600;
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
    pub superfly_sparkle_timer: Timer,
    /// The sparkle pool's alphas, 0 for a free slot.
    pub sparkle_alpha: [i32; SPARKLE_SLOTS],
    /// +0x18e8: the render fades the sparkles only while this is set.
    pub sparkles_visible: bool,
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
    /// fly+0x314, set when a stun ends or by fan or arc damage at or below the death total; the
    /// render counts the hit and clears it.
    pub post_stun: bool,
    /// fly+0x2c8: the effective damage of the latest hit, which the render adds to the stats.
    pub stat_damage: i32,
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
    /// The direction flags E, W, N, S and FACE (+0x1ec..+0x1fc) as one heading.
    pub heading: Heading,
    pub heading_frame: i32,
    pub heading_turn: Turn,
    pub drop_pop: bool,
    pub respawn_request: bool,
    pub appearing: bool,
    pub respawn_timer: Timer,
    /// Port: the crash animation's end, which the original detected in render.
    pub crash_timer: Timer,
    /// Port: ticks of the appear tween so far (render-driven in the original, §2).
    pub appear_ticks: u32,
    /// The splat's 11 body pieces; the splat ends when all are at y ≥ 600 (gaps §2.2).
    pub pieces: [SplatPiece; SPLAT_PIECES],
    pub splat_gravity: i32,
    /// fly+0x2a8: a zapper death, which ends after [`ZAPPER_SPLAT_TICKS`] instead.
    pub zapper_splat: bool,
    pub splat_ticks: u32,
    /// Port: slot 7 is looping (so the next alive tick moves it instead of starting it).
    pub buzzing: bool,
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
            superfly_sparkle_timer: Timer::UNARMED,
            sparkle_alpha: [0; SPARKLE_SLOTS],
            sparkles_visible: false,
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
            stat_damage: 0,
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
            heading_frame: 0,
            heading_turn: Turn::In,
            drop_pop: false,
            respawn_request: false,
            appearing: false,
            respawn_timer: Timer::UNARMED,
            crash_timer: Timer::UNARMED,
            appear_ticks: 0,
            pieces: [SplatPiece::default(); SPLAT_PIECES],
            splat_gravity: 0,
            zapper_splat: false,
            splat_ticks: 0,
            buzzing: false,
            life_started_ms: Some(cx.now),
        }
    }

    pub fn life(&self) -> Life {
        if self.splatted {
            Life::Splatted
        } else if self.dying || self.appearing {
            Life::DyingOrAppearing
        } else {
            Life::Alive
        }
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
        self.turbo.timer.arm(cx.now, ms.unwrap_or(TURBO_MS));
        self.turbo.active = true;
        self.turbo.decaying = false;
        let boost = (f64::from(self.max_x) * 0.5) as i16;
        self.turbo.boost_x = boost;
        self.turbo.boost_y = boost;
        let away = |v: i32, b: i16| {
            if v >= 0 {
                v + i32::from(b)
            } else {
                v - i32::from(b)
            }
        };
        if self.vx.abs() < self.vy.abs() {
            self.vy = away(self.vy, boost);
        } else {
            self.vx = away(self.vx, boost);
        }
        cx.emit(Event::play(SoundSlot::TURBO));
    }

    /// Turbo off at once (death, splat, mystery expiry with flag 2).
    pub fn end_turbo(&mut self) {
        self.turbo = Turbo::default();
    }

    /// `FUN_0043728f`: 5 sparkles, looping slot 36, 8000 ms and the 40 ms sparkle timer
    /// (answers.md "Fly reset(0) and SuperFly sparkles").
    pub fn start_superfly(&mut self, cx: &mut Ctx) {
        self.sparkles_visible = true;
        self.sparkles(SUPERFLY_SPARKLES, cx);
        self.superfly = true;
        self.superfly_timer.arm(cx.now, SUPERFLY_MS);
        self.superfly_sparkle_timer.arm(cx.now, SUPERFLY_SPARKLE_MS);
        cx.sound(SoundSlot::SUPERFLY, SoundOp::Loop(PlayParams::default()));
    }

    /// `FUN_00437302`, while SuperFly is on: the 8000 ms expiry ends it with no sparkles that
    /// tick; otherwise 5 sparkles per firing of the 40 ms timer (answers.md "SuperFly sparkles:
    /// call site, lifetime and pool").
    fn superfly_tick(&mut self, cx: &mut Ctx) {
        if self.superfly_timer.expired(cx.now) {
            self.end_superfly(cx);
            return;
        }
        if self.superfly_sparkle_timer.expired_rearm(cx.now) {
            self.sparkles(SUPERFLY_SPARKLES, cx);
        }
    }

    /// `FUN_00439450`: up to `count` (at most 89) sparkles into free slots; busy slots are
    /// skipped without a draw, so a full pool draws nothing.
    fn sparkles(&mut self, count: u32, cx: &mut Ctx) {
        let (x, y, size) = (self.x, self.y, self.size);
        let free = self.sparkle_alpha.iter_mut().filter(|a| **a == 0);
        for alpha in free.take(count.min(SPARKLE_REQUEST_CAP) as usize) {
            *alpha = SPARKLE_ALPHA;
            let effect = sparkle(x, y, size, cx);
            cx.emit(Event::Effect(effect));
        }
    }

    /// `FUN_0043964c`, from the fly render while +0x18e8 is set: each sparkle fades, and with
    /// none left the flag clears. The render draws no rand.
    // shortcut: the quit-dialog pause skips the world tick (screen.rs), so the sparkles do not
    // fade through it and answers.md's stuck-slot case after a pause cannot arise. Revisit if the
    // paused match gets a render step.
    fn render_sparkles(&mut self) {
        if !self.sparkles_visible {
            return;
        }
        for alpha in self.sparkle_alpha.iter_mut() {
            *alpha = (*alpha - SPARKLE_FADE).max(0);
        }
        self.sparkles_visible = self.sparkle_alpha.iter().any(|&a| a > 0);
    }

    fn end_superfly(&mut self, cx: &mut Ctx) {
        self.superfly = false;
        self.superfly_timer.disarm();
        cx.sound(SoundSlot::SUPERFLY, SoundOp::Stop);
    }

    /// `FUN_004397c0`, from the boot or a fan: instant death into particles. `dir` is the fan's
    /// push direction: 0 for the boot, > 0 for a fan pushing right (fan+0x918 == 1, pieces fly
    /// right), < 0 for a fan pushing left (gaps §2.1 burst table). Ignored when already splatted.
    pub fn splat(&mut self, dir: i32, cx: &mut Ctx) {
        if self.splatted {
            return;
        }
        let burst = match dir {
            0 => BOOT_BURST,
            d if d > 0 => FAN_RIGHT_BURST,
            _ => FAN_LEFT_BURST,
        };
        self.die_into_pieces(&burst, dir, cx);
    }

    /// The zapper's grill: the burst, then sound 43 (answers.md "Zapper arc on the fly").
    pub fn grill(&mut self, cx: &mut Ctx) {
        self.zapper_burst(cx);
        cx.emit(Event::play(SoundSlot::ZAPPER_GRILL));
    }

    /// `FUN_00439f29(50, 80, 50, 100, 1, 1, 1, 13)` (answers.md "Zapper arc on the fly"): +0x2a8,
    /// then 40 extra blood particles, each with an R(6) acceleration roll, then the common
    /// splat. Their fades were drawn at construction, so none are drawn here. It runs even on a
    /// splatted fly: the zapper does not re-test its entry gate between the grill and the arc.
    fn zapper_burst(&mut self, cx: &mut Ctx) {
        self.zapper_splat = true;
        for _ in 0..ZAPPER_EXTRA_BLOOD {
            self.throw(&ZAPPER_BURST, Throw::Blood, cx);
            cx.rng.r(ZAPPER_ACCEL_ROLL);
        }
        self.die_into_pieces(&ZAPPER_BURST, 0, cx);
    }

    /// The common splat (FW §3.6 Splat, gaps §2.1): state, then 60 blood particles and 11 body
    /// pieces in rand order, then sound 46.
    fn die_into_pieces(&mut self, burst: &Burst, dir: i32, cx: &mut Ctx) {
        self.splatted = true;
        self.hits_this_life += 1;
        (self.vx, self.vy) = (0, 0);
        self.clear_grab();
        self.end_turbo();
        if self.superfly {
            self.end_superfly(cx);
        }
        for _ in 0..SPLAT_BLOOD {
            self.throw(burst, Throw::Blood, cx);
        }
        self.pieces = std::array::from_fn(|_| self.throw(burst, Throw::Piece, cx));
        self.splat_gravity = burst.g;
        self.splat_ticks = 0;
        cx.emit(Event::play(SoundSlot::SPLASH));
        cx.emit(Event::Effect(Effect::Splat {
            x: self.x,
            y: self.y,
            dir,
        }));
    }

    /// One particle launch (gaps §2.1): x, y, the blood frame roll, then a sign roll and a speed
    /// per axis. A negative range relies on C's `rand % n` with n < 0 being `rand % |n|`.
    fn throw(&self, b: &Burst, kind: Throw, cx: &mut Ctx) -> SplatPiece {
        let x = self.x + cx.rng.r(self.size.w - 20);
        let y = self.y + cx.rng.r(self.size.h - 20);
        let neg_x_below = match kind {
            Throw::Blood => {
                cx.rng.r(BLOOD_FRAMES - 3);
                6
            }
            Throw::Piece => 4,
        };
        let sx = if cx.rng.r(10) < neg_x_below && b.sign_x {
            -1
        } else {
            1
        };
        let vx = (cx.rng.r(b.range_x + 1) + b.min_x) * sx;
        let sy = if cx.rng.r(10) < 4 && b.sign_y { -1 } else { 1 };
        let vy = (cx.rng.r(b.range_y + 1) + b.min_y) * sy;
        SplatPiece { x, y, vx, vy }
    }

    fn clear_grab(&mut self) {
        self.aspirating = false;
        self.carrying = false;
        self.grab_lock = false;
        self.grab_counter = 0;
        self.grab_frame = 0;
        self.grab_turn = Turn::In;
    }

    /// One tick of the body pieces (gaps §2.1): v += g, y += sign(v) × floor(|v| / fps).
    /// True once the splat has ended (gaps §2.2).
    fn advance_splat(&mut self) -> bool {
        self.splat_ticks += 1;
        for p in &mut self.pieces {
            p.vy += self.splat_gravity;
            p.y += p.vy.signum() * (p.vy.abs() / FPS);
        }
        if self.zapper_splat {
            return self.splat_ticks >= ZAPPER_SPLAT_TICKS;
        }
        self.pieces.iter().all(|p| p.y >= SPLAT_FLOOR_Y)
    }

    /// Damage added straight to the total, bypassing the stun (fan §7.4, zapper arc 1 per arc).
    /// At or below 100 sets the post-stun flag; above it, a fan splats (`dir`) and an arc runs
    /// the zapper burst without sound 43 (answers.md "Zapper arc on the fly"). The caller has
    /// already applied the SuperFly reduction (the fan's R(10) + 20). Both callers gate on the
    /// fly's state; the arc's gate is not re-tested after a same-tick grill.
    pub fn add_raw_damage(&mut self, damage: i32, source: RawDamage, cx: &mut Ctx) {
        self.damage_total += damage;
        self.stat_damage = damage;
        if self.damage_total <= DEATH_TOTAL {
            self.post_stun = true;
            return;
        }
        match source {
            RawDamage::Fan { dir } => self.splat(dir, cx),
            RawDamage::ZapperArc => self.zapper_burst(cx),
        }
    }

    /// `FUN_0043734c` mode 2: place the fly against the boot on `side1`, then on the tie's
    /// `side2`, cap y at 600 − h, and splat it when pushed down below y 249 on side1 2 (§7.3).
    pub fn push_by_boot(&mut self, side1: u8, side2: Option<u8>, boot: Rect, cx: &mut Ctx) {
        self.place_against(side1, boot);
        if let Some(side2) = side2 {
            self.place_against(side2, boot);
        }
        self.y = self.y.min(SCREEN_H - self.size.h);
        if self.y > BOOT_SPLAT_Y && side1 == 2 {
            self.splat(0, cx);
        }
    }

    /// The §7.3 placement table: side 0 top, 1 left, 2 below, 3 right.
    fn place_against(&mut self, side: u8, boot: Rect) {
        let (w, h) = (self.size.w, self.size.h);
        match side {
            0 => self.y = boot.y - (h - 20),
            1 => self.x = boot.x - (w - 25),
            2 => self.y = boot.y + boot.h - 20,
            _ => self.x = boot.x + boot.w - 25,
        }
    }

    /// `FUN_004385c6`: respawn request with a fresh x and the fixed y (§3.7.1).
    pub fn request_respawn(&mut self, cx: &mut Ctx) {
        self.respawn_request = true;
        (self.vx, self.vy) = (0, 0);
        self.reroll_position(cx);
    }

    /// The position part of `FUN_004385c6`, for the placement re-rolls of step 9.
    fn reroll_position(&mut self, cx: &mut Ctx) {
        self.x = cx.rng.r(SCREEN_W - self.size.w);
        self.y = (f64::from(SCREEN_H - self.size.h) / 2.0 - 20.0) as i32;
    }

    /// `FUN_00438643(appear)`: clear hit, stun, damage, splat, grab, carrying, Turbo, SuperFly, and
    /// the crash and respawn timers (a fan can splat a dying fly). Argument 0 is the period start:
    /// `FUN_004387d5` at once.
    pub fn reset(&mut self, appear: bool, cx: &mut Ctx) {
        self.hit_in_progress = false;
        self.crash_timer.disarm();
        self.respawn_timer.disarm();
        self.hit_frame = 0;
        self.stunned = false;
        self.ricochet = 0;
        self.damage_total = 0;
        self.splatted = false;
        self.zapper_splat = false;
        self.clear_grab();
        self.end_turbo();
        if self.superfly {
            self.end_superfly(cx);
        }
        if !appear {
            self.become_alive(cx);
            return;
        }
        self.appearing = true;
        self.appear_ticks = 0;
        let (x, y) = self.centre();
        cx.emit(Event::Effect(Effect::Stardust { x, y }));
    }

    /// `FUN_0040bc5e` with this life's hits, and the survival stop when the kill is recorded
    /// (answers.md "Fly survival stat: the period end also stops it").
    fn record_death(&mut self, m: &mut MatchState, cx: &Ctx) {
        m.record_kill(self.hits_this_life);
        self.hits_this_life = 0;
        self.stop_survival(m, cx.now);
    }

    /// `FUN_0040db52`: the life in progress into the survival total and maximum. Called at the
    /// kill record and at the period end, the world destructor `FUN_00433904`.
    pub fn stop_survival(&mut self, m: &mut MatchState, now: u64) {
        if let Some(start) = self.life_started_ms.take() {
            m.stats.record_life(now.saturating_sub(start));
        }
    }

    /// The fly render `FUN_00439156` counts a hit once +0x314 is set and the fly is not dying, so
    /// a fatal hit never counts (answers.md "Fly hits and damage stats: counted in the render,
    /// fatal hits excluded").
    fn count_hit(&mut self, m: &mut MatchState) {
        if !self.post_stun || self.dying {
            return;
        }
        self.post_stun = false;
        m.stats.hits += 1;
        m.stats.damage += self.stat_damage;
    }

    /// `FUN_004387d5`, at reset(0) and the appear end: the flags cleared, the render flag set, 45
    /// sparkles, sound 38, the survival start (answers.md "SuperFly sparkles: call site, lifetime
    /// and pool", reset(0) order corrected).
    fn become_alive(&mut self, cx: &mut Ctx) {
        self.phase = 0;
        self.dying = false;
        self.splatted = false;
        self.appearing = false;
        self.sparkles_visible = true;
        self.sparkles(RESET_SPARKLES, cx);
        cx.emit(Event::play(SoundSlot::GOODIE_FLY));
        self.life_started_ms = Some(cx.now);
    }
}

/// One sparkle's 7 draws (`FUN_00439450`, answers.md "Fly reset(0) and SuperFly sparkles").
fn sparkle(x: i32, y: i32, size: Size, cx: &mut Ctx) -> Effect {
    let delay_ms = cx.rng.r(SPARKLE_DELAY_ROLL) + SPARKLE_DELAY_BASE;
    let sign_x = sparkle_sign(cx);
    let x = cx.rng.r(size.w - SPARKLE_INSET) + x;
    let y = cx.rng.r(size.h - SPARKLE_INSET) + y;
    let vx = cx.rng.r(SPARKLE_SPEED_ROLL) * sign_x;
    let sign_y = sparkle_sign(cx);
    let vy = cx.rng.r(SPARKLE_SPEED_ROLL) * sign_y;
    Effect::Sparkles {
        x,
        y,
        vx,
        vy,
        delay_ms,
    }
}

fn sparkle_sign(cx: &mut Ctx) -> i32 {
    if cx.rng.r(SPARKLE_SIGN_ROLL) < SPARKLE_SIGN_BELOW {
        -1
    } else {
        1
    }
}

/// Physics (§3.4) and update (§3.5, §3.6).
impl Fly {
    /// §3.4 steps 1–11; returns the camera delta of step 11.
    fn physics(&mut self, pad: &FlyPad, cx: &mut Ctx) -> f64 {
        if self.superfly {
            self.superfly_tick(cx);
        }
        self.turbo_tick(cx.now);
        let mx = self.max_x + i32::from(self.turbo.boost_x);
        let my = self.max_y + i32::from(self.turbo.boost_y);
        if !self.dying && self.ricochet == 0 {
            self.steer(pad, mx, my);
        }
        if self.ricochet > 0 {
            self.ricochet_tick(pad);
        }
        let (jx, jy) = self.jitter(cx);
        (self.prev_x, self.prev_y) = (self.x, self.y);
        self.x += (f64::from(self.vx - self.scroll) * DT) as i32 + jx;
        self.y += (f64::from(self.vy) * DT) as i32 + jy;
        self.x = self.x.clamp(0, SCREEN_W - self.size.w);
        self.y = self.y.clamp(0, SCREEN_H - self.size.h);
        self.buzz(cx);
        self.edge_scroll(mx)
    }

    /// §3.4 step 2: the expiry tick only switches to the decay.
    fn turbo_tick(&mut self, now: u64) {
        if !self.turbo.active {
            return;
        }
        if !self.turbo.decaying {
            self.turbo.decaying = self.turbo.timer.expired(now);
            return;
        }
        let decay = |b: i16, max: i32| ((f64::from(b) - f64::from(max) * 0.05) as i16).max(0);
        self.turbo.boost_x = decay(self.turbo.boost_x, self.max_x);
        self.turbo.boost_y = decay(self.turbo.boost_y, self.max_y);
        if self.turbo.boost_x == 0 && self.turbo.boost_y == 0 {
            self.end_turbo();
        }
    }

    /// §3.4 step 4.
    fn climb(&self, my: i32) -> i32 {
        let tenth = f64::from(my) * TENTH_F32;
        if !self.carrying {
            return tenth as i32;
        }
        let bonus = if self.superfly { 0.5 } else { 0.0 };
        let c = (f64::from(self.strength_f) + bonus).min(1.0);
        (75.0 * c + tenth / 1.6) as i32
    }

    /// §3.4 step 5.
    fn steer(&mut self, pad: &FlyPad, mx: i32, my: i32) {
        let tenth = |m: i32| f64::from(m) * TENTH_F32;
        let climb = self.climb(my);
        if pad.right {
            self.vx = ((f64::from(self.vx) + tenth(mx)) as i32).min(mx);
        }
        if pad.left {
            self.vx = ((f64::from(self.vx) - tenth(mx)) as i32).max(-mx);
        }
        if pad.up {
            self.vy = (self.vy - climb).max(-my);
        }
        if pad.down {
            self.vy = ((f64::from(self.vy) + tenth(my)) as i32).min(my);
        }
        if std::mem::take(&mut self.drop_pop) {
            self.vy = -(self.vy.abs() + 300);
        }
        if !pad.left && !pad.right {
            self.vx = toward_zero(self.vx, FRICTION);
        }
        if !pad.up && !pad.down {
            self.vy = toward_zero(self.vy, FRICTION);
        }
    }

    /// §3.4 step 6.
    fn ricochet_tick(&mut self, pad: &FlyPad) {
        let dx = if pad.left || pad.right {
            (f64::from(self.kick_x) / 37.5) as i32
        } else {
            self.kick_x / 25
        };
        let dy = self.kick_y / 25;
        self.ricochet += 1;
        if self.ricochet < RICOCHET_END {
            self.vx = toward_zero(self.vx, dx);
            self.vy = toward_zero(self.vy, dy);
            return;
        }
        self.ricochet = 0;
        let (w, half) = (self.size.w, self.size.w / 2);
        if self.x + w > SCREEN_W - half {
            self.x = SCREEN_W - half - w;
        } else if self.x < half {
            self.x = half;
        }
    }

    /// §3.4 step 7: U, sign, U, sign.
    fn jitter(&self, cx: &mut Ctx) -> (i32, i32) {
        if self.heading != Heading::Face || self.dying || self.ricochet != 0 || self.carrying {
            return (0, 0);
        }
        let mut axis = || (cx.rng.u() * 2.0) as i32 * cx.rng.sign();
        let jx = axis();
        (jx, axis())
    }

    /// §3.4 step 10, `FUN_0040d005`: the rand first, then pan, frequency and volume; the loop
    /// starts whenever the slot is not playing. The volume only changes off screen, which the
    /// clamped fly never is, so it is set at the loop start.
    fn buzz(&mut self, cx: &mut Ctx) {
        if !self.alive() {
            if std::mem::take(&mut self.buzzing) {
                cx.sound(SoundSlot::FLY_BUZZ, SoundOp::Stop);
            }
            return;
        }
        let r = cx.rng.r(BUZZ_FREQ_ROLL);
        let hz = BUZZ_FREQ_BASE
            + r
            + self.vy * BUZZ_VY_HZ / BUZZ_SPEED_DIV
            + self.vx.abs() * BUZZ_VX_HZ / BUZZ_SPEED_DIV;
        let freq = u32::try_from(hz).unwrap_or(0);
        let pan = Pan::Fixed(self.buzz_pan());
        if std::mem::replace(&mut self.buzzing, true) {
            cx.sound(SoundSlot::FLY_BUZZ, SoundOp::Move(pan));
            cx.sound(SoundSlot::FLY_BUZZ, SoundOp::SetFrequency(freq));
            return;
        }
        let params = PlayParams {
            volume_cb: BUZZ_VOLUME_CB - offscreen_px(self.x),
            freq_hz: Some(freq),
            pan,
        };
        cx.sound(SoundSlot::FLY_BUZZ, SoundOp::Loop(params));
    }

    /// Left of 400 − w: −trunc((1 − x / (400 − w)) × 2000); from 400: trunc((x − 400) / 400 ×
    /// 2000); 0 between.
    fn buzz_pan(&self) -> i32 {
        let left = BUZZ_PAN_MID - self.size.w;
        let (x, mid) = (f64::from(self.x), f64::from(BUZZ_PAN_MID));
        if self.x <= left {
            -((1.0 - x / f64::from(left)) * BUZZ_PAN_CB) as i32
        } else if self.x >= BUZZ_PAN_MID {
            ((x - mid) / mid * BUZZ_PAN_CB) as i32
        } else {
            0
        }
    }

    /// §3.4 step 11: assigns `scroll` and returns this tick's camera delta.
    fn edge_scroll(&mut self, mx: i32) -> f64 {
        let (w, half) = (self.size.w, self.size.w / 2);
        let o = if self.x + w >= SCREEN_W - w {
            self.x + w - (SCREEN_W - w)
        } else if self.x <= w {
            self.x - w
        } else {
            0
        };
        self.scroll = mx * o / half;
        f64::from(mx) * f64::from(o) * DT / f64::from(half)
    }

    /// §3.5 (`FUN_00433ddd`).
    fn update(
        &mut self,
        pad: &FlyPad,
        hit: SwatHit,
        action: bool,
        m: &mut MatchState,
        cx: &mut Ctx,
    ) {
        if self.splatted || self.appearing {
            return;
        }
        if self.dying && self.respawn_timer.is_armed() {
            if self.respawn_timer.expired(cx.now) {
                self.record_death(m, cx);
                self.request_respawn(cx);
                self.respawn_timer.disarm();
                for _ in 0..SPLAT_BLOOD {
                    cx.rng.r(BLOOD_FADE_ROLL);
                }
            }
            return;
        }
        if hit.damage >= 1 || self.dying || self.stunned {
            self.damage(hit, cx);
            if self.dying {
                self.aspirating = false;
            }
            return;
        }
        self.grab_toggle(action, cx);
        if self.grab_lock {
            self.grab_step();
        } else {
            self.heading_step(pad);
        }
    }

    /// §3.6 (`FUN_00437e99`). A new hit (A) and the stun (B) are exclusive branches, so the hit
    /// frame stays 0 on the hit tick (answers.md "Fly hit: a new hit does not run B in the same
    /// call").
    fn damage(&mut self, hit: SwatHit, cx: &mut Ctx) {
        if hit.damage >= 1 && !self.hit_in_progress {
            self.new_hit(hit, cx);
            return;
        }
        if self.stunned && self.phase == 0 {
            self.stun_in(cx);
        } else if self.stunned {
            self.stun_out();
        } else if self.dying && self.phase == 0 && self.crash_timer.expired(cx.now) {
            self.hit_frame = HIT_FRAME_TOP;
            (self.vx, self.vy) = (0, 0);
            self.phase = 1;
            self.respawn_timer.arm(cx.now, RESPAWN_DELAY_MS);
            self.crash_timer.disarm();
        }
    }

    /// §3.6 A. The effective (SuperFly-cut) damage is kept for the render's stats.
    fn new_hit(&mut self, hit: SwatHit, cx: &mut Ctx) {
        self.saved_heading = self.heading;
        self.hits_this_life += 1;
        let effective = if self.superfly {
            (f64::from(hit.damage) / 2.5) as i32
        } else {
            hit.damage
        };
        self.damage_total += effective;
        self.stat_damage = effective;
        if self.damage_total <= DEATH_TOTAL {
            self.stunned = true;
        } else {
            self.start_dying(cx);
        }
        self.hit_k = hit.damage / 50;
        self.hit_frame = 0;
        self.hit_in_progress = true;
        self.hit_dir = (hit.dir_x, hit.dir_y);
    }

    /// §3.6 A step 4. `FUN_0043a158` is a blood spray, not the crash motion (answers.md "Crash
    /// animation length"): per particle one discarded rand (the 200 ms scale-in), then a blood
    /// launch at speeds 0..850. The frontend draws both from [`Effect::Crash`].
    fn start_dying(&mut self, cx: &mut Ctx) {
        self.dying = true;
        cx.emit(Event::Effect(Effect::Crash {
            x: self.x,
            y: self.y,
        }));
        self.clear_grab();
        self.end_turbo();
        self.end_superfly(cx);
        for _ in 0..CRASH_SPRAY {
            cx.rng.rand();
            self.throw(&CRASH_SPRAY_BURST, Throw::Blood, cx);
        }
        cx.sound(SoundSlot::FLY_CRASH, SoundOp::SetVolume(CRASH_VOLUME));
        cx.emit(Event::play(SoundSlot::FLY_CRASH));
        self.crash_timer.arm(cx.now, CRASH_MS);
    }

    /// §3.6 B: the frame climbs to 3, then the kick unless SuperFly is on.
    fn stun_in(&mut self, cx: &mut Ctx) {
        self.hit_frame += 1;
        if self.hit_frame <= HIT_FRAME_TOP {
            return;
        }
        self.hit_frame = HIT_FRAME_TOP;
        self.phase = 1;
        if self.superfly {
            return;
        }
        self.vx = kicked(self.vx, self.hit_k, self.hit_dir.0, cx);
        self.vy = kicked(self.vy, self.hit_k, self.hit_dir.1, cx);
        (self.kick_x, self.kick_y) = (self.vx.abs(), self.vy.abs());
        self.ricochet = 1;
        cx.emit(Event::play(SoundSlot::FLY_RICOCHET));
    }

    /// §3.6 C.
    fn stun_out(&mut self) {
        self.hit_frame -= 1;
        if self.hit_frame >= 0 {
            return;
        }
        self.hit_frame = 0;
        self.stunned = false;
        self.hit_in_progress = false;
        self.heading = self.saved_heading;
        self.post_stun = true;
        self.phase = 0;
    }

    /// §3.5 normal path: the grab toggle on a fresh action, then the lock.
    fn grab_toggle(&mut self, action: bool, cx: &mut Ctx) {
        if action && !self.prev_action {
            if !self.aspirating {
                cx.emit(Event::play(SoundSlot::ASPIRATE));
                self.aspirating = true;
            } else if self.carrying {
                self.aspirating = false;
            }
        }
        self.prev_action = action;
        if self.aspirating && !self.grab_lock && !self.carrying {
            self.grab_lock = true;
        }
    }

    /// §3.5 grab animation: turn in to frame 4, loop 4..=9 for 51 ticks, turn out.
    fn grab_step(&mut self) {
        match self.grab_turn {
            Turn::In => {
                self.grab_frame += 1;
                if self.grab_frame > TURN_IN_END {
                    self.grab_frame = TURN_IN_END;
                    self.grab_turn = Turn::Loop;
                }
            }
            Turn::Loop => {
                self.grab_frame = loop_frame(self.grab_frame);
                self.grab_counter += 1;
                if self.grab_counter > GRAB_LOOP_TICKS {
                    self.grab_turn = Turn::Out;
                }
            }
            Turn::Out => {
                self.grab_frame -= 1;
                if self.grab_frame >= 0 {
                    return;
                }
                self.heading = Heading::Face;
                self.grab_counter = 0;
                self.grab_lock = false;
                self.grab_frame = 0;
                self.grab_turn = Turn::In;
                self.aspirating &= self.carrying;
            }
        }
    }

    /// §3.5 heading animation at one frame per tick (answers.md "Fly heading machine: mid-turn
    /// reversals and opposite keys"). Visual, except that FACE gates the jitter. Key patterns
    /// other than none, one key or one diagonal change nothing.
    fn heading_step(&mut self, pad: &FlyPad) {
        let Some(t) = Heading::from_pad(pad) else {
            return;
        };
        if t == Heading::Face {
            self.heading_release();
        } else if self.heading == Heading::Face {
            (self.heading, self.heading_frame, self.heading_turn) = (t, 0, Turn::In);
        } else if self.heading_turn == Turn::Loop {
            self.heading_steady(t);
        } else {
            self.heading_turning(t);
        }
    }

    /// No key: FACE cycles its frames; a heading turns out, onto FACE below frame 0.
    fn heading_release(&mut self) {
        if self.heading == Heading::Face {
            self.heading_frame = (self.heading_frame + 1) % FACE_FRAMES;
            return;
        }
        self.heading_turn = Turn::Out;
        self.heading_frame -= 1;
        if self.heading_frame < 0 {
            (self.heading, self.heading_frame, self.heading_turn) = (Heading::Face, 0, Turn::Loop);
        }
    }

    /// Steady on a heading with `t` held. An opposite flag turns out from frame 3. A single key
    /// with a perpendicular flag set switches to `t` at frame 4. A diagonal from a pure
    /// horizontal heading keeps it (E held with Right+Up stays E); otherwise `t` loops.
    fn heading_steady(&mut self, t: Heading) {
        if self.heading.opposes(t) {
            (self.heading_frame, self.heading_turn) = (TURN_OUT_FRAME, Turn::Out);
            return;
        }
        if t.is_diagonal() {
            if self.heading.contains(t.vertical()) {
                self.heading = t;
            }
            self.heading_frame = loop_frame(self.heading_frame);
        } else if self.heading != t {
            (self.heading, self.heading_frame) = (t, TURN_IN_END);
        } else {
            self.heading_frame = loop_frame(self.heading_frame);
        }
    }

    /// Mid-turn with `t` held: toward frame 4 while every flag of `t` is set, else turn out and,
    /// below frame 0, start the turn-in toward `t`.
    fn heading_turning(&mut self, t: Heading) {
        if !self.heading.contains(t) {
            self.heading_turn = Turn::Out;
            self.heading_frame -= 1;
            if self.heading_frame < 0 {
                (self.heading, self.heading_frame, self.heading_turn) = (t, 0, Turn::In);
            }
        } else if self.heading_turn == Turn::Out {
            self.heading_turn = Turn::In;
            self.heading_frame = (self.heading_frame + 1).min(TURN_IN_END);
        } else if self.heading_frame + 1 > TURN_IN_END {
            (self.heading_frame, self.heading_turn) = (TURN_IN_END, Turn::Loop);
        } else {
            self.heading_frame += 1;
        }
    }
}

impl Heading {
    /// The exact key patterns the heading machine reads: none, one key, or one diagonal.
    /// Opposite keys or three and more keys are None.
    fn from_pad(pad: &FlyPad) -> Option<Self> {
        Some(match (pad.right, pad.left, pad.up, pad.down) {
            (false, false, false, false) => Heading::Face,
            (true, false, false, false) => Heading::E,
            (false, true, false, false) => Heading::W,
            (false, false, true, false) => Heading::N,
            (false, false, false, true) => Heading::S,
            (true, false, true, false) => Heading::NE,
            (true, false, false, true) => Heading::SE,
            (false, true, true, false) => Heading::NW,
            (false, true, false, true) => Heading::SW,
            _ => return None,
        })
    }

    /// Direction flags [E, W, N, S]; a flag's opposite is at index ^ 1.
    fn flags(self) -> [bool; 4] {
        match self {
            Heading::Face => [false; 4],
            Heading::E => [true, false, false, false],
            Heading::W => [false, true, false, false],
            Heading::N => [false, false, true, false],
            Heading::S => [false, false, false, true],
            Heading::NE => [true, false, true, false],
            Heading::SE => [true, false, false, true],
            Heading::NW => [false, true, true, false],
            Heading::SW => [false, true, false, true],
        }
    }

    /// Every flag of `t` is set here.
    fn contains(self, t: Heading) -> bool {
        self.flags().iter().zip(t.flags()).all(|(h, t)| *h || !t)
    }

    /// A flag opposite one of `t`'s is set here.
    fn opposes(self, t: Heading) -> bool {
        let h = self.flags();
        t.flags().iter().enumerate().any(|(d, on)| *on && h[d ^ 1])
    }

    fn is_diagonal(self) -> bool {
        matches!(self, Heading::NE | Heading::SE | Heading::NW | Heading::SW)
    }

    /// The vertical part of a heading (FACE for E, W and FACE).
    fn vertical(self) -> Heading {
        match self {
            Heading::N | Heading::NE | Heading::NW => Heading::N,
            Heading::S | Heading::SE | Heading::SW => Heading::S,
            _ => Heading::Face,
        }
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

/// The buzz volume's off-screen part: |x| left of 0, x − 800 right of the screen.
fn offscreen_px(x: i32) -> i32 {
    if x < 0 { -x } else { (x - SCREEN_W).max(0) }
}

/// The loop frames 4..=9: one step, past 9 wraps to 4.
fn loop_frame(f: i32) -> i32 {
    if f + 1 > LOOP_LAST {
        TURN_IN_END
    } else {
        f + 1
    }
}

/// §3.6 B steps 1–2 for one axis: U first, then the sign roll for direction code 2.
fn kicked(v: i32, k: i32, dir: KickDir, cx: &mut Ctx) -> i32 {
    let r = cx.rng.u();
    let kick = if v == 0 {
        (r * 500.0) as i32
    } else {
        (f64::from(v.abs()) * f64::from(k) * r + 800.0) as i32
    };
    match dir {
        KickDir::Minus => v - kick,
        KickDir::Plus => v + kick,
        KickDir::Random => v + cx.rng.sign() * kick,
    }
}

/// Splat bursts (gaps §2.1): `FUN_004397c0(minX, minY, rangeX, rangeY, signX, signY, –, g)`.
#[derive(Debug, Clone, Copy)]
struct Burst {
    min_x: i32,
    min_y: i32,
    range_x: i32,
    range_y: i32,
    sign_x: bool,
    sign_y: bool,
    g: i32,
}

const BOOT_BURST: Burst = Burst {
    min_x: 0,
    min_y: 0,
    range_x: 280,
    range_y: 200,
    sign_x: true,
    sign_y: false,
    g: 12,
};
const FAN_RIGHT_BURST: Burst = Burst {
    min_x: 550,
    min_y: -550,
    range_x: 400,
    range_y: -400,
    sign_x: false,
    sign_y: false,
    g: 20,
};
const FAN_LEFT_BURST: Burst = Burst {
    min_x: -850,
    min_y: -550,
    range_x: -700,
    range_y: -400,
    sign_x: false,
    sign_y: false,
    g: 20,
};
const ZAPPER_BURST: Burst = Burst {
    min_x: 50,
    min_y: 80,
    range_x: 50,
    range_y: 100,
    sign_x: true,
    sign_y: true,
    g: 13,
};

/// The crash spray: vx = R(851) × sign (+ fly vx, presentation only), vy = R(851) × sign.
const CRASH_SPRAY_BURST: Burst = Burst {
    min_x: 0,
    min_y: 0,
    range_x: 850,
    range_y: 850,
    sign_x: true,
    sign_y: true,
    g: 0,
};

/// The boot splats a fly pushed down below this y (FW §7.3).
const BOOT_SPLAT_Y: i32 = 249;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Throw {
    Blood,
    Piece,
}

/// A splat body piece (gaps §2.1), screen pixels and px/s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplatPiece {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
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
        if !pad.action {
            self.fly_action_memory = -1;
        }
        pad.action && self.fly_action_memory < 0
    }

    /// Step 5 (`FUN_00437547`), skipped while appearing. Writes the camera delta.
    /// The delta stays as it was on skipped ticks; the layers scroll by it every tick
    /// (gaps §4.3).
    pub fn fly_physics(&mut self, pad: &FlyPad, cx: &mut Ctx) {
        let pad = if self.goodies.fly_mystery == Mystery::First {
            pad.inverted()
        } else {
            *pad
        };
        if !self.fly.appearing {
            self.camera.delta = self.fly.physics(&pad, cx);
        }
        self.camera.scroll();
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
        self.fly.update(pad, hit, action, m, cx);
    }

    /// Step 9: re-roll while overlapping an object (≤ [`RESPAWN_TRIES`]), then appear.
    pub fn respawn_placement(&mut self, cx: &mut Ctx) {
        self.place_fly(RESPAWN_TRIES, true, cx);
    }

    /// Step 9 with a try cap: [`RESPAWN_TRIES`], or [`RESPAWN_TRIES_PERIOD_START`] from
    /// [`World::new`]. Reads and clears the request, re-rolls while the inner box (§4.5) touches
    /// an object, then resets: the appear, or at period start reset(0).
    pub(super) fn place_fly(&mut self, tries: u32, appear: bool, cx: &mut Ctx) {
        if !std::mem::take(&mut self.fly.respawn_request) {
            return;
        }
        for _ in 0..tries {
            if !self.objects.box_hit(&self.fly.inner_box()) {
                break;
            }
            self.fly.reroll_position(cx);
        }
        self.fly.reset(appear, cx);
    }

    /// Step 11: fly not alive → both goodie selectors 0, inverted controls off.
    pub fn fly_not_alive_resets(&mut self) {
        if self.fly.alive() {
            return;
        }
        self.goodies.fly_selector = 0;
        self.goodies.farmer_selector = 0;
        self.goodies.fly_mystery = Mystery::None;
    }

    /// Step 18b: the hits and damage stats; appear done → alive (`FUN_004387d5`, slot 38,
    /// survival timer); splat done → kill recorded and respawn requested.
    pub fn fly_render_driven(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let fly = &mut self.fly;
        // shortcut: the sparkle fade's place in the fly render `FUN_004388ca` relative to the
        // appear end is not in answers.md; first here. It shifts a slot's release by one tick only.
        fly.render_sparkles();
        fly.count_hit(m);
        if fly.appearing {
            if fly.appear_ticks >= APPEAR_TICKS {
                fly.become_alive(cx);
            } else {
                fly.appear_ticks += 1;
            }
            return;
        }
        if !fly.splatted || fly.respawn_request || !fly.advance_splat() {
            return;
        }
        fly.record_death(m, cx);
        fly.request_respawn(cx);
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

    /// A fresh fly from seed 1 plus a helper to run one call with a context.
    struct Bench {
        rng: MsvcRand,
        data: GameData,
        events: Vec<Event>,
        now: u64,
    }

    impl Bench {
        fn new(seed: u32) -> Self {
            Self {
                rng: MsvcRand::new(seed),
                data: GameData::synthetic(),
                events: Vec::new(),
                now: 0,
            }
        }

        fn with<T>(&mut self, f: impl FnOnce(&mut Ctx) -> T) -> T {
            let mut cx = Ctx::new(self.now, &mut self.rng, &self.data, &mut self.events);
            f(&mut cx)
        }

        fn fly(&mut self) -> Fly {
            self.with(|cx| Fly::new(0, 0, cx.data.size("Face"), cx))
        }

        fn sounds(&self, slot: SoundSlot) -> Vec<SoundOp> {
            self.events
                .iter()
                .filter_map(|e| match e {
                    Event::Sound { slot: s, op } if *s == slot => Some(*op),
                    _ => None,
                })
                .collect()
        }

        /// How many rand() calls the stream is past a fresh stream of the same seed.
        fn draws_since(&self, seed: u32, from: u32) -> u32 {
            let mut r = MsvcRand::new(seed);
            for n in 0..10_000 {
                if r.state() == self.rng.state() && n >= from {
                    return n - from;
                }
                r.rand();
            }
            panic!("rng not reached within 10000 draws");
        }
    }

    /// How many rand() calls `after` is past `before`.
    fn draws_between(before: &MsvcRand, after: &MsvcRand) -> u32 {
        let mut r = before.clone();
        for n in 0..10_000 {
            if r == *after {
                return n;
            }
            r.rand();
        }
        panic!("rng not reached within 10000 draws");
    }

    #[test]
    fn turbo_boosts_by_half_max_x_and_kicks_the_dominant_axis() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        (fly.vx, fly.vy) = (100, -300);
        b.with(|cx| fly.start_turbo(None, cx));
        // FW §3.8: 550 × 0.5 = 275 on both; |vx| < |vy| so vy, which is negative, gets −275.
        assert_eq!((fly.turbo.boost_x, fly.turbo.boost_y), (275, 275));
        assert_eq!((fly.vx, fly.vy), (100, -575));
        assert!(fly.turbo.active && !fly.turbo.decaying);
        assert!(!fly.turbo.timer.expired(TURBO_MS) && fly.turbo.timer.expired(TURBO_MS + 1));
        assert_eq!(b.sounds(SoundSlot::TURBO).len(), 1);

        (fly.vx, fly.vy) = (-200, 200);
        b.with(|cx| fly.start_turbo(Some(MYSTERY_TURBO_MS), cx));
        assert_eq!((fly.vx, fly.vy), (-475, 200));
        assert!(
            fly.turbo.timer.expired(MYSTERY_TURBO_MS + 1)
                && !fly.turbo.timer.expired(MYSTERY_TURBO_MS)
        );
        fly.end_turbo();
        assert_eq!(fly.turbo, Turbo::default());
    }

    #[test]
    fn superfly_loops_slot_36_for_8000_ms() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        b.with(|cx| fly.start_superfly(cx));
        assert!(fly.superfly && fly.superfly_timer.expired(8001));
        assert!(!fly.superfly_timer.expired(8000));
        assert!(matches!(
            b.sounds(SoundSlot::SUPERFLY)[..],
            [SoundOp::Loop(_)]
        ));
    }

    #[test]
    fn a_boot_splat_draws_60_blood_and_11_pieces_once() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        (fly.x, fly.y, fly.vx, fly.vy) = (300, 400, 120, -80);
        (fly.aspirating, fly.carrying) = (true, true);
        b.with(|cx| fly.start_superfly(cx));
        b.with(|cx| fly.splat(0, cx));
        // gaps §2.1: 7 draws per blood particle, 6 per body piece, after Fly::new's one draw and
        // SuperFly's 5 start sparkles of 7.
        assert_eq!(b.draws_since(1, 1 + 5 * 7), 60 * 7 + 11 * 6);
        assert_eq!(fly.life(), Life::Splatted);
        assert_eq!((fly.vx, fly.vy, fly.hits_this_life), (0, 0, 1));
        assert!(!fly.carrying && !fly.aspirating && !fly.superfly && !fly.turbo.active);
        assert_eq!(b.sounds(SoundSlot::SPLASH).len(), 1);
        assert_eq!(b.sounds(SoundSlot::SUPERFLY).last(), Some(&SoundOp::Stop));
        for p in fly.pieces {
            assert!((300..300 + 100).contains(&p.x) && (400..400 + 74).contains(&p.y));
            assert!((-280..=280).contains(&p.vx) && (0..=200).contains(&p.vy));
        }
        assert_eq!(fly.splat_gravity, 12);
        // Already splatted: nothing more happens.
        b.with(|cx| fly.splat(0, cx));
        assert_eq!(b.draws_since(1, 1 + 5 * 7), 60 * 7 + 11 * 6);
        assert_eq!(fly.hits_this_life, 1);
    }

    #[test]
    fn fan_splats_throw_the_pieces_with_the_push() {
        for (dir, vx) in [(1, 550..=950), (-1, -850..=-152)] {
            let mut b = Bench::new(7);
            let mut fly = b.fly();
            b.with(|cx| fly.splat(dir, cx));
            assert!(fly.pieces.iter().all(|p| vx.contains(&p.vx)), "dir {dir}");
            assert!(fly.pieces.iter().all(|p| (-550..=-152).contains(&p.vy)));
            assert_eq!(fly.splat_gravity, 20);
        }
    }

    /// answers.md "Zapper arc on the fly": the burst throws 40 extras of 8 draws (7 + the R(6)
    /// acceleration), then the common splat (60 × 7 + 11 × 6) and its sound 46; no fade draws.
    /// The grill adds sound 43 after the burst.
    #[test]
    fn the_grill_bursts_40_extras_then_the_common_splat_then_sound_43() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        b.with(|cx| fly.grill(cx));
        assert!(fly.zapper_splat && fly.splatted);
        assert_eq!(b.draws_since(1, 1), 40 * 8 + 60 * 7 + 11 * 6);
        let order: Vec<SoundSlot> = b
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Sound { slot, .. } => Some(*slot),
                _ => None,
            })
            .collect();
        assert_eq!(order, [SoundSlot::SPLASH, SoundSlot::ZAPPER_GRILL]);
    }

    /// An arc that takes the total past 100 runs the same burst, without sound 43.
    #[test]
    fn an_arc_death_is_the_burst_without_sound_43() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        fly.damage_total = 100;
        b.with(|cx| fly.add_raw_damage(1, RawDamage::ZapperArc, cx));
        assert!(fly.splatted && fly.zapper_splat);
        assert_eq!(b.draws_since(1, 1), 40 * 8 + 60 * 7 + 11 * 6);
        assert_eq!(b.sounds(SoundSlot::SPLASH).len(), 1);
        assert!(b.sounds(SoundSlot::ZAPPER_GRILL).is_empty());
    }

    /// The zapper's entry gate is not re-tested between the grill and the arc test, so a fly
    /// grilled at a total of 100 that takes the same tick's arc bursts a second time.
    #[test]
    fn a_grilled_fly_hit_by_the_same_ticks_arc_bursts_again() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        fly.damage_total = 100;
        b.with(|cx| {
            fly.grill(cx);
            fly.add_raw_damage(1, RawDamage::ZapperArc, cx);
        });
        assert_eq!(b.draws_since(1, 1), 2 * (40 * 8 + 60 * 7 + 11 * 6));
        assert_eq!((fly.damage_total, fly.hits_this_life), (101, 2));
    }

    #[test]
    fn raw_damage_sets_post_stun_until_the_total_passes_100() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        b.with(|cx| fly.add_raw_damage(60, RawDamage::Fan { dir: 1 }, cx));
        assert_eq!(
            (fly.damage_total, fly.post_stun, fly.splatted),
            (60, true, false)
        );
        b.with(|cx| fly.add_raw_damage(40, RawDamage::ZapperArc, cx));
        assert!(!fly.splatted, "100 is still at or below the death total");
        b.with(|cx| fly.add_raw_damage(1, RawDamage::Fan { dir: -1 }, cx));
        assert!(fly.splatted && !fly.zapper_splat);
        assert!(fly.pieces.iter().all(|p| p.vx < 0));
        let before = b.draws_since(1, 0);
        b.with(|cx| fly.add_raw_damage(50, RawDamage::Fan { dir: -1 }, cx));
        assert_eq!(
            b.draws_since(1, before),
            0,
            "a splatted fly is not splatted again"
        );

        let mut arc = b.fly();
        arc.damage_total = 100;
        b.with(|cx| arc.add_raw_damage(1, RawDamage::ZapperArc, cx));
        assert!(arc.splatted && arc.zapper_splat);
    }

    #[test]
    fn the_boot_places_the_fly_by_side_and_splats_it_low() {
        let mut b = Bench::new(1);
        let boot = Rect::new(300, 200, 270, 575);
        // (side, x, y) with the fly starting at (161, 253).
        let cases = [
            (0, 161, 200 - 74),
            (1, 300 - 95, 253),
            (3, 300 + 270 - 25, 253),
        ];
        for (side, x, y) in cases {
            let mut fly = b.fly();
            (fly.x, fly.y) = (161, 253);
            b.with(|cx| fly.push_by_boot(side, None, boot, cx));
            assert_eq!((fly.x, fly.y), (x, y), "side {side}");
            assert!(!fly.splatted);
        }
        // Side 2: y = 200 + 575 − 20, capped at 600 − 94 = 506 > 249, so splat.
        let mut fly = b.fly();
        b.with(|cx| fly.push_by_boot(2, None, boot, cx));
        assert_eq!(fly.y, 506);
        assert!(fly.splatted);
        // Side 2 but high: y = 0 + 150 − 20 = 130, no splat.
        let mut high = b.fly();
        b.with(|cx| high.push_by_boot(2, None, Rect::new(300, 0, 270, 150), cx));
        assert_eq!(high.y, 130);
        assert!(!high.splatted);
    }

    /// FW §7.3 on a tie: side1 (left) places x = 300 − 95, side2 (below) y = 200 + 575 − 20,
    /// capped at 506; above 249 but side1 is not 2, so no splat.
    #[test]
    fn a_tie_places_both_sides_and_caps_y() {
        let mut b = Bench::new(1);
        let boot = Rect::new(300, 200, 270, 575);
        let mut fly = b.fly();
        b.with(|cx| fly.push_by_boot(1, Some(2), boot, cx));
        assert_eq!((fly.x, fly.y), (300 - 95, 506));
        assert!(!fly.splatted);
        let mut top = b.fly();
        b.with(|cx| top.push_by_boot(3, Some(0), boot, cx));
        assert_eq!((top.x, top.y), (300 + 270 - 25, 200 - 74));
    }

    #[test]
    fn a_respawn_request_draws_x_and_sets_the_fixed_y() {
        let mut b = Bench::new(1);
        let mut fly = b.with(|cx| Fly::new(0, 0, cx.data.size("Face"), cx));
        (fly.vx, fly.vy) = (300, 300);
        b.with(|cx| fly.request_respawn(cx));
        // Seed 1's second draw is 18467; 18467 % (800 − 120) = 107.
        assert_eq!((fly.x, fly.y), (107, (600 - 94) / 2 - 20));
        assert_eq!((fly.vx, fly.vy), (0, 0));
        assert!(fly.respawn_request);
    }

    #[test]
    fn reset_with_appear_starts_the_appear() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        b.with(|cx| fly.splat(0, cx));
        fly.damage_total = 70;
        b.with(|cx| fly.reset(true, cx));
        assert_eq!(fly.life(), Life::DyingOrAppearing);
        assert_eq!((fly.damage_total, fly.appear_ticks), (0, 0));
        assert!(
            b.events
                .iter()
                .any(|e| matches!(e, Event::Effect(Effect::Stardust { .. })))
        );
    }

    /// answers.md "SuperFly sparkles: call site, lifetime and pool": argument 0 is alive at once,
    /// then the 45 sparkles of 7 draws, then sound 38 and the survival start.
    #[test]
    fn reset_without_appear_bursts_45_sparkles_then_plays_38_and_starts_survival() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        (fly.appearing, fly.dying, fly.phase) = (true, true, 1);
        fly.life_started_ms = None;
        b.now = 700;
        let before = b.draws_since(1, 0);
        b.with(|cx| fly.reset(false, cx));
        assert_eq!(b.draws_since(1, before), 45 * 7);
        assert!(fly.alive() && fly.phase == 0);
        assert_eq!(fly.life_started_ms, Some(700));
        assert_eq!(b.sounds(SoundSlot::GOODIE_FLY).len(), 1);
        let kinds: Vec<bool> = b
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Effect(Effect::Sparkles { .. }) => Some(true),
                Event::Sound { slot, .. } if *slot == SoundSlot::GOODIE_FLY => Some(false),
                _ => None,
            })
            .collect();
        assert_eq!(kinds.len(), 46);
        assert!(
            kinds[..45].iter().all(|&k| k) && !kinds[45],
            "sparkles before 38"
        );
    }

    /// The first sparkle after `Fly::new` (draw 0 = 41, so x = 161, y = 253; Face 120 × 94):
    /// delay 18467 % 60 + 20 = 67; sign 6334 % 10 = 4 < 5 → −; x 26500 % 100 + 161 = 161;
    /// y 19169 % 74 + 253 = 256; |vx| 15724 % 160 = 44; sign 11478 % 10 = 8 → +; vy 29358 % 160
    /// = 78.
    #[test]
    fn a_sparkle_rolls_delay_sign_x_y_vx_sign_vy() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        b.with(|cx| fly.reset(false, cx));
        let first = b
            .events
            .iter()
            .find(|e| matches!(e, Event::Effect(Effect::Sparkles { .. })));
        let expected = Event::Effect(Effect::Sparkles {
            x: 161,
            y: 256,
            vx: -44,
            vy: 78,
            delay_ms: 67,
        });
        assert_eq!(first, Some(&expected));
    }

    /// The pool: 90 slots, busy ones skipped without a draw, a request capped at 89. Alpha 255
    /// falls by trunc(255 / (65 × 200 / 1000)) = 19 per render, so a slot frees on the 14th
    /// (255 − 13 × 19 = 8 ≥ 1, 255 − 14 × 19 < 1).
    #[test]
    fn a_full_sparkle_pool_draws_nothing_until_the_renders_fade_it() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        let burst = |b: &mut Bench, fly: &mut Fly| {
            let before = b.draws_since(1, 0);
            b.with(|cx| fly.reset(false, cx));
            b.draws_since(1, before)
        };
        assert_eq!(burst(&mut b, &mut fly), 315);
        assert_eq!(burst(&mut b, &mut fly), 315);
        assert_eq!(burst(&mut b, &mut fly), 0, "90 busy slots");
        for _ in 0..13 {
            fly.render_sparkles();
        }
        assert_eq!(burst(&mut b, &mut fly), 0, "alpha 8 still shows");
        fly.render_sparkles();
        assert!(
            !fly.sparkles_visible,
            "no sparkle left clears the render flag"
        );
        assert_eq!(burst(&mut b, &mut fly), 315);
        for _ in 0..14 {
            fly.render_sparkles();
        }
        let before = b.draws_since(1, 0);
        b.with(|cx| fly.sparkles(100, cx));
        assert_eq!(
            b.draws_since(1, before),
            89 * 7,
            "a request is capped at 89"
        );
    }

    /// The render flag: only the SuperFly start and `FUN_004387d5` set it, so sparkles emitted
    /// once it is clear never fade and hold their slots.
    #[test]
    fn sparkles_emitted_while_the_render_flag_is_clear_never_fade() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        b.with(|cx| fly.sparkles(5, cx));
        for _ in 0..20 {
            fly.render_sparkles();
        }
        assert_eq!(fly.sparkle_alpha.iter().filter(|&&a| a > 0).count(), 5);
    }

    /// The 8000 ms expiry wins: on that tick SuperFly ends with no sparkles, though the 40 ms
    /// timer has fired too. Only the buzz's R(1000) is drawn.
    #[test]
    fn the_superfly_expiry_tick_ends_it_without_sparkles() {
        let mut b = Bench::new(1);
        let mut fly = centred(&mut b);
        b.with(|cx| fly.start_superfly(cx));
        b.now = 8001;
        let before = b.draws_since(1, 0);
        b.with(|cx| fly.physics(&NONE, cx));
        assert_eq!(b.draws_since(1, before), 1);
        assert!(!fly.superfly);
        assert_eq!(b.sounds(SoundSlot::SUPERFLY).last(), Some(&SoundOp::Stop));
    }

    /// SuperFly: 5 sparkles at the start, then 5 per firing of the 40 ms timer while it lasts.
    #[test]
    fn superfly_sparkles_5_at_the_start_and_5_every_40_ms() {
        let mut b = Bench::new(1);
        let mut fly = centred(&mut b);
        let before = b.draws_since(1, 0);
        b.with(|cx| fly.start_superfly(cx));
        assert_eq!(b.draws_since(1, before), 5 * 7);
        // Each physics call draws the buzz's R(1000); heading E skips the jitter.
        for (now, sparkles) in [(40, 0), (41, 5), (81, 0), (82, 5)] {
            b.now = now;
            let before = b.draws_since(1, 0);
            b.with(|cx| fly.physics(&NONE, cx));
            assert_eq!(b.draws_since(1, before), 1 + sparkles * 7, "at {now} ms");
        }
    }

    /// answers.md "Fly survival stat: the period end also stops it": the life in progress is
    /// added, and only once.
    #[test]
    fn the_period_end_adds_the_life_in_progress() {
        let mut b = Bench::new(1);
        let (mut m, mut w) = world(&mut b);
        w.fly.life_started_ms = Some(1000);
        w.fly.stop_survival(&mut m, 5000);
        w.fly.stop_survival(&mut m, 9000);
        assert_eq!(
            (m.stats.survival_max_ms, m.stats.survival_total_ms),
            (4000, 4000)
        );
    }

    fn fall_ticks(fly: &mut Fly, y: i32, vy: i32, g: i32) -> u32 {
        fly.pieces = [SplatPiece { x: 0, y, vx: 0, vy }; SPLAT_PIECES];
        fly.splat_gravity = g;
        fly.splat_ticks = 0;
        (1..=200).find(|_| fly.advance_splat()).unwrap_or(0)
    }

    /// gaps §2.2 fall-time table at fps 65.
    #[test]
    fn splat_end_follows_the_slowest_piece() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        assert_eq!(fall_ticks(&mut fly, 250, 0, 12), 64);
        assert_eq!(fall_ticks(&mut fly, 323, 200, 12), 43);
        assert_eq!(fall_ticks(&mut fly, 506, 0, 12), 35);
        assert_eq!(fall_ticks(&mut fly, 0, -550, 20), 96);
        assert_eq!(fall_ticks(&mut fly, 250, -550, 20), 83);
        assert_eq!(fall_ticks(&mut fly, 506, -550, 20), 64);
        fly.pieces[3].y = 0;
        fly.pieces[3].vy = -550;
        fly.splat_ticks = 0;
        assert!(!fly.advance_splat(), "one slow piece holds the splat");
        fly.zapper_splat = true;
        assert_eq!(fall_ticks(&mut fly, 0, -550, 20), ZAPPER_SPLAT_TICKS);
    }

    fn world(b: &mut Bench) -> (MatchState, World) {
        use crate::match_state::{MatchConfig, Players};
        b.with(|cx| {
            let config = MatchConfig {
                players: Players::Two,
                ..MatchConfig::default()
            };
            let m = MatchState::new(config, false, cx);
            let w = World::bare(&m, cx);
            (m, w)
        })
    }

    #[test]
    fn the_action_is_enter_held_unless_blocked_since_a_goodie_use() {
        let mut b = Bench::new(1);
        let (_, mut w) = world(&mut b);
        let enter = FlyPad {
            action: true,
            ..FlyPad::default()
        };
        assert!(w.fly_action_edge(&enter) && w.fly_action_edge(&enter));
        w.fly_action_memory = 10;
        assert!(!w.fly_action_edge(&enter));
        assert!(!w.fly_action_edge(&FlyPad::default()));
        assert_eq!(w.fly_action_memory, -1);
        assert!(w.fly_action_edge(&enter));
    }

    /// Acceptance W1 10, the W1 half: a splat in step 10 makes step 11 clear both selectors and
    /// the inverted controls on the same tick. The boot's own contact test is W2's.
    #[test]
    fn a_splat_resets_both_goodie_selectors_in_step_11() {
        let mut b = Bench::new(1);
        let (_, mut w) = world(&mut b);
        (w.goodies.fly_selector, w.goodies.farmer_selector) = (2, 3);
        w.goodies.fly_mystery = Mystery::First;
        w.fly_not_alive_resets();
        assert_eq!((w.goodies.fly_selector, w.goodies.farmer_selector), (2, 3));
        b.with(|cx| w.fly.splat(0, cx));
        w.fly_not_alive_resets();
        assert_eq!((w.goodies.fly_selector, w.goodies.farmer_selector), (0, 0));
        assert_eq!(w.goodies.fly_mystery, Mystery::None);
    }

    /// Acceptance W1 14: step 9 starts the appear on tick 0; 18b makes the fly alive on tick 33.
    #[test]
    fn the_fly_is_alive_33_ticks_after_the_appear_starts() {
        let mut b = Bench::new(1);
        let (mut m, mut w) = world(&mut b);
        b.with(|cx| w.fly.request_respawn(cx));
        b.with(|cx| w.respawn_placement(cx));
        assert!(w.fly.appearing && !w.fly.respawn_request);
        for tick in 0..=33 {
            b.now = crate::clock::ms_at(tick);
            assert!(!w.fly.alive(), "alive too early, tick {tick}");
            let before = b.rng.clone();
            b.with(|cx| w.fly_render_driven(&mut m, cx));
            // answers.md "SuperFly sparkles": the appear end bursts 45 sparkles of 7 draws.
            let draws = if tick == 33 { 45 * 7 } else { 0 };
            assert_eq!(draws_between(&before, &b.rng), draws, "tick {tick}");
        }
        assert!(w.fly.alive());
        assert_eq!(w.fly.life_started_ms, Some(crate::clock::ms_at(33)));
        assert_eq!(b.sounds(SoundSlot::GOODIE_FLY).len(), 1);
    }

    /// Acceptance W1 13: the kill is recorded on the tick the last piece lands, the respawn
    /// placement runs on the next.
    #[test]
    fn a_splat_records_the_kill_when_it_ends_and_respawns_next_tick() {
        let mut b = Bench::new(1);
        let (mut m, mut w) = world(&mut b);
        b.with(|cx| w.fly.splat(0, cx));
        w.fly.pieces = [SplatPiece {
            x: 0,
            y: 506,
            vx: 0,
            vy: 0,
        }; SPLAT_PIECES];
        for _ in 0..34 {
            b.with(|cx| w.fly_render_driven(&mut m, cx));
        }
        assert_eq!(m.stats.kills, 0);
        b.with(|cx| w.fly_render_driven(&mut m, cx));
        assert_eq!(m.stats.kills, 1);
        assert!(w.fly.respawn_request && w.fly.splatted);
        b.with(|cx| w.respawn_placement(cx));
        assert!(w.fly.appearing && !w.fly.splatted);
    }

    const RIGHT: FlyPad = FlyPad {
        up: false,
        down: false,
        left: false,
        right: true,
        action: false,
        goodie_cycle: false,
    };
    const UP: FlyPad = FlyPad {
        up: true,
        right: false,
        ..RIGHT
    };
    const NONE: FlyPad = FlyPad {
        right: false,
        ..RIGHT
    };

    /// A fly mid-screen with jitter off (heading E), so only the velocity rules act.
    fn centred(b: &mut Bench) -> Fly {
        let mut fly = b.fly();
        (fly.x, fly.y, fly.heading) = (340, 250, Heading::E);
        fly
    }

    /// Acceptance W1 2.
    #[test]
    fn steering_accelerates_by_a_tenth_and_friction_brings_vx_to_exactly_zero() {
        let mut b = Bench::new(1);
        let mut fly = centred(&mut b);
        let mut seen = Vec::new();
        for _ in 0..11 {
            b.with(|cx| fly.physics(&RIGHT, cx));
            fly.x = 340;
            seen.push(fly.vx);
        }
        // trunc(550 × 0.1f) = 55 per tick, capped at 550.
        assert_eq!(seen, [55, 110, 165, 220, 275, 330, 385, 440, 495, 550, 550]);
        let mut ticks = 0;
        while fly.vx != 0 && ticks < 100 {
            b.with(|cx| fly.physics(&NONE, cx));
            fly.x = 340;
            ticks += 1;
            assert!(fly.vx >= 0, "friction overshot");
        }
        // 550 = 61 × 9 + 1: 61 steps of 9 and a last step of 1.
        assert_eq!((fly.vx, ticks), (0, 62));

        b.with(|cx| fly.physics(&UP, cx));
        assert_eq!(fly.vy, -55);
        fly.vy = 0;
        fly.carrying = true;
        fly.strength_f = 0.6;
        b.with(|cx| fly.physics(&UP, cx));
        // trunc(75 × 0.6f + 550 × 0.1f / 1.6) = trunc(45.0000018 + 34.375) = 79.
        assert_eq!(fly.vy, -79);
    }

    /// Acceptance W1 3.
    #[test]
    fn turbo_holds_for_1500_ms_then_decays_by_five_percent_of_max() {
        let mut b = Bench::new(1);
        let mut fly = centred(&mut b);
        b.with(|cx| fly.start_turbo(None, cx));
        let expiry = crate::clock::first_tick_at(TURBO_MS);
        for tick in 1..expiry {
            b.now = crate::clock::ms_at(tick);
            b.with(|cx| fly.physics(&NONE, cx));
            assert_eq!(fly.turbo.boost_x, 275, "tick {tick}");
        }
        b.now = crate::clock::ms_at(expiry);
        b.with(|cx| fly.physics(&NONE, cx));
        assert!(fly.turbo.decaying && fly.turbo.boost_x == 275);
        let mut boosts = Vec::new();
        for _ in 0..10 {
            b.with(|cx| fly.physics(&NONE, cx));
            boosts.push(fly.turbo.boost_y);
        }
        // trunc(b − 27.5), the last one clamped from −4 to 0, which ends the turbo.
        assert_eq!(boosts, [247, 219, 191, 163, 135, 107, 79, 51, 23, 0]);
        assert!(!fly.turbo.active);
    }

    /// Acceptance W1 6.
    #[test]
    fn the_edge_scroll_sets_the_delta_for_that_tick_only() {
        let mut b = Bench::new(1);
        let mut fly = centred(&mut b);
        assert_eq!(b.with(|cx| fly.physics(&NONE, cx)), 0.0);
        assert_eq!(fly.scroll, 0);
        // x + w = 690 ≥ 800 − 120: o = 10, delta = 550 × 10 × dt / 60, scroll = 5500 / 60.
        fly.x = 570;
        let delta = b.with(|cx| fly.physics(&NONE, cx));
        assert_eq!(delta, 550.0 * 10.0 * crate::clock::DT / 60.0);
        assert_eq!(fly.scroll, 91);
        // Left edge: x = 100 ≤ w gives o = −20.
        fly.x = 100;
        fly.scroll = 0;
        let delta = b.with(|cx| fly.physics(&NONE, cx));
        assert_eq!(
            (delta, fly.scroll),
            (550.0 * -20.0 * crate::clock::DT / 60.0, -183)
        );
    }

    fn update(b: &mut Bench, m: &mut MatchState, fly: &mut Fly, damage: i32) {
        let hit = SwatHit {
            damage,
            dir_x: KickDir::Plus,
            dir_y: KickDir::Minus,
        };
        b.with(|cx| fly.update(&NONE, hit, false, m, cx));
    }

    /// Acceptance W1 4. The hit (A) and the stun (B) are exclusive branches of one call
    /// (answers.md "Fly hit: a new hit does not run B in the same call"), so the hit frame is 0
    /// on the hit tick and the kick comes on the fifth call.
    #[test]
    fn a_hit_stuns_kicks_and_ricochets_for_23_ticks() {
        let mut b = Bench::new(1);
        let (mut m, mut w) = world(&mut b);
        // Fresh stream so the kick's draws are seed 1's 2nd and 3rd values.
        b.rng = MsvcRand::new(1);
        let mut fly = centred(&mut b);
        let mut frames = Vec::new();
        update(&mut b, &mut m, &mut fly, 40);
        frames.push(fly.hit_frame);
        assert!(fly.stunned && fly.hit_in_progress && fly.alive());
        update(&mut b, &mut m, &mut fly, 40);
        frames.push(fly.hit_frame);
        for _ in 0..2 {
            update(&mut b, &mut m, &mut fly, 0);
            frames.push(fly.hit_frame);
        }
        assert_eq!(
            (fly.damage_total, fly.hits_this_life),
            (40, 1),
            "second hit ignored"
        );
        assert_eq!(fly.ricochet, 0);
        update(&mut b, &mut m, &mut fly, 0);
        frames.push(fly.hit_frame);
        assert_eq!(frames, [0, 1, 2, 3, 3]);
        // v was 0: kick = trunc(U × 500). 18467 / 32767 × 500 = 281.8 → vx += 281;
        // 6334 / 32767 × 500 = 96.7 → vy −= 96.
        assert_eq!(
            (fly.vx, fly.vy, fly.kick_x, fly.kick_y),
            (281, -96, 281, 96)
        );
        assert_eq!((fly.ricochet, fly.phase), (1, 1));
        assert_eq!(b.sounds(SoundSlot::FLY_RICOCHET).len(), 1);

        for _ in 0..23 {
            b.with(|cx| fly.physics(&NONE, cx));
            fly.x = 340;
        }
        // 23 decelerations of 281 / 25 = 11 and 96 / 25 = 3.
        assert_eq!((fly.vx, fly.vy, fly.ricochet), (281 - 253, -96 + 69, 24));
        b.with(|cx| fly.physics(&NONE, cx));
        assert_eq!((fly.vx, fly.ricochet), (28, 0));

        // Phase 1: frames 3 → 2 → 1 → 0 → below 0 ends the stun.
        for _ in 0..3 {
            update(&mut b, &mut m, &mut fly, 0);
            assert!(fly.stunned);
        }
        update(&mut b, &mut m, &mut fly, 0);
        assert!(!fly.stunned && !fly.hit_in_progress && fly.post_stun);
        assert_eq!((fly.phase, fly.hit_frame, fly.heading), (0, 0, Heading::E));

        // answers.md "Fly hits and damage stats": the render counts the hit once the stun ends.
        assert_eq!((m.stats.hits, m.stats.damage), (0, 0));
        w.fly = fly;
        for _ in 0..2 {
            b.with(|cx| w.fly_render_driven(&mut m, cx));
        }
        assert_eq!(
            (m.stats.hits, m.stats.damage, w.fly.post_stun),
            (1, 40, false)
        );
    }

    /// The render counts a non-fatal fan or arc hit with the damage passed in (SuperFly already
    /// applied), and never a fatal hit.
    #[test]
    fn raw_hits_count_in_the_render_and_fatal_hits_never() {
        let mut b = Bench::new(1);
        let (mut m, mut w) = world(&mut b);
        b.with(|cx| w.fly.add_raw_damage(45, RawDamage::Fan { dir: 1 }, cx));
        b.with(|cx| w.fly.add_raw_damage(1, RawDamage::ZapperArc, cx));
        b.with(|cx| w.fly_render_driven(&mut m, cx));
        assert_eq!((m.stats.hits, m.stats.damage), (1, 1));
        b.with(|cx| w.fly.add_raw_damage(60, RawDamage::Fan { dir: 1 }, cx));
        b.with(|cx| w.fly_render_driven(&mut m, cx));
        assert!(w.fly.splatted);
        assert_eq!((m.stats.hits, m.stats.damage), (1, 1));

        let mut fly = centred(&mut b);
        update(&mut b, &mut m, &mut fly, 120);
        w.fly = fly;
        b.with(|cx| w.fly_render_driven(&mut m, cx));
        assert_eq!((m.stats.hits, m.stats.damage), (1, 1));
    }

    #[test]
    fn superfly_cuts_the_damage_and_skips_the_kick() {
        let mut b = Bench::new(1);
        let (mut m, _) = world(&mut b);
        let mut fly = centred(&mut b);
        b.with(|cx| fly.start_superfly(cx));
        let before = b.rng.state();
        for _ in 0..5 {
            update(&mut b, &mut m, &mut fly, 40);
        }
        assert_eq!(fly.damage_total, 16);
        assert_eq!((fly.vx, fly.vy, fly.ricochet, fly.phase), (0, 0, 0, 1));
        assert_eq!(b.rng.state(), before, "no kick draws");
        assert!(b.sounds(SoundSlot::FLY_RICOCHET).is_empty());
    }

    /// Acceptance W1 5, up to the respawn timer (the kill itself is W3's).
    #[test]
    fn a_total_above_100_dies_then_arms_the_respawn_timer_after_the_crash() {
        let mut b = Bench::new(1);
        let (mut m, _) = world(&mut b);
        let mut fly = centred(&mut b);
        (fly.carrying, fly.aspirating) = (true, true);
        let before = b.draws_since(1, 0);
        update(&mut b, &mut m, &mut fly, 101);
        // answers.md "Crash animation length": the 60-particle spray, 8 draws each.
        assert_eq!(b.draws_since(1, before), 60 * 8);
        assert_eq!(CRASH_MS, 200);
        assert!(fly.dying && !fly.stunned && !fly.carrying && !fly.aspirating);
        assert_eq!(fly.life(), Life::DyingOrAppearing);
        assert!(
            b.events
                .iter()
                .any(|e| matches!(e, Event::Effect(Effect::Crash { .. })))
        );
        assert_eq!(
            b.sounds(SoundSlot::FLY_CRASH),
            [
                SoundOp::SetVolume(5000),
                SoundOp::Play(PlayParams::default())
            ]
        );
        b.now = CRASH_MS;
        update(&mut b, &mut m, &mut fly, 0);
        assert!(!fly.respawn_timer.is_armed());
        b.now = CRASH_MS + 1;
        fly.vx = 100;
        update(&mut b, &mut m, &mut fly, 0);
        assert_eq!((fly.phase, fly.hit_frame, fly.vx), (1, 3, 0));
        assert!(
            fly.respawn_timer
                .expired(CRASH_MS + 1 + RESPAWN_DELAY_MS + 1)
        );
        assert!(!fly.respawn_timer.expired(CRASH_MS + 1 + RESPAWN_DELAY_MS));
    }

    /// Acceptance W1 5: with an object covering the whole table every re-roll fails, so the
    /// placement draws exactly its cap (one R(800 − w) per try) and the fly appears anyway.
    #[test]
    fn respawn_placement_stops_after_its_try_cap() {
        use crate::world::objects::{NORMAL_TYPES, Object};
        for tries in [RESPAWN_TRIES, RESPAWN_TRIES_PERIOD_START] {
            let mut b = Bench::new(1);
            let (_, mut w) = world(&mut b);
            let mut wall = Object::of_type(0, &NORMAL_TYPES[0], Size { w: 4990, h: 600 });
            wall.y = 0;
            w.objects.list.push(wall);
            b.with(|cx| w.fly.request_respawn(cx));
            let before = b.draws_since(1, 0);
            b.with(|cx| w.place_fly(tries, true, cx));
            assert_eq!(b.draws_since(1, before), tries);
            assert!(w.fly.appearing && !w.fly.respawn_request);
        }
    }

    /// A fan splats a dying fly (it skips only a splatted one, answers.md "Fan strength"): the
    /// reset must not carry the crash or respawn timer into the next life.
    #[test]
    fn a_reset_disarms_the_crash_and_respawn_timers() {
        for crash_over in [false, true] {
            let mut b = Bench::new(1);
            let (mut m, _) = world(&mut b);
            let mut fly = centred(&mut b);
            update(&mut b, &mut m, &mut fly, 120);
            if crash_over {
                b.now = CRASH_MS + 1;
                update(&mut b, &mut m, &mut fly, 0);
            }
            assert!(fly.crash_timer.is_armed() != fly.respawn_timer.is_armed());
            b.with(|cx| {
                fly.splat(1, cx);
                fly.reset(true, cx);
            });
            assert!(!fly.crash_timer.is_armed() && !fly.respawn_timer.is_armed());
        }
    }

    #[test]
    fn the_respawn_timer_records_the_kill_and_requests_a_respawn() {
        let mut b = Bench::new(1);
        let (mut m, _) = world(&mut b);
        let mut fly = centred(&mut b);
        update(&mut b, &mut m, &mut fly, 120);
        b.now = CRASH_MS + 1;
        update(&mut b, &mut m, &mut fly, 0);
        b.now = CRASH_MS + 1 + RESPAWN_DELAY_MS + 1;
        let before = b.draws_since(1, 0);
        update(&mut b, &mut m, &mut fly, 0);
        // The respawn x, then the 60 common fades re-rolled (answers.md "Zapper arc on the fly").
        assert_eq!(b.draws_since(1, before), 1 + 60);
        assert_eq!((m.stats.kills, fly.hits_this_life), (1, 0));
        assert!(fly.respawn_request && !fly.respawn_timer.is_armed());
        assert_eq!(fly.y, (600 - 94) / 2 - 20);
    }

    #[test]
    fn a_failed_grab_runs_5_51_and_the_turn_out_then_drops_aspirating() {
        let mut b = Bench::new(1);
        let (mut m, _) = world(&mut b);
        let mut fly = centred(&mut b);
        let press = |b: &mut Bench, m: &mut MatchState, fly: &mut Fly, action| {
            b.with(|cx| fly.update(&NONE, SwatHit::default(), action, m, cx));
        };
        press(&mut b, &mut m, &mut fly, true);
        assert!(fly.aspirating && fly.grab_lock);
        assert_eq!(b.sounds(SoundSlot::ASPIRATE).len(), 1);
        let mut ticks = 1;
        while fly.grab_lock && ticks < 200 {
            press(&mut b, &mut m, &mut fly, true);
            ticks += 1;
        }
        // 5 turn-in ticks, 51 loop ticks, then frames back down past 0.
        assert!((5 + 51..=5 + 51 + 10).contains(&ticks), "{ticks}");
        assert!(!fly.aspirating && fly.heading == Heading::Face);
        assert_eq!(b.sounds(SoundSlot::ASPIRATE).len(), 1, "held, not fresh");
    }

    fn pad(right: bool, left: bool, up: bool, down: bool) -> FlyPad {
        FlyPad {
            right,
            left,
            up,
            down,
            ..FlyPad::default()
        }
    }

    /// Runs the heading machine over `pads` from (heading, frame, turn), recording each tick.
    fn headings(start: (Heading, i32, Turn), pads: &[FlyPad]) -> Vec<(Heading, i32, Turn)> {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        (fly.heading, fly.heading_frame, fly.heading_turn) = start;
        pads.iter()
            .map(|p| {
                fly.heading_step(p);
                (fly.heading, fly.heading_frame, fly.heading_turn)
            })
            .collect()
    }

    /// answers.md "Fly heading machine": FACE + Right starts the turn-in at frame 0; the turn-in
    /// ends one tick past frame 4; then the loop runs 5..9 and wraps to 4.
    #[test]
    fn a_held_key_turns_in_past_frame_4_then_loops() {
        let r = pad(true, false, false, false);
        let got = headings((Heading::Face, 7, Turn::Loop), &[r; 11]);
        let frames: Vec<i32> = got.iter().map(|g| g.1).collect();
        assert_eq!(frames, [0, 1, 2, 3, 4, 4, 5, 6, 7, 8, 9]);
        assert_eq!(got[4], (Heading::E, 4, Turn::In));
        assert_eq!(got[5], (Heading::E, 4, Turn::Loop));
        let more = headings((Heading::E, 9, Turn::Loop), &[r]);
        assert_eq!(more, [(Heading::E, 4, Turn::Loop)]);
    }

    /// A turn-in reverses at once when its key is released, and lands on FACE below frame 0.
    #[test]
    fn a_released_turn_in_reverses_at_once() {
        let none = pad(false, false, false, false);
        let got = headings((Heading::E, 2, Turn::In), &[none; 4]);
        assert_eq!(
            got,
            [
                (Heading::E, 1, Turn::Out),
                (Heading::E, 0, Turn::Out),
                (Heading::Face, 0, Turn::Loop),
                (Heading::Face, 1, Turn::Loop),
            ]
        );
    }

    /// Mid-turn, a different key turns out; below 0 the turn-in toward the new key starts.
    #[test]
    fn a_new_key_mid_turn_turns_out_then_in_toward_it() {
        let up = pad(false, false, true, false);
        let got = headings((Heading::E, 1, Turn::In), &[up; 3]);
        assert_eq!(
            got,
            [
                (Heading::E, 0, Turn::Out),
                (Heading::N, 0, Turn::In),
                (Heading::N, 1, Turn::In),
            ]
        );
    }

    /// Steady with the opposite set: frame 3 and the turn-out. Pressing the heading's own key
    /// mid turn-out turns back in, capped at frame 4.
    #[test]
    fn the_opposite_key_turns_out_and_the_own_key_turns_back_in() {
        let (r, l) = (
            pad(true, false, false, false),
            pad(false, true, false, false),
        );
        let got = headings((Heading::E, 8, Turn::Loop), &[l, r, r]);
        assert_eq!(
            got,
            [
                (Heading::E, 3, Turn::Out),
                (Heading::E, 4, Turn::In),
                (Heading::E, 4, Turn::Loop),
            ]
        );
        let none = pad(false, false, false, false);
        let out = headings((Heading::E, 8, Turn::Loop), &[none, r]);
        assert_eq!(out, [(Heading::E, 7, Turn::Out), (Heading::E, 4, Turn::In)]);
    }

    /// Steady with a perpendicular set: clear it, set the key's direction, frame 4.
    #[test]
    fn a_perpendicular_key_switches_the_heading_at_frame_4() {
        let r = pad(true, false, false, false);
        assert_eq!(
            headings((Heading::NE, 7, Turn::Loop), &[r]),
            [(Heading::E, 4, Turn::Loop)]
        );
        assert_eq!(
            headings((Heading::N, 7, Turn::Loop), &[r]),
            [(Heading::E, 4, Turn::Loop)]
        );
    }

    /// Diagonals: E held with Right+Up stays E (the quirk), N goes to NE, S turns out.
    #[test]
    fn a_diagonal_from_a_horizontal_heading_stays_on_it() {
        let ru = pad(true, false, true, false);
        let from = |h| headings((h, 6, Turn::Loop), &[ru])[0];
        assert_eq!(from(Heading::E), (Heading::E, 7, Turn::Loop));
        assert_eq!(from(Heading::N), (Heading::NE, 7, Turn::Loop));
        assert_eq!(from(Heading::NE), (Heading::NE, 7, Turn::Loop));
        assert_eq!(from(Heading::S), (Heading::S, 3, Turn::Out));
        assert_eq!(from(Heading::W), (Heading::W, 3, Turn::Out));
        let face = headings((Heading::Face, 2, Turn::Loop), &[ru]);
        assert_eq!(face, [(Heading::NE, 0, Turn::In)]);
    }

    /// Two opposite keys, or three keys, leave the heading and its frame alone for the tick.
    #[test]
    fn opposite_or_three_keys_freeze_the_heading() {
        for p in [
            pad(true, true, false, false),
            pad(false, false, true, true),
            pad(true, false, true, true),
            pad(true, true, true, true),
        ] {
            for start in [
                (Heading::Face, 3, Turn::Loop),
                (Heading::E, 2, Turn::In),
                (Heading::NW, 6, Turn::Loop),
            ] {
                assert_eq!(headings(start, &[p]), [start]);
            }
        }
    }

    /// answers.md "Fly buzz: pitch, pan and volume", seed 1 after the init's x draw. x 100 (≤ 400
    /// − 120 = 280): pan −trunc((1 − 100 / 280) × 2000) = −1285; R(1000) = 18467 % 1000 = 467;
    /// frequency 10000 + 467 + 200 × 1500 / 700 (428) + 300 × 2500 / 700 (1071) = 11966; volume
    /// −700. Then x 500: pan trunc(100 / 400 × 2000) = 500, R(1000) = 334, frequency 10334 −
    /// 350 × 1500 / 700 (750) = 9584. Then x 300: pan 0, R(1000) = 500, frequency 10500.
    #[test]
    fn the_buzz_pans_and_pitches_from_position_and_speed() {
        let mut b = Bench::new(1);
        let mut fly = b.fly();
        (fly.x, fly.vx, fly.vy) = (100, -300, 200);
        b.with(|cx| fly.buzz(cx));
        (fly.x, fly.vx, fly.vy) = (500, 0, -350);
        b.with(|cx| fly.buzz(cx));
        (fly.x, fly.vx, fly.vy) = (300, 0, 0);
        b.with(|cx| fly.buzz(cx));
        let start = PlayParams {
            volume_cb: -700,
            freq_hz: Some(11966),
            pan: Pan::Fixed(-1285),
        };
        assert_eq!(
            b.sounds(SoundSlot::FLY_BUZZ),
            [
                SoundOp::Loop(start),
                SoundOp::Move(Pan::Fixed(500)),
                SoundOp::SetFrequency(9584),
                SoundOp::Move(Pan::Fixed(0)),
                SoundOp::SetFrequency(10500),
            ]
        );
    }
}
