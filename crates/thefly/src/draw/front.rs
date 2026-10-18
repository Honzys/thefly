//! Front-end screens: logo, main menu, keys page, quit dialog, high scores, farewell (SR §2, §5.3,
//! §5.4).

use thefly_platform::{Color, Error};
use thefly_sim::highscores::{HighScoreScreen, ROW_SWEEP_MS, ROWS};
use thefly_sim::match_state::Players;
use thefly_sim::menu::{BOOT_KICK_MS, BOOT_X, Menu, Page, SlidePhase};
use thefly_sim::screen::{Logo, QuitDialog, Screen};

use super::View;
use crate::assets::{ARIAL_BLACK, JOINT};
use crate::gfx::{BLACK, Gfx, NORMAL, SELECTED, WHITE};

pub fn logo(g: &mut Gfx, v: &View, logo: &Logo) -> Result<(), Error> {
    g.spr("Present", 0.0, 0.0, 0)?;
    // shortcut: "default GDI font" is the Windows system face; the Arial stand-in is used
    g.text_fit(ARIAL_BLACK, 13, "Release 1.2", (700.0, 7.0), 90.0, WHITE)?;
    let prev = match v.prev.map(|s| &s.screen) {
        Some(Screen::Logo(p)) => Some(p.fade.alpha),
        _ => None,
    };
    g.fade(f64::from(v.l.f(prev, logo.fade.alpha)))
}

/// The decor's resting y once the intro is over (SR §2.2 slot 0).
const DECOR_REST_Y: f64 = -600.0;
const PIG_HEAD_X: f32 = 550.0;
const SCREEN_H: f32 = 600.0;
/// `i_items1` positions, Play / Options / Credits / Quit (SR §2.2).
const TOP_ITEMS: [(f32, f32); 4] = [
    (257.0, 431.0),
    (119.0, 495.0),
    (257.0, 499.0),
    (385.0, 497.0),
];
/// The boot's rest top y: 350 − h for the 575-px `i_botte` (SR §2.2 boot slide).
const BOOT_REST_Y: f32 = 350.0 - 575.0;
const BOOT_W: f32 = 270.0;
/// SR §2.2 boot slide: pig and boot jiggle at 250 px/s between rest y and rest y + 15.
const JIGGLE_PX: f32 = 15.0;
const JIGGLE_PX_PER_MS: f32 = 0.25;

/// A sub-page item: label, dx from bootX − 55, and y (SR §2.2 sub-page table).
type Item = (&'static str, f32, f32);

fn page_items(page: Page) -> &'static [Item] {
    match page {
        Page::Play => &[
            ("1 Player", 3.0, 127.0),
            ("2 Players", 3.0, 177.0),
            ("HighScores", 13.0, 252.0),
        ],
        Page::Options => &[
            ("Music", 0.0, 120.0),
            ("Tutorial", 5.0, 175.0),
            ("ViewKeys", -10.0, 240.0),
        ],
        Page::OnePlayerMode => &[("Normal", -5.0, 130.0), ("Highscore", 0.0, 205.0)],
        Page::Difficulty => &[
            ("Easy", -4.0, 130.0),
            ("Normal", -4.0, 180.0),
            ("Inteligent", 8.0, 230.0),
        ],
        Page::TwoPlayerMode => &[("Normal", -5.0, 130.0), ("Extreme", -2.0, 205.0)],
        Page::Length => &[
            ("2 days", 0.0, 130.0),
            ("4 days", 0.0, 180.0),
            ("6 days", 0.0, 230.0),
        ],
        Page::Top | Page::Credits(_) | Page::ViewKeys => &[],
    }
}

fn prev_menu<'a>(v: &'a View) -> Option<&'a Menu> {
    match &v.prev?.screen {
        Screen::Menu(m) => Some(m),
        _ => None,
    }
}

pub fn menu(g: &mut Gfx, v: &View, m: &Menu) -> Result<(), Error> {
    let p = prev_menu(v);
    let intro_on = m.intro.step.is_some();
    let decor = if intro_on {
        m.intro.decor_y
    } else {
        DECOR_REST_Y
    };
    let prev_decor = p.map(|p| {
        if p.intro.step.is_some() {
            p.intro.decor_y
        } else {
            DECOR_REST_Y
        }
    });
    let credits = matches!(m.page, Page::Credits(_));
    let scene = if credits { m.credits.scene_y } else { 0.0 };
    let dy =
        v.l.f(p.map(|p| p.credits.scene_y).filter(|_| credits), scene);
    let decor_y = v.l.f(prev_decor, decor) + dy;

    g.spr("i_decors1", 0.0, decor_y, 0)?;
    g.spr("i_soleil", 162.0, decor_y + 450.0, v.anim(100, 8))?;
    // clouds ride the decor (its y −50..699): sky-high at rest, across the screen at intro start
    v.fx.draw_clouds(g, v.l, decor_y)?;
    g.spr("i_decors2", 231.0, decor_y + 610.0, 0)?;
    if m.intro.step == Some(2) {
        // the big fly crossing (SR §2.2 intro step 2): transi-est frames 6–9 at 20 ms
        let x = v.l.f(p.map(|p| p.intro.fly_x), m.intro.fly_x);
        let y = v.l.f(p.map(|p| p.intro.fly_y), m.intro.fly_y);
        g.spr("transi-est", x, y, 5 + v.anim(20, 4))?;
    }
    pig(g, v, m, p, dy)?;
    if credits {
        return credits_page(g, v, m, p);
    }
    if m.page == Page::Top && !intro_on {
        for (i, &(x, y)) in TOP_ITEMS.iter().enumerate() {
            let frame = 2 * i as u32 + u32::from(m.selected == i);
            g.spr("i_items1", x, y + dy, frame)?;
        }
    }
    boot_page(g, v, m, p)?;
    if m.page == Page::ViewKeys {
        view_keys(g)?;
    }
    Ok(())
}

