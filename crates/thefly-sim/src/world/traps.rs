//! W2. Trap entities on the table: flypapers, fans and zappers (game-spec §4.6, fly-and-world
//! §7.4). Built each period from the shop's [`TrapLayout`]. Ticks step 13.

use super::World;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::match_state::TrapLayout;

pub const MAX_FLYPAPERS: usize = 7;
pub const MAX_FANS: usize = 5;
pub const MAX_ZAPPERS: usize = 2;
/// Fans hit the fly at most once per this many ms.
pub const FAN_REHIT_MS: u64 = 100;
/// Flypaper divides both velocity components by this each tick of overlap.
pub const FLYPAPER_DRAG: f64 = 1.4;
/// Zapper arc: upward push and damage per tick.
pub const ARC_PUSH: i32 = 180;
pub const ARC_DAMAGE: i32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapKind {
    Flypaper,
    FanLeft,
    FanRight,
    Zapper,
}

/// A row of the trap table (ai-and-rules §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrapSpec {
    pub kind: TrapKind,
    pub sprite: &'static str,
    pub cost: i32,
    /// Cap per type; the two fans share theirs.
    pub cap: usize,
    /// Width for the placement spacing, shop px.
    pub width: i32,
    pub stored_y: i32,
}

pub const TRAP_TABLE: [TrapSpec; 4] = [
    TrapSpec {
        kind: TrapKind::Flypaper,
        sprite: "colle_mouche1",
        cost: 1,
        cap: 7,
        width: 8,
        stored_y: 0,
    },
    TrapSpec {
        kind: TrapKind::FanLeft,
        sprite: "ventilo_gauche",
        cost: 2,
        cap: 5,
        width: 34,
        stored_y: 600,
    },
    TrapSpec {
        kind: TrapKind::FanRight,
        sprite: "ventilo_droite",
        cost: 2,
        cap: 5,
        width: 34,
        stored_y: 600,
    },
    TrapSpec {
        kind: TrapKind::Zapper,
        sprite: "tue_mouche",
        cost: 4,
        cap: 2,
        width: 97,
        stored_y: 0,
    },
];

impl TrapKind {
    pub fn spec(self) -> &'static TrapSpec {
        &TRAP_TABLE[self as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flypaper {
    pub variant2: bool,
    /// Layer x on the 1.25 layer, and y.
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fan {
    pub facing_right: bool,
    pub x: i32,
    pub y: i32,
    /// +0x91c. Open item O9: its value.
    pub strength: i32,
    /// Loud loop (−100) while blowing, else −400. Open item O9: the blow schedule.
    pub blowing: bool,
    pub rehit: Timer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zapper {
    pub x: i32,
    pub y: i32,
    /// Arc active (loop −200, slot 39). Open item O9: the arc schedule (`FUN_00428b47`).
    pub arcing: bool,
    pub arc_timer: Timer,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Traps {
    pub flypapers: Vec<Flypaper>,
    pub fans: Vec<Fan>,
    pub zappers: Vec<Zapper>,
}

impl Traps {
    /// The period's traps from the shop slots (stored x is trunc(shop px / 0.152), gaps §11.4).
    /// Caps 7/5/2 skip the rest (gaps §8.2); each flypaper built under its cap draws `R(10)` and
    /// uses the `colle_mouche2` sprite when the draw is above 4 (`variant2`), in slot order.
    pub fn from_layout(layout: &TrapLayout, cx: &mut Ctx) -> Self {
        let _ = (layout, cx);
        todo!("W2 game-spec §4.6, ai-and-rules §4.3")
    }

    /// `FUN_00427bb4`: may an object occupy [x, x + w] (object layer px)? Used by placement.
    pub fn span_free(&self, x: i32, w: i32) -> bool {
        let _ = (x, w);
        todo!("W2 fly-and-world §5.2 step 3; open item O22")
    }
}

impl World {
    /// Step 13 (`FUN_00427270`): flypaper drag, fan push and damage, zapper grill and arc, the
    /// loops (44, 40) and their positional single-trap variants.
    pub fn traps_step(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 game-spec §4.6, fly-and-world §7.4")
    }
}
