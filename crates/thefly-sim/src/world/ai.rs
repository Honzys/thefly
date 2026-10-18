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
    pub fn reroll_offset(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 ai-and-rules §2.3")
    }

    /// Refresh dx, dy and distance from the swatter centre and the target's position.
    pub fn refresh(&mut self, swatter: (i32, i32), target: (i32, i32)) {
        let _ = (swatter, target);
        todo!("W2 ai-and-rules §2.3")
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
    pub fn step(&mut self, dx: i32, dy: i32, cx: &mut Ctx) -> Alignment {
        let _ = (dx, dy, cx);
        todo!("W2 ai-and-rules §2.4 step 7")
    }

    /// The four direction flags for [`super::farmer::Farmer::move_by`].
    pub fn pad(&self) -> FarmerPad {
        todo!("W2 ai-and-rules §2.4 step 9")
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
