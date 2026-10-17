//! W2. The friendly boot (fly-and-world §7, ai-and-rules §6.1). Ticks steps 7 and 10.

use super::World;
use crate::clock::Timer;
use crate::ctx::Ctx;

pub const RETARGET_FIRST_MS: u64 = 1000;
/// The boot leaves after this many floor hits (count > 9).
pub const MAX_STOMPS: i32 = 9;
/// Splatting the fly sets the count to this, ending the boot after the stomp.
pub const SPLAT_STOMPS: i32 = 15;
/// Stomp when the horizontal distance to the target is below this.
pub const STOMP_DISTANCE: f64 = 11.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Boot {
    /// Not spawned, or gone (`FUN_0043b33b` false).
    pub active: bool,
    /// Layer x on the 1.25 layer, and y.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub vx: i32,
    pub vy: i32,
    pub accel_y: i32,
    pub horizontal_on: bool,
    pub vertical_on: bool,
    pub chasing: bool,
    pub stomping: bool,
    pub stomps: i32,
    /// Burst acceleration (§7.2 step 5); 0 is none.
    pub accel: i32,
    pub offset: i32,
    pub retarget: Timer,
    /// +0x178. Open item: what sets it.
    pub ready: bool,
}

impl Boot {
    /// An inactive boot with the `i_botte` frame size.
    pub fn new(w: i32, h: i32) -> Self {
        Self {
            active: false,
            x: 0,
            y: 0,
            w,
            h,
            vx: 0,
            vy: 0,
            accel_y: 0,
            horizontal_on: false,
            vertical_on: false,
            chasing: false,
            stomping: false,
            stomps: 0,
            accel: 0,
            offset: 0,
            retarget: Timer::UNARMED,
            ready: false,
        }
    }

    /// `FUN_0043aabe`, from the farmer's Boot goodie.
    pub fn spawn(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 fly-and-world §7.1")
    }
}

impl World {
    /// Step 7: clear the active flag when gone; otherwise update unless paused.
    pub fn boot_step(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 fly-and-world §7.2; open item: the sin call (ai-and-rules Still open 5)")
    }

    /// Step 10: only while the fly is alive, the contact test (`FUN_0043b157`) and the push
    /// (`Fly::push_by_boot`), which may splat the fly.
    pub fn boot_collision(&mut self, cx: &mut Ctx) {
        let _ = cx;
        todo!("W2 fly-and-world §2 step 10, §7.3")
    }
}
