//! W2. The farmer and swatter: movement, swing machine, hit test, Triple, Nuclear, voices and
//! footsteps (ai-and-rules §1). Ticks steps 2 and 6.

use super::collision::Rect;
use super::fly::KickDir;
use super::{SCREEN_W, SwatHit, World};
use crate::clock::{DT, Timer};
use crate::ctx::Ctx;
use crate::event::{Effect, Event, Pan, PlayParams, SoundOp, SoundSlot};
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
/// With vx = 0 a running step counter jumps here, so the next movement steps at once (AR §1.5).
pub const STEP_COUNTER_RESTART: i32 = 30;
pub const STEP_SOUNDS: u8 = 5;
/// The tapette sprite width (positional sound) and its frame-0 height (AR §1.1, gaps §8.1).
pub const SPRITE_W: i32 = 181;
pub const SPRITE_H: i32 = 178;
/// The goodie catch box width (gaps §5.1).
pub const CATCH_W: i32 = 131;
/// Swing sound 27: 35000 + rand % 15000 Hz, 40000 with Triple (AR §1.3).
pub const SWING_HZ: i32 = 35_000;
pub const SWING_HZ_RAND: i32 = 15_000;
pub const TRIPLE_SWING_HZ: i32 = 40_000;
pub const SHOUT_PERCENT: i32 = 20;
pub const NUCLEAR_SPARK_BELOW: i32 = 7;
/// The shockwave sits at x + 60 (AR §1.3).
pub const SHOCKWAVE_DX: i32 = 60;
/// Damage = trunc((40 + bonus) · f), bonus 6·L or 34 with Nuclear (AR §1.4).
pub const DAMAGE_BASE: i32 = 40;
pub const LEVEL_DAMAGE_BONUS: i32 = 6;
pub const NUCLEAR_DAMAGE_BONUS: i32 = 34;
/// Triple re-swing jitter: ±20 on reach and x (AR §1.6).
pub const TRIPLE_JITTER: i32 = 20;
/// Voices (AR §1.5).
pub const HATE_PERCENT: i32 = 7;
pub const VOICE_BASE_MS: i32 = 12_000;
pub const VOICE_RAND_MS: i32 = 7000;
pub const MISS_STREAK_VOICE: i32 = 7;
pub const MISS_ROLL: i32 = 4000;
pub const MISS_BELOW: i32 = 5;
pub const LOSE_PERCENT: i32 = 15;
pub const WIN_PERCENT: i32 = 10;

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
    /// Rising-edge memories for voices 33 (the fly holds an object) and 30 (the fly is not
    /// alive).
    pub prev_holding: bool,
    pub prev_fly_down: bool,
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
            prev_holding: false,
            prev_fly_down: false,
        }
    }

    /// The hit box (x, y, 130, 154) in screen space.
    pub fn hit_box(&self) -> Rect {
        Rect::new(self.x, self.y, HITBOX_W, HITBOX_H)
    }

    /// `FUN_0043cea5` with the four direction flags (shared by the human and the AI).
    pub fn move_by(&mut self, pad: &FarmerPad, camera_delta: f64) {
        let delta = if self.ignore_scroll_once {
            self.ignore_scroll_once = false;
            0.0
        } else {
            camera_delta
        };
        let cap = if self.slowed {
            self.slow_speed
        } else {
            self.max_speed_x
        };
        if !pad.right && !pad.left {
            self.vx = 0;
        }
        if pad.right {
            self.vx = (self.vx + VX_STEP).min(cap);
        }
        if pad.left {
            self.vx = (self.vx - VX_STEP).max(-cap);
        }
        if !pad.up && !pad.down {
            self.vy = 0;
        }
        if pad.up {
            self.vy = (self.vy - VY_STEP).max(-VY_MAX);
        }
        if pad.down {
            self.vy = (self.vy + VY_STEP).min(VY_MAX);
        }
        // AR §6.3: slowed by midges, the swatter cannot move vertically.
        if self.slowed {
            self.vy = 0;
        }
        self.x += (f64::from(self.vx) * DT - delta) as i32;
        self.y += (f64::from(self.vy) * DT) as i32;
        if self.x < 1 {
            self.x += RING;
        }
        if self.x >= RING {
            self.x -= RING;
        }
        self.y = self.y.clamp(Y_TOP, Y_BOTTOM);
        self.reach = reach_at(self.y);
    }

    /// `FUN_0043d1a0`/`FUN_0043d510`: the swing machine, then the hit test on active ticks.
    pub fn swing_and_hit(
        &mut self,
        edge: bool,
        fly_box: Rect,
        stats: &mut Stats,
        cx: &mut Ctx,
    ) -> SwatHit {
        if self.triple.on {
            self.triple_step(edge, cx);
        } else if self.nuclear.on {
            self.nuclear_step(cx);
        }
        let s = &mut self.swing;
        if s.retracting && !s.swinging && !self.nuclear.on {
            s.cooldown += 1;
            if s.cooldown > self.cooldown_limit {
                s.retracting = false;
                s.cooldown = 0;
            }
        }
        let boosted = (self.triple.on || self.nuclear.on) && s.swinging;
        let run = boosted || ((edge || s.swinging) && s.cooldown == 0);
        s.active = false;
        if run {
            self.swing_step(stats, cx);
        }
        if self.swing.blocked {
            self.swing.active = false;
        }
        self.swing.miss_streak += 1;
        if !self.swing.active {
            return SwatHit::default();
        }
        self.hit_test(fly_box)
    }

    /// AR §1.3 step 4.
    fn swing_step(&mut self, stats: &mut Stats, cx: &mut Ctx) {
        let s = &mut self.swing;
        if s.retracting {
            s.frame -= 1;
            if s.frame < 1 {
                s.frame = 0;
                s.swinging = false;
            }
            return;
        }
        s.swinging = true;
        s.frame += 1;
        if s.frame > 2 {
            s.active = true;
        }
        if s.frame == 1 {
            stats.swings += 1;
            self.swing_sounds(cx);
        }
        if self.swing.frame > 4 {
            self.swing.frame = 4;
            self.swing.retracting = true;
            self.shockwave(cx);
        }
    }

    /// Frame 1: slot 27 at 35000 + R(15000) Hz, or 40000 with Triple and no draw (answers.md
    /// "Triple swing skips the frequency draw"), and voice 34 one time in five.
    fn swing_sounds(&mut self, cx: &mut Ctx) {
        let freq = if self.triple.on {
            TRIPLE_SWING_HZ
        } else {
            SWING_HZ + cx.rng.r(SWING_HZ_RAND)
        };
        let pan = self.pan();
        let params = PlayParams {
            freq_hz: u32::try_from(freq).ok(),
            pan,
            ..PlayParams::default()
        };
        cx.sound(SoundSlot::SWING, SoundOp::Play(params));
        if cx.rng.r(100) < SHOUT_PERCENT {
            self.play(SoundSlot::VOICE_SHOUT, cx);
        }
    }

    /// Frame 5: the cosmetic shockwave, power L or 5 with sparks under Nuclear (AR §1.3).
    fn shockwave(&mut self, cx: &mut Ctx) {
        if self.swing.blocked {
            return;
        }
        let (power, sparks) = if self.nuclear.on {
            (MAX_LEVEL, cx.rng.r(10) < NUCLEAR_SPARK_BELOW)
        } else {
            (self.level, false)
        };
        cx.emit(Event::Effect(Effect::Shockwave {
            x: self.x + SHOCKWAVE_DX,
            y: self.swatter_y(),
            scale: power * 70 + 150,
            sparks,
        }));
    }

    /// AR §1.4 on an active tick.
    fn hit_test(&mut self, fly: Rect) -> SwatHit {
        let (cx, cy) = (self.x + HITBOX_W / 2, self.swatter_y());
        let (fx, fy) = (fly.x + fly.w / 2, fly.y + fly.h / 2);
        let d = f64::from(fx - cx).hypot(f64::from(fy - cy));
        let radius = f64::from(HITBOX_W / 2 + fly.w / 2);
        let f = if d <= radius { 1.0 - d / radius } else { 0.0 };
        if d <= radius {
            self.swing.miss_streak = 0;
        }
        let bonus = if self.nuclear.on {
            NUCLEAR_DAMAGE_BONUS
        } else {
            LEVEL_DAMAGE_BONUS * self.level
        };
        SwatHit {
            damage: (f64::from(DAMAGE_BASE + bonus) * f) as i32,
            dir_x: kick_dir(fly.x, fly.w, cx),
            dir_y: kick_dir(fly.y, fly.h, cy),
        }
    }

    /// AR §1.6, per tick while Triple is on.
    fn triple_step(&mut self, edge: bool, cx: &mut Ctx) {
        let s = &mut self.swing;
        let idle = !s.swinging && !s.retracting;
        if edge && idle && s.cooldown == 0 && self.triple.burst_swing == 0 {
            s.swinging = true;
        }
        let settled = (!s.swinging && s.cooldown != 0) || !s.retracting;
        if settled && self.triple.bursts > TRIPLE_MAX_BURSTS {
            self.triple.on = false;
            return;
        }
        if !(s.retracting && !s.swinging && s.cooldown == 0) {
            return;
        }
        self.triple.burst_swing += 1;
        if self.triple.burst_swing >= TRIPLE_BURST_SWINGS {
            self.triple.burst_swing = 0;
            self.triple.bursts += 1;
            self.swing.cooldown = 1;
            return;
        }
        let reach = (self.reach - f64::from(TRIPLE_JITTER)
            + f64::from(cx.rng.r(2 * TRIPLE_JITTER)))
        .max(0.0);
        self.place_at_reach(reach);
        self.x += cx.rng.r(2 * TRIPLE_JITTER) - TRIPLE_JITTER;
        self.reswing();
    }

    /// AR §1.7 step 2–3, per tick while Nuclear is on.
    fn nuclear_step(&mut self, cx: &mut Ctx) {
        if self.nuclear.timer.expired(cx.now) {
            self.end_nuclear();
            return;
        }
        if !(self.swing.retracting && !self.swing.swinging) {
            return;
        }
        let reach = f64::from(cx.rng.r(REACH_MAX));
        self.place_at_reach(reach);
        self.x = cx.rng.r(SCREEN_W);
        self.reswing();
    }

    /// y from a reach (AR §1.6): trunc(472 − (1 − reach/375) · 121).
    fn place_at_reach(&mut self, reach: f64) {
        self.reach = reach;
        self.y =
            (f64::from(Y_BOTTOM) - (1.0 - reach / f64::from(REACH_MAX)) * f64::from(Y_SPAN)) as i32;
    }

    /// Swing again at once, with no cooldown.
    fn reswing(&mut self) {
        self.swing.cooldown = 0;
        self.swing.retracting = false;
        self.swing.swinging = true;
    }

    /// Triple goodie (`FUN_0043c8e2`): Nuclear off; on, or refresh the burst counter.
    pub fn start_triple(&mut self) {
        self.end_nuclear();
        if self.triple.on {
            self.triple.bursts = 0;
            return;
        }
        self.triple = Triple {
            on: true,
            bursts: 0,
            burst_swing: 0,
        };
        self.swing.swinging = false;
        self.swing.retracting = false;
        self.swing.cooldown = 0;
    }

    /// Nuclear (`FUN_0043cb71`); None is the goodie's −1 (5000 ms).
    pub fn start_nuclear(&mut self, ms: Option<u64>, cx: &mut Ctx) {
        self.triple.on = false;
        let was_on = self.nuclear.on;
        self.nuclear.on = true;
        self.nuclear.timer.arm(cx.now, ms.unwrap_or(NUCLEAR_MS));
        if !was_on {
            self.swing.swinging = true;
        }
    }

    pub fn end_nuclear(&mut self) {
        self.nuclear.on = false;
        self.nuclear.timer.disarm();
    }

    /// `FUN_0043cd23`: the midges slowdown pulse (AR §6.3 steps 1–3).
    // shortcut: answers.md "Farmer pulse timer" [I], an unarmed timer reads as expired, so the
    // first pulse fires at once; the farmer's own timer init was not checked. Revisit with it.
    pub fn midges_pulse(&mut self, stuck: i32, total: i32, cx: &mut Ctx) {
        let r = if total == 0 {
            0.0
        } else {
            f64::from(stuck) / f64::from(total)
        };
        let due = self.slow_timer.expired(cx.now);
        if due && self.slowed {
            self.slowed = false;
            self.slow_timer
                .arm(cx.now, ((1.0 - r) * 500.0 + 200.0) as u64);
        } else if due {
            self.slow_timer.arm(cx.now, (r * 400.0 + 150.0) as u64);
            if r != 0.0 {
                self.slowed = true;
                self.slow_speed = (f64::from(self.max_speed_x) * (1.0 - r)) as i32;
            }
        }
        if self.slowed && self.swing.swinging && !self.swing.retracting {
            self.swing.retracting = true;
        }
    }

    /// `FUN_0043c730`: voices 31/32/33/30 and footsteps (AR §1.5, answers.md "Farmer voice 33").
    /// `holding` is the object manager's grabbed index (+0x1a) being set; `fly_down` is the fly's
    /// life state (`FUN_0043885d`) being non-zero.
    pub fn voices(&mut self, holding: bool, fly_down: bool, cx: &mut Ctx) {
        if self.voice_timer.expired(cx.now) && cx.rng.r(100) < HATE_PERCENT {
            let ms = VOICE_BASE_MS + cx.rng.r(VOICE_RAND_MS);
            self.voice_timer.arm(cx.now, u64::try_from(ms).unwrap_or(0));
            self.play(SoundSlot::VOICE_HATE, cx);
        }
        if self.swing.miss_streak > MISS_STREAK_VOICE && cx.rng.r(MISS_ROLL) < MISS_BELOW {
            self.play(SoundSlot::VOICE_MISS, cx);
        }
        if holding && !self.prev_holding && cx.rng.r(100) < LOSE_PERCENT {
            self.play(SoundSlot::VOICE_LOSE, cx);
        }
        if fly_down && !self.prev_fly_down && cx.rng.r(100) < WIN_PERCENT {
            self.play(SoundSlot::VOICE_WIN, cx);
        }
        self.prev_holding = holding;
        self.prev_fly_down = fly_down;
        if self.vx == 0 {
            if self.step_counter != 0 {
                self.step_counter = STEP_COUNTER_RESTART;
            }
            return;
        }
        self.step_counter += 1;
        if self.step_counter > STEP_COUNTER_LIMIT {
            self.step_counter = 0;
            let slot = SoundSlot(SoundSlot::STEP_FIRST.0 + rand_u8(cx, STEP_SOUNDS));
            self.play(slot, cx);
        }
    }

    /// trunc(77 + reach): the swatter's reference y (AR §1.4, gaps §9.1).
    pub fn swatter_y(&self) -> i32 {
        (f64::from(HITBOX_H / 2) + self.reach) as i32
    }

    /// The box that catches the farmer's flying goodie and touches midges: (x, trunc(reach), 131,
    /// 154) (gaps §5.1, §8.1).
    pub fn catch_box(&self) -> Rect {
        Rect::new(self.x, self.reach as i32, CATCH_W, HITBOX_H)
    }

    /// The catch and midge-contact gate: swing frame ≥ 3 and the swatter not blocked.
    pub fn catch_open(&self) -> bool {
        self.swing.frame >= 3 && !self.swing.blocked
    }

    fn pan(&self) -> Pan {
        Pan::ScreenX {
            x: self.x,
            w: SPRITE_W,
        }
    }

    fn play(&self, slot: SoundSlot, cx: &mut Ctx) {
        let params = PlayParams {
            pan: self.pan(),
            ..PlayParams::default()
        };
        cx.sound(slot, SoundOp::Play(params));
    }
}

