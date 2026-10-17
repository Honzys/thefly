//! The scenario harness: a seed, a script of held keys per tick, and a run that collects every
//! event with its tick. Every worker's acceptance tests are scenarios (plan, "Acceptance").

use crate::clock::first_tick_at;
use crate::data::GameData;
use crate::event::Event;
use crate::input::{KeyState, Scancode};
use crate::saves::Persisted;
use crate::sim::Sim;

/// Hold `keys` for ticks `from..to` (half-open).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub from: u64,
    pub to: u64,
    pub keys: Vec<Scancode>,
}

impl Hold {
    /// A one-tick press at the first tick whose `now_ms` is ≥ `ms`.
    pub fn tap_at_ms(ms: u64, key: Scancode) -> Self {
        let from = first_tick_at(ms);
        Self {
            from,
            to: from + 1,
            keys: vec![key],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scenario {
    pub seed: u32,
    pub persisted: Persisted,
    pub script: Vec<Hold>,
}

/// Everything a scenario produced, in tick order.
#[derive(Debug, Default)]
pub struct Run {
    pub events: Vec<(u64, Event)>,
}

impl Scenario {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            ..Self::default()
        }
    }

    pub fn hold(mut self, hold: Hold) -> Self {
        self.script.push(hold);
        self
    }

    pub fn sim(&self) -> Sim {
        Sim::new(self.seed, GameData::synthetic(), self.persisted.clone())
    }

    fn held_at(&self, tick: u64) -> impl Iterator<Item = Scancode> + '_ {
        self.script
            .iter()
            .filter(move |h| (h.from..h.to).contains(&tick))
            .flat_map(|h| h.keys.iter().copied())
    }

    /// Runs `sim` for `ticks` ticks from its current clock, feeding the script.
    pub fn run(&self, sim: &mut Sim, ticks: u64) -> Run {
        let mut run = Run::default();
        let mut keys = KeyState::NONE;
        for _ in 0..ticks {
            let tick = sim.clock().tick();
            keys = KeyState::next(&keys, self.held_at(tick));
            run.events
                .extend(sim.tick(&keys).into_iter().map(|e| (tick, e)));
        }
        run
    }
}

impl Run {
    /// Ticks at which `event` was emitted.
    pub fn ticks_of(&self, event: &Event) -> Vec<u64> {
        self.events
            .iter()
            .filter(|(_, e)| e == event)
            .map(|(t, _)| *t)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{LOGO_AUTO_MS, LOGO_KEY_MS, Screen};

    /// Fade length in ticks at 65 Hz: 255 / (255 / 32.5) reaches the end on tick 33.
    const FADE_TICKS: u64 = 33;

    fn on_logo(sim: &Sim) -> bool {
        matches!(sim.state().screen, Screen::Logo(_))
    }

    #[test]
    fn seed_one_gives_the_msvc_sequence() {
        let mut sim = Scenario::new(1).sim();
        let rng = sim.rng().clone();
        let mut probe = rng.clone();
        assert_eq!(
            [probe.rand(), probe.rand(), probe.rand()],
            [41, 18467, 6334]
        );
        // Boot and logo consume no rand.
        Scenario::new(1).run(&mut sim, 100);
        assert_eq!(sim.rng(), &rng);
    }

    #[test]
    fn logo_fades_out_by_itself_after_10_s() {
        let sc = Scenario::new(1);
        let leave = first_tick_at(LOGO_AUTO_MS);

        let mut sim = sc.sim();
        sc.run(&mut sim, leave + FADE_TICKS);
        assert!(
            on_logo(&sim),
            "still fading out on tick {}",
            leave + FADE_TICKS - 1
        );

        let mut sim = sc.sim();
        let run = sc.run(&mut sim, leave + FADE_TICKS + 1);
        assert_eq!(
            run.ticks_of(&Event::ScreenChanged),
            vec![0, leave + FADE_TICKS]
        );
        assert!(matches!(sim.state().screen, Screen::Menu(_)));
    }

    #[test]
    fn logo_ignores_keys_before_3_s_and_takes_them_after() {
        let early = Scenario::new(1).hold(Hold::tap_at_ms(2000, Scancode::RETURN));
        let mut sim = early.sim();
        early.run(&mut sim, first_tick_at(LOGO_KEY_MS));
        assert!(on_logo(&sim));
        let Screen::Logo(logo) = &sim.state().screen else {
            unreachable!()
        };
        assert!(!logo.leaving);

        let on_time = Scenario::new(1).hold(Hold::tap_at_ms(3100, Scancode::ESCAPE));
        let mut sim = on_time.sim();
        let run = on_time.run(&mut sim, first_tick_at(3100) + FADE_TICKS + 1);
        assert_eq!(
            run.ticks_of(&Event::ScreenChanged),
            vec![0, first_tick_at(3100) + FADE_TICKS]
        );
    }
}
