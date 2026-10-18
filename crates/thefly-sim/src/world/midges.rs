//! W2. The Midges goodie's swarm (fly-and-world §8, ai-and-rules §6.3) and the ambient midges
//! (tick step 14). Ticks steps 14 and 18a.

use super::World;
use super::flying_goodies::Wander;
use crate::ctx::Ctx;

pub const SWARM_SLOTS: usize = 300;
/// Each use adds min(this, 300 − active) midges.
pub const SWARM_SPAWN: usize = 100;
/// A swatter strike kills at most this many touched midges.
pub const SWAT_KILLS: usize = 20;
/// Traps push a touching midge by this on each axis, keeping the sign.
pub const TRAP_PUSH: i32 = 180;
/// Per-tick chance (%) to re-roll a midge's homing point.
pub const RETARGET_PERCENT: i32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Midge {
    pub active: bool,
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    /// R(5) + 30 px.
    pub size: i32,
    pub mover: Wander,
    /// Homing point on the farmer: offsets R(farmerW), R(farmerH − 60); tolerances R(30).
    pub home_dx: i32,
    pub home_dy: i32,
    pub tol_x: i32,
    pub tol_y: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Swarm {
    /// game+0x407d0.
    pub on: bool,
    pub slots: Vec<Midge>,
    /// Midges spawned in this swarm (the slowdown's denominator).
    pub total: i32,
}

impl Swarm {
    pub fn new() -> Self {
        Self {
            on: false,
            slots: vec![Midge::default(); SWARM_SLOTS],
            total: 0,
        }
    }

    /// `FUN_00404caf`: add min(100, 300 − active) midges.
    pub fn spawn(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 fly-and-world §8.1")
    }
}

impl Default for Swarm {
    fn default() -> Self {
        Self::new()
    }
}

/// The cosmetic population at game+0x15a20 that reacts to the swatter and fans. Open item O2: its
/// count and spawn rule are not in the docs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AmbientMidges {
    pub slots: Vec<Midge>,
}

impl World {
    /// Step 14.
    pub fn ambient_midges_step(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 fly-and-world §2 step 14, gaps §8.1 contact routine; open item O2")
    }

    /// Step 18a (`FUN_00404ef3`, render pass in the original): homing, swatter kills, trap push,
    /// the farmer slowdown pulse, swarm end.
    pub fn midges_step(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 fly-and-world §8.2, gaps §8.1 (contact box and the 20-slot count)")
    }
}
