//! W3. The main menu (0x14): first-visit intro script, page tree, boot slide, credits pages and
//! the view-keys dialog (game-spec §2.3, screens-and-rendering §2.2–§2.4). Clouds and dust puffs
//! are frontend cosmetics (W4); only state that gates input or emits sound lives here.

use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::event::{Event, Music, Pan, PlayParams, SaveFile, SoundOp, SoundSlot};
use crate::highscores::HighScoreScreen;
use crate::input::{KeyState, Scancode};
use crate::match_state::{MatchConfig, Mode, Players};
use crate::mover::{FpsRun, Wander};
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

/// Credits choreography (answers.md "Credits: slide-in, page 9, slide-out and Esc"): page 7
/// slides the scene in, page 9 shows it (letterbox, text pairs), page 8 slides it out.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Credits {
    /// The scene's offset from its rest pose: layer 0 (the decor) is at y −600 + `scene_y`.
    pub scene_y: f64,
    /// px/s, moved through the FPS integrator.
    pub scene_vy: i32,
    scene_run: FpsRun,
    /// Page 7's stop test passed in the render step; the next input step goes to page 9.
    slid_in: bool,
    /// The slide-out has bounced once: its accel is −40 from then on.
    pub bounced: bool,
    pub letterbox: i32,
    pub pair: u8,
    pub hold: Timer,
    /// Top-left of `i_credit_seb` and `i_credit_gaga` in scene space; draw at y + `scene_y`.
    pub photos: [[i32; 2]; 2],
    pub movers: [Wander; 2],
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
pub const CREDIT_PAIRS: u8 = 5;
pub const CREDIT_HOLD_MS: u64 = 7000;
/// Page 7: the scene slides down at 520 px/s, 5 px/s slower each tick, until layer 0's y ≥ −200
/// or vy ≤ 0. Page 9: the letterbox grows 10 px a tick to 120. Page 8: vy −20, accel −5, then −40
/// after the first bounce; once layer 0's y + h < 601 the scene snaps back, booms at volume 300,
/// and stops if |vy| < 61, else bounces with −(vy / 2).
const CREDIT_SLIDE_VY: i32 = 520;
const CREDIT_SLIDE_DECEL: i32 = 5;
const SLIDE_IN_STOP: f64 = 400.0;
const SLIDE_OUT_VY: i32 = -20;
const SLIDE_OUT_ACCEL: [i32; 2] = [-5, -40];
const SLIDE_OUT_SNAP_BELOW: f64 = 1.0;
const BOUNCE_STOP: i32 = 61;
const BOOM_VOLUME_CB: i32 = 300;
const LETTERBOX_STEP: i32 = 10;
const LETTERBOX_MAX: i32 = 120;
/// The photos start at (100, −300) and (400, −300) and wander: reroll 500, speeds 30/30, runs
/// 8/6, accel 0, y accel flag set (so only x re-rolls); page 8 sets the y accel to 5. In page 9
/// they keep to x + w ≤ 820 and letterbox − 20 ≤ screen y ≤ 620 − letterbox − h.
const PHOTO_START: [[i32; 2]; 2] = [[100, -300], [400, -300]];
const PHOTO_REROLL_MS: i32 = 500;
const PHOTO_SPEED: (i32, i32) = (30, 30);
const PHOTO_RUNS: (i32, i32) = (8, 6);
const SLIDE_OUT_PHOTO_ACCEL: (i32, i32) = (0, 5);
/// `i_credit_seb` and `i_credit_gaga` frames (SR §2.3).
const PHOTO_SIZE: [(i32, i32); 2] = [(200, 111), (200, 113)];
const PHOTO_RIGHT: i32 = 820;
const PHOTO_TOP_SLACK: i32 = 20;
const PHOTO_BOTTOM: i32 = 620;

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
                self.go(parent, cfg);
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

    fn go(&mut self, page: Page, cfg: &MatchConfig) {
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
            let mover = Wander::new(PHOTO_REROLL_MS, PHOTO_SPEED, PHOTO_RUNS, (0, 0))
                .with_accel_flags(false, true);
            self.credits = Credits {
                scene_vy: CREDIT_SLIDE_VY,
                photos: PHOTO_START,
                movers: [mover; 2],
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
        self.go(next, cfg);
        Flow::Stay
    }

    /// The input step runs the page machine (`FUN_0041be05`); the render step then moves the
    /// layers, then the photos, seb first (`FUN_0041eb60`).
    fn credits_tick(&mut self, keys: &KeyState, cfg: &MatchConfig, cx: &mut Ctx) -> Flow {
        let c = &mut self.credits;
        match self.page {
            Page::Credits(7) if c.slid_in => {
                self.page = Page::Credits(9);
                c.hold.arm(cx.now, CREDIT_HOLD_MS);
            }
            // Page 9 reads only the Esc edge; pages 7 and 8 read nothing.
            Page::Credits(9) if keys.pressed(Scancode::ESCAPE) => {
                self.page = Page::Credits(8);
                c.scene_vy = SLIDE_OUT_VY;
                for mover in &mut c.movers {
                    mover.set_accel(SLIDE_OUT_PHOTO_ACCEL);
                }
            }
            _ => {}
        }
        let shown = self.page == Page::Credits(9);
        let done = match self.page {
            Page::Credits(7) => {
                c.slide_in();
                false
            }
            Page::Credits(9) => {
                c.show(cx.now);
                false
            }
            _ => c.slide_out(cx),
        };
        c.move_photos(shown, cx);
        if done {
            self.go(Page::Top, cfg);
            self.boot.phase = SlidePhase::Parked;
        }
        Flow::Stay
    }
}

