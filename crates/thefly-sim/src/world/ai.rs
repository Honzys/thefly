//! W2. The in-game farmer AI (ai-and-rules §2): target controller, per-tick decisions, goodie
//! decisions and budgets. [`Aligner`] is shared with the bonus-round AI (W3, `bonus.rs`).

use super::World;
use super::farmer::HITBOX_W;
use super::goodies::FarmerGoodie;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::FarmerPad;
use crate::match_state::MatchState;

/// The fly's controller ring (`FUN_0043b845`).
pub const FLY_RING: i32 = 4392;
/// Aim spread while wandering.
pub const WANDER_SPREAD: i32 = 700;
/// Nuclear decision distance split (ai-and-rules §2.5 step 5).
pub const NUKE_NEAR: i32 = 801;
/// The far timer is destroyed below 800, one less than the split (AR §2.5 step 5.4).
pub const NUKE_FAR_DROP: i32 = 800;
/// UB guard for the frenzy-on modulus `m = 1 − trunc(frenzyOnRand / L)`, which is 0 when the
/// quotient is 1 (the original divides by zero; the shipped table never gets there). The port
/// uses `rand % m.abs().max(FRENZY_DIVISOR_FLOOR)`, which equals `rand % m` for every m ≠ 0.
pub const FRENZY_DIVISOR_FLOOR: i32 = 1;
/// UB guard for the Triple roll: with reason 1 and price ≤ 70 the original adds an
/// uninitialised value. The port adds this.
pub const TRIPLE_CHEAP_OBJECT_BONUS: i32 = 0;
/// UB guard: `rand % 0` when a controller's spread is 0 (never with the table values).
pub const MIN_SPREAD: i32 = 1;
/// UB guard: `rand % 0` when an aligner is built with no reaction spread (the bonus AI's choice).
pub const MIN_REACT_RAND: i32 = 1;
/// An aligned x axis stops when `rand % 5 < 2` (AR §2.4 step 7.1).
pub const ALIGN_STOP_ROLL: i32 = 5;
pub const ALIGN_STOP_BELOW: i32 = 2;
/// The Triple bonus applies above this carried-object price.
pub const TRIPLE_PRICE_THRESHOLD: f32 = 70.0;
/// Above the threshold the Triple roll gains trunc(pTriple · 0.3).
pub const TRIPLE_PRICE_BONUS: f64 = 0.3;
/// The swatter centre is x + 130/2 (AR §1.4).
pub const SWATTER_CENTRE_DX: i32 = HITBOX_W / 2;
/// The midge-escape point (farmerX + 90, 430) (AR §2.4 step 4).
pub const ESCAPE_DX: i32 = 90;
pub const ESCAPE_Y: i32 = 430;
/// Boot interval divisors: data+0x54 (5) on the level term, and the trap caps at data+0x29c,
/// +0x29e and +0x2a0 (5 + 2 + 7 = 14) on the trap term (answers.md "AI divisor").
pub const BOOT_LEVEL_DIVISOR: i32 = 5;
pub const BOOT_TRAP_DIVISOR: i32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiLevel {
    Easy,
    #[default]
    Normal,
    Intelligent,
}

/// One column of `FUN_0043f0f4` (ai-and-rules §2.2). Times in ms, probabilities in %.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiParams {
    pub align_tol: i32,
    pub aim_spread: i32,
    pub aim_spread_cut: i32,
    pub dwell_ms: u64,
    pub react_base: i32,
    pub react_rand: i32,
    pub wander_check_ms: u64,
    pub wander_ms: u64,
    pub frenzy_off_base: i32,
    pub frenzy_off_rand: i32,
    pub frenzy_on_base: i32,
    pub frenzy_on_rand: i32,
    pub escape_wait_ms: u64,
    pub escape_ms: u64,
    pub chase_off_ms: u64,
    pub chase_on_ms: u64,
    pub triple_idle_ms: u64,
    pub triple_armed_ms: u64,
    pub nuke_wait_ms: u64,
    pub nuke_far_ms: u64,
    pub boot_base_ms: i32,
    pub boot_level_k: i32,
    pub boot_trap_k: i32,
    pub p_wander: i32,
    pub p_frenzy: i32,
    pub p_triple: i32,
    pub p_nuke: i32,
    pub p_nuke_escape: i32,
    pub p_escape: i32,
    pub p_boot: i32,
    pub p_chase: i32,
}

