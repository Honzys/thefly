//! W2. Goodie selectors and use for both sides, and the mystery effects' expiry
//! (game-spec §4.7, ai-and-rules §5, fly-and-world §2 steps 3, 4 and 15).

use super::World;
use super::farmer::{SPRITE_H, SPRITE_W};
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Effect, Event, SoundSlot};
use crate::input::{FarmerPad, FlyPad};
use crate::match_state::MatchState;

/// A mystery effect's timer (both sides).
pub const MYSTERY_MS: u64 = 15_000;
/// After a goodie use or an unpause, the action/swing memory is set to this (blocks until the
/// key is released).
pub const USE_BLOCK: i32 = 10;

/// Selector values 1..=3 for the fly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlyGoodie {
    Turbo,
    SuperFly,
    Midges,
}

/// Selector values 1..=3 for the farmer, also the AI's request codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FarmerGoodie {
    Triple,
    Nuclear,
    Boot,
}

/// A side's active mystery (game+0x40898 for the fly, game+0x40899 for the farmer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mystery {
    #[default]
    None,
    /// Fly: inverted controls. Farmer: the swatter cannot hit.
    First,
    /// Fly: 25 s Turbo. Farmer: 20 s Nuclear.
    Second,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Goodies {
    /// game+0x408a8, 0..=3; 0 is closed.
    pub fly_selector: u8,
    /// game+0x408aa, 0..=3.
    pub farmer_selector: u8,
    pub fly_mystery: Mystery,
    pub fly_mystery_timer: Timer,
    pub farmer_mystery: Mystery,
    pub farmer_mystery_timer: Timer,
    /// The AI's request collected this tick (`FUN_0043f885`), spent by step 4.
    pub farmer_request: Option<FarmerGoodie>,
}

/// The selector has three goodies plus 0 (closed).
pub const SELECTOR_STATES: u8 = 4;

impl FlyGoodie {
    fn from_selector(selector: u8) -> Option<Self> {
        match selector {
            1 => Some(Self::Turbo),
            2 => Some(Self::SuperFly),
            3 => Some(Self::Midges),
            _ => None,
        }
    }
}

impl FarmerGoodie {
    fn from_selector(selector: u8) -> Option<Self> {
        match selector {
            1 => Some(Self::Triple),
            2 => Some(Self::Nuclear),
            3 => Some(Self::Boot),
            _ => None,
        }
    }
}

fn next(selector: u8) -> u8 {
    (selector + 1) % SELECTOR_STATES
}

impl World {
    /// Step 3 (`FUN_00404198`): Right Shift cycles; Enter uses when the count > 0 (stardust,
    /// slot 38; Midges also slot 48). Returns the action, cancelled on any use attempt.
    pub fn fly_goodie(
        &mut self,
        pad: &FlyPad,
        action: bool,
        m: &mut MatchState,
        cx: &mut Ctx,
    ) -> bool {
        let g = &mut self.goodies;
        if pad.goodie_cycle {
            g.fly_selector = next(g.fly_selector);
        }
        let Some(goodie) = FlyGoodie::from_selector(g.fly_selector) else {
            return action;
        };
        if !pad.action {
            return action;
        }
        g.fly_selector = 0;
        self.fly_action_memory = USE_BLOCK;
        let inv = &mut m.economy.fly;
        let count = match goodie {
            FlyGoodie::Turbo => &mut inv.turbo,
            FlyGoodie::SuperFly => &mut inv.superfly,
            FlyGoodie::Midges => &mut inv.midges,
        };
        if *count <= 0 {
            return false;
        }
        *count -= 1;
        match goodie {
            FlyGoodie::Turbo => self.fly.start_turbo(None, cx),
            FlyGoodie::SuperFly => self.fly.start_superfly(cx),
            FlyGoodie::Midges => {
                self.midges.spawn(cx);
                cx.emit(Event::play(SoundSlot::MIDGES));
            }
        }
        let (x, y) = self.fly.centre();
        cx.emit(Event::Effect(Effect::Stardust { x, y }));
        cx.emit(Event::play(SoundSlot::GOODIE_FLY));
        false
    }

