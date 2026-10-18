//! W2. Tick step 17: random animal sounds (`FUN_00441b3c`), the farmer's voices and footsteps
//! (`FUN_0043c730`), and the tutorial hint bubbles (`FUN_00443772`, `FUN_00443488`).

use super::World;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, SoundSlot};
use crate::match_state::MatchState;

/// The animal noises, slots 14..=21 (GS §3, §8 audio map).
pub const ANIMAL_SLOTS: [u8; 8] = [14, 15, 16, 17, 18, 19, 20, 21];
/// A sound plays on ticks where rand % 150 == 0.
pub const ANIMAL_CHANCE: i32 = 150;
/// Below 71 on R(100) the pick is one of the first three slots, else any of the eight.
pub const ANIMAL_COMMON_BELOW: i32 = 71;
pub const ANIMAL_COMMON_SLOTS: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ambient {
    /// Tutorial mode only: the bubble shown (fly or farmer side) and its timer. The port never
    /// sets it (see [`World::ambient_step`]).
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
    /// Step 17. Port decision (answers.md, O12): the tutorial bubbles are left out; they draw no
    /// rand, play no sound and write no game state, so nothing desyncs.
    pub fn ambient_step(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        let _ = m;
        self.ambient.animal_sound(cx);
        // shortcut: answers.md "Farmer voice 33" [I], a plain drop clears the port's carried
        // index, while the original may keep +0x1a until delivery. Revisit with FUN_0042c888's
        // drop path.
        let holding = self.objects.carried.is_some();
        self.farmer.voices(holding, !self.fly.alive(), cx);
    }
}

impl Ambient {
    /// `FUN_00441b3c` (answers.md "Animal sounds"): one draw every tick, a sound one time in 150.
    fn animal_sound(&mut self, cx: &mut Ctx) {
        if cx.rng.r(ANIMAL_CHANCE) != 0 {
            return;
        }
        let i = if cx.rng.r(100) < ANIMAL_COMMON_BELOW {
            cx.rng.r(ANIMAL_COMMON_SLOTS)
        } else {
            cx.rng.r(ANIMAL_SLOTS.len() as i32)
        };
        let slot = usize::try_from(i).map_or(ANIMAL_SLOTS[0], |i| ANIMAL_SLOTS[i]);
        cx.emit(Event::play(SoundSlot(slot)));
    }
}

#[cfg(test)]
mod tests {
    use super::super::farmer::rig::Rig;
    use super::*;

    /// answers.md "Animal sounds": seed 1 draws 41, and 41 % 150 != 0, so nothing plays and
    /// only that one draw is spent (the next is 18467).
    #[test]
    fn most_ticks_spend_one_draw_and_play_nothing() {
        let mut rig = Rig::new(1);
        Ambient::default().animal_sound(&mut rig.cx(0));
        assert!(rig.events.is_empty());
        assert_eq!(rig.rng.rand(), 18467);
    }

    /// Seed 126: 450 % 150 = 0, 18530 % 100 = 30 < 71, 11131 % 3 = 1, so slot 15; next 19758.
    #[test]
    fn a_hit_below_71_picks_one_of_the_first_three_slots() {
        let mut rig = Rig::new(126);
        Ambient::default().animal_sound(&mut rig.cx(0));
        assert_eq!(rig.events, [Event::play(SoundSlot(15))]);
        assert_eq!(rig.rng.rand(), 19758);
    }

    /// Seed 2193: 7200 % 150 = 0, 18782 % 100 = 82 ≥ 71, 6838 % 8 = 6, so slot 20; next 16804.
    #[test]
    fn a_hit_from_71_picks_any_of_the_eight_slots() {
        let mut rig = Rig::new(2193);
        Ambient::default().animal_sound(&mut rig.cx(0));
        assert_eq!(rig.events, [Event::play(SoundSlot(20))]);
        assert_eq!(rig.rng.rand(), 16804);
    }

    /// answers.md "Farmer voice 33": voice 33 plays on the tick the fly grabs an object. Seed 15
    /// draws 87 (no animal), 5105 % 100 = 5 < 15 (voice 33); the next tick, still holding,
    /// draws only 27056 for the animal roll, leaving 2938 next.
    #[test]
    fn voice_33_plays_when_the_fly_grabs_an_object() {
        use crate::match_state::{MatchConfig, Players};
        use crate::rng::MsvcRand;
        let mut rig = Rig::new(1);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = MatchState::new(config, false, &mut rig.cx(0));
        let mut w = World::bare(&m, &mut rig.cx(0));
        assert!(w.fly.alive());
        (rig.rng, rig.events) = (MsvcRand::new(15), Vec::new());
        w.objects.carried = Some(0);
        w.ambient_step(&mut m, &mut rig.cx(0));
        let voice_33 =
            |e: &Event| matches!(e, Event::Sound { slot, .. } if *slot == SoundSlot::VOICE_LOSE);
        assert_eq!(rig.events.iter().filter(|e| voice_33(e)).count(), 1);
        w.ambient_step(&mut m, &mut rig.cx(15));
        assert_eq!(rig.events.iter().filter(|e| voice_33(e)).count(), 1);
        assert_eq!(rig.rng.rand(), 2938);
    }
}