/// Indexed by [`AiLevel`] as usize.
pub const AI_PARAMS: [AiParams; 3] = [
    AiParams {
        align_tol: 60,
        aim_spread: 271,
        aim_spread_cut: 90,
        dwell_ms: 1500,
        react_base: 195,
        react_rand: 150,
        wander_check_ms: 1000,
        wander_ms: 1300,
        frenzy_off_base: 6000,
        frenzy_off_rand: 6000,
        frenzy_on_base: 2000,
        frenzy_on_rand: 2000,
        escape_wait_ms: 9000,
        escape_ms: 2200,
        chase_off_ms: 4500,
        chase_on_ms: 1000,
        triple_idle_ms: 12_000,
        triple_armed_ms: 4500,
        nuke_wait_ms: 12_000,
        nuke_far_ms: 3000,
        boot_base_ms: 18_000,
        boot_level_k: 10_000,
        boot_trap_k: 4000,
        p_wander: 19,
        p_frenzy: 17,
        p_triple: 17,
        p_nuke: 17,
        p_nuke_escape: 15,
        p_escape: 30,
        p_boot: 20,
        p_chase: 20,
    },
    AiParams {
        align_tol: 40,
        aim_spread: 181,
        aim_spread_cut: 60,
        dwell_ms: 1000,
        react_base: 130,
        react_rand: 100,
        wander_check_ms: 1500,
        wander_ms: 700,
        frenzy_off_base: 5000,
        frenzy_off_rand: 5000,
        frenzy_on_base: 3000,
        frenzy_on_rand: 3000,
        escape_wait_ms: 6000,
        escape_ms: 3500,
        chase_off_ms: 3000,
        chase_on_ms: 2000,
        triple_idle_ms: 9000,
        triple_armed_ms: 3000,
        nuke_wait_ms: 9000,
        nuke_far_ms: 2000,
        boot_base_ms: 11_000,
        boot_level_k: 7000,
        boot_trap_k: 4000,
        p_wander: 13,
        p_frenzy: 30,
        p_triple: 30,
        p_nuke: 30,
        p_nuke_escape: 20,
        p_escape: 40,
        p_boot: 30,
        p_chase: 30,
    },
    AiParams {
        align_tol: 30,
        aim_spread: 120,
        aim_spread_cut: 51,
        dwell_ms: 700,
        react_base: 130,
        react_rand: 100,
        wander_check_ms: 2200,
        wander_ms: 450,
        frenzy_off_base: 3800,
        frenzy_off_rand: 4000,
        frenzy_on_base: 4000,
        frenzy_on_rand: 3000,
        escape_wait_ms: 4500,
        escape_ms: 3500,
        chase_off_ms: 2000,
        chase_on_ms: 1300,
        triple_idle_ms: 7000,
        triple_armed_ms: 3000,
        nuke_wait_ms: 7000,
        nuke_far_ms: 1200,
        boot_base_ms: 9000,
        boot_level_k: 6000,
        boot_trap_k: 2000,
        p_wander: 10,
        p_frenzy: 37,
        p_triple: 40,
        p_nuke: 40,
        p_nuke_escape: 30,
        p_escape: 39,
        p_boot: 40,
        p_chase: 45,
    },
];

/// What the controller aims at (ai-and-rules §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    Fly,
    /// The farmer's flying goodie.
    FlyingGoodie,
    /// The midge-escape point (farmerX + 90, 430), refreshed each tick.
    Dummy { x: i32, y: i32 },
    /// Bonus round: a farmer target slot.
    BonusTarget(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Controller {
    pub target: Target,
    /// Ring width passed when the target is set (4392 for the fly, 920 in the bonus round).
    pub ring: i32,
    /// Aim offset, each ±(rand % spread).
    pub ox: i32,
    pub oy: i32,
    pub spread: i32,
    /// Signed ring distance to the aim point, and its y counterpart. dy is the target's top y
    /// (plus h/2 with the centre flag) minus trunc(reach + 77) (gaps §9.1).
    pub dx: i32,
    pub dy: i32,
    /// Ring distance without the offset (the Nuclear decision).
    pub distance: i32,
}

impl Controller {
    /// `FUN_0043baa4`: a fresh offset within the current spread. Per axis, x first, the parity
    /// sign (− on an even rand) and then R(S) (answers.md "AI aim offset").
    pub fn reroll_offset(&mut self, cx: &mut Ctx) {
        let spread = self.spread.max(MIN_SPREAD);
        self.ox = signed_draw(spread, cx);
        self.oy = signed_draw(spread, cx);
    }

    /// Refresh dx, dy and distance from the swatter reference point (centre x, trunc(reach + 77))
    /// and the target's reference point.
    pub fn refresh(&mut self, swatter: (i32, i32), target: (i32, i32)) {
        self.dx = self.ring_delta(swatter.0, target.0 + self.ox);
        self.dy = target.1 + self.oy - swatter.1;
        self.distance = self.ring_delta(swatter.0, target.0).abs();
    }

    /// Signed distance from `from` to `to`, the shorter way round the ring (AR §2.3 `dx`).
    fn ring_delta(&self, from: i32, to: i32) -> i32 {
        let d = to - from;
        if self.ring > 0 && d.abs() > self.ring / 2 {
            return d - d.signum() * self.ring;
        }
        d
    }
}

fn signed_draw(spread: i32, cx: &mut Ctx) -> i32 {
    let sign = if cx.rng.odd() { 1 } else { -1 };
    sign * cx.rng.r(spread)
}

/// Alignment with the reaction delay (`FUN_0043e306`, `FUN_0043e4e9`), shared by both AIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Aligner {
    pub align_tol: i32,
    pub react_base: i32,
    pub react_rand: i32,
    /// −1, 0, +1 as applied to the keys.
    pub x_dir: i32,
    pub y_dir: i32,
    /// A changed x direction waiting for the reaction timer. The last chosen x direction stays
    /// in force until the delay expires [I] (gaps §11.1).
    pub pending_x_dir: Option<i32>,
    pub react: Timer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Alignment {
    pub x_aligned: bool,
    pub y_aligned: bool,
}

impl Aligner {
    /// AR §2.4 step 7.1–7.3. The x direction waits for the reaction timer; y applies at once.
    pub fn step(&mut self, dx: i32, dy: i32, cx: &mut Ctx) -> Alignment {
        let x_aligned = dx.abs() <= self.align_tol;
        let chosen = self.pending_x_dir.unwrap_or(self.x_dir);
        let want = if !x_aligned {
            dx.signum()
        } else if cx.rng.r(ALIGN_STOP_ROLL) < ALIGN_STOP_BELOW {
            0
        } else {
            chosen
        };
        if want == self.x_dir {
            self.pending_x_dir = None;
        } else if self.pending_x_dir != Some(want) {
            self.pending_x_dir = Some(want);
            let delay = self.react_base + cx.rng.r(self.react_rand.max(MIN_REACT_RAND));
            self.react.arm(cx.now, u64::try_from(delay).unwrap_or(0));
        }
        if self.pending_x_dir.is_some() && self.react.expired(cx.now) {
            self.x_dir = want;
            self.pending_x_dir = None;
        }
        let y_aligned = dy.abs() <= self.align_tol;
        self.y_dir = if y_aligned { 0 } else { dy.signum() };
        Alignment {
            x_aligned,
            y_aligned,
        }
    }

    /// The four direction flags for [`super::farmer::Farmer::move_by`]. dy > 0 means the target
    /// is lower on screen, which the down key (toward full reach) closes.
    pub fn pad(&self) -> FarmerPad {
        FarmerPad {
            right: self.x_dir > 0,
            left: self.x_dir < 0,
            up: self.y_dir < 0,
            down: self.y_dir > 0,
            ..FarmerPad::default()
        }
    }
}

/// What the AI reads each tick, gathered by step 6 so the AI never borrows the world.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AiView {
    pub swatter_centre: (i32, i32),
    pub fly_centre: (i32, i32),
    /// The fly's life state (`FUN_0043885d`) is 0.
    pub fly_alive: bool,
    pub fly_carrying: bool,
    /// Price of the carried object's type, when carrying.
    pub carried_price: Option<f32>,
    /// The farmer's flying goodie position, when it exists and is active.
    pub farmer_goodie: Option<(i32, i32)>,
    pub boot_active: bool,
    pub triple_on: bool,
    pub nuke_on: bool,
    pub midges_on: bool,
    pub slowed: bool,
    /// Owned Triple, Nuclear and Boot (data+0x80/0x82/0x84).
    pub owned: [i32; 3],
    /// Farmer level (data+0x7c), fly strength and speed (data+0x86/0x88), traps placed.
    pub farmer_level: i32,
    pub fly_levels: (i32, i32),
    pub trap_count: i32,
}

