//! W3. The application state machine (game-spec §2.2): one [`Screen`] at a time, plus the quit
//! dialog overlay owned by [`crate::sim::State`]. Screens report where to go with a [`Flow`];
//! the state builds the next screen and emits [`crate::Event::ScreenChanged`].

use crate::bonus::BonusRound;
use crate::bourse::Bourse;
use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::event::{Event, Music, SoundSlot};
use crate::highscores::{HighScoreScreen, HighScoreTable};
use crate::input::{KeyState, Scancode};
use crate::match_state::{Advance, MatchState, PeriodClock, Players};
use crate::menu::Menu;
use crate::shop::{GoodiesShop, TrapsShop};
use crate::stats::StatsScreen;
use crate::winner::Winner;
use crate::world::World;

/// Fade durations (screens-and-rendering §1.4).
pub const FADE_MS: u64 = 500;
pub const FADE_END_MS: u64 = 600;
pub const LOGO_KEY_MS: u64 = 3000;
pub const LOGO_AUTO_MS: u64 = 10_000;
/// Loading: 9 progress steps, then 500 ms. docs/re/observed.md measured 2.69–2.79 s under Wine,
/// i.e. 250 ms steps; observed wins over the 200 ms of screens-and-rendering §4.1. Open item
/// O16: the step count of 9 is inferred.
pub const LOADING_STEPS: u32 = 9;
pub const LOADING_STEP_MS: u64 = 250;
pub const LOADING_TAIL_MS: u64 = 500;
/// "3, 2, 1, Go!" banner step.
pub const BANNER_STEP_MS: u64 = 1000;

/// A full-screen fade. Alpha moves by 255 / (fps × ms / 1000) per tick, so its length is fixed
/// in ticks at the original rate (screens-and-rendering §1.4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fade {
    /// 255 is fully covered.
    pub alpha: f64,
    pub step: f64,
    /// True: 0 → 255 (fade out to the colour). False: 255 → 0 (fade in).
    pub out: bool,
    pub done: bool,
}

impl Fade {
    pub fn new(out: bool, ms: u64) -> Self {
        Self {
            alpha: if out { 0.0 } else { 255.0 },
            step: 255.0 / (f64::from(FPS) * ms as f64 / 1000.0),
            out,
            done: false,
        }
    }

    /// One tick; true on the tick the fade reaches its end (`FUN_0041331b`).
    pub fn step(&mut self) -> bool {
        if self.done {
            return false;
        }
        let a = if self.out {
            self.alpha + self.step
        } else {
            self.alpha - self.step
        };
        self.alpha = a.clamp(0.0, 255.0);
        self.done = self.alpha == 0.0 || self.alpha == 255.0;
        self.done
    }
}

/// Where an out-of-match screen wants to go after this tick.
#[derive(Debug, Clone)]
pub enum Flow {
    Stay,
    Go(Screen),
    /// To the main menu (0x14); the state knows whether the intro was seen. Leaving a match
    /// this way drops its [`MatchState`].
    Menu,
    /// Menu result 2: reset the match, then the keys page (tutorial) or the first period.
    StartMatch,
    /// Open the quit dialog over this screen.
    Quit,
}

/// Where an in-match screen wants to go. Staying inside the match keeps the [`MatchState`];
/// `Leave` hands an app-level flow up.
#[derive(Debug, Clone)]
pub enum MatchFlow {
    Stay,
    Next(MatchScreen),
    Leave(Flow),
}
/// Publisher logo (0x0a). Implemented: the scenario example drives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Logo {
    pub entered_ms: u64,
    pub fade: Fade,
    pub leaving: bool,
}

impl Logo {
    pub fn new(now: u64) -> Self {
        Self {
            entered_ms: now,
            fade: Fade::new(false, FADE_MS),
            leaving: false,
        }
    }

    pub fn tick(&mut self, keys: &KeyState, cx: &mut Ctx) -> Flow {
        let ended = self.fade.step();
        if self.leaving {
            return if ended { Flow::Menu } else { Flow::Stay };
        }
        let shown = cx.now - self.entered_ms;
        if (shown >= LOGO_KEY_MS && keys.skip()) || shown >= LOGO_AUTO_MS {
            self.leaving = true;
            self.fade = Fade::new(true, FADE_MS);
        }
        Flow::Stay
    }
}

/// Keys page (0x5a). From the menu it returns there; from match start (inside a match) it
/// goes to the tutorial page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeysPage {
    pub players: Players,
}

impl KeysPage {
    /// True when Esc or Enter closes the page.
    pub fn tick(&mut self, keys: &KeyState) -> bool {
        keys.skip()
    }
}

/// Tutorial page (0xa0): Enter starts the period.
pub fn tutorial_page_tick(keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
    if !keys.pressed(Scancode::RETURN) {
        return MatchFlow::Stay;
    }
    Gameplay::enter(m, cx)
}

