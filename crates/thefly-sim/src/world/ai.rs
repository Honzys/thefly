//! W2. The in-game farmer AI (ai-and-rules §2): target controller, per-tick decisions, goodie
//! decisions and budgets. [`Aligner`] is shared with the bonus-round AI (W3, `bonus.rs`).

use super::goodies::FarmerGoodie;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::FarmerPad;

/// The fly's controller ring (`FUN_0043b845`).
pub const FLY_RING: i32 = 4392;
/// Aim spread while wandering.
pub const WANDER_SPREAD: i32 = 700;
/// Nuclear decision distance split (ai-and-rules §2.5 step 5).
pub const NUKE_NEAR: i32 = 801;
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
    /// `FUN_0043baa4`: a fresh offset within the current spread.
    // shortcut: AR §2.3 gives "±(rand % S) with an independent sign" but not the draw order; the
    // port draws x magnitude, x sign, y magnitude, y sign. Revisit if a trace shows otherwise.
    pub fn reroll_offset(&mut self, cx: &mut Ctx) {
        let spread = self.spread.max(MIN_SPREAD);
        self.ox = cx.rng.r(spread) * cx.rng.sign();
        self.oy = cx.rng.r(spread) * cx.rng.sign();
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
    /// `FUN_0043ee0b`: parameters, timers, target the fly, offset, budgets.
    pub fn new(level: AiLevel, view: &AiView, cx: &mut Ctx) -> Self {
        let _ = (level, view, cx);
        todo!("W2 ai-and-rules §2.1, §2.6")
    }

    /// `FUN_0043f611` steps 1–8 and 11; the caller moves and swings the farmer with the output.
    pub fn update(&mut self, view: &AiView, cx: &mut Ctx) -> AiOutput {
        let _ = (view, cx);
        todo!("W2 ai-and-rules §2.4, §2.5")
    }

    /// `FUN_00440068`.
    pub fn boot_interval(&self, view: &AiView) -> u64 {
        let _ = view;
        todo!("W2 ai-and-rules §2.6")
    }

    /// `FUN_0044021a`: item 0 Triple, 1 Nuclear, 2 Boot.
    pub fn bump_budget(&mut self, item: usize, owned: i32, cx: &mut Ctx) {
        let _ = (item, owned, cx);
        todo!("W2 ai-and-rules §2.6")
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

    /// AR §2.3: ox = 41 % 181 = 41, sign of 18467 (> 16383) is +; oy = 6334 % 181 = 180, sign
    /// of 26500 is +.
    #[test]
    fn the_offset_is_a_signed_draw_per_axis() {
        let mut c = Controller {
            spread: 181,
            ..Controller::default()
        };
        let ((), next) = with_seed_1(0, |cx| c.reroll_offset(cx));
        assert_eq!((c.ox, c.oy, next), (41, 180, 19169));
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

    /// AR §2.4 step 7, gaps §11.1: a new x direction waits 130 + 41 % 100 = 171 ms; an aligned
    /// axis keeps it while rand % 5 ≥ 2 (18467 % 5 = 2, 6334 % 5 = 4) and drops it on 26500 % 5
    /// = 0, which waits 130 + 19169 % 100 = 199 ms; y applies at once.
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
        step(170, 100, 0, &mut a);
        assert_eq!((a.x_dir, a.y_dir), (0, 0));
        step(171, 100, 0, &mut a);
        assert_eq!(a.x_dir, 1);
        assert!(a.pad().right);
        let al = step(200, 10, 0, &mut a);
        assert!(al.x_aligned && al.y_aligned && a.x_dir == 1);
        step(215, 10, 0, &mut a);
        assert_eq!(a.x_dir, 1);
        step(230, 10, 0, &mut a);
        assert_eq!((a.x_dir, a.pending_x_dir), (1, Some(0)));
        step(428, -10, 0, &mut a);
        assert_eq!(a.x_dir, 1);
        step(429, -10, 0, &mut a);
        assert_eq!(a.x_dir, 0);
    }
}