impl Credits {
    fn move_scene(&mut self) {
        self.scene_y += f64::from(self.scene_run.step([0, self.scene_vy]).1);
    }

    /// Page 7: move, then the stop test, else slow down; the letterbox does not grow. This order
    /// gives answers.md's ends: tick 86 at y −200, vy 100 (60 fps) and tick 106 at y −228 (65 fps),
    /// counting the Enter tick as tick 1.
    fn slide_in(&mut self) {
        self.move_scene();
        self.slid_in = self.scene_y >= SLIDE_IN_STOP || self.scene_vy <= 0;
        if !self.slid_in {
            self.scene_vy -= CREDIT_SLIDE_DECEL;
        }
    }

    /// Page 9: the letterbox grows and the text pairs loop.
    fn show(&mut self, now: u64) {
        self.letterbox = (self.letterbox + LETTERBOX_STEP).min(LETTERBOX_MAX);
        if self.hold.expired_rearm(now) {
            self.pair = (self.pair + 1) % CREDIT_PAIRS;
        }
    }

    /// Page 8; true once a bounce is slow enough to stop.
    // shortcut: answers.md gives the page 8 steps but not whether the accel is added before or
    // after the move; it follows page 7 (move first). Revisit with a capture of the bounce ticks.
    fn slide_out(&mut self, cx: &mut Ctx) -> bool {
        self.letterbox = (self.letterbox - LETTERBOX_STEP).max(0);
        self.move_scene();
        if self.scene_y < SLIDE_OUT_SNAP_BELOW {
            self.scene_y = 0.0;
            let boom = PlayParams {
                volume_cb: BOOM_VOLUME_CB,
                ..PlayParams::default()
            };
            cx.sound(SoundSlot::BOOM, SoundOp::Play(boom));
            if self.scene_vy.abs() < BOUNCE_STOP {
                return true;
            }
            self.scene_vy = -(self.scene_vy / 2);
            self.bounced = true;
        }
        self.scene_vy += SLIDE_OUT_ACCEL[usize::from(self.bounced)];
        false
    }