/// The UI click on a selection move.
pub const CLICK: Event = Event::Click {
    volume_cb: 0,
    freq_hz: None,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GamePhase {
    /// The loading screen's fixed busy-waits, replayed as time (one click per step).
    Loading {
        step: u32,
        next: Timer,
    },
    /// "3, 2, 1, Go!": 3 down to 0, one per 1000 ms; slot 52 at the start. The world ticks
    /// paused, with every key released (answers.md "Banner").
    Banner {
        count: i32,
        next: Timer,
    },
    Playing,
}

/// Gameplay period (0x32).
#[derive(Debug, Clone)]
pub struct Gameplay {
    pub phase: GamePhase,
    pub world: World,
    /// The loading backdrop: `LOADING2` when true (picked 50/50).
    pub loading_alt: bool,
    /// Loading start; the steps are due at fixed offsets from it, like the original's
    /// busy-waits, so tick rounding does not add up.
    pub loading_from: u64,
}

impl Gameplay {
    /// Entry (`FUN_00408c2a`): advance the period; over → menu, else build the world.
    pub fn enter(m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        if m.advance(cx) == Advance::Over {
            return MatchFlow::Leave(Flow::Menu);
        }
        // `FUN_004015c8`: the backdrop parity is the first draw of the game init.
        let loading_alt = cx.rng.odd();
        let world = World::new(m, cx);
        MatchFlow::Next(MatchScreen::Gameplay(Box::new(Self::loading(
            world,
            loading_alt,
            cx.now,
        ))))
    }

    /// The period's first phase, around a built world.
    pub fn loading(world: World, loading_alt: bool, now: u64) -> Self {
        Self {
            phase: GamePhase::Loading {
                step: 0,
                next: Timer::armed(now, 0),
            },
            world,
            loading_alt,
            loading_from: now,
        }
    }

    /// Esc opens the quit dialog unless game+0x407bc blocks it; period over → bonus or shop.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        match self.phase {
            GamePhase::Loading { step, next } => self.load_step(step, next, cx),
            GamePhase::Banner { count, mut next } => {
                // answers.md "Banner": the whole update runs, paused (`World::paused`) and with
                // every key released; the fly is already alive from the load's reset(0).
                self.world.tick(&KeyState::NONE, m, cx);
                if next.expired_rearm(cx.now) {
                    self.banner_step(count - 1, next, m, cx);
                }
            }
            GamePhase::Playing => return self.play(keys, m, cx),
        }
        MatchFlow::Stay
    }

    /// screens-and-rendering §4.1: nine clicks `LOADING_STEP_MS` apart, then 500 ms more.
    fn load_step(&mut self, step: u32, next: Timer, cx: &mut Ctx) {
        if !next.expired(cx.now) {
            return;
        }
        if step == LOADING_STEPS {
            self.world.paused = true;
            cx.emit(Event::play(SoundSlot::COUNTDOWN));
            self.phase = GamePhase::Banner {
                count: 3,
                next: Timer::armed(cx.now, BANNER_STEP_MS),
            };
            return;
        }
        cx.emit(Event::Click {
            volume_cb: -1000,
            freq_hz: Some(33_000),
        });
        let tail = if step + 1 == LOADING_STEPS {
            LOADING_TAIL_MS
        } else {
            0
        };
        let due = LOADING_STEP_MS * u64::from(step + 1) + tail;
        self.phase = GamePhase::Loading {
            step: step + 1,
            next: Timer::armed(self.loading_from, due),
        };
    }

    /// "3, 2, 1, Go!"; below 0 the period starts.
    fn banner_step(&mut self, count: i32, next: Timer, m: &MatchState, cx: &mut Ctx) {
        if count >= 0 {
            self.phase = GamePhase::Banner { count, next };
            return;
        }
        // `FUN_004015c8`: the clock is paused through the load and the banner, so starting it here
        // matches within one tick. Then, with music on, one parity picks the track (odd → 2).
        self.world.clock = PeriodClock::new(cx.now, m.period_minutes());
        self.world.paused = false;
        if m.music_on {
            let alt = cx.rng.odd();
            cx.emit(Event::Music(Music::Game(alt)));
        }
        self.phase = GamePhase::Playing;
    }

    /// ai-and-rules §9.2: the flag set by the previous world tick is acted on here. The period
    /// end adds the fly's life in progress (answers.md "Fly survival stat: the period end also
    /// stops it").
    fn play(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        if self.world.period_over {
            self.world.fly.stop_survival(m, cx.now);
            cx.emit(Event::Music(Music::Stop));
            return MatchFlow::Next(after_period(m, cx));
        }
        if self.world.paused {
            return MatchFlow::Stay;
        }
        if keys.pressed(Scancode::ESCAPE) {
            return MatchFlow::Leave(Flow::Quit);
        }
        self.world.tick(keys, m, cx);
        MatchFlow::Stay
    }

    /// The quit dialog opened: freeze the period clock and the paused steps.
    pub fn pause(&mut self, now: u64) {
        self.world.paused = true;
        self.world.clock.pause(now);
    }

    /// Dialog answered No: resume, and block the action/swing keys until released.
    pub fn resume(&mut self, now: u64) {
        self.world.paused = false;
        self.world.clock.resume(now);
        self.world.fly_action_memory = 10;
        self.world.farmer_swing_memory = 10;
    }
}

/// ai-and-rules §9.2 steps 2–4.
fn after_period(m: &mut MatchState, cx: &mut Ctx) -> MatchScreen {
    if m.bonus_due() {
        MatchScreen::Bonus(Box::new(BonusRound::new(m, cx)))
    } else if m.over_after_period() {
        MatchScreen::Winner(Box::new(Winner::new(m, cx)))
    } else {
        MatchScreen::GoodiesShop(Box::new(GoodiesShop::new(m, cx)))
    }
}