/// The AI's decisions for one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AiOutput {
    pub pad: FarmerPad,
    pub swing: bool,
    pub goodie: Option<FarmerGoodie>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FarmerAi {
    pub level: AiLevel,
    pub params: AiParams,
    pub controller: Controller,
    pub aligner: Aligner,
    pub paused: bool,
    pub dwell: Option<Timer>,
    pub wander_timer: Timer,
    pub wandering: bool,
    pub escape_timer: Option<Timer>,
    pub escaping: bool,
    /// Set by the midge-escape Nuclear reason (§2.5 step 5.1).
    pub escape_flag: bool,
    pub chase_timer: Timer,
    pub chasing: bool,
    pub frenzy_timer: Timer,
    pub frenzy: bool,
    pub swing_request: bool,
    pub triple_timer: Timer,
    pub triple_armed: bool,
    pub nuke_timer: Timer,
    pub nuke_far: Option<Timer>,
    pub boot_timer: Timer,
    /// Triple, Nuclear, Boot. Reset every period: the AI is rebuilt with the world (gaps §6.1).
    pub budgets: [i32; 3],
    pub prev_triple_on: bool,
    pub prev_midges_on: bool,
}

impl FarmerAi {
    /// `FUN_0043ee0b`: parameters, timers, target the fly, offset, budgets. Every timer is armed
    /// here (answers.md "AI timers armed at creation"); the frenzy's first span has no rand.
    pub fn new(level: AiLevel, view: &AiView, cx: &mut Ctx) -> Self {
        let params = AI_PARAMS[level as usize];
        let mut ai = Self {
            level,
            params,
            controller: Controller {
                target: Target::Fly,
                ring: FLY_RING,
                spread: params.aim_spread,
                ..Controller::default()
            },
            aligner: Aligner {
                align_tol: params.align_tol,
                react_base: params.react_base,
                react_rand: params.react_rand,
                ..Aligner::default()
            },
            paused: false,
            dwell: None,
            wander_timer: Timer::armed(cx.now, params.wander_check_ms),
            wandering: false,
            escape_timer: None,
            escaping: false,
            escape_flag: false,
            chase_timer: Timer::armed(cx.now, params.chase_off_ms),
            chasing: false,
            frenzy_timer: Timer::armed(cx.now, frenzy_first_ms(&params)),
            frenzy: false,
            swing_request: false,
            triple_timer: Timer::armed(cx.now, params.triple_idle_ms),
            triple_armed: false,
            nuke_timer: Timer::armed(cx.now, params.nuke_wait_ms),
            nuke_far: None,
            boot_timer: Timer::UNARMED,
            budgets: [0; 3],
            prev_triple_on: false,
            prev_midges_on: false,
        };
        ai.boot_timer.arm(cx.now, ai.boot_interval(view));
        ai.controller.reroll_offset(cx);
        for (item, owned) in view.owned.into_iter().enumerate() {
            ai.bump_budget(item, owned, cx);
        }
        ai
    }

    /// `FUN_0043f611` steps 1–8 and 11; the caller moves and swings the farmer with the output.
    /// Step 11 never fires (gaps §6.2).
    pub fn update(&mut self, view: &AiView, cx: &mut Ctx) -> AiOutput {
        if self.paused {
            return AiOutput::default();
        }
        self.controller.spread = self.spread(view);
        let target = self.target_point(view);
        self.controller.refresh(view.swatter_centre, target);
        self.dwell_step(cx);
        self.wander_step(cx);
        self.escape_step(view, cx);
        self.chase_step(view, cx);
        self.frenzy_step(view, cx);
        self.align_step(view, cx);
        let goodie = self.goodie_step(view, cx);
        let swing = std::mem::take(&mut self.swing_request);
        AiOutput {
            pad: self.aligner.pad(),
            swing,
            goodie,
        }
    }