/// (1 − (472 − y)/121) · 375.
fn reach_at(y: i32) -> f64 {
    (1.0 - f64::from(Y_BOTTOM - y) / f64::from(Y_SPAN)) * f64::from(REACH_MAX)
}

/// AR §1.4 step 8: 0 when the fly's far edge is before the centre, 1 when the fly starts after
/// it, 2 when they overlap.
fn kick_dir(start: i32, len: i32, centre: i32) -> KickDir {
    if start + len < centre {
        KickDir::Minus
    } else if centre < start {
        KickDir::Plus
    } else {
        KickDir::Random
    }
}

fn rand_u8(cx: &mut Ctx, n: u8) -> u8 {
    u8::try_from(cx.rng.r(i32::from(n))).unwrap_or(0)
}

impl World {
    /// Step 2: the swing key counts on a press edge (memory < 0 before, key ≥ 1 now).
    pub fn farmer_swing_edge(&mut self, pad: &FarmerPad) -> bool {
        let edge = pad.swing && self.farmer_swing_memory < 0;
        self.farmer_swing_memory = if pad.swing { 1 } else { -1 };
        edge
    }

    /// Step 6: human move then hit test, or the AI (which moves and swings itself). The human
    /// swing edge comes from step 4, already cancelled by a goodie use attempt.
    pub fn farmer_step(
        &mut self,
        pad: &FarmerPad,
        swing_edge: bool,
        m: &mut MatchState,
        cx: &mut Ctx,
    ) -> SwatHit {
        let fly_box = Rect::new(self.fly.x, self.fly.y, self.fly.size.w, self.fly.size.h);
        let (pad, swing) = match self.ai.is_some() {
            false => (*pad, swing_edge),
            true => {
                self.spend_farmer_request(m, cx);
                let view = self.ai_view(m);
                let Some(ai) = self.ai.as_mut() else {
                    return SwatHit::default();
                };
                ai.paused = self.paused;
                let out = ai.update(&view, cx);
                if out.goodie.is_some() {
                    self.goodies.farmer_request = out.goodie;
                }
                (out.pad, out.swing)
            }
        };
        self.farmer.move_by(&pad, self.camera.delta);
        self.farmer.swing_and_hit(swing, fly_box, &mut m.stats, cx)
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
        assert!(
            farmer.voice_timer.expired(VOICE_FIRST_MS + 1)
                && !farmer.voice_timer.expired(VOICE_FIRST_MS)
        );
    }
}