    /// Each photo wanders on the scene (its camera delta is the layer step); in page 9 it is
    /// clamped in screen space, the left bound never applied.
    // shortcut: whether the page 9 bounds stay on in page 8 is not given; they are dropped there.
    fn move_photos(&mut self, bounded: bool, cx: &mut Ctx) {
        let scene = self.scene_y as i32;
        let top = self.letterbox - PHOTO_TOP_SLACK;
        for (i, (w, h)) in PHOTO_SIZE.into_iter().enumerate() {
            let (dx, dy) = self.movers[i].step(cx);
            let [x, y] = &mut self.photos[i];
            (*x, *y) = (*x + dx, *y + dy);
            if !bounded {
                continue;
            }
            let bottom = PHOTO_BOTTOM - self.letterbox - h;
            *x = (*x).min(PHOTO_RIGHT - w);
            *y = (*y + scene).max(top).min(bottom) - scene;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::MsvcRand;
    use crate::screen::rig::Rig;

    const TICK_CAP: usize = 400;

    fn in_credits(rig: &mut Rig) -> (Menu, MatchConfig) {
        let (mut m, cfg) = (Menu::new(false), MatchConfig::default());
        m.go(Page::Credits(7), &cfg);
        rig.rng = MsvcRand::new(1);
        (m, cfg)
    }

    fn tick(rig: &mut Rig, m: &mut Menu, cfg: &MatchConfig) {
        rig.step(&[], |k, cx| m.credits_tick(k, cfg, cx));
    }

    /// answers.md "Credits: slide-in, page 9, slide-out and Esc", seed 1: the y accel flag is set,
    /// so only x re-rolls. seb x: timer 41, d 18467 % 10 = 7 (+1), 6334 % 30 + 65 = 69; gaga x:
    /// timer 26500, d 9 (+1), 15724 % 30 + 65 = 69. Each moves 1 px right; y stays.
    #[test]
    fn the_credit_photos_wander_on_x_only() {
        let mut rig = Rig::new(1);
        let (mut m, cfg) = in_credits(&mut rig);
        tick(&mut rig, &mut m, &cfg);
        let c = m.credits;
        assert_eq!(c.movers.map(|w| w.v), [[69, 0], [69, 0]]);
        assert_eq!(c.photos, [[101, -300], [401, -300]]);
        assert_eq!(rig.rng.rand(), 11478);
    }

    /// Page 7: v changes every tick, so each FPS run is fresh and moves trunc(v / 65):
    /// Σ trunc((520 − 5k) / 65) for k = 0..=104 is 372. Tick 105 moves at vy 0 and ends the
    /// slide; page 9 starts on tick 106. The letterbox stays 0 in page 7 and Esc is ignored.
    #[test]
    fn the_scene_slides_in_372_px_then_page_9_shows() {
        let mut rig = Rig::new(1);
        let (mut m, cfg) = in_credits(&mut rig);
        for t in 1..=105 {
            let esc: &[Scancode] = if t == 50 { &[Scancode::ESCAPE] } else { &[] };
            rig.step(esc, |k, cx| m.credits_tick(k, &cfg, cx));
            assert_eq!((m.page, m.credits.letterbox), (Page::Credits(7), 0), "{t}");
        }
        assert_eq!((m.credits.scene_y, m.credits.scene_vy), (372.0, 0));
        tick(&mut rig, &mut m, &cfg);
        assert_eq!((m.page, m.credits.letterbox), (Page::Credits(9), 10));
    }

    fn shown(rig: &mut Rig) -> (Menu, MatchConfig) {
        let (mut m, cfg) = in_credits(rig);
        for _ in 0..TICK_CAP {
            tick(rig, &mut m, &cfg);
            if m.page == Page::Credits(9) {
                break;
            }
        }
        (m, cfg)
    }

    /// Page 9 bounds, in screen space: at scene 372 the photos sit at screen y 72, and the top
    /// bound letterbox − 20 pushes them to 100 once the letterbox is 120 (12 ticks): scene y
    /// 100 − 372 = −272. x stays ≤ 820 − 200.
    #[test]
    fn page_9_keeps_the_photos_inside_the_letterbox() {
        let mut rig = Rig::new(1);
        let (mut m, cfg) = shown(&mut rig);
        for _ in 1..12 {
            tick(&mut rig, &mut m, &cfg);
        }
        let c = m.credits;
        assert_eq!(c.letterbox, 120);
        assert_eq!(c.photos.map(|p| p[1]), [-272, -272]);
        for _ in 0..TICK_CAP {
            tick(&mut rig, &mut m, &cfg);
            assert!(m.credits.photos.iter().all(|p| p[0] <= 620));
        }
    }

    /// Esc in page 9 starts page 8 at vy −20, accel −5 (moving first, as in page 7): the scene
    /// is back at rest on tick 101 at vy −520 (boom, bounce to 260, accel −40), again on 113 at
    /// −220 (110) and 118 at −90 (45). 45 − 40 = 5 moves 0 px, so tick 119 snaps again at
    /// |vy| 5 < 61: the last boom, and the top page.
    #[test]
    fn esc_slides_the_scene_out_with_bounces() {
        let mut rig = Rig::new(1);
        let (mut m, cfg) = shown(&mut rig);
        let mut booms = Vec::new();
        for t in 1..=200 {
            let esc: &[Scancode] = if t == 1 { &[Scancode::ESCAPE] } else { &[] };
            let before = rig.events.len();
            rig.step(esc, |k, cx| m.credits_tick(k, &cfg, cx));
            let boom =
                |e: &Event| matches!(e, Event::Sound { slot, .. } if *slot == SoundSlot::BOOM);
            if rig.events[before..].iter().any(boom) {
                booms.push(t);
            }
            if t == 12 {
                assert_eq!((m.page, m.credits.letterbox), (Page::Credits(8), 0));
            }
            if m.page == Page::Top {
                break;
            }
        }
        assert_eq!(booms, [101, 113, 118, 119]);
        assert_eq!((m.page, m.credits.scene_y), (Page::Top, 0.0));
    }

    /// answers.md "Credits: the first hold is 7000 ms": the pairs cycle in page 9, the first hold
    /// counted from its start; each flip is due on the first tick strictly past 7000 ms.
    #[test]
    fn every_credit_pair_holds_7000_ms_the_first_included() {
        let mut rig = Rig::new(1);
        let (mut m, cfg) = shown(&mut rig);
        let mut last = rig.now() - 16;
        let mut gaps = Vec::new();
        for _ in 0..1000 {
            let (pair, now) = (m.credits.pair, rig.now());
            tick(&mut rig, &mut m, &cfg);
            if m.credits.pair != pair {
                gaps.push(now - last);
                last = now;
            }
        }
        assert!(gaps.len() >= 2, "{gaps:?}");
        assert!(gaps.iter().all(|g| (7000..7032).contains(g)), "{gaps:?}");
    }
}