fn pig(g: &mut Gfx, v: &View, m: &Menu, p: Option<&Menu>, dy: f32) -> Result<(), Error> {
    let (_, body_h) = g.size("i_cochon_corps")?;
    let rest = f64::from(SCREEN_H - body_h);
    let y = match m.intro.step {
        Some(3) => v.l.f(p.map(|p| p.intro.pig_y), m.intro.pig_y),
        Some(_) => return Ok(()),
        None => rest as f32,
    } + dy;
    let jiggle = kick_jiggle(v, m);
    let head = if kicking(v, m) { v.anim(50, 2) } else { 0 };
    g.spr("i_cochon_corps", 0.0, y + jiggle, 0)?;
    g.spr("i_cochon_tete", PIG_HEAD_X, y + jiggle, head)
}

/// The 1000 ms after the boot lands on the pig.
fn kicking(v: &View, m: &Menu) -> bool {
    m.boot.phase == SlidePhase::Kick && !m.boot.kick.expired(v.now)
}

fn kick_jiggle(v: &View, m: &Menu) -> f32 {
    if !kicking(v, m) {
        return 0.0;
    }
    let elapsed = BOOT_KICK_MS.saturating_sub(m.boot.kick.remaining_ms(v.now));
    let t = (elapsed as f32 * JIGGLE_PX_PER_MS) % (2.0 * JIGGLE_PX);
    if t < JIGGLE_PX {
        t
    } else {
        2.0 * JIGGLE_PX - t
    }
}

fn boot_page(g: &mut Gfx, v: &View, m: &Menu, p: Option<&Menu>) -> Result<(), Error> {
    if m.boot.phase == SlidePhase::Parked && m.page == Page::Top {
        return Ok(());
    }
    let boot_y = v.l.f(p.map(|p| p.boot.y), m.boot.y) + kick_jiggle(v, m);
    g.spr("i_botte", BOOT_X as f32, boot_y, 0)?;
    // the labels ride with the boot: listed y is for the boot at rest
    let ride = boot_y - BOOT_REST_Y;
    // observed: the labels centre on bootX − 55 + dx + 135, half the boot's width
    let x0 = (BOOT_X - 55) as f32 + BOOT_W / 2.0;
    for (i, &(label, dx, y)) in page_items(m.page).iter().enumerate() {
        let c = if i == m.selected { SELECTED } else { NORMAL };
        g.text_centred(JOINT, 25, label, (x0 + dx, y + ride), c)?;
        let subtitle = matches!(m.page, Page::OnePlayerMode | Page::TwoPlayerMode);
        if subtitle {
            g.text_centred(JOINT, 25, "Mode", (x0 + dx, y + 25.0 + ride), c)?;
        }
    }
    if m.page == Page::Options {
        let on_off = |b: bool| if b { "On" } else { "Off" };
        let o = &v.cur.options;
        g.text_centred(JOINT, 25, on_off(o.music), (x0, 140.0 + ride), NORMAL)?;
        g.text_centred(JOINT, 25, on_off(o.tutorial), (x0, 195.0 + ride), NORMAL)?;
    }
    Ok(())
}

/// SR §2.4. shortcut: the three lines' wording is not in the docs; they list the default keys
/// (game-spec §4) until the strings are recovered.
fn view_keys(g: &mut Gfx) -> Result<(), Error> {
    g.spr("boitedlg", 145.0, 100.0, 0)?;
    let lines = [
        "Fly: arrows, Enter, R-Shift",
        "Farmer: T F G H, Tab, Q",
        "Esc: pause / quit",
    ];
    for (i, line) in lines.iter().enumerate() {
        g.text(JOINT, 18, line, 265.0, 200.0 + 25.0 * i as f32, WHITE)?;
    }
    g.text(JOINT, 25, "OK", 400.0, 440.0, SELECTED)
}