/// Farewell (0x82), `FUN_00449fae` / `FUN_0044a641`: a logo zoom, then a key starts the exit
/// fade. There is no timeout (answers.md "Farewell length").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Farewell {
    /// The `logo` scale, 0.045 up to 1.0.
    pub scale: f64,
    /// The 1000 ms wait, then the zoom step timer.
    pub step: Timer,
    pub zooming: bool,
    /// The second layer's 150 ms effect, started when the zoom ends.
    pub effect: Timer,
    /// The 700 ms exit fade (alpha 0 → 255).
    pub fade: Option<Fade>,
}

pub const FAREWELL_WAIT_MS: u64 = 1000;
pub const FAREWELL_FIRST_STEP_MS: u64 = 25;
pub const FAREWELL_EFFECT_MS: u64 = 150;
pub const FAREWELL_FADE_MS: u64 = 700;
const FAREWELL_START_SCALE: f64 = 0.045;
const FAREWELL_SCALE_STEP: f64 = 0.05;

impl Farewell {
    pub fn new(cx: &mut Ctx) -> Self {
        Self {
            scale: FAREWELL_START_SCALE,
            step: Timer::armed(cx.now, FAREWELL_WAIT_MS),
            zooming: false,
            effect: Timer::UNARMED,
            fade: None,
        }
    }

    pub fn tick(&mut self, keys: &KeyState, cx: &mut Ctx) -> Flow {
        if let Some(fade) = &mut self.fade {
            return if fade.step() {
                Flow::Go(Screen::Exit)
            } else {
                Flow::Stay
            };
        }
        if self.scale < 1.0 {
            self.zoom(cx);
            return Flow::Stay;
        }
        if self.effect.expired(cx.now) && keys.skip() {
            self.fade = Some(Fade::new(true, FAREWELL_FADE_MS));
            cx.emit(Event::play(SoundSlot::VOICE_LOSE));
        }
        Flow::Stay
    }

    fn zoom(&mut self, cx: &mut Ctx) {
        if !self.step.expired(cx.now) {
            return;
        }
        if !self.zooming {
            self.zooming = true;
            self.step.arm(cx.now, FAREWELL_FIRST_STEP_MS);
            return;
        }
        // The next interval comes from the scale before the step.
        let next = (40.0 - 40.0 * self.scale + 2.0) as u64;
        self.step.arm(cx.now, next);
        self.scale += FAREWELL_SCALE_STEP;
        if self.scale < 1.0 {
            return;
        }
        self.scale = 1.0;
        self.effect.arm(cx.now, FAREWELL_EFFECT_MS);
        cx.emit(Event::play(SoundSlot::SPLASH));
    }
}

/// The quit / stop dialog (0x3c), an overlay: the screen below keeps ticking with input zeroed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitDialog {
    /// From the menu it asks to quit to Windows; otherwise to stop the game.
    pub from_menu: bool,
    /// Default NO.
    pub yes: bool,
}

impl QuitDialog {
    pub fn new(from_menu: bool) -> Self {
        Self {
            from_menu,
            yes: false,
        }
    }

    /// `FUN_00411b8f`: Left −1 and Right +1, wrapping over YES/NO, so each toggles; Enter
    /// answers; Esc is not read. Some(true) is YES.
    pub fn tick(&mut self, keys: &KeyState, cx: &mut Ctx) -> Option<bool> {
        if keys.pressed(Scancode::RETURN) {
            return Some(self.yes);
        }
        let moves =
            usize::from(keys.pressed(Scancode::LEFT)) + usize::from(keys.pressed(Scancode::RIGHT));
        if moves == 0 {
            return None;
        }
        self.yes ^= moves % 2 == 1;
        cx.emit(CLICK);
        None
    }
}

/// Every out-of-match screen of game-spec §2.2, plus the running match. Large screens are boxed
/// to keep the enum small.
#[derive(Debug, Clone)]
pub enum Screen {
    /// 0x00: creates the sound manager → intro video.
    Boot,
    /// 0x8c. Port decision: the video is deferred and Boot goes straight to the logo (Esc skips the
    /// video in the original anyway); upgrade when the frontend can decode `Intro.avi`.
    IntroVideo,
    Logo(Logo),
    Menu(Box<Menu>),
    /// Reached from the menu.
    KeysPage(KeysPage),
    HighScores(Box<HighScoreScreen>),
    Farewell(Farewell),
    /// 0x46: the application closes.
    Exit,
    Match(Box<InMatch>),
}

/// The screens that exist only inside a match, so they can only ever see a live [`MatchState`].
#[derive(Debug, Clone)]
pub enum MatchScreen {
    /// Reached from match start with the tutorial on.
    KeysPage(KeysPage),
    TutorialPage,
    Gameplay(Box<Gameplay>),
    Bonus(Box<BonusRound>),
    GoodiesShop(Box<GoodiesShop>),
    TrapsShop(Box<TrapsShop>),
    Bourse(Box<Bourse>),
    Winner(Box<Winner>),
    Stats(Box<StatsScreen>),
}

#[derive(Debug, Clone)]
pub struct InMatch {
    pub m: MatchState,
    pub screen: MatchScreen,
}