    /// 700 while wandering, else aimSpread, cut while Triple is on (AR §2.3).
    fn spread(&self, view: &AiView) -> i32 {
        if self.wandering {
            return WANDER_SPREAD;
        }
        if view.triple_on {
            return self.params.aim_spread - self.params.aim_spread_cut;
        }
        self.params.aim_spread
    }

    fn target_point(&self, view: &AiView) -> (i32, i32) {
        match self.controller.target {
            Target::Dummy { x, y } => (x, y),
            Target::FlyingGoodie => view.farmer_goodie.unwrap_or(view.fly_centre),
            Target::Fly | Target::BonusTarget(_) => view.fly_centre,
        }
    }

    /// A mode change: new target and a fresh offset (AR §2.3).
    fn retarget(&mut self, target: Target, view: &AiView, cx: &mut Ctx) {
        self.controller.target = target;
        self.aim_reroll(view, cx);
    }

    /// `FUN_0043f8b1`'s re-roll, after a mode change or an alignment swing (AR §2.3): with Triple
    /// off and not wandering, only while the fly is alive (answers.md "AI aim offset"). Which
    /// caller carries the gate is read off those two sections [I].
    fn aim_reroll(&mut self, view: &AiView, cx: &mut Ctx) {
        if !self.wandering && !view.triple_on && !view.fly_alive {
            return;
        }
        self.controller.spread = self.spread(view);
        self.controller.reroll_offset(cx);
    }

    /// Step 2 (answers.md "AI dwell expiry", `FUN_0043e716`): re-roll the offset at 700 or the aim
    /// spread, always, request a swing, destroy the dwell timer. The keys and the pending reaction
    /// stay.
    fn dwell_step(&mut self, cx: &mut Ctx) {
        if !self.dwell.is_some_and(|t| t.expired(cx.now)) {
            return;
        }
        self.controller.spread = if self.wandering {
            WANDER_SPREAD
        } else {
            self.params.aim_spread
        };
        self.controller.reroll_offset(cx);
        self.swing_request = true;
        self.dwell = None;
    }

    /// Step 3.
    fn wander_step(&mut self, cx: &mut Ctx) {
        let fired = self
            .wander_timer
            .ai_tick(cx.now, self.params.wander_check_ms);
        if !fired || self.escaping || self.chasing {
            return;
        }
        if self.wandering {
            self.wandering = false;
            self.wander_timer.arm(cx.now, self.params.wander_check_ms);
        } else if cx.rng.r(100) < self.params.p_wander {
            self.wandering = true;
            self.wander_timer.arm(cx.now, self.params.wander_ms);
        } else {
            return;
        }
        self.controller.spread = if self.wandering {
            WANDER_SPREAD
        } else {
            self.params.aim_spread
        };
        self.controller.reroll_offset(cx);
    }

    /// Step 4.
    fn escape_step(&mut self, view: &AiView, cx: &mut Ctx) {
        let point = Target::Dummy {
            x: view.swatter_centre.0 - SWATTER_CENTRE_DX + ESCAPE_DX,
            y: ESCAPE_Y,
        };
        let Some(timer) = self.escape_timer.as_mut() else {
            return;
        };
        let fired = !self.wandering && !self.chasing && timer.expired_rearm(cx.now);
        if fired && !self.escaping && cx.rng.r(100) < self.params.p_escape {
            timer.arm(cx.now, self.params.escape_ms);
            self.escaping = true;
            self.retarget(point, view, cx);
        } else if fired {
            self.escaping = false;
            self.retarget(Target::Fly, view, cx);
        }
        if self.escaping {
            self.controller.target = point;
        }
    }

    /// Step 5. The goodie ring is not documented; the port keeps the fly's 4392.
    fn chase_step(&mut self, view: &AiView, cx: &mut Ctx) {
        let fired = self.chase_timer.ai_tick(cx.now, self.params.chase_off_ms);
        if fired && !self.escaping && !self.wandering {
            if self.chasing {
                self.chasing = false;
                self.chase_timer.arm(cx.now, self.params.chase_off_ms);
                self.retarget(Target::Fly, view, cx);
            } else if view.farmer_goodie.is_some() && cx.rng.r(100) < self.params.p_chase {
                self.chasing = true;
                self.chase_timer.arm(cx.now, self.params.chase_on_ms);
                self.retarget(Target::FlyingGoodie, view, cx);
            }
        }
        if self.chasing && view.farmer_goodie.is_none() {
            self.chasing = false;
            self.retarget(Target::Fly, view, cx);
        }
    }

    /// Step 6. L is the farmer's bought level.
    fn frenzy_step(&mut self, view: &AiView, cx: &mut Ctx) {
        if self
            .frenzy_timer
            .ai_tick(cx.now, frenzy_first_ms(&self.params))
        {
            self.toggle_frenzy(view.farmer_level, cx);
        }
        if self.frenzy && !view.triple_on {
            self.swing_request = true;
        }
    }

    fn toggle_frenzy(&mut self, level: i32, cx: &mut Ctx) {
        let p = self.params;
        if self.frenzy {
            let n = if level == 0 {
                p.frenzy_off_rand
            } else {
                p.frenzy_off_rand / level
            };
            self.frenzy = false;
            let ms = p.frenzy_off_base + cx.rng.r(n.max(FRENZY_DIVISOR_FLOOR));
            self.frenzy_timer
                .arm(cx.now, u64::try_from(ms).unwrap_or(0));
            return;
        }
        if cx.rng.r(100) >= p.p_frenzy {
            return;
        }
        let m = if level == 0 {
            1
        } else {
            1 - p.frenzy_on_rand / level
        };
        self.frenzy = true;
        let ms = p.frenzy_on_base + cx.rng.r(m.abs().max(FRENZY_DIVISOR_FLOOR));
        self.frenzy_timer
            .arm(cx.now, u64::try_from(ms).unwrap_or(0));
    }

