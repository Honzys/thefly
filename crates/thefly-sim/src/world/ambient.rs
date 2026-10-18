//! W2. Tick step 17: random animal sounds (`FUN_00441b3c`), the farmer's voices and footsteps
//! (`FUN_0043c730`), and the tutorial hint bubbles (`FUN_00443772`, `FUN_00443488`).

use super::World;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::match_state::MatchState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ambient {
    /// Open item O11: the animal-sound interval and choice rule are not in the docs.
    pub animal_timer: Timer,
    /// Tutorial mode only: the bubble shown (fly or farmer side) and its timer. Open item O12: the
    /// hint sequence and triggers are not decoded.
    pub tutorial: Option<TutorialBubble>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TutorialBubble {
    pub farmer_side: bool,
    /// Sheet frame of `tutoBulFly`/`tutoBulFarmer` (2 frames).
    pub frame: i32,
    pub until: Timer,
}

impl World {
    /// Step 17.
    pub fn ambient_step(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let _ = (m, cx);
        todo!("W2 fly-and-world §2 step 17, ai-and-rules §1.5; open items O11, O12")
    }
}
