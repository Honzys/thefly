//! W3. The main menu (0x14): first-visit intro script, page tree, boot slide, credits pages and
//! the view-keys dialog (game-spec §2.3, screens-and-rendering §2.2–§2.4). Clouds and dust puffs
//! are frontend cosmetics (W4); only state that gates input or emits sound lives here.

use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::event::{Event, Music, Pan, PlayParams, SaveFile, SoundOp, SoundSlot};
use crate::highscores::HighScoreScreen;
use crate::input::{KeyState, Scancode};
use crate::match_state::{MatchConfig, Mode, Players};
use crate::saves::{Options, encode_options};
use crate::screen::{CLICK, Flow, Screen};
use crate::world::ai::AiLevel;

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
    /// The first tick has set the start pose and started the music.
    pub started: bool,
}

/// One tick in seconds, for the px/s numbers of screens-and-rendering §2.2.
const DT: f64 = 1.0 / FPS as f64;
const DECOR_REST_Y: f64 = -600.0;
const DECOR_EASE_Y: f64 = 300.0;
const EASE_FLOOR: f64 = 0.2;
const FLY_START_X: f64 = -120.0;
/// Port decision: `i_cochon_corps` is not a sized sprite; the real value is its asset height.
const PIG_H: i32 = 300;
const BOOT_H: i32 = 575;
const BOOT_REST_Y: f64 = 350.0;
/// Credits: five text pairs held 7000 ms each, looping until Esc (`FUN_0041be05`).
/// shortcut: the two scene sprites wander during the slide and draw rand through the shared
/// Wander rule (answers.md "Credits: end and rand"); not modelled until the wander mover lands.
pub const CREDIT_PAIRS: u8 = 5;
pub const CREDIT_HOLD_MS: u64 = 7000;

/// Top page grid, 3 × 2, cells [−1, 0, −2 / 1, 2, 3].
const TOP_GRID: [[Option<usize>; 3]; 2] = [[None, Some(0), None], [Some(1), Some(2), Some(3)]];
const GRID_STEPS: usize = 6;

impl Page {
    pub fn items(self) -> usize {
        match self {
            Page::Top => 4,
            Page::Play | Page::Options | Page::Difficulty | Page::Length => 3,
            Page::OnePlayerMode | Page::TwoPlayerMode => 2,
            Page::Credits(_) | Page::ViewKeys => 1,
        }
    }

    /// Esc: back one page; the top page does not read Esc (`FUN_0041be05`).
    fn parent(self, cfg: &MatchConfig) -> Option<Page> {
        Some(match self {
            Page::Top => return None,
            Page::Play | Page::Options | Page::Credits(_) => Page::Top,
            Page::OnePlayerMode | Page::TwoPlayerMode => Page::Play,
            Page::Difficulty => Page::OnePlayerMode,
            Page::Length if cfg.players == Players::One => Page::Difficulty,
            Page::Length => Page::TwoPlayerMode,
            Page::ViewKeys => Page::Options,
        })
    }

    /// game-spec §2.3 / SR §2.2: the stored option is preselected.
    fn preselect(self, cfg: &MatchConfig) -> usize {
        match self {
            Page::Play => usize::from(cfg.players == Players::Two),
            Page::OnePlayerMode | Page::TwoPlayerMode => usize::from(cfg.mode == Mode::Special),
            Page::Difficulty => cfg.ai_level as usize,
            Page::Length => usize::from(cfg.days.saturating_sub(2) / 2).min(2),
            _ => 0,
        }
    }
}