/// SR §2.3. shortcut: the credit text pairs are not reproduced in the docs, so only the photos
/// and the letterbox are drawn.
fn credits_page(g: &mut Gfx, v: &View, m: &Menu, p: Option<&Menu>) -> Result<(), Error> {
    const PHOTO_START_Y: f32 = -300.0;
    let dy = v.l.f(p.map(|p| p.credits.scene_y), m.credits.scene_y);
    let frame = v.anim(20, 3);
    g.spr("i_credit_seb", 100.0, PHOTO_START_Y + dy, frame)?;
    g.spr("i_credit_gaga", 400.0, PHOTO_START_Y + dy, frame)?;
    let bar = v.l.i(p.map(|p| p.credits.letterbox), m.credits.letterbox);
    g.fill(0.0, 0.0, 800.0, bar, BLACK)?;
    g.fill(0.0, SCREEN_H - bar, 800.0, bar, BLACK)
}

/// SR §2.6.
pub fn keys_page(g: &mut Gfx, players: Players) -> Result<(), Error> {
    g.spr("touches", 0.0, 0.0, 0)?;
    if players != Players::One {
        return Ok(());
    }
    // shortcut: open item O6, the fill colour is unresolved; black until it is
    g.fill(0.0, 0.0, 400.0, SCREEN_H, BLACK)?;
    g.text_fit(ARIAL_BLACK, 35, "1 Player : ", (130.0, 245.0), 260.0, WHITE)?;
    g.text_fit(ARIAL_BLACK, 35, "The Fly", (130.0, 285.0), 260.0, WHITE)
}

/// SR §2.5. The YES/NO x use the steady-state values (open item O15 covers the first show).
pub fn quit_dialog(g: &mut Gfx, d: &QuitDialog) -> Result<(), Error> {
    const YES_X: f32 = 257.0;
    const NO_X: f32 = 517.0;
    const BUTTON_Y: f32 = 425.0;
    g.spr("boitedlg", 145.0, 100.0, 0)?;
    if d.from_menu {
        // shortcut: the question's exact wording is not in the docs
        g.text(JOINT, 20, "Do you really want", 300.0, 281.0, WHITE)?;
        g.text(JOINT, 20, "to quit ?", 335.0, 311.0, WHITE)?;
    } else {
        g.text(JOINT, 20, "Stop the game ?", 325.0, 281.0, WHITE)?;
    }
    let c = |on: bool| if on { SELECTED } else { NORMAL };
    g.text(JOINT, 25, "YES", YES_X, BUTTON_Y, c(d.yes))?;
    g.text(JOINT, 25, "NO", NO_X, BUTTON_Y, c(!d.yes))
}

const RANK_COLOURS: [Color; 3] = [
    Color::RGB(255, 208, 24),
    Color::RGB(162, 162, 162),
    Color::RGB(221, 112, 46),
];

fn row_y(i: usize) -> f32 {
    163.0 + 40.0 * i as f32
}

/// SR §5.3.
pub fn high_scores(g: &mut Gfx, v: &View, hs: &HighScoreScreen) -> Result<(), Error> {
    g.spr("highscores", 0.0, 0.0, 0)?;
    let (_, cup_h) = g.size("winnercoupe")?;
    g.spr("winnercoupe", 415.0, 585.0 - cup_h, 0)?;
    let first_shown = hs.revealed.min(ROWS);
    for (i, row) in v
        .cur
        .scores
        .rows
        .iter()
        .enumerate()
        .take(ROWS)
        .skip(first_shown)
    {
        let c = RANK_COLOURS.get(i).copied().unwrap_or(WHITE);
        let name = match &hs.entry {
            Some(e) if e.row == i => format!("{}{}", e.name, if e.cursor_on { "_" } else { "" }),
            _ => row.name.clone(),
        };
        let cells = [
            (50.0, 45.0, format!("{}.", i + 1)),
            (100.0, 145.0, name),
            (250.0, 300.0, format!("{}  Pts", row.score)),
        ];
        for (x, w, s) in &cells {
            let size = g.fit_size(ARIAL_BLACK, 25, s, *w)?;
            g.text(ARIAL_BLACK, size, s, x - 1.0, row_y(i) + 1.0, BLACK)?;
            g.text(ARIAL_BLACK, size, s, *x, row_y(i), c)?;
        }
    }
    if hs.revealed == 0 || !hs.sweep.is_armed() {
        return Ok(());
    }
    // the highlight bar sweeping the next row: 20×20 cell ×15 wide, alpha 255 → 0 in 200 ms
    let left = hs.sweep.remaining_ms(v.now).min(ROW_SWEEP_MS) as f32;
    let alpha = 255.0 * left / ROW_SWEEP_MS as f32;
    let row = (hs.revealed - 1).min(ROWS - 1);
    let (w, h) = g.size("highscores_color")?;
    g.spr_stretched(
        "highscores_color",
        0,
        (50.0, row_y(row), w * 15.0, h),
        alpha,
    )
}

/// SR §5.4. shortcut: open item O5, the farewell layout and timing are not decoded; the logo is
/// centred on black and the midges are left out.
pub fn farewell(g: &mut Gfx) -> Result<(), Error> {
    let (w, h) = g.size("logo")?;
    g.spr("logo", (800.0 - w) / 2.0, (SCREEN_H - h) / 2.0, 0)
}
