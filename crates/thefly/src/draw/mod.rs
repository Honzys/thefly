//! One frame of the game from the sim state: every `Screen` and `MatchScreen`, laid out as
//! screens-and-rendering.md (SR) gives it.

mod between;
mod end;
mod front;
mod play;

pub use play::spark_shift;

use thefly_platform::Error;
use thefly_sim::match_state::MatchState;
use thefly_sim::screen::{InMatch, MatchScreen, Screen};
use thefly_sim::sim::State;

use crate::fx::Fx;
use crate::gfx::{BLACK, Gfx};
use crate::lerp::Lerp;

/// What a frame is drawn from.
pub struct View<'a> {
    pub cur: &'a State,
    /// The previous tick's state, `None` right after a screen change (nothing to slide from).
    pub prev: Option<&'a State>,
    pub l: Lerp,
    /// Sim time in ms at the current tick, for wall-clock sprite animations (SR §1.2).
    pub now: u64,
    pub fx: &'a Fx,
    /// Sim time the winner trophy landed, which starts the `you_win` tween (SR §5.1 state 2).
    pub trophy_landed_ms: Option<u64>,
}

impl View<'_> {
    /// Frame `n` of a looping wall-clock animation with `ms` per frame.
    pub fn anim(&self, ms: u64, frames: u32) -> u32 {
        ((self.now / ms.max(1)) % u64::from(frames.max(1))) as u32
    }

    fn prev_match(&self) -> Option<&InMatch> {
        match &self.prev?.screen {
            Screen::Match(im) => Some(im),
            _ => None,
        }
    }

    /// The previous tick's match screen when it is the same kind as the current one.
    pub fn prev_screen(&self) -> Option<&MatchScreen> {
        self.prev_match().map(|im| &im.screen)
    }
}

pub fn frame(g: &mut Gfx, v: &View) -> Result<(), Error> {
    g.r.clear(BLACK);
    match &v.cur.screen {
        Screen::Boot | Screen::IntroVideo | Screen::Exit => {}
        Screen::Logo(logo) => front::logo(g, v, logo)?,
        Screen::Menu(menu) => front::menu(g, v, menu)?,
        Screen::KeysPage(p) => front::keys_page(g, p.players)?,
        Screen::HighScores(hs) => front::high_scores(g, v, hs)?,
        Screen::Farewell(f) => front::farewell(g, v, f)?,
        Screen::Match(im) => in_match(g, v, &im.m, &im.screen)?,
    }
    if let Some(d) = &v.cur.dialog {
        front::quit_dialog(g, d)?;
    }
    Ok(())
}

fn in_match(g: &mut Gfx, v: &View, m: &MatchState, s: &MatchScreen) -> Result<(), Error> {
    match s {
        MatchScreen::KeysPage(p) => front::keys_page(g, p.players),
        MatchScreen::TutorialPage => g.spr("page_mongol", 0.0, 0.0, 0),
        MatchScreen::Gameplay(gp) => play::gameplay(g, v, m, gp),
        MatchScreen::Bonus(b) => between::bonus(g, v, b),
        MatchScreen::GoodiesShop(s) => between::goodies_shop(g, v, m, s),
        MatchScreen::TrapsShop(s) => between::traps_shop(g, v, m, s),
        MatchScreen::Bourse(b) => between::bourse(g, v, m, b),
        MatchScreen::Winner(w) => end::winner(g, v, w),
        MatchScreen::Stats(s) => end::stats(g, v, m, s),
    }
}
