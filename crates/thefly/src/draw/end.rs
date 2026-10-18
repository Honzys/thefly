//! End-of-match screens: winner (SR §5.1) and statistics (SR §5.2).

use thefly_platform::{Color, Error};
use thefly_sim::match_state::{MatchState, Players};
use thefly_sim::screen::MatchScreen;
use thefly_sim::stats::StatsScreen;
use thefly_sim::winner::{BAR_MAX, TROPHY_FLOOR, Winner};

use super::View;
use crate::assets::{ARIAL_BLACK, JOINT};
use crate::gfx::{Gfx, WHITE};

const FARMER_BAR: Color = Color::RGB(255, 99, 99);
const FLY_BAR: Color = Color::RGB(0, 168, 255);
const RED: Color = Color::RGB(255, 0, 0);
const BLUE: Color = Color::RGB(0, 0, 255);
const BAR_BOTTOM: f32 = 503.0;
/// The `you_win` caption tween (SR §5.1).
const CAPTION_MS: u64 = 2000;

/// Bar height = counter × 413 / winner score (SR §5.1 state 1).
pub fn bar_height(counter: i32, winner_score: i32) -> f32 {
    if winner_score <= 0 {
        return 0.0;
    }
    (i64::from(counter.max(0)) * i64::from(BAR_MAX) / i64::from(winner_score)) as f32
}

pub fn winner(g: &mut Gfx, v: &View, w: &Winner) -> Result<(), Error> {
    let prev = match v.prev_screen() {
        Some(MatchScreen::Winner(p)) => Some(p.as_ref()),
        _ => None,
    };
    g.spr("winnerbck", 0.0, 0.0, 0)?;
    let fly_crushed = w.loser_crushed && !w.fly_won;
    let farmer_crushed = w.loser_crushed && w.fly_won;
    if !fly_crushed {
        g.spr("winnerfly", 504.0, 326.0, 0)?;
    }
    if !farmer_crushed {
        g.spr("winnerfarmer", 74.0, 235.0, 0)?;
    }
    let (_, th) = g.size("winnercoupe")?;
    let tx = if w.fly_won { 50.0 } else { 430.0 };
    let ty =
        v.l.f(prev.map(|p| p.trophy_y), w.trophy_y)
            .min(TROPHY_FLOOR as f32 - th);
    g.spr("winnercoupe", tx, ty, 0)?;

    let top = w.fly_score.max(w.farmer_score);
    let fh = bar_height(w.farmer_counter, top);
    let yh = bar_height(w.fly_counter, top);
    g.fill(16.0, BAR_BOTTOM - fh, 10.0, fh, FARMER_BAR)?;
    g.fill(774.0, BAR_BOTTOM - yh, 10.0, yh, FLY_BAR)?;
    // SR §5.1 writes "<n> $"; the captures (60-winner-*) show the "$" fixed at the box's right
    // end whatever the width of the number, so it is drawn on its own (observed x).
    let counters = [
        (w.farmer_counter, 18.0, 92.0, RED),
        (w.fly_counter, 690.0, 763.0, BLUE),
    ];
    for (n, x, dollar_x, c) in counters {
        g.text(JOINT, 25, &n.to_string(), x, 51.0, c)?;
        g.text(JOINT, 25, "$", dollar_x, 51.0, c)?;
    }

    if let Some(landed) = v.trophy_landed_ms {
        let since = v.now.saturating_sub(landed);
        let alpha = since.min(CAPTION_MS) as f32 / CAPTION_MS as f32 * 255.0;
        let cx = if w.fly_won { 452.0 } else { 71.0 };
        g.spr_ex("you_win", cx, 148.0, 0, alpha, false)?;
    }
    v.fx.draw_particles(g, v.l)?;
    g.fade(w.fade.alpha)
}

const PANEL: Color = Color::RGB(124, 150, 228);
const TITLE_RED: Color = Color::RGB(222, 19, 0);
const RESULT_FILL: Color = Color::RGB(89, 110, 171);
const ROW_DY: f32 = 23.0;
/// Stats columns: labels run 280 to the value column at 530, values to the right edge.
const LABEL_W: f32 = 245.0;
const VALUE_W: f32 = 260.0;

/// One size for the whole block, so the rows stay level: the largest at which every label and
/// value fits its column.
fn rows(g: &mut Gfx, y0: f32, rows: &[(&str, String)]) -> Result<(), Error> {
    let values: Vec<String> = rows.iter().map(|(_, v)| format!(": {v}")).collect();
    let mut size = 22;
    for ((label, _), value) in rows.iter().zip(&values) {
        size = g.fit_size(ARIAL_BLACK, size, label, LABEL_W)?;
        size = g.fit_size(ARIAL_BLACK, size, value, VALUE_W)?;
    }
    for (i, ((label, _), value)) in rows.iter().zip(&values).enumerate() {
        let y = y0 + ROW_DY * i as f32;
        g.text(ARIAL_BLACK, size, label, 280.0, y, WHITE)?;
        g.text(ARIAL_BLACK, size, value, 530.0, y, WHITE)?;
    }
    Ok(())
}