impl InMatch {
    /// One tick of the current in-match screen; `Next` is applied here.
    pub fn tick(&mut self, keys: &KeyState, scores: &HighScoreTable, cx: &mut Ctx) -> Flow {
        let m = &mut self.m;
        let flow = match &mut self.screen {
            MatchScreen::KeysPage(p) => {
                if p.tick(keys) {
                    MatchFlow::Next(MatchScreen::TutorialPage)
                } else {
                    MatchFlow::Stay
                }
            }
            MatchScreen::TutorialPage => tutorial_page_tick(keys, m, cx),
            MatchScreen::Gameplay(g) => g.tick(keys, m, cx),
            MatchScreen::Bonus(b) => b.tick(keys, m, cx),
            MatchScreen::GoodiesShop(s) => s.tick(keys, m, cx),
            MatchScreen::TrapsShop(s) => s.tick(keys, m, cx),
            MatchScreen::Bourse(b) => b.tick(keys, m, cx),
            MatchScreen::Winner(w) => w.tick(keys, m, cx),
            MatchScreen::Stats(s) => s.tick(keys, m, scores, cx),
        };
        match flow {
            MatchFlow::Stay => Flow::Stay,
            MatchFlow::Next(next) => {
                self.screen = next;
                cx.emit(Event::ScreenChanged);
                Flow::Stay
            }
            MatchFlow::Leave(flow) => {
                if matches!(flow, Flow::Quit) {
                    self.pause(true, cx.now);
                }
                flow
            }
        }
    }

    /// The quit dialog opened (true) or was answered No (false) over this match.
    pub fn pause(&mut self, paused: bool, now: u64) {
        let MatchScreen::Gameplay(g) = &mut self.screen else {
            return;
        };
        if paused {
            g.pause(now);
        } else {
            g.resume(now);
        }
    }
}

/// A 65 Hz test driver: one call is one tick with the given keys held.
#[cfg(test)]
pub(crate) mod rig {
    use crate::clock::Clock;
    use crate::ctx::Ctx;
    use crate::data::GameData;
    use crate::event::Event;
    use crate::input::{KeyState, Scancode};
    use crate::rng::MsvcRand;

    pub struct Rig {
        pub clock: Clock,
        pub rng: MsvcRand,
        pub data: GameData,
        pub events: Vec<Event>,
        prev: KeyState,
    }

    impl Rig {
        pub fn new(seed: u32) -> Self {
            Self {
                clock: Clock::default(),
                rng: MsvcRand::new(seed),
                data: GameData::synthetic(),
                events: Vec::new(),
                prev: KeyState::NONE,
            }
        }

        pub fn now(&self) -> u64 {
            self.clock.now_ms()
        }

        /// One tick with `held` down; returns what `f` returns.
        pub fn step<T>(
            &mut self,
            held: &[Scancode],
            f: impl FnOnce(&KeyState, &mut Ctx) -> T,
        ) -> T {
            let keys = KeyState::next(&self.prev, held.iter().copied());
            self.prev = keys;
            let now = self.clock.now_ms();
            let out = f(
                &keys,
                &mut Ctx::new(now, &mut self.rng, &self.data, &mut self.events),
            );
            self.clock.advance();
            out
        }

