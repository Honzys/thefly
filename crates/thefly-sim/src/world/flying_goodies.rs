//! W2. Flying goodies: luck, spawn, the wander mover, update and catch (fly-and-world §6,
//! ai-and-rules §6.2). Ticks step 16. [`Wander`] is the shared mover (`FUN_004172a8`).

use super::World;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::match_state::MatchState;

pub const FIRST_CHECK_BASE_MS: u64 = 8000;
pub const FIRST_CHECK_RAND: i32 = 7000;
pub const FLY_LIFETIME_MS: u64 = 8000;
pub const FARMER_LIFETIME_MS: u64 = 12_000;
/// Kind 15 is the mystery goodie; it is drawn when R(100) is below this.
pub const MYSTERY_KIND: i32 = 15;
pub const MYSTERY_PERCENT: i32 = 3;
/// n = the `Goodies` sheet's frame count (5 × 2); kinds are R(trunc(n/2 − 2)) + 3.
pub const SHEET_FRAMES: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Fly,
    Farmer,
}

/// The wander mover: per-axis target speed re-rolled every R(reroll) ms, ramped by `accel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wander {
    pub reroll_rand_ms: i32,
    pub speed_x: i32,
    pub speed_y: i32,
    /// Same-direction run lengths per axis.
    pub run_x: i32,
    pub run_y: i32,
    pub accel: i32,
    pub vx: i32,
    pub vy: i32,
    pub target_vx: i32,
    pub target_vy: i32,
    pub timer: Timer,
    /// Remainder state of the FPS-based integrator (`FUN_004180af`).
    pub frames: i32,
    pub rem_x: i32,
    pub rem_y: i32,
}

impl Wander {
    /// One tick: maybe re-roll, ramp, integrate; returns (dx, dy) in px.
    pub fn step(&mut self, cx: &mut Ctx) -> (i32, i32) {
        let _ = cx;
        todo!("W2 fly-and-world §4.1 FPS integrator, §6.2 wander mover")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flyer {
    pub active: bool,
    /// 3, 4, 5, or [`MYSTERY_KIND`].
    pub kind: i32,
    /// Sheet frame for the renderer.
    pub frame: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub mover: Wander,
    pub lifetime: Timer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlyingGoodies {
    pub fly: Flyer,
    pub farmer: Flyer,
    pub fly_check: Timer,
    pub farmer_check: Timer,
}

/// `FUN_0040c086(side)`: the balance meter p in [0, 1]; the trailing side gets more than 0.5.
/// W3 owns the formula ([`crate::economy::Economy::meter`]); this is the call site's name.
pub fn luck(m: &MatchState, side: Side) -> f64 {
    m.economy.meter(side)
}

impl FlyingGoodies {
    /// Both check timers at R(7000) + 8000 ms, farmer first.
    pub fn new(cx: &mut Ctx) -> Self {
        let mut first_check = || {
            let ms = FIRST_CHECK_BASE_MS + cx.rng.r(FIRST_CHECK_RAND) as u64;
            Timer::armed(cx.now, ms)
        };
        let farmer_check = first_check();
        let fly_check = first_check();
        Self {
            farmer_check,
            fly_check,
            ..Self::default()
        }
    }
}

impl World {
    /// Step 16: spawn (`FUN_00432684`), update (`FUN_00432dfb`), catch (`FUN_0043309b`).
    /// The farmer catches only while the swing frame is 3 or more and the "cannot hit" flag is
    /// clear, against the box (x, trunc(reach), 131, 154), bounds inclusive (gaps §5.1). The
    /// trailing-side re-roll is `R(100) <= 2q` for the fly and `< 2q` for the farmer (gaps §5.2).
    pub fn flying_goodies_step(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let _ = (m, cx);
        todo!("W2 fly-and-world §6.2, §6.3")
    }
}