/// A seeded RNG, the synthetic data and an event sink, shared by the W2 test modules.
#[cfg(test)]
pub(crate) mod rig {
    use crate::clock::ms_at;
    use crate::ctx::Ctx;
    use crate::data::GameData;
    use crate::event::Event;
    use crate::rng::MsvcRand;

    pub struct Rig {
        pub rng: MsvcRand,
        pub data: GameData,
        pub events: Vec<Event>,
    }

    impl Rig {
        pub fn new(seed: u32) -> Self {
            Self {
                rng: MsvcRand::new(seed),
                data: GameData::synthetic(),
                events: Vec::new(),
            }
        }

        pub fn cx(&mut self, now: u64) -> Ctx<'_> {
            Ctx::new(now, &mut self.rng, &self.data, &mut self.events)
        }

        pub fn at(&mut self, tick: u64) -> Ctx<'_> {
            self.cx(ms_at(tick))
        }
    }
}

#[cfg(test)]
mod swing_tests {
    use super::rig::Rig;
    use super::*;
    use crate::match_state::{MatchConfig, Players};

    fn farmer(level: i32) -> Farmer {
        let mut rig = Rig::new(1);
        let mut f = Farmer::new(level, &mut rig.cx(0));
        (f.x, f.y) = (100, 400);
        f
    }