    /// Step 4 (`FUN_004043c1`), human farmer: the goodie key's press cycles the selector, and Tab
    /// held with a selection uses it. Returns the swing edge, cancelled on any use attempt. The
    /// AI's request is spent in step 6 instead ([`World::spend_farmer_request`]).
    pub fn farmer_goodie(
        &mut self,
        pad: &FarmerPad,
        swing_edge: bool,
        m: &mut MatchState,
        cx: &mut Ctx,
    ) -> bool {
        if self.ai.is_some() {
            return swing_edge;
        }
        let Some(goodie) = self.human_farmer_goodie(pad) else {
            return swing_edge;
        };
        self.use_farmer_goodie(goodie, m, cx);
        false
    }

    /// The AI path of `FUN_004043c1` (answers.md "Farmer goodie use"): after fly physics and
    /// before the AI update, so its draws (the boot spawn) follow the fly's.
    pub(super) fn spend_farmer_request(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        if let Some(goodie) = self.goodies.farmer_request.take() {
            self.use_farmer_goodie(goodie, m, cx);
        }
    }

    /// A use attempt: the selector closes and the swing memory blocks, in stock or not. Triple
    /// and Nuclear only while Nuclear is off; slot 37.
    fn use_farmer_goodie(&mut self, goodie: FarmerGoodie, m: &mut MatchState, cx: &mut Ctx) {
        self.goodies.farmer_selector = 0;
        self.farmer_swing_memory = USE_BLOCK;
        let inv = &mut m.economy.farmer;
        let nuclear = self.farmer.nuclear.on;
        let count = match goodie {
            FarmerGoodie::Triple if !nuclear => &mut inv.triple,
            FarmerGoodie::Nuclear if !nuclear => &mut inv.nuclear,
            FarmerGoodie::Boot => &mut inv.boot,
            FarmerGoodie::Triple | FarmerGoodie::Nuclear => return,
        };
        if *count <= 0 {
            return;
        }
        *count -= 1;
        match goodie {
            FarmerGoodie::Triple => self.farmer.start_triple(),
            FarmerGoodie::Nuclear => self.farmer.start_nuclear(None, cx),
            FarmerGoodie::Boot => self.boot.spawn(cx),
        }
        let (x, y) = (self.farmer.x + SPRITE_W / 2, self.farmer.y + SPRITE_H / 2);
        cx.emit(Event::Effect(Effect::Stardust { x, y }));
        cx.emit(Event::play(SoundSlot::GOODIE_FARMER));
    }

    /// The human path of step 4: a goodie-key press cycles, Tab with a selection uses it.
    fn human_farmer_goodie(&mut self, pad: &FarmerPad) -> Option<FarmerGoodie> {
        let g = &mut self.goodies;
        if pad.goodie_pressed {
            g.farmer_selector = next(g.farmer_selector);
        }
        if !pad.swing {
            return None;
        }
        FarmerGoodie::from_selector(g.farmer_selector)
    }