    /// Step 7.4–7.5.
    fn align_step(&mut self, view: &AiView, cx: &mut Ctx) {
        let al = self
            .aligner
            .step(self.controller.dx, self.controller.dy, cx);
        if al.x_aligned && al.y_aligned {
            self.swing_request = true;
            self.aim_reroll(view, cx);
            self.dwell = None;
        } else if al.x_aligned && self.dwell.is_none() {
            self.dwell = Some(Timer::armed(cx.now, self.params.dwell_ms));
        }
    }

    /// `FUN_0043f930` (AR §2.5).
    fn goodie_step(&mut self, view: &AiView, cx: &mut Ctx) -> Option<FarmerGoodie> {
        if self.prev_triple_on && !view.triple_on {
            self.triple_timer.arm(cx.now, self.params.triple_idle_ms);
        }
        if self.prev_midges_on && !view.midges_on && self.escape_flag {
            self.end_escape(view, cx);
        }
        self.prev_triple_on = view.triple_on;
        self.prev_midges_on = view.midges_on;
        if view.triple_on || view.nuke_on {
            return None;
        }
        self.triple_decision(view, cx)
            .or_else(|| self.nuke_decision(view, cx))
            .or_else(|| self.boot_decision(view, cx))
    }

    fn end_escape(&mut self, view: &AiView, cx: &mut Ctx) {
        self.escape_flag = false;
        self.escape_timer = None;
        if self.escaping {
            self.escaping = false;
            self.retarget(Target::Fly, view, cx);
        }
    }

    /// Step 4: reason 1 while the fly carries, 2 on the idle timer.
    fn triple_decision(&mut self, view: &AiView, cx: &mut Ctx) -> Option<FarmerGoodie> {
        if view.owned[0] <= 0 {
            return None;
        }
        let reason = if !view.fly_carrying {
            self.triple_idle_reason(cx)
        } else if !self.triple_armed {
            self.triple_timer.arm(cx.now, self.params.triple_armed_ms);
            self.triple_armed = true;
            1
        } else if self.triple_timer.expired_rearm(cx.now) {
            self.triple_armed = false;
            1
        } else {
            0
        };
        if reason == 0 {
            return None;
        }
        let p_triple = self.params.p_triple;
        let bonus = match view.carried_price {
            Some(price) if reason == 1 && price > TRIPLE_PRICE_THRESHOLD => {
                (f64::from(p_triple) * TRIPLE_PRICE_BONUS) as i32
            }
            _ if reason == 1 => TRIPLE_CHEAP_OBJECT_BONUS,
            _ => 0,
        };
        let p = (p_triple - self.budgets[0]).max(0) + bonus;
        self.roll(p, FarmerGoodie::Triple, view.owned[0], cx)
    }

    fn triple_idle_reason(&mut self, cx: &mut Ctx) -> i32 {
        if self.triple_armed {
            self.triple_timer.arm(cx.now, self.params.triple_idle_ms);
            self.triple_armed = false;
            return 0;
        }
        if self.triple_timer.expired_rearm(cx.now) {
            return 2;
        }
        0
    }

    /// Step 5.
    fn nuke_decision(&mut self, view: &AiView, cx: &mut Ctx) -> Option<FarmerGoodie> {
        if view.owned[1] <= 0 {
            return None;
        }
        let d = self.controller.distance;
        let reason = if self.nuke_timer.expired_rearm(cx.now) {
            1
        } else if d < NUKE_NEAR {
            self.escape_reason(view, cx)
        } else {
            let far = self.nuke_far.get_or_insert(Timer::UNARMED);
            if far.ai_tick(cx.now, self.params.nuke_far_ms) {
                2
            } else {
                0
            }
        };
        let out = if reason == 0 {
            None
        } else {
            let extra = if reason == 3 {
                self.params.p_nuke_escape
            } else {
                0
            };
            let p = (self.params.p_nuke - self.budgets[1]).max(0) + extra;
            self.roll(p, FarmerGoodie::Nuclear, view.owned[1], cx)
        };
        if d < NUKE_FAR_DROP {
            self.nuke_far = None;
        }
        out
    }

    fn escape_reason(&mut self, view: &AiView, cx: &mut Ctx) -> i32 {
        if !(view.midges_on && !self.escape_flag && view.slowed) {
            return 0;
        }
        self.escape_flag = true;
        self.escape_timer = Some(Timer::armed(cx.now, self.params.escape_wait_ms));
        3
    }

    /// Step 6.
    fn boot_decision(&mut self, view: &AiView, cx: &mut Ctx) -> Option<FarmerGoodie> {
        if view.boot_active || view.owned[2] <= 0 || !self.boot_timer.expired(cx.now) {
            return None;
        }
        let p = self.params.p_boot - self.budgets[2];
        let out = self.roll(p, FarmerGoodie::Boot, view.owned[2], cx);
        let interval = self.boot_interval(view);
        self.boot_timer.arm(cx.now, interval);
        out
    }

    fn roll(
        &mut self,
        p: i32,
        goodie: FarmerGoodie,
        owned: i32,
        cx: &mut Ctx,
    ) -> Option<FarmerGoodie> {
        if cx.rng.r(100) >= p {
            return None;
        }
        self.bump_budget(goodie as usize, owned, cx);
        Some(goodie)
    }

