//! The engine's entry point: [`Sim::tick`] at 65 Hz, and [`State`], the whole game state the
//! frontend renders from. `State` is `Clone`: the frontend keeps the previous tick's copy and
//! interpolates between the two with the frame's alpha.

use crate::clock::Clock;
use crate::ctx::Ctx;
use crate::data::GameData;
use crate::event::Event;
use crate::highscores::HighScoreTable;
use crate::input::KeyState;
use crate::match_state::{MatchConfig, MatchState};
use crate::menu::Menu;
use crate::rng::MsvcRand;
use crate::saves::{Options, Persisted};
use crate::screen::{
    Farewell, Flow, Gameplay, InMatch, KeysPage, Logo, MatchFlow, MatchScreen, QuitDialog, Screen,
};

pub struct Sim {
    clock: Clock,
    /// The one rand stream, seeded once (no per-period reseeding).
    rng: MsvcRand,
    data: GameData,
    events: Vec<Event>,
    state: State,
}

impl Sim {
    pub fn new(seed: u32, data: GameData, persisted: Persisted) -> Self {
        Self {
            clock: Clock::default(),
            rng: MsvcRand::new(seed),
            data,
            events: Vec::new(),
            state: State::new(persisted),
        }
    }

    /// One 65 Hz tick at `clock.now_ms()`; returns what happened.
    pub fn tick(&mut self, keys: &KeyState) -> Vec<Event> {
        let mut cx = Ctx::new(
            self.clock.now_ms(),
            &mut self.rng,
            &self.data,
            &mut self.events,
        );
        self.state.tick(keys, &mut cx);
        self.clock.advance();
        std::mem::take(&mut self.events)
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    pub fn rng(&self) -> &MsvcRand {
        &self.rng
    }

    pub fn data(&self) -> &GameData {
        &self.data
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub screen: Screen,
    /// The quit dialog overlay; the screen below ticks with input zeroed.
    pub dialog: Option<QuitDialog>,
    /// The menu's selections, copied into the next match.
    pub config: MatchConfig,
    pub options: Options,
    pub scores: HighScoreTable,
    /// The menu intro plays on the first visit only.
    pub menu_seen: bool,
}

impl State {
    pub fn new(persisted: Persisted) -> Self {
        Self {
            screen: Screen::Boot,
            dialog: None,
            config: MatchConfig::default(),
            options: persisted.options,
            scores: persisted.scores,
            menu_seen: false,
        }
    }

    pub fn tick(&mut self, keys: &KeyState, cx: &mut Ctx) {
        let answer = self.dialog.as_mut().map(|d| d.tick(keys, cx));
        let keys = match answer {
            None => *keys,
            Some(None) => KeyState::NONE,
            // The new screen starts next tick, like every other transition.
            Some(Some(true)) => return self.answer_yes(cx),
            Some(Some(false)) => {
                self.answer_no(cx);
                KeyState::NONE
            }
        };
        let flow = self.screen_tick(&keys, cx);
        self.apply(flow, cx);
    }

    fn screen_tick(&mut self, keys: &KeyState, cx: &mut Ctx) -> Flow {
        match &mut self.screen {
            // Port decision: no sound-manager or video step; see `Screen::IntroVideo`.
            Screen::Boot | Screen::IntroVideo => Flow::Go(Screen::Logo(Logo::new(cx.now))),
            Screen::Logo(logo) => logo.tick(keys, cx),
            Screen::Menu(menu) => menu.tick(keys, &mut self.config, &mut self.options, cx),
            Screen::KeysPage(page) => {
                if page.tick(keys) {
                    Flow::Menu
                } else {
                    Flow::Stay
                }
            }
            Screen::HighScores(hs) => hs.tick(keys, &mut self.scores, cx),
            Screen::Farewell(f) => f.tick(keys, cx),
            Screen::Exit => Flow::Stay,
            Screen::Match(im) => im.tick(keys, &self.scores, cx),
        }
    }

    fn apply(&mut self, flow: Flow, cx: &mut Ctx) {
        let next = match flow {
            Flow::Stay => return,
            Flow::Quit => {
                self.dialog = Some(QuitDialog::new(matches!(self.screen, Screen::Menu(_))));
                return;
            }
            Flow::Go(screen) => screen,
            Flow::Menu => {
                let first_visit = !std::mem::replace(&mut self.menu_seen, true);
                Screen::Menu(Box::new(Menu::new(first_visit)))
            }
            Flow::StartMatch => self.start_match(cx),
        };
        self.screen = next;
        cx.emit(Event::ScreenChanged);
    }

    /// `FUN_00408113` step 4: tutorial → keys page → tutorial page → period; else the period.
    fn start_match(&mut self, cx: &mut Ctx) -> Screen {
        self.config.tutorial = self.options.tutorial;
        let mut m = MatchState::new(self.config, self.options.music, cx);
        let first = if self.config.tutorial {
            MatchFlow::Next(MatchScreen::KeysPage(KeysPage {
                players: self.config.players,
            }))
        } else {
            Gameplay::enter(&mut m, cx)
        };
        match first {
            MatchFlow::Next(screen) => Screen::Match(Box::new(InMatch { m, screen })),
            // The first advance is never "over"; fall back to the menu if it ever is.
            MatchFlow::Stay | MatchFlow::Leave(_) => Screen::Menu(Box::new(Menu::new(false))),
        }
    }

    /// YES from the menu → farewell; YES in a match → menu.
    fn answer_yes(&mut self, cx: &mut Ctx) {
        let from_menu = self.dialog.take().is_some_and(|d| d.from_menu);
        let flow = if from_menu {
            Flow::Go(Screen::Farewell(Farewell::new(cx)))
        } else {
            Flow::Menu
        };
        self.apply(flow, cx);
    }

    /// NO: close the dialog and resume what is below.
    fn answer_no(&mut self, cx: &mut Ctx) {
        self.dialog = None;
        if let Screen::Match(im) = &mut self.screen {
            im.pause(false, cx.now);
        }
    }
}