        /// A context at the current time, outside any tick.
        pub fn cx<T>(&mut self, f: impl FnOnce(&mut Ctx) -> T) -> T {
            let now = self.clock.now_ms();
            f(&mut Ctx::new(
                now,
                &mut self.rng,
                &self.data,
                &mut self.events,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rig::Rig;
    use super::*;
    use crate::event::{SoundOp, SoundSlot};
    use crate::match_state::MatchConfig;
    use crate::saves::{Options, Persisted};
    use crate::sim::State;

    const TICK_CAP: usize = 20_000;

    fn two_player(rig: &mut Rig) -> MatchState {
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        rig.cx(|cx| {
            let mut m = MatchState::new(config, true, cx);
            m.advance(cx);
            m
        })
    }

    /// Ticks with no keys until `done` holds; panics past the cap.
    fn idle_until(rig: &mut Rig, s: &mut State, done: impl Fn(&State) -> bool) {
        for _ in 0..TICK_CAP {
            if done(s) {
                return;
            }
            rig.step(&[], |k, cx| s.tick(k, cx));
        }
        panic!("condition not reached in {TICK_CAP} ticks");
    }

    fn tap(rig: &mut Rig, s: &mut State, key: Scancode) {
        rig.step(&[key], |k, cx| s.tick(k, cx));
        rig.step(&[], |k, cx| s.tick(k, cx));
    }

    /// SR §1.4: 255 / (65 × 0.5) per tick, so 33 ticks.
    #[test]
    fn a_500_ms_fade_ends_on_tick_33() {
        let mut f = Fade::new(true, FADE_MS);
        let ended: Vec<usize> = (1..=40).filter(|_| f.step()).collect();
        assert_eq!(ended, [33]);
    }

    /// answers.md "Fly reset(0) and SuperFly sparkles": the period start's reset(0) makes the fly
    /// alive at the load (slot 38 and the survival start), so it is drawn under the whole banner
    /// (gaps §2.3's Wine capture) and plays slot 38 once.
    #[test]
    fn the_period_start_fly_is_alive_through_the_banner() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        let world = rig.cx(|cx| World::new(&mut m, cx));
        assert!(world.fly.alive());
        let mut g = Gameplay::loading(world, false, rig.now());
        for _ in 0..TICK_CAP {
            if g.phase == GamePhase::Playing {
                break;
            }
            rig.step(&[], |k, cx| g.tick(k, &mut m, cx));
            assert!(g.world.fly.alive());
        }
        assert_eq!(g.phase, GamePhase::Playing);
        let slot_38 = Event::play(SoundSlot::GOODIE_FLY);
        assert_eq!(rig.events.iter().filter(|e| **e == slot_38).count(), 1);
    }

    /// Ticks with no keys until the banner is up.
    fn to_banner(rig: &mut Rig, g: &mut Gameplay, m: &mut MatchState) {
        for _ in 0..TICK_CAP {
            if matches!(g.phase, GamePhase::Banner { .. }) {
                return;
            }
            rig.step(&[], |k, cx| g.tick(k, m, cx));
        }
        panic!("no banner in {TICK_CAP} ticks");
    }

    /// answers.md "Banner": the whole world update runs under the banner, paused and with every
    /// key released. Held keys do not steer the fly, and every tick draws rand (the animal
    /// sounds' R(150) at least).
    #[test]
    fn the_banner_runs_the_world_paused_with_the_keys_released() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        let world = rig.cx(|cx| World::new(&mut m, cx));
        let mut g = Gameplay::loading(world, false, rig.now());
        to_banner(&mut rig, &mut g, &mut m);
        assert!(g.world.paused);
        for _ in 0..TICK_CAP {
            if g.phase == GamePhase::Playing {
                break;
            }
            let before = rig.rng.clone();
            rig.step(&[Scancode::RIGHT, Scancode::UP], |k, cx| {
                g.tick(k, &mut m, cx)
            });
            assert_ne!(rig.rng, before);
            assert_eq!((g.world.fly.vx, g.world.fly.vy), (0, 0));
        }
        assert_eq!(g.phase, GamePhase::Playing);
        assert!(!g.world.paused && !g.world.fly.appearing);
    }

    /// The pause gates the AI tick: one player's AI is unchanged across the banner.
    #[test]
    fn the_ai_holds_still_under_the_banner() {
        let mut rig = Rig::new(1);
        let mut m = rig.cx(|cx| {
            let mut m = MatchState::new(MatchConfig::default(), true, cx);
            m.advance(cx);
            m
        });
        let world = rig.cx(|cx| World::new(&mut m, cx));
        let mut g = Gameplay::loading(world, false, rig.now());
        to_banner(&mut rig, &mut g, &mut m);
        let mut ai = g.world.ai.clone();
        // The farmer step copies the pause into the AI on the first banner tick.
        ai.as_mut().expect("one player has an AI").paused = true;
        for _ in 0..TICK_CAP {
            if g.phase == GamePhase::Playing {
                break;
            }
            rig.step(&[], |k, cx| g.tick(k, &mut m, cx));
        }
        assert_eq!(g.world.ai, ai);
    }

    /// observed.md: nine clicks 250 ms apart, slot 52 at 2750 ms, then 3, 2, 1, Go at 1000 ms.
    #[test]
    fn loading_clicks_nine_times_then_the_banner_counts_down() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig);
        let world = rig.cx(|cx| World::bare(&m, cx));
        let mut g = Gameplay::loading(world, false, rig.now());
        let mut countdown_at = None;
        let mut playing_at = None;
        for _ in 0..TICK_CAP {
            let before = rig.events.len();
            let now = rig.now();
            rig.step(&[], |k, cx| g.tick(k, &mut m, cx));
            if rig.events[before..].contains(&Event::play(SoundSlot::COUNTDOWN)) {
                countdown_at = Some(now);
            }
            if g.phase == GamePhase::Playing {
                playing_at = Some(now);
                break;
            }
        }
        let clicks = rig
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::Click {
                        volume_cb: -1000,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(clicks, 9);
        let countdown_at = countdown_at.unwrap();
        assert!((2750..2800).contains(&countdown_at), "{countdown_at}");
        let playing = playing_at.unwrap() - countdown_at;
        assert!((4000..4100).contains(&playing), "{playing}");
        assert_eq!(g.world.clock, PeriodClock::new(playing_at.unwrap(), 2));
    }

    /// answers.md "Period clock start": with music on, the banner end draws one parity for the
    /// track (odd → `music_game2`); with music off it draws nothing.
    #[test]
    fn the_banner_end_draws_the_music_track() {
        let run = |music: bool| {
            let mut rig = Rig::new(1);
            let mut m = two_player(&mut rig);
            m.music_on = music;
            let world = rig.cx(|cx| World::bare(&m, cx));
            let mut g = Gameplay::loading(world, false, rig.now());
            for _ in 0..TICK_CAP {
                if matches!(g.phase, GamePhase::Banner { count: 0, .. }) {
                    break;
                }
                rig.step(&[], |k, cx| g.tick(k, &mut m, cx));
            }
            let before = rig.rng.clone();
            for _ in 0..TICK_CAP {
                if g.phase == GamePhase::Playing {
                    break;
                }
                rig.step(&[], |k, cx| g.tick(k, &mut m, cx));
            }
            (before, rig.rng, rig.events)
        };
        let (before_off, off, events_off) = run(false);
        let (before_on, on, events_on) = run(true);
        assert_eq!(before_off, before_on);
        let mut probe = off.clone();
        let alt = probe.odd();
        assert_eq!(on, probe);
        assert!(events_on.contains(&Event::Music(Music::Game(alt))));
        assert!(
            !events_off
                .iter()
                .any(|e| matches!(e, Event::Music(Music::Game(_))))
        );
    }

    /// AR §9.2: the period-over flag sends a no-bonus period to the goodies shop.
    #[test]
    fn a_finished_breakfast_goes_to_the_goodies_shop() {
        let mut rig = Rig::new(1);
        let m = two_player(&mut rig);
        let mut g = rig.cx(|cx| Gameplay::loading(World::bare(&m, cx), false, cx.now));
        g.phase = GamePhase::Playing;
        g.world.period_over = true;
        let mut im = InMatch {
            m,
            screen: MatchScreen::Gameplay(Box::new(g)),
        };
        let table = HighScoreTable::default();
        rig.step(&[], |k, cx| im.tick(k, &table, cx));
        assert!(matches!(im.screen, MatchScreen::GoodiesShop(_)));
        assert!(rig.events.contains(&Event::Music(Music::Stop)));
    }