    /// `FUN_00440068`: (flyStrength + flySpeed) / 2.0 is a float division (answers.md "AI
    /// divisor").
    pub fn boot_interval(&self, view: &AiView) -> u64 {
        let p = &self.params;
        let (strength, speed) = view.fly_levels;
        let fly_avg = f64::from(strength + speed) / 2.0;
        let a = ((f64::from(view.farmer_level) - fly_avg) * f64::from(p.boot_level_k)
            / f64::from(BOOT_LEVEL_DIVISOR)) as i32;
        let a = a.min(0);
        let b = view.trap_count * p.boot_trap_k / BOOT_TRAP_DIVISOR;
        let interval = p.boot_base_ms - a - b;
        let interval = if interval <= 0 {
            p.boot_base_ms
        } else {
            interval
        };
        u64::try_from(interval).unwrap_or(0)
    }

    /// `FUN_0044021a`: item 0 Triple, 1 Nuclear, 2 Boot.
    pub fn bump_budget(&mut self, item: usize, owned: i32, cx: &mut Ctx) {
        let p_item = [self.params.p_triple, self.params.p_nuke, self.params.p_boot];
        let (Some(&p), Some(_)) = (p_item.get(item), self.budgets.get(item)) else {
            return;
        };
        if cx.rng.rand() % 2 != 0 || owned <= 1 {
            return;
        }
        let k = (p - 5) / owned;
        if k > 0 {
            self.budgets[item] += cx.rng.r(k) + 1;
        }
    }
}

/// The frenzy timer's first span, frenzyOffBase + frenzyOffRand (AR §2.4 step 6).
fn frenzy_first_ms(p: &AiParams) -> u64 {
    u64::try_from(p.frenzy_off_base + p.frenzy_off_rand).unwrap_or(0)
}