    /// Step 15: fly side ends Turbo when the mystery was Turbo; farmer side lets the swatter hit
    /// again or ends Nuclear.
    pub fn mystery_expiry(&mut self, cx: &mut Ctx) {
        let g = &mut self.goodies;
        if g.fly_mystery != Mystery::None && g.fly_mystery_timer.expired(cx.now) {
            if g.fly_mystery == Mystery::Second {
                self.fly.end_turbo();
            }
            g.fly_mystery = Mystery::None;
        }
        if g.farmer_mystery != Mystery::None && g.farmer_mystery_timer.expired(cx.now) {
            match g.farmer_mystery {
                Mystery::First => self.farmer.swing.blocked = false,
                Mystery::Second => self.farmer.end_nuclear(),
                Mystery::None => {}
            }
            g.farmer_mystery = Mystery::None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::input::{FarmerPad, FlyPad};
    use crate::match_state::{MatchConfig, MatchState, Players};

    fn setup(rig: &mut Rig) -> (World, MatchState) {
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let m = MatchState::new(config, false, &mut rig.cx(0));
        let w = World::bare(&m, &mut rig.cx(0));
        (w, m)
    }

    const CYCLE: FlyPad = FlyPad {
        up: false,
        down: false,
        left: false,
        right: false,
        action: false,
        goodie_cycle: true,
    };
    const ENTER: FlyPad = FlyPad {
        action: true,
        goodie_cycle: false,
        ..CYCLE
    };

    #[test]
    fn right_shift_cycles_the_selector_through_0_to_3() {
        let mut rig = Rig::new(1);
        let (mut w, mut m) = setup(&mut rig);
        let mut seen = Vec::new();
        for _ in 0..4 {
            assert!(w.fly_goodie(&CYCLE, true, &mut m, &mut rig.cx(0)));
            seen.push(w.goodies.fly_selector);
        }
        assert_eq!(seen, [1, 2, 3, 0]);
        assert!(rig.events.is_empty());
    }

    /// FW §2 step 3: with count 0 nothing is used, but the selector closes, the action is
    /// cancelled and Enter is blocked until released.
    #[test]
    fn enter_with_count_0_uses_nothing() {
        let mut rig = Rig::new(1);
        let (mut w, mut m) = setup(&mut rig);
        m.economy.fly.midges = 0;
        w.goodies.fly_selector = 3;
        assert!(!w.fly_goodie(&ENTER, true, &mut m, &mut rig.cx(0)));
        assert_eq!(
            (w.goodies.fly_selector, w.fly_action_memory),
            (0, USE_BLOCK)
        );
        assert!(rig.events.is_empty() && !w.midges.on);
    }

    #[test]
    fn a_midges_use_spends_one_and_plays_38_and_48() {
        let mut rig = Rig::new(1);
        let (mut w, mut m) = setup(&mut rig);
        m.economy.fly.midges = 2;
        w.goodies.fly_selector = 3;
        assert!(!w.fly_goodie(&ENTER, true, &mut m, &mut rig.cx(0)));
        assert_eq!(m.economy.fly.midges, 1);
        assert!(w.midges.on);
        assert!(rig.events.contains(&Event::play(SoundSlot::GOODIE_FLY)));
        assert!(rig.events.contains(&Event::play(SoundSlot::MIDGES)));
    }

    /// AR §5: the goodie key cycles, Tab uses; Triple is refused while Nuclear is on.
    #[test]
    fn the_farmer_uses_triple_with_tab_unless_nuclear_is_on() {
        let mut rig = Rig::new(1);
        let (mut w, mut m) = setup(&mut rig);
        m.economy.farmer.triple = 1;
        let press = FarmerPad {
            goodie_pressed: true,
            goodie: true,
            ..FarmerPad::default()
        };
        let tab = FarmerPad {
            swing: true,
            ..FarmerPad::default()
        };
        w.farmer_goodie(&press, false, &mut m, &mut rig.cx(0));
        w.farmer.nuclear.on = true;
        assert!(!w.farmer_goodie(&tab, true, &mut m, &mut rig.cx(0)));
        assert_eq!(m.economy.farmer.triple, 1);
        w.farmer.nuclear.on = false;
        w.goodies.farmer_selector = 1;
        assert!(!w.farmer_goodie(&tab, true, &mut m, &mut rig.cx(0)));
        assert_eq!(m.economy.farmer.triple, 0);
        assert_eq!(
            (w.goodies.farmer_selector, w.farmer_swing_memory),
            (0, USE_BLOCK)
        );
        assert!(rig.events.contains(&Event::play(SoundSlot::GOODIE_FARMER)));
    }

    /// Steps 2, 4 and 6 for one tick of the human farmer.
    fn farmer_tick(w: &mut World, pad: &FarmerPad, m: &mut MatchState, rig: &mut Rig) {
        let edge = w.farmer_swing_edge(pad);
        let edge = w.farmer_goodie(pad, edge, m, &mut rig.cx(0));
        w.farmer_step(pad, edge, m, &mut rig.cx(0));
    }

    /// answers.md "Farmer goodie use cancels the same tick's swing": the Tab press that uses a
    /// goodie starts no swat, and an attempt with nothing in stock cancels it too.
    #[test]
    fn a_farmer_goodie_attempt_cancels_that_ticks_swing() {
        let mut rig = Rig::new(1);
        let (mut w, mut m) = setup(&mut rig);
        let tab = FarmerPad {
            swing: true,
            ..FarmerPad::default()
        };
        for triples in [1, 0] {
            m.economy.farmer.triple = triples;
            w.goodies.farmer_selector = 1;
            farmer_tick(&mut w, &tab, &mut m, &mut rig);
            assert_eq!((w.farmer.swing.frame, m.stats.swings), (0, 0));
            assert_eq!(w.goodies.farmer_selector, 0);
            farmer_tick(&mut w, &FarmerPad::default(), &mut m, &mut rig);
        }
        assert_eq!(m.economy.farmer.triple, 0);
        farmer_tick(&mut w, &tab, &mut m, &mut rig);
        assert_eq!((w.farmer.swing.frame, m.stats.swings), (1, 1));
    }

    /// The stardust sits at (farmer x + 181/2, farmer y + 178/2).
    #[test]
    fn the_farmer_goodie_effect_is_centred_on_the_sprite() {
        let mut rig = Rig::new(1);
        let (mut w, mut m) = setup(&mut rig);
        m.economy.farmer.nuclear = 1;
        (w.farmer.x, w.farmer.y, w.goodies.farmer_selector) = (100, 400, 2);
        let tab = FarmerPad {
            swing: true,
            ..FarmerPad::default()
        };
        w.farmer_goodie(&tab, true, &mut m, &mut rig.cx(0));
        let stardust = Event::Effect(Effect::Stardust { x: 190, y: 489 });
        assert!(rig.events.contains(&stardust), "{:?}", rig.events);
    }

    /// AI path: the request is spent after fly physics, in step 6 before the AI update, not in
    /// step 4.
    #[test]
    fn the_ai_request_is_spent_in_the_farmer_step() {
        let mut rig = Rig::new(1);
        let mut m = MatchState::new(MatchConfig::default(), false, &mut rig.cx(0));
        m.advance(&mut rig.cx(0));
        let mut w = World::new(&mut m, &mut rig.cx(0));
        assert!(w.ai.is_some());
        m.economy.farmer.boot = 1;
        w.goodies.farmer_request = Some(FarmerGoodie::Boot);
        let pad = FarmerPad::default();
        assert!(w.farmer_goodie(&pad, true, &mut m, &mut rig.cx(0)));
        assert!(!w.boot.active && m.economy.farmer.boot == 1);
        w.farmer_step(&pad, false, &mut m, &mut rig.cx(0));
        assert!(w.boot.active && m.economy.farmer.boot == 0);
    }

    /// A mystery lasts 15000 ms: tick 975 is exactly 15000, so the "cannot hit" flag clears on
    /// tick 976.
    #[test]
    fn a_mystery_lasts_15000_ms() {
        let mut rig = Rig::new(1);
        let (mut w, _) = setup(&mut rig);
        w.goodies.farmer_mystery = Mystery::First;
        w.goodies.farmer_mystery_timer = Timer::armed(0, MYSTERY_MS);
        w.farmer.swing.blocked = true;
        w.mystery_expiry(&mut rig.at(975));
        assert!(w.farmer.swing.blocked);
        w.mystery_expiry(&mut rig.at(976));
        assert!(!w.farmer.swing.blocked);
        assert_eq!(w.goodies.farmer_mystery, Mystery::None);
    }
}