    fn pad(right: bool, left: bool, up: bool, down: bool) -> FarmerPad {
        FarmerPad {
            right,
            left,
            up,
            down,
            ..FarmerPad::default()
        }
    }

    /// AR §1.2 at L 0 (cap 650): +400, then capped; x += trunc(400/65) = 6, then trunc(650/65) =
    /// 10; vy −200, −400, clamped; y += trunc(−200/65) = −3, then trunc(−400/65) = −6.
    #[test]
    fn movement_steps_and_clamps() {
        let mut f = farmer(0);
        f.move_by(&pad(true, false, true, false), 0.0);
        assert_eq!((f.vx, f.vy, f.x, f.y), (400, -200, 106, 397));
        f.move_by(&pad(true, false, true, false), 0.0);
        assert_eq!((f.vx, f.vy, f.x, f.y), (650, -400, 116, 391));
        f.move_by(&pad(true, false, true, false), 0.0);
        assert_eq!((f.vx, f.vy), (650, -400));
        f.move_by(&pad(false, true, false, true), 0.0);
        assert_eq!((f.vx, f.vy), (250, -200));
        f.move_by(&pad(false, false, false, false), 10.5);
        assert_eq!((f.vx, f.vy), (0, 0));
        // trunc(0 − 10.5) = −10.
        assert_eq!(f.x, 116 + 10 + 3 - 10);
    }

