//! W3. The application state machine (game-spec §2.2): one [`Screen`] at a time, plus the quit
//! dialog overlay owned by [`crate::sim::State`]. Screens report where to go with a [`Flow`];
//! the state builds the next screen and emits [`crate::Event::ScreenChanged`].

use crate::bonus::BonusRound;
use crate::bourse::Bourse;
use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::event::Event;
use crate::highscores::{HighScoreScreen, HighScoreTable};
use crate::input::{KeyState, Scancode};
use crate::match_state::{MatchState, Players};
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
/// Loading: 9 progress steps of 200 ms busy-wait, then 500 ms (screens-and-rendering §4.1).
/// Open item: the step count of 9 is inferred.
pub const LOADING_STEPS: u32 = 9;
pub const LOADING_STEP_MS: u64 = 200;
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
        let _ = keys;
        todo!("W3 game-spec §2.2 keys page, screens-and-rendering §2.6")
    }
}

/// Tutorial page (0xa0): Enter starts the period.
pub fn tutorial_page_tick(keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
    if !keys.pressed(Scancode::RETURN) {
        return MatchFlow::Stay;
    }
    Gameplay::enter(m, cx)
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GamePhase {
    /// The loading screen's fixed busy-waits, replayed as time (one click per step).
    Loading {
        step: u32,
        next: Timer,
    },
    /// "3, 2, 1, Go!": 3 down to 0, one per 1000 ms; slot 52 at the start. Open item: whether
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
}

impl Gameplay {
    /// Entry (`FUN_00408c2a`): advance the period; over → menu, else build the world.
    pub fn enter(m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let _ = (m, cx);
        todo!(
            "W3 game-spec §2.2 gameplay, ai-and-rules §9.1; open item: timers armed before the loading waits"
        )
    }

    /// Esc opens the quit dialog unless game+0x407bc blocks it; period over → bonus or shop.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let _ = (keys, m, cx);
        todo!("W3 ai-and-rules §9.2, screens-and-rendering §4.1, §4.5")
    }

    /// The quit dialog opened: freeze the period clock and the paused steps.
    pub fn pause(&mut self, now: u64) {
        let _ = now;
        todo!("W3 ai-and-rules §8.6")
    }

    /// Dialog answered No: resume, and block the action/swing keys until released.
    pub fn resume(&mut self, now: u64) {
        let _ = now;
        todo!("W3 ai-and-rules §8.6, fly-and-world §2 step 1")
    }
}

/// Farewell (0x82). Open item: timing and choreography.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Farewell {
    pub until: Timer,
}

impl Farewell {
    pub fn new(cx: &mut Ctx) -> Self {
        let _ = cx;
        todo!("W3 screens-and-rendering §5.4; open item")
    }

    pub fn tick(&mut self, cx: &mut Ctx) -> Flow {
        let _ = cx;
        todo!("W3 screens-and-rendering §5.4; open item")
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
        let _ = (keys, cx);
        todo!("W3 screens-and-rendering §2.5; Esc behaviour is an open item")
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