impl Menu {
    /// Port decision: fields start at their defaults; the first tick sets the start pose.
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
        if !self.started {
            self.start(options, cx);
        }
        if self.intro.step.is_some() {
            self.intro_tick(keys, cx);
            return Flow::Stay;
        }
        self.boot_tick(cx);
        if matches!(self.boot.phase, SlidePhase::In | SlidePhase::Out) {
            return Flow::Stay;
        }
        if let Page::Credits(_) = self.page {
            return self.credits_tick(keys, cfg, cx);
        }
        if keys.pressed(Scancode::ESCAPE) {
            if let Some(parent) = self.page.parent(cfg) {
                self.go(parent, cfg, cx);
            }
            return Flow::Stay;
        }
        self.navigate(keys, cx);
        if keys.pressed(Scancode::RETURN) {
            return self.choose(cfg, options, cx);
        }
        Flow::Stay
    }

    fn start(&mut self, options: &Options, cx: &mut Ctx) {
        self.started = true;
        if options.music {
            cx.emit(Event::Music(Music::Menu));
        }
        self.boot.y = -5.0 - f64::from(BOOT_H);
        if !self.first_visit {
            self.intro.decor_y = DECOR_REST_Y;
            return;
        }
        self.intro = Intro {
            step: Some(0),
            entered_ms: cx.now,
            decor_vy: INTRO_SCROLL_VY,
            ..Intro::default()
        };
        cx.emit(Event::play(SoundSlot::MENU_MELODY));
    }

    /// SR §2.2 intro steps 0–3; Enter or Esc after 900 ms jumps to the end pose.
    fn intro_tick(&mut self, keys: &KeyState, cx: &mut Ctx) {
        let i = &mut self.intro;
        if cx.now - i.entered_ms >= INTRO_SKIP_MS && keys.skip() {
            return self.intro_end(cx);
        }
        match i.step {
            Some(0) | Some(1) => self.scroll(cx),
            Some(2) => self.fly_cross(cx),
            _ => self.pig_drop(cx),
        }
    }

    fn scroll(&mut self, cx: &mut Ctx) {
        let i = &mut self.intro;
        if i.decor_y < -DECOR_EASE_Y {
            i.step = Some(1);
            let ease = 1.0 - (i.decor_y.abs() - DECOR_EASE_Y) / DECOR_EASE_Y;
            i.decor_vy = INTRO_SCROLL_VY * ease.max(EASE_FLOOR);
        }
        i.decor_y += i.decor_vy * DT;
        if i.decor_y > DECOR_REST_Y {
            return;
        }
        i.decor_y = DECOR_REST_Y;
        i.step = Some(2);
        i.fly_x = FLY_START_X;
        i.fly_y = f64::from(cx.rng.r(300) + 100);
        i.fly_timer = Timer::armed(cx.now, INTRO_FLY_MAX_MS);
        cx.sound(SoundSlot::FLY_BUZZ, SoundOp::Loop(PlayParams::default()));
    }

    fn fly_cross(&mut self, cx: &mut Ctx) {
        let i = &mut self.intro;
        i.fly_x += INTRO_FLY_VX * DT;
        let pan = Pan::ScreenX {
            x: i.fly_x as i32,
            w: 120,
        };
        cx.sound(SoundSlot::FLY_BUZZ, SoundOp::Move(pan));
        if i.fly_x <= 800.0 && !i.fly_timer.expired(cx.now) {
            return;
        }
        cx.sound(SoundSlot::FLY_BUZZ, SoundOp::Stop);
        i.step = Some(3);
        i.pig_y = -50.0 - f64::from(pig_h(cx));
        i.pig_vy = 50.0;
    }

    fn pig_drop(&mut self, cx: &mut Ctx) {
        let floor = f64::from(600 - pig_h(cx));
        let i = &mut self.intro;
        i.pig_vy += PIG_GRAVITY;
        i.pig_y += i.pig_vy * DT;
        if i.pig_y < floor {
            return;
        }
        i.pig_y = floor;
        if i.pig_vy.abs() <= PIG_BOUNCE_STOP {
            i.step = None;
            return;
        }
        i.pig_vy = -i.pig_vy / 3.0;
        let freq = 10_000 + cx.rng.r(5000) as u32;
        cx.sound(
            SoundSlot::BOOM,
            SoundOp::Play(PlayParams {
                volume_cb: -500,
                freq_hz: Some(freq),
                pan: Pan::Centre,
            }),
        );
    }

    /// `FUN_0041e1be`: the end pose.
    fn intro_end(&mut self, cx: &mut Ctx) {
        if self.intro.step == Some(2) {
            cx.sound(SoundSlot::FLY_BUZZ, SoundOp::Stop);
        }
        self.intro.step = None;
        self.intro.decor_y = DECOR_REST_Y;
        self.intro.pig_y = f64::from(600 - pig_h(cx));
    }

    /// SR §2.2 boot slide: in, kick, then out when the top page returns.
    fn boot_tick(&mut self, cx: &mut Ctx) {
        let b = &mut self.boot;
        let h = f64::from(BOOT_H);
        match b.phase {
            SlidePhase::In => {
                b.vy += BOOT_ACCEL;
                b.y += b.vy * DT;
                if b.y < BOOT_REST_Y - h {
                    return;
                }
                b.y = BOOT_REST_Y - h;
                b.phase = SlidePhase::Kick;
                b.kick = Timer::armed(cx.now, BOOT_KICK_MS);
                cx.sound(
                    SoundSlot::MENU_PIG,
                    SoundOp::Play(PlayParams {
                        pan: Pan::Fixed(500),
                        ..PlayParams::default()
                    }),
                );
                let freq = 11_000 + cx.rng.r(3000) as u32;
                cx.sound(
                    SoundSlot::MENU_PIG_KICKED,
                    SoundOp::Play(PlayParams {
                        freq_hz: Some(freq),
                        ..PlayParams::default()
                    }),
                );
            }
            SlidePhase::Out => {
                b.vy -= BOOT_ACCEL;
                b.y += b.vy * DT;
                if b.y > -h {
                    return;
                }
                b.y = -5.0 - h;
                b.phase = SlidePhase::Parked;
            }
            SlidePhase::Parked | SlidePhase::Kick => {}
        }
    }

    fn go(&mut self, page: Page, cfg: &MatchConfig, cx: &mut Ctx) {
        let was_top = self.page == Page::Top;
        self.page = page;
        self.selected = page.preselect(cfg);
        let b = &mut self.boot;
        if page == Page::Top {
            b.phase = SlidePhase::Out;
            b.vy = -BOOT_IN_VY;
        } else if was_top || b.phase == SlidePhase::Parked {
            b.phase = SlidePhase::In;
            b.y = -50.0 - f64::from(BOOT_H);
            b.vy = BOOT_IN_VY;
        }
        b.target = page;
        if let Page::Credits(_) = page {
            self.credits = Credits {
                hold: Timer::armed(cx.now, CREDIT_HOLD_MS),
                ..Credits::default()
            };
            self.boot.phase = SlidePhase::Parked;
        }
    }

    fn navigate(&mut self, keys: &KeyState, cx: &mut Ctx) {
        let before = self.selected;
        if self.page == Page::Top {
            self.grid_move(keys);
        } else {
            let n = self.page.items();
            if keys.pressed(Scancode::UP) {
                self.selected = (self.selected + n - 1) % n;
            }
            if keys.pressed(Scancode::DOWN) {
                self.selected = (self.selected + 1) % n;
            }
        }
        if self.selected != before {
            cx.emit(CLICK);
        }
    }

    /// gaps §1.3 grid mode: rows by Up/Down, columns by Left/Right, empty cells skipped.
    fn grid_move(&mut self, keys: &KeyState) {
        // Two rows: up and down both flip the row.
        let (dr, dc) = if keys.pressed(Scancode::UP) || keys.pressed(Scancode::DOWN) {
            (1, 0)
        } else if keys.pressed(Scancode::LEFT) {
            (0, 2)
        } else if keys.pressed(Scancode::RIGHT) {
            (0, 1)
        } else {
            return;
        };
        let at = |item| {
            (0..GRID_STEPS)
                .map(|k| (k / 3, k % 3))
                .find(|&(r, c)| TOP_GRID[r][c] == Some(item))
                .unwrap_or((0, 1))
        };
        let (mut r, mut c) = at(self.selected);
        for _ in 0..GRID_STEPS {
            (r, c) = ((r + dr) % 2, (c + dc) % 3);
            if let Some(item) = TOP_GRID[r][c] {
                self.selected = item;
                return;
            }
        }
    }

    /// game-spec §2.3 page actions.
    fn choose(&mut self, cfg: &mut MatchConfig, options: &mut Options, cx: &mut Ctx) -> Flow {
        let s = self.selected;
        let next = match (self.page, s) {
            (Page::Top, 0) => Page::Play,
            (Page::Top, 1) => Page::Options,
            (Page::Top, 2) => Page::Credits(7),
            (Page::Top, _) => return Flow::Quit,
            (Page::Play, 0) => {
                cfg.players = Players::One;
                Page::OnePlayerMode
            }
            (Page::Play, 1) => {
                cfg.players = Players::Two;
                Page::TwoPlayerMode
            }
            (Page::Play, _) => {
                return Flow::Go(Screen::HighScores(Box::new(HighScoreScreen::new(None, cx))));
            }
            (Page::Options, 2) => Page::ViewKeys,
            (Page::Options, _) => {
                toggle(s, options, cx);
                return Flow::Stay;
            }
            (Page::OnePlayerMode, 0) | (Page::TwoPlayerMode, 0) => {
                cfg.mode = Mode::Normal;
                if cfg.players == Players::One {
                    Page::Difficulty
                } else {
                    Page::Length
                }
            }
            (Page::OnePlayerMode, _) => {
                cfg.mode = Mode::Special;
                cfg.ai_level = AiLevel::Normal;
                cfg.days = 3;
                return Flow::StartMatch;
            }
            (Page::TwoPlayerMode, _) => {
                cfg.mode = Mode::Special;
                return Flow::StartMatch;
            }
            (Page::Difficulty, _) => {
                cfg.ai_level = [AiLevel::Easy, AiLevel::Normal, AiLevel::Intelligent][s.min(2)];
                Page::Length
            }
            (Page::Length, _) => {
                cfg.days = [2, 4, 6][s.min(2)];
                return Flow::StartMatch;
            }
            (Page::ViewKeys, _) => Page::Options,
            (Page::Credits(_), _) => Page::Top,
        };
        self.go(next, cfg, cx);
        Flow::Stay
    }

    fn credits_tick(&mut self, keys: &KeyState, cfg: &MatchConfig, cx: &mut Ctx) -> Flow {
        let c = &mut self.credits;
        if c.hold.expired_rearm(cx.now) {
            c.pair = (c.pair + 1) % CREDIT_PAIRS;
        }
        // Page 9 reads only the Esc edge; the pairs loop until then.
        if keys.pressed(Scancode::ESCAPE) {
            self.go(Page::Top, cfg, cx);
            self.boot.phase = SlidePhase::Parked;
        }
        Flow::Stay
    }
}

/// Options page: Music (0) and Tutorial (1) toggles, saved to `boite.btt`.
fn toggle(item: usize, options: &mut Options, cx: &mut Ctx) {
    if item == 0 {
        options.music = !options.music;
        cx.emit(Event::Music(if options.music {
            Music::Menu
        } else {
            Music::Stop
        }));
    } else {
        options.tutorial = !options.tutorial;
    }
    cx.emit(Event::Save {
        file: SaveFile::Options,
        bytes: encode_options(options),
    });
}

fn pig_h(cx: &Ctx) -> i32 {
    cx.data.sizes.get("i_cochon_corps").map_or(PIG_H, |s| s.h)
}