/// A white box of `n` nested 1-px rectangles.
fn frame_box(g: &mut Gfx, (x, y, w, h): (f32, f32, f32, f32), n: u32) -> Result<(), Error> {
    for k in 0..n {
        let d = k as f32;
        let (x, y, w, h) = (x + d, y + d, w - 2.0 * d, h - 2.0 * d);
        g.fill(x, y, w, 1.0, WHITE)?;
        g.fill(x, y + h - 1.0, w, 1.0, WHITE)?;
        g.fill(x, y, 1.0, h, WHITE)?;
        g.fill(x + w - 1.0, y, 1.0, h, WHITE)?;
    }
    Ok(())
}

/// A sprite frame with the white 8 × 5 px padded box SR §5.2 draws round the coin and dead fly.
fn boxed(g: &mut Gfx, name: &str, (x, y): (f32, f32), index: u32, scale: f32) -> Result<(), Error> {
    let (w, h) = g.size(name)?;
    g.spr_scaled(name, x, y, index, scale, 255.0)?;
    frame_box(g, (x - 8.0, y - 5.0, w * scale + 16.0, h * scale + 10.0), 1)
}

pub fn stats(g: &mut Gfx, _v: &View, m: &MatchState, s: &StatsScreen) -> Result<(), Error> {
    g.spr("highscores", 0.0, 0.0, 0)?;
    g.fill(203.0, 15.0, 383.0, 100.0, PANEL)?;
    g.fill(11.0, 119.0, 361.0, 471.0, PANEL)?;
    g.text_outlined(JOINT, 70, "Stats", (320.0, 30.0), (TITLE_RED, WHITE))?;
    // SR frame numbers are 1-based (SR §1.1), sheet indices 0-based
    g.spr("transi-est", 100.0, 160.0, 4)?;
    // SR §5.2 cuts `tapette-8` to 134 × 187: the handle ends at y 527 (68-stats-2p)
    g.spr_part("tapette-8", 0, (0.0, 0.0, 134.0, 187.0), 100.0, 340.0)?;
    let p = &s.page;
    rows(
        g,
        153.0,
        &[
            ("Objects saved", p.delivered.to_string()),
            (
                "Average objects saved",
                format!("{:.0} %", p.delivered_percent),
            ),
            (
                "Average price of saved obj",
                format!("{:.0}", p.avg_price.trunc()),
            ),
            ("Maximum life time", format!("{:.0} s", p.max_survival_s)),
            ("Average life time", format!("{:.0} s", p.avg_survival_s)),
        ],
    )?;
    if m.config.players == Players::Two {
        rows(
            g,
            363.0,
            &[
                ("Flies killed", p.kills.to_string()),
                ("Average Flies killed", format!("{:.2}", p.kills_per_period)),
                ("Fly swater Hits", p.swings.to_string()),
                ("Average damage done", format!("{:.0}", p.avg_damage)),
                ("Ratio", format!("{:.0} %", p.hit_ratio_percent)),
            ],
        )?;
    }
    if let Some(b) = &s.breakdown {
        frame_box(g, (185.0, 315.0, 450.0, 210.0), 4)?;
        g.text_outlined(JOINT, 38, "Your Score", (317.0, 330.0), (TITLE_RED, WHITE))?;
        g.text(JOINT, 27, &b.points.to_string(), 240.0, 410.0, WHITE)?;
        boxed(g, "piece", (313.0, 398.0), 14, 1.0)?;
        g.text(JOINT, 27, "-", 400.0, 410.0, WHITE)?;
        g.text(JOINT, 27, &b.kills.to_string(), 448.0, 410.0, WHITE)?;
        boxed(g, "g_flymort", (501.0, 394.0), 1, 0.8)?;
        let result = b.points - 10 * b.kills;
        g.text_outlined(
            JOINT,
            38,
            &format!("= {result} Pts"),
            (318.0, 470.0),
            (RESULT_FILL, WHITE),
        )?;
    }
    g.fade(s.fade.alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_winners_bar_ends_413_tall() {
        assert_eq!(bar_height(1200, 1200), 413.0);
        assert_eq!(bar_height(600, 1200), 206.0);
        assert_eq!(bar_height(10, 0), 0.0);
    }
}