    /// y stays in 351..=472; reach is (1 − (472 − y)/121) · 375; x wraps on the 4000 ring.
    #[test]
    fn y_clamps_reach_follows_and_x_wraps() {
        let mut f = farmer(0);
        (f.x, f.y) = (2, 352);
        for _ in 0..3 {
            f.move_by(&pad(false, true, true, false), 0.0);
        }
        assert_eq!(f.y, Y_TOP);
        assert_eq!(f.reach, 0.0);
        // 2 − 6 = −4 wraps to 3996, then two more steps of trunc(−650/65) = −10.
        assert_eq!(f.x, 3976);
        for _ in 0..30 {
            f.move_by(&pad(false, false, false, true), 0.0);
        }
        assert_eq!((f.y, f.reach), (Y_BOTTOM, 375.0));
        f.y = 411;
        f.move_by(&FarmerPad::default(), 0.0);
        // (1 − (472 − 411)/121) · 375.
        assert_eq!(f.reach, (1.0 - 61.0 / 121.0) * 375.0);
    }

    #[test]
    fn a_held_swing_key_is_one_edge() {
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut cx = rig.cx(0);
        let m = MatchState::new(config, false, &mut cx);
        let mut w = World::bare(&m, &mut cx);
        let tab = |on| FarmerPad {
            swing: on,
            ..FarmerPad::default()
        };
        let edges: Vec<bool> = [true, true, true, false, true]
            .map(|on| w.farmer_swing_edge(&tab(on)))
            .into();
        assert_eq!(edges, [true, false, false, false, true]);
    }

