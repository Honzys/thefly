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

/// Farewell length. shortcut: open item O5, the timing was not read; revisit with a capture.
pub const FAREWELL_MS: u64 = 3000;

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
    /// "3, 2, 1, Go!": 3 down to 0, one per 1000 ms; slot 52 at the start. Open item O16: whether
    /// the world ticks under the banner.
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
        // shortcut: open item O16, the backdrop pick is drawn before the world is built;
        // revisit if a capture shows the other order.
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
        // shortcut: open item O16, the world waits under the loading screen and the banner and
        // its period clock starts here; revisit if a capture shows the clock running earlier.
        self.world.clock = PeriodClock::new(cx.now, m.period_minutes());
        if m.music_on {
            cx.emit(Event::Music(Music::Game(m.music_game2)));
        }
        self.phase = GamePhase::Playing;
    }

    /// ai-and-rules §9.2: the flag set by the previous world tick is acted on here.
    fn play(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        if self.world.period_over {
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

/// Farewell (0x82). Open item O5: timing and choreography.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Farewell {
    pub until: Timer,
}

impl Farewell {
    pub fn new(cx: &mut Ctx) -> Self {
        Self {
            until: Timer::armed(cx.now, FAREWELL_MS),
        }
    }

    pub fn tick(&mut self, cx: &mut Ctx) -> Flow {
        if self.until.expired(cx.now) {
            Flow::Go(Screen::Exit)
        } else {
            Flow::Stay
        }
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

    /// Left/Right toggle, Enter answers. Some(true) is YES.
    pub fn tick(&mut self, keys: &KeyState, cx: &mut Ctx) -> Option<bool> {
        // shortcut: open item O15, Esc answers No (dialogs read only arrows and Enter, gaps
        // §1.3); revisit if a capture shows Esc ignored.
        if keys.pressed(Scancode::ESCAPE) {
            return Some(false);
        }
        if keys.pressed(Scancode::RETURN) {
            return Some(self.yes);
        }
        let yes = if keys.pressed(Scancode::LEFT) {
            true
        } else if keys.pressed(Scancode::RIGHT) {
            false
        } else {
            return None;
        };
        self.yes = yes;
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
    /// 0x8c. shortcut: the video is deferred and Boot goes straight to the logo (Esc skips the
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