    /// answers.md "Fly survival stat: the period end also stops it": a fly that never died
    /// still counts its life, so the stats page's maximum is not 0 s.
    #[test]
    fn the_period_end_counts_the_life_of_a_fly_that_never_died() {
        let mut rig = Rig::new(1);
        let m = two_player(&mut rig);
        let mut g = rig.cx(|cx| Gameplay::loading(World::bare(&m, cx), false, cx.now));
        g.phase = GamePhase::Playing;
        g.world.period_over = true;
        let mut im = InMatch {
            m,
            screen: MatchScreen::Gameplay(Box::new(g)),
        };
        for _ in 0..100 {
            rig.step(&[], |_, _| ());
        }
        let now = rig.now();
        let table = HighScoreTable::default();
        rig.step(&[], |k, cx| im.tick(k, &table, cx));
        assert_eq!(im.m.stats.survival_max_ms, now);
        assert!(im.m.stats.page().max_survival_s > 0.0);
    }

    #[test]
    fn resume_blocks_the_action_keys_until_released() {
        let mut rig = Rig::new(1);
        let m = two_player(&mut rig);
        let mut g = rig.cx(|cx| Gameplay::loading(World::bare(&m, cx), false, cx.now));
        g.pause(100);
        assert!(g.world.paused);
        g.resume(200);
        assert!(!g.world.paused);
        assert_eq!(
            (g.world.fly_action_memory, g.world.farmer_swing_memory),
            (10, 10)
        );
    }

    fn in_shop(rig: &mut Rig) -> State {
        let mut m = two_player(rig);
        let shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        let mut s = State::new(Persisted::default());
        s.screen = Screen::Match(Box::new(InMatch {
            m,
            screen: MatchScreen::GoodiesShop(Box::new(shop)),
        }));
        s
    }

    /// GS §2.2: the dialog defaults to NO; NO resumes the match, YES in a match → menu.
    #[test]
    fn the_quit_dialog_defaults_to_no_and_yes_in_a_match_goes_to_the_menu() {
        let mut rig = Rig::new(1);
        let mut s = in_shop(&mut rig);
        tap(&mut rig, &mut s, Scancode::ESCAPE);
        assert_eq!(s.dialog, Some(QuitDialog::new(false)));
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert!(s.dialog.is_none());
        assert!(matches!(s.screen, Screen::Match(_)));
        tap(&mut rig, &mut s, Scancode::ESCAPE);
        tap(&mut rig, &mut s, Scancode::LEFT);
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert!(matches!(s.screen, Screen::Menu(_)));
    }

    /// answers.md "Quit dialog: Esc": Esc is never read; Left and Right each step one item and
    /// wrap over the two, so either key toggles.
    #[test]
    fn the_quit_dialog_ignores_esc_and_either_arrow_toggles() {
        let mut rig = Rig::new(1);
        let mut s = in_shop(&mut rig);
        tap(&mut rig, &mut s, Scancode::ESCAPE);
        tap(&mut rig, &mut s, Scancode::ESCAPE);
        assert_eq!(s.dialog, Some(QuitDialog::new(false)));
        tap(&mut rig, &mut s, Scancode::LEFT);
        tap(&mut rig, &mut s, Scancode::LEFT);
        assert_eq!(s.dialog.map(|d| d.yes), Some(false));
        tap(&mut rig, &mut s, Scancode::RIGHT);
        assert_eq!(s.dialog.map(|d| d.yes), Some(true));
        tap(&mut rig, &mut s, Scancode::RIGHT);
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert!(s.dialog.is_none());
        assert!(matches!(s.screen, Screen::Match(_)));
    }

    /// Top grid: Down to Credits (2), Right to Quit (3); YES → farewell → exit.
    #[test]
    fn yes_from_the_menu_goes_to_the_farewell_then_exits() {
        let mut rig = Rig::new(1);
        let mut s = State::new(Persisted::default());
        s.screen = Screen::Menu(Box::new(Menu::new(false)));
        tap(&mut rig, &mut s, Scancode::DOWN);
        tap(&mut rig, &mut s, Scancode::RIGHT);
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert_eq!(s.dialog, Some(QuitDialog::new(true)));
        tap(&mut rig, &mut s, Scancode::LEFT);
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert!(matches!(s.screen, Screen::Farewell(_)));
    }

    /// answers.md "Credits: end and rand": the five pairs loop, Enter does nothing, Esc leaves.
    #[test]
    fn the_credits_loop_until_esc() {
        let mut rig = Rig::new(1);
        let mut s = State::new(Persisted::default());
        s.screen = Screen::Menu(Box::new(Menu::new(false)));
        tap(&mut rig, &mut s, Scancode::DOWN);
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert!(matches!(menu_page(&s), Some(crate::menu::Page::Credits(_))));
        let entered = rig.now();
        while rig.now() < entered + 6 * 7000 + 100 {
            rig.step(&[], |k, cx| s.tick(k, cx));
        }
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert!(matches!(menu_page(&s), Some(crate::menu::Page::Credits(_))));
        let Screen::Menu(m) = &s.screen else {
            panic!("left the menu");
        };
        assert_eq!(m.credits.pair, 1);
        tap(&mut rig, &mut s, Scancode::ESCAPE);
        assert_eq!(menu_page(&s), Some(crate::menu::Page::Top));
    }