impl World {
    /// What the AI reads this tick (step 6, and `World::new` for [`FarmerAi::new`]). The carried
    /// object's `kind` indexes the active food prices; out of range reads as cheap, like the
    /// original's 0.0 (answers.md "Carried object's kind is the price index").
    pub fn ai_view(&self, m: &MatchState) -> AiView {
        let f = &self.farmer;
        let carried_price = self
            .objects
            .carried
            .and_then(|i| self.objects.list.get(i))
            .and_then(|o| o.kind)
            .and_then(|k| m.economy.food_prices.get(k).copied());
        let g = &self.flying_goodies.farmer;
        let eco = &m.economy;
        AiView {
            swatter_centre: (f.x + SWATTER_CENTRE_DX, f.swatter_y()),
            fly_centre: self.fly.centre(),
            fly_alive: self.fly.alive(),
            fly_carrying: self.objects.carrying,
            carried_price,
            farmer_goodie: g.active.then_some((g.x + g.w / 2, g.y + g.h / 2)),
            boot_active: self.boot.active,
            triple_on: f.triple.on,
            nuke_on: f.nuclear.on,
            midges_on: f.midges_on,
            slowed: f.slowed,
            owned: [eco.farmer.triple, eco.farmer.nuclear, eco.farmer.boot],
            farmer_level: eco.farmer.level,
            fly_levels: (eco.fly.strength, eco.fly.speed),
            trap_count: i32::try_from(m.traps.slots.len()).unwrap_or(i32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::rng::MsvcRand;

    /// Seed 1 draws 41, 18467, 6334, 26500, 19169, 15724, ...
    fn with_seed_1<T>(now: u64, f: impl FnOnce(&mut Ctx) -> T) -> (T, i32) {
        let (mut rng, data, mut events) = (MsvcRand::new(1), GameData::synthetic(), Vec::new());
        let mut cx = Ctx::new(now, &mut rng, &data, &mut events);
        let out = f(&mut cx);
        let next = cx.rng.rand();
        (out, next)
    }

    /// answers.md "AI aim offset": per axis the parity sign, then the magnitude. 41 is odd (+),
    /// ox = 18467 % 181 = 5; 6334 is even (−), oy = −(26500 % 181) = −74.
    #[test]
    fn the_offset_is_a_signed_draw_per_axis() {
        let mut c = Controller {
            spread: 181,
            ..Controller::default()
        };
        let ((), next) = with_seed_1(0, |cx| c.reroll_offset(cx));
        assert_eq!((c.ox, c.oy, next), (5, -74, 19169));
    }

    #[test]
    fn dx_takes_the_shorter_way_round_the_ring() {
        let mut c = Controller {
            ring: FLY_RING,
            ox: 10,
            oy: -5,
            ..Controller::default()
        };
        c.refresh((100, 300), (4000, 250));
        // 4010 − 100 = 3910 is more than half of 4392, so the way round is 3910 − 4392.
        assert_eq!((c.dx, c.dy, c.distance), (-482, -55, 492));
        c.refresh((100, 300), (900, 400));
        assert_eq!((c.dx, c.dy, c.distance), (810, 95, 800));
    }

    /// AR §2.4 step 7, gaps §11.1: a new x direction applies once 130 + 41 % 100 = 171 ms are
    /// strictly past (at 172); an aligned axis keeps it while rand % 5 ≥ 2 (18467 % 5 = 2,
    /// 6334 % 5 = 4) and drops it on 26500 % 5 = 0, which waits 130 + 19169 % 100 = 199 ms
    /// (due at 430); y applies at once.
    #[test]
    fn the_x_direction_waits_for_the_reaction_delay() {
        let (mut rng, data, mut events) = (MsvcRand::new(1), GameData::synthetic(), Vec::new());
        let mut a = Aligner {
            align_tol: 40,
            react_base: 130,
            react_rand: 100,
            ..Aligner::default()
        };
        let mut step = |now, dx, dy, a: &mut Aligner| {
            let mut cx = Ctx::new(now, &mut rng, &data, &mut events);
            a.step(dx, dy, &mut cx)
        };
        let al = step(0, 100, -80, &mut a);
        assert_eq!(
            (a.x_dir, a.y_dir, al.x_aligned, al.y_aligned),
            (0, -1, false, false)
        );
        assert!(a.pad().up && !a.pad().right);
        step(171, 100, 0, &mut a);
        assert_eq!((a.x_dir, a.y_dir), (0, 0));
        step(172, 100, 0, &mut a);
        assert_eq!(a.x_dir, 1);
        assert!(a.pad().right);
        let al = step(200, 10, 0, &mut a);
        assert!(al.x_aligned && al.y_aligned && a.x_dir == 1);
        step(215, 10, 0, &mut a);
        assert_eq!(a.x_dir, 1);
        step(230, 10, 0, &mut a);
        assert_eq!((a.x_dir, a.pending_x_dir), (1, Some(0)));
        step(429, -10, 0, &mut a);
        assert_eq!(a.x_dir, 1);
        step(430, -10, 0, &mut a);
        assert_eq!(a.x_dir, 0);
    }
}

#[cfg(test)]
mod farmer_ai_tests {
    use super::super::collision::Rect;
    use super::super::farmer::Farmer;
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::rng::MsvcRand;
    use crate::stats::Stats;

    fn view(farmer: &Farmer, fly: (i32, i32), owned: [i32; 3]) -> AiView {
        AiView {
            swatter_centre: (farmer.x + SWATTER_CENTRE_DX, farmer.swatter_y()),
            fly_centre: fly,
            fly_alive: true,
            owned,
            fly_levels: (2, 2),
            ..AiView::default()
        }
    }

    /// A Normal AI with a still fly, its rand reset to seed 1 after creation.
    fn normal_ai(rig: &mut Rig) -> (FarmerAi, AiView) {
        let farmer = Farmer::new(0, &mut rig.cx(0));
        let v = view(&farmer, (farmer.x + 65, 300), [0; 3]);
        let ai = FarmerAi::new(AiLevel::Normal, &v, &mut rig.cx(0));
        rig.rng = MsvcRand::new(1);
        (ai, v)
    }

    /// answers.md "AI dwell expiry": the offset is re-rolled (spread 181, so (5, −74) on seed 1),
    /// a swing is requested and the dwell timer goes; the direction keys stay as they were. The
    /// step takes no view: `FUN_0043e716` has no fly-alive gate and no Triple cut.
    #[test]
    fn dwell_expiry_rerolls_the_offset_and_keeps_the_keys() {
        let mut rig = Rig::new(1);
        let (mut ai, _) = normal_ai(&mut rig);
        ai.dwell = Some(Timer::armed(0, 1000));
        (ai.aligner.x_dir, ai.aligner.y_dir, ai.aligner.pending_x_dir) = (1, -1, Some(0));
        ai.dwell_step(&mut rig.cx(1001));
        assert_eq!((ai.controller.ox, ai.controller.oy), (5, -74));
        assert!(ai.swing_request && ai.dwell.is_none());
        assert_eq!(
            (ai.aligner.x_dir, ai.aligner.y_dir, ai.aligner.pending_x_dir),
            (1, -1, Some(0))
        );
        assert_eq!(rig.rng.rand(), 19169);
    }

    /// AR §2.3: a mode change re-rolls through `FUN_0043f8b1`, so with the fly down it retargets
    /// without drawing.
    #[test]
    fn a_mode_change_retargets_without_a_roll_while_the_fly_is_down() {
        let mut rig = Rig::new(1);
        let (mut ai, mut v) = normal_ai(&mut rig);
        v.fly_alive = false;
        ai.retarget(Target::FlyingGoodie, &v, &mut rig.cx(0));
        assert_eq!(ai.controller.target, Target::FlyingGoodie);
        assert_eq!(rig.rng.rand(), 41);
    }

    /// answers.md "AI aim offset": not wandering and Triple off, the offset is only re-rolled
    /// while the fly is alive; wandering it is re-rolled at spread 700 regardless (41 odd, so
    /// ox = 18467 % 700 = 267; 6334 even, so oy = −(26500 % 700) = −600).
    #[test]
    fn the_aim_reroll_waits_for_a_live_fly_unless_wandering() {
        let mut rig = Rig::new(1);
        let (mut ai, mut v) = normal_ai(&mut rig);
        let before = (ai.controller.ox, ai.controller.oy);
        v.fly_alive = false;
        ai.aim_reroll(&v, &mut rig.cx(0));
        assert_eq!((ai.controller.ox, ai.controller.oy), before);
        assert_eq!(rig.rng.rand(), 41);
        rig.rng = MsvcRand::new(1);
        ai.wandering = true;
        ai.aim_reroll(&v, &mut rig.cx(0));
        assert_eq!((ai.controller.ox, ai.controller.oy), (267, -600));
    }

    /// answers.md "AI timers armed at creation": wander (wanderCheckMs), chase (chaseOffMs) and
    /// frenzy (frenzyOffBase + frenzyOffRand) count from the creation time.
    #[test]
    fn the_ai_timers_are_armed_at_creation() {
        let mut rig = Rig::new(1);
        let farmer = Farmer::new(0, &mut rig.cx(0));
        let v = view(&farmer, (0, 0), [0; 3]);
        let ai = FarmerAi::new(AiLevel::Normal, &v, &mut rig.cx(500));
        let left = |t: &Timer| t.remaining_ms(500);
        assert_eq!(
            (
                left(&ai.wander_timer),
                left(&ai.chase_timer),
                left(&ai.frenzy_timer)
            ),
            (1500, 3000, 10_000)
        );
        assert_eq!((left(&ai.triple_timer), left(&ai.nuke_timer)), (9000, 9000));
    }

    #[test]
    fn normal_parameters_follow_the_table() {
        let p = AI_PARAMS[AiLevel::Normal as usize];
        assert_eq!(
            (p.align_tol, p.aim_spread, p.aim_spread_cut, p.dwell_ms),
            (40, 181, 60, 1000)
        );
    }

    /// A 700 ms AI timer is armed on tick 0 (0 ms) and fires on ticks 46 (707 ms), 92
    /// (1415 ≥ 1407) and 138 (2123 ≥ 2115): each re-arm counts from the firing tick.
    #[test]
    fn ai_timers_fire_on_ticks_46_92_and_138_for_700_ms() {
        let mut rig = Rig::new(1);
        let farmer = Farmer::new(0, &mut rig.cx(0));
        let v = view(&farmer, (farmer.x + 65, 300), [0; 3]);
        let mut ai = FarmerAi::new(AiLevel::Normal, &v, &mut rig.cx(0));
        (
            ai.params.wander_check_ms,
            ai.params.wander_ms,
            ai.params.p_wander,
        ) = (700, 700, 100);
        ai.wander_timer = Timer::armed(0, 700);
        let mut flips = Vec::new();
        for tick in 0..=140 {
            let was = ai.wandering;
            ai.update(&v, &mut rig.at(tick));
            if ai.wandering != was {
                flips.push(tick);
            }
        }
        assert_eq!(flips, [46, 92, 138]);
    }

    /// FUN_00440068 at Normal, L 0, fly levels (2, 2), 7 traps: a = trunc((0 − 2) · 7000 / 5) =
    /// −2800, b = 7 · 4000 / 14 = 2000, so 11000 + 2800 − 2000.
    #[test]
    fn the_boot_interval_follows_the_levels_and_traps() {
        let mut rig = Rig::new(1);
        let farmer = Farmer::new(0, &mut rig.cx(0));
        let mut v = view(&farmer, (0, 0), [0; 3]);
        v.trap_count = 7;
        let ai = FarmerAi::new(AiLevel::Normal, &v, &mut rig.cx(0));
        assert_eq!(ai.boot_interval(&v), 11_800);
        v.farmer_level = 5;
        assert_eq!(ai.boot_interval(&v), 11_000 - 2000);
    }

    /// Seed 1: 41 and 18467 are odd (no bump); 6334 is even, k = (30 − 5) / 2 = 12, and
    /// 26500 % 12 + 1 = 5. With one owned the draw still happens but nothing is added.
    #[test]
    fn a_budget_grows_on_an_even_draw_with_more_than_one_owned() {
        let mut rig = Rig::new(1);
        let farmer = Farmer::new(0, &mut rig.cx(0));
        let v = view(&farmer, (0, 0), [0; 3]);
        let mut ai = FarmerAi::new(AiLevel::Normal, &v, &mut rig.cx(0));
        (rig.rng, ai.budgets) = (MsvcRand::new(1), [0; 3]);
        for _ in 0..3 {
            ai.bump_budget(0, 2, &mut rig.cx(0));
        }
        assert_eq!(ai.budgets, [5, 0, 0]);
        ai.bump_budget(1, 1, &mut rig.cx(0));
        // The single-owned bump draws 19169, so the next draw is 15724.
        assert_eq!((ai.budgets, rig.rng.rand()), ([5, 0, 0], 15724));
    }

    /// A W1-free stand-in for the 1P scenario: a still fly, the AI drives the real farmer for 30
    /// s. It closes to within alignTol, swings, and never asks for a goodie it does not own.
    #[test]
    fn the_ai_closes_on_a_still_fly_and_swings_without_unowned_goodies() {
        let mut rig = Rig::new(1);
        let mut farmer = Farmer::new(0, &mut rig.cx(0));
        let fly = (700, 470);
        let mut ai = FarmerAi::new(AiLevel::Normal, &view(&farmer, fly, [0; 3]), &mut rig.cx(0));
        let mut stats = Stats::default();
        let (mut closest, mut goodies) = (i32::MAX, 0);
        let fly_box = Rect::new(fly.0 - 60, fly.1 - 47, 120, 94);
        for tick in 1..=30 * 65 {
            let v = view(&farmer, fly, [0; 3]);
            let mut cx = rig.at(tick);
            let out = ai.update(&v, &mut cx);
            goodies += i32::from(out.goodie.is_some());
            farmer.move_by(&out.pad, 0.0);
            farmer.swing_and_hit(out.swing, fly_box, &mut stats, &mut cx);
            closest = closest.min((v.swatter_centre.0 - fly.0).abs());
        }
        assert!(closest <= ai.params.align_tol, "closest {closest}");
        assert!(stats.swings > 0 && goodies == 0);
    }

    /// With goodies owned, the decisions do fire.
    #[test]
    fn owned_goodies_get_requested() {
        let mut rig = Rig::new(3);
        let farmer = Farmer::new(0, &mut rig.cx(0));
        let v = view(&farmer, (farmer.x + 2000, 450), [3, 3, 3]);
        let mut ai = FarmerAi::new(AiLevel::Intelligent, &v, &mut rig.cx(0));
        let asked: Vec<FarmerGoodie> = (1..=120 * 65)
            .filter_map(|t| ai.update(&v, &mut rig.at(t)).goodie)
            .collect();
        assert!(asked.contains(&FarmerGoodie::Nuclear) && asked.contains(&FarmerGoodie::Triple));
        assert!(asked.contains(&FarmerGoodie::Boot));
    }
}