    /// One frame per tick: (frame, active) for ticks 1–10 after an edge on tick 1.
    #[test]
    fn a_press_winds_up_hits_and_retracts() {
        let mut rig = Rig::new(1);
        let mut f = farmer(0);
        let mut stats = Stats::default();
        let far = Rect::new(2000, 0, 120, 94);
        let mut seen = Vec::new();
        for tick in 1..=10 {
            f.swing_and_hit(tick == 1, far, &mut stats, &mut rig.at(tick));
            seen.push((f.swing.frame, f.swing.active));
        }
        let want = [
            (1, false),
            (2, false),
            (3, true),
            (4, true),
            (4, true),
            (3, false),
            (2, false),
            (1, false),
            (0, false),
            (0, false),
        ];
        assert_eq!(seen, want);
        assert_eq!(stats.swings, 1);
        assert!(f.swing.retracting && f.swing.cooldown == 1);
    }

    /// The tick of the second swing's frame 1 with an edge on every tick: 9 + 36 − 5L, so 45 at
    /// L 0 and 20 at L 5 (counting the first swing's frame 1 as tick 1).
    #[test]
    fn the_minimum_swing_period_depends_on_the_level() {
        for (level, want) in [(0, 45), (5, 20)] {
            let mut rig = Rig::new(1);
            let mut f = farmer(level);
            let mut stats = Stats::default();
            let far = Rect::new(2000, 0, 120, 94);
            let second = (1..=100).find(|&tick| {
                f.swing_and_hit(true, far, &mut stats, &mut rig.at(tick));
                stats.swings == 2
            });
            assert_eq!(second, Some(want), "level {level}");
        }
    }