    fn farewell(s: &State) -> Option<Farewell> {
        match &s.screen {
            Screen::Farewell(f) => Some(*f),
            _ => None,
        }
    }

    /// answers.md "Farewell length": a 1000 ms wait, then 20 zoom steps 25, 40, 38, …, 4 ms apart
    /// (1443 ms in all, plus up to one tick of lateness per step), slot 46; a key before the zoom
    /// and its 150 ms effect are done is ignored; then a key starts a 700 ms fade, slot 33, exit.
    #[test]
    fn the_farewell_waits_for_a_key_after_the_logo_zoom() {
        let mut rig = Rig::new(1);
        let mut s = State::new(Persisted::default());
        s.screen = Screen::Farewell(rig.cx(Farewell::new));
        tap(&mut rig, &mut s, Scancode::RETURN);
        idle_until(&mut rig, &mut s, |s| {
            farewell(s).is_some_and(|f| f.scale >= 1.0)
        });
        let zoomed = rig.now();
        assert!((1443..1443 + 20 * 16).contains(&zoomed), "{zoomed}");
        assert_eq!(farewell(&s).map(|f| f.scale), Some(1.0));
        assert!(rig.events.contains(&Event::play(SoundSlot(46))));
        assert!(farewell(&s).is_some_and(|f| f.fade.is_none()));
        while rig.now() < zoomed + 3000 {
            rig.step(&[], |k, cx| s.tick(k, cx));
        }
        assert!(farewell(&s).is_some(), "no timeout");
        rig.events.clear();
        tap(&mut rig, &mut s, Scancode::ESCAPE);
        assert!(rig.events.contains(&Event::play(SoundSlot(33))));
        let pressed = rig.now();
        idle_until(&mut rig, &mut s, |s| matches!(s.screen, Screen::Exit));
        // 700 ms at 65 Hz is 45.5 ticks of alpha, so 46 ticks.
        assert!(
            (690..=720).contains(&(rig.now() - pressed)),
            "{}",
            rig.now() - pressed
        );
    }

    fn menu_page(s: &State) -> Option<crate::menu::Page> {
        match &s.screen {
            Screen::Menu(m) => Some(m.page),
            _ => None,
        }
    }

    fn input_open(s: &State) -> bool {
        matches!(&s.screen, Screen::Menu(m) if m.boot.phase == crate::menu::SlidePhase::Kick)
    }

    /// GS §2.3: defaults 2 days, 1P, mode 1, AI 1; Highscore forces AI 1 and 3 days.
    #[test]
    fn highscore_mode_forces_normal_ai_and_three_days() {
        use crate::match_state::Mode;
        use crate::menu::Page;
        use crate::world::ai::AiLevel;
        let d = MatchConfig::default();
        assert_eq!(
            (d.days, d.players, d.mode, d.ai_level),
            (2, Players::One, Mode::Normal, AiLevel::Normal)
        );
        let mut rig = Rig::new(1);
        let persisted = Persisted {
            options: Options {
                tutorial: true,
                ..Options::default()
            },
            ..Persisted::default()
        };
        let mut s = State::new(persisted);
        s.config.ai_level = AiLevel::Easy;
        s.config.days = 6;
        s.screen = Screen::Menu(Box::new(Menu::new(false)));
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert_eq!(menu_page(&s), Some(Page::Play));
        idle_until(&mut rig, &mut s, input_open);
        tap(&mut rig, &mut s, Scancode::RETURN);
        assert_eq!(menu_page(&s), Some(Page::OnePlayerMode));
        tap(&mut rig, &mut s, Scancode::DOWN);
        tap(&mut rig, &mut s, Scancode::RETURN);
        let Screen::Match(im) = &s.screen else {
            panic!("no match started");
        };
        let c = im.m.config;
        assert_eq!(
            (c.mode, c.ai_level, c.days),
            (Mode::Special, AiLevel::Normal, 3)
        );
        assert!(matches!(im.screen, MatchScreen::KeysPage(_)));
        assert!(rig.events.iter().any(|e| matches!(
            e,
            Event::Sound {
                op: SoundOp::Play(_),
                ..
            }
        )));
    }

    /// Boot → logo → menu → Play page with its input open. The logo reads a key only after
    /// `LOGO_KEY_MS` and the first-visit intro only after `INTRO_SKIP_MS`, so Enter is tapped
    /// until each gives way. The top page has no boot slide; the Play page's boot must land
    /// before its keys are read.
    fn boot_to_play_page(rig: &mut Rig, s: &mut State) {
        let intro_done =
            |s: &State| matches!(&s.screen, Screen::Menu(m) if m.started && m.intro.step.is_none());
        for _ in 0..TICK_CAP {
            if intro_done(s) {
                break;
            }
            tap(rig, s, Scancode::RETURN);
        }
        assert!(intro_done(s), "menu intro never ended");
        tap(rig, s, Scancode::RETURN);
        assert_eq!(menu_page(s), Some(crate::menu::Page::Play));
        idle_until(rig, s, input_open);
    }

