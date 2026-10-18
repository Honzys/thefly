//! W2. Tick step 17: random animal sounds (`FUN_00441b3c`), the farmer's voices and footsteps
//! (`FUN_0043c730`), and the tutorial hint bubbles (`FUN_00443772`, `FUN_00443488`).

use super::World;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, SoundSlot};
use crate::match_state::MatchState;

/// Slots 14..=21 are the animal noises (GS §3, §8 audio map).
pub const ANIMAL_FIRST_SLOT: u8 = 14;
pub const ANIMAL_SLOTS: u8 = 8;
/// Open item O11: a guessed interval.
pub const ANIMAL_BASE_MS: i32 = 15_000;
pub const ANIMAL_RAND_MS: i32 = 15_000;

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
    // shortcut: O12, the tutorial bubbles are not decoded; the port shows none. Revisit with
    // FUN_00443772 and FUN_00443488.
    // shortcut: AR §1.5 "objects left on the table" is read literally (a collectible object is
    // still active), so its rising edge is the period's first tick; "an object left the table"
    // fits the slot's name (perdu) better. Revisit with FUN_0043c730's caller.
    pub fn ambient_step(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let _ = m;
        self.ambient.animal_sound(cx);
        let objects_left = self.objects.list.iter().any(|o| o.active && o.collectible);
        let fly_dead = self.fly.dying || self.fly.splatted;
        self.farmer.voices(objects_left, fly_dead, cx);
    }
}

impl Ambient {
    /// GS §3: animal noises (slots 14–21) at random intervals.
    // shortcut: O11, the interval and choice rule are not documented; the port plays one of the
    // eight slots, chosen by R(8), every ANIMAL_BASE_MS + R(ANIMAL_RAND_MS). Revisit with
    // FUN_00441b3c.
    fn animal_sound(&mut self, cx: &mut Ctx) {
        let due = !self.animal_timer.is_armed() || self.animal_timer.expired(cx.now);
        if !due {
            return;
        }
        let first_call = !self.animal_timer.is_armed();
        let ms = ANIMAL_BASE_MS + cx.rng.r(ANIMAL_RAND_MS);
        self.animal_timer
            .arm(cx.now, u64::try_from(ms).unwrap_or(0));
        if first_call {
            return;
        }
        let slot = SoundSlot(ANIMAL_FIRST_SLOT + rand_u8(cx, ANIMAL_SLOTS));
        cx.emit(Event::play(slot));
    }
}

fn rand_u8(cx: &mut Ctx, n: u8) -> u8 {
    u8::try_from(cx.rng.r(i32::from(n))).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::super::farmer::rig::Rig;
    use super::*;

    /// The first call only arms the timer: seed 1 gives 15000 + 41 ms; the expiry plays slot
    /// 14 + 6334 % 8 = 20 after re-arming with 18467.
    #[test]
    fn animal_sounds_play_one_of_eight_slots_on_a_timer() {
        let mut rig = Rig::new(1);
        let mut a = Ambient::default();
        a.animal_sound(&mut rig.cx(0));
        assert_eq!(a.animal_timer.remaining_ms(0), 15_041);
        a.animal_sound(&mut rig.cx(15_040));
        assert!(rig.events.is_empty());
        a.animal_sound(&mut rig.cx(15_041));
        assert_eq!(rig.events, [Event::play(SoundSlot(20))]);
    }
}