    /// Farmer (100, 351): centre (165, 77). A 126-wide fly gives R = 128; its centre 64 px right
    /// gives f = 0.5, so damage 20 at L 0, trunc(70 · 0.5) = 35 at L 5, trunc(74 · 0.5) = 37
    /// with Nuclear. The fly starts right of the centre (+) and spans it vertically (random).
    #[test]
    fn the_hit_falls_off_with_distance() {
        let fly = Rect::new(166, 30, 126, 94);
        for (level, nuclear, want) in [(0, false, 20), (5, false, 35), (0, true, 37)] {
            let mut f = farmer(level);
            (f.y, f.reach) = (Y_TOP, 0.0);
            f.nuclear.on = nuclear;
            let hit = f.hit_test(fly);
            assert_eq!(
                (hit.damage, hit.dir_x, hit.dir_y),
                (want, KickDir::Plus, KickDir::Random)
            );
        }
        let mut f = farmer(0);
        (f.y, f.reach) = (Y_TOP, 0.0);
        let beyond = f.hit_test(Rect::new(166 + 65, 30, 126, 94));
        assert_eq!(beyond.damage, 0);
        let left = f.hit_test(Rect::new(0, 200, 126, 94));
        assert_eq!((left.dir_x, left.dir_y), (KickDir::Minus, KickDir::Plus));
    }

    /// The damage only comes out on the active ticks 3–5.
    #[test]
    fn damage_lands_only_on_active_ticks() {
        let mut rig = Rig::new(1);
        let mut f = farmer(0);
        (f.y, f.reach) = (Y_TOP, 0.0);
        let mut stats = Stats::default();
        let fly = Rect::new(105, 30, 120, 94);
        let dmg: Vec<i32> = (1..=7)
            .map(|t| {
                let hit = f.swing_and_hit(t == 1, fly, &mut stats, &mut rig.at(t));
                hit.damage
            })
            .collect();
        assert_eq!(dmg, [0, 0, 40, 40, 40, 0, 0]);
        assert_eq!(f.swing.miss_streak, 2);
    }

    /// AR §1.6: each press is a burst of 3 swings; Triple ends once more than 6 bursts are done,
    /// so 7 bursts or 21 swings.
    #[test]
    fn triple_bursts_three_swings_and_ends_after_seven_bursts() {
        let mut rig = Rig::new(1);
        let mut f = farmer(5);
        let mut stats = Stats::default();
        let far = Rect::new(2000, 0, 120, 94);
        f.start_triple();
        let mut tick = 0;
        while f.triple.on && tick < 2000 {
            tick += 1;
            f.swing_and_hit(true, far, &mut stats, &mut rig.at(tick));
        }
        assert!(!f.triple.on);
        assert_eq!((f.triple.bursts, stats.swings), (7, 21));
    }

    /// AR §1.7: −1 is 5000 ms, the mystery 20000 ms. Tick 325 is exactly 5000 ms and tick 1300
    /// exactly 20000 ms, not yet strictly past, so Nuclear ends a tick later. Nuclear and Triple
    /// exclude each other.
    #[test]
    fn nuclear_lasts_its_duration_and_excludes_triple() {
        for (ms, last_on, off) in [(None, 325, 326), (Some(MYSTERY_NUCLEAR_MS), 1300, 1301)] {
            let mut rig = Rig::new(1);
            let mut f = farmer(0);
            let mut stats = Stats::default();
            f.start_triple();
            f.start_nuclear(ms, &mut rig.at(0));
            assert!(f.nuclear.on && !f.triple.on && f.swing.swinging);
            let far = Rect::new(2000, 0, 120, 94);
            for tick in 1..=last_on {
                f.swing_and_hit(false, far, &mut stats, &mut rig.at(tick));
            }
            assert!(f.nuclear.on);
            f.swing_and_hit(false, far, &mut stats, &mut rig.at(off));
            assert!(!f.nuclear.on);
            assert!(stats.swings > 1, "Nuclear re-swings with no cooldown");
            f.start_nuclear(None, &mut rig.at(off));
            f.start_triple();
            assert!(f.triple.on && !f.nuclear.on);
        }
    }
}