    /// The in-match screen's name, for the visit log.
    fn match_screen(s: &State) -> Option<&'static str> {
        let Screen::Match(im) = &s.screen else {
            return None;
        };
        Some(match &im.screen {
            MatchScreen::KeysPage(_) => "keys",
            MatchScreen::TutorialPage => "tutorial",
            MatchScreen::Gameplay(_) => "gameplay",
            MatchScreen::Bonus(_) => "bonus",
            MatchScreen::GoodiesShop(_) => "goodies",
            MatchScreen::TrapsShop(_) => "traps",
            MatchScreen::Bourse(_) => "bourse",
            MatchScreen::Winner(_) => "winner",
            MatchScreen::Stats(_) => "stats",
        })
    }

    /// Plays the started match back to the menu, tapping a fixed cycle of shop, confirm and
    /// swing keys, one key every other tick. `force` ends each period as soon as it is playing;
    /// otherwise the period clock ends it. Returns the in-match screens in visit order.
    fn play_to_menu(rig: &mut Rig, s: &mut State, force: bool) -> Vec<&'static str> {
        const MATCH_TICKS: usize = 200_000;
        let cycle = [Scancode::F, Scancode::TAB, Scancode::LEFT, Scancode::RETURN];
        let mut visits: Vec<&'static str> = Vec::new();
        for t in 0..MATCH_TICKS {
            if matches!(s.screen, Screen::Menu(_)) {
                return visits;
            }
            let name = match_screen(s).expect("the match ended outside the menu");
            if visits.last() != Some(&name) {
                visits.push(name);
            }
            if force
                && let Screen::Match(im) = &mut s.screen
                && let MatchScreen::Gameplay(g) = &mut im.screen
                && g.phase == GamePhase::Playing
            {
                g.world.period_over = true;
            }
            let held: &[Scancode] = if t % 2 == 0 {
                &cycle[t / 2 % cycle.len()..][..1]
            } else {
                &[]
            };
            rig.step(held, |k, cx| s.tick(k, cx));
        }
        panic!("match not over after {MATCH_TICKS} ticks: {visits:?}");
    }

    fn start_match(rig: &mut Rig, s: &mut State, path: &[Scancode]) -> MatchConfig {
        boot_to_play_page(rig, s);
        for &key in path {
            tap(rig, s, key);
        }
        let Screen::Match(im) = &s.screen else {
            panic!("no match started");
        };
        im.m.config
    }

    fn visits_of(visits: &[&str], name: &str) -> usize {
        visits.iter().filter(|v| **v == name).count()
    }

    /// GS §2.2 end to end: boot → logo → menu → 2P match of 2 days → shops and bourse each
    /// period → bonus → winner → stats → menu. Every period ends at once via `period_over`.
    #[test]
    fn a_two_day_match_runs_from_boot_back_to_the_menu() {
        use crate::match_state::Mode;
        let mut rig = Rig::new(1);
        let mut s = State::new(Persisted::default());
        let path = [
            Scancode::DOWN,
            Scancode::RETURN,
            Scancode::RETURN,
            Scancode::RETURN,
        ];
        let c = start_match(&mut rig, &mut s, &path);
        assert_eq!((c.players, c.mode, c.days), (Players::Two, Mode::Normal, 2));
        let visits = play_to_menu(&mut rig, &mut s, true);
        assert_eq!(visits_of(&visits, "gameplay"), 6, "{visits:?}");
        // AR §9.1: a bonus after each dinner; the shops and the bourse between periods.
        assert_eq!(visits_of(&visits, "bonus"), 2, "{visits:?}");
        assert_eq!(visits_of(&visits, "goodies"), 5, "{visits:?}");
        assert_eq!(visits_of(&visits, "bourse"), 5, "{visits:?}");
        assert_eq!(visits[visits.len() - 2..], ["winner", "stats"]);
    }

    /// One player against the AI farmer, every period played to its 2-minute clock: the AI
    /// shops and swings on its own.
    #[test]
    fn a_one_player_match_against_the_ai_runs_on_the_clock_back_to_the_menu() {
        use crate::match_state::Mode;
        use crate::world::ai::AiLevel;
        let mut rig = Rig::new(3);
        let mut s = State::new(Persisted::default());
        // 1 Player, Normal mode, Intelligent (one below the preselected Normal), 2 days.
        let path = [
            Scancode::RETURN,
            Scancode::RETURN,
            Scancode::DOWN,
            Scancode::RETURN,
            Scancode::RETURN,
        ];
        let c = start_match(&mut rig, &mut s, &path);
        assert_eq!(
            (c.players, c.mode, c.ai_level, c.days),
            (Players::One, Mode::Normal, AiLevel::Intelligent, 2)
        );
        let started = rig.now();
        let visits = play_to_menu(&mut rig, &mut s, false);
        assert_eq!(visits_of(&visits, "gameplay"), 6, "{visits:?}");
        assert_eq!(visits_of(&visits, "bonus"), 2, "{visits:?}");
        assert_eq!(visits[visits.len() - 2..], ["winner", "stats"]);
        let played = rig.now() - started;
        assert!(played > 6 * 120_000, "{played} ms");
    }

    /// Extreme (2P, mode 2): one period in the arena, then the bonus, winner and stats.
    #[test]
    fn an_extreme_match_is_one_period_and_a_bonus() {
        let mut rig = Rig::new(5);
        let mut s = State::new(Persisted::default());
        let path = [
            Scancode::DOWN,
            Scancode::RETURN,
            Scancode::DOWN,
            Scancode::RETURN,
        ];
        let c = start_match(&mut rig, &mut s, &path);
        assert!(c.extreme());
        let visits = play_to_menu(&mut rig, &mut s, false);
        assert_eq!(visits, ["gameplay", "bonus", "winner", "stats"]);
    }
}
