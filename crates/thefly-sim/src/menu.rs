//! W3. The main menu (0x14): first-visit intro script, page tree, boot slide, credits pages and
//! the view-keys dialog (game-spec §2.3, screens-and-rendering §2.2–§2.4). Clouds and dust puffs
//! are frontend cosmetics (W4); only state that gates input or emits sound lives here.

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::KeyState;
use crate::match_state::MatchConfig;
use crate::saves::Options;
use crate::screen::Flow;

/// Enter/Esc skip the first-visit intro after this.
pub const INTRO_SKIP_MS: u64 = 900;
pub const INTRO_SCROLL_VY: f64 = -60.0;
pub const INTRO_FLY_VX: f64 = 400.0;
pub const INTRO_FLY_MAX_MS: u64 = 3000;
/// The pig's gravity per tick and its bounce stop.
pub const PIG_GRAVITY: f64 = 120.0;
pub const PIG_BOUNCE_STOP: f64 = 370.0;
pub const BOOT_X: i32 = 200;
pub const BOOT_IN_VY: f64 = 200.0;
pub const BOOT_ACCEL: f64 = 90.0;
pub const BOOT_KICK_MS: u64 = 1000;

/// Menu pages by their code in `FUN_0041c95d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    /// 0: Play, Options, Credits, Quit (3×2 grid, cells [−1, 0, −2 / 1, 2, 3]).
    #[default]
    Top,
    /// 5: 1 Player, 2 Players, HighScores.
    Play,
    /// 1: Music, Tutorial, ViewKeys.
    Options,
    /// 2: Normal, Highscore.
    OnePlayerMode,
    /// 6: Easy, Normal, Inteligent.
    Difficulty,
    /// 0x0b: Normal, Extreme.
    TwoPlayerMode,
    /// 3: 2, 4, 6 days.
    Length,
    /// 7 → 9 → 8, then back to Top.
    Credits(u8),
    /// 10.
    ViewKeys,
}

/// The first-visit intro (menu+0x32a4), steps 0–3 of screens-and-rendering §2.2.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Intro {
    /// None when done (later visits start at the end pose).
    pub step: Option<u8>,
    pub entered_ms: u64,
    pub decor_y: f64,
    pub decor_vy: f64,
    pub fly_x: f64,
    pub fly_y: f64,
    pub fly_timer: Timer,
    pub pig_y: f64,
    pub pig_vy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlidePhase {
    #[default]
    Parked,
    In,
    Kick,
    Out,
}

/// The sub-page boot (`FUN_0041e63c`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BootSlide {
    pub phase: SlidePhase,
    pub y: f64,
    pub vy: f64,
    pub kick: Timer,
    /// The page shown once the boot is back in.
    pub target: Page,
}

/// Credits choreography (scene slide, letterbox, five text pairs at 7000 ms).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Credits {
    pub scene_y: f64,
    pub scene_vy: f64,
    pub letterbox: i32,
    pub pair: u8,
    pub hold: Timer,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Menu {
    pub first_visit: bool,
    pub intro: Intro,
    pub page: Page,
    pub selected: usize,
    pub boot: BootSlide,
    pub credits: Credits,
}

impl Menu {
    /// shortcut: fields start at their defaults; W3 sets the intro's start pose here.
    pub fn new(first_visit: bool) -> Self {
        Self {
            first_visit,
            ..Self::default()
        }
    }

    /// Intro, then page input (edge-triggered, Up/Down wrap, grid on Top, `i_bouton` per move).
    /// Toggling an option emits [`crate::Event::Save`] for `boite.btt`.
    pub fn tick(
        &mut self,
        keys: &KeyState,
        cfg: &mut MatchConfig,
        options: &mut Options,
        cx: &mut Ctx,
    ) -> Flow {
        let _ = (keys, cfg, options, cx);
        todo!("W3 game-spec §2.3, screens-and-rendering §2.2–§2.4")
    }
}
