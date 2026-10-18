//! Between-period screens: goodies shop, traps shop, bourse (SR §3) and the bonus round (SR §6).

use thefly_platform::{Color, Error};
use thefly_sim::bonus::{BonusPhase, BonusRound, COIN_VALUE, HIT_VALUE, Piece};
use thefly_sim::bourse::{Bourse, COLUMN_OBJECT, COLUMNS};
use thefly_sim::economy::ITEMS;
use thefly_sim::match_state::MatchState;
use thefly_sim::screen::MatchScreen;
use thefly_sim::shop::{GoodiesShop, ShopSide, TrapsShop};

use super::View;
use crate::assets::{ARIAL_BLACK, JOINT};
use crate::gfx::{BLACK, Gfx, WHITE, centred_x};

const GREEN: Color = Color::RGB(0, 255, 0);
const CAPTION_BLUE: Color = Color::RGB(25, 60, 220);

/// Goodies-shop slots (SR §3.1); slot 5 is the exit button.
const SHOP_SLOTS: [f32; 6] = [44.0, 179.0, 309.0, 440.0, 570.0, 713.0];
const SHOP_SLOT_Y: f32 = 3.0;
/// The price and count boxes right of each icon (observed): offset and width.
const BOX_DX: f32 = 56.0;
const BOX_W: f32 = 58.0;
const PRICE_RED: Color = Color::RGB(220, 30, 30);

fn shop_fade(v: &View, cur: f64) -> f64 {
    let prev = match v.prev_screen() {
        Some(MatchScreen::GoodiesShop(s)) => Some(s.fade.alpha),
        Some(MatchScreen::TrapsShop(s)) => Some(s.fade.alpha),
        _ => None,
    };
    f64::from(v.l.f(prev, cur))
}

/// SR §3.1, laid out as the original's captures show it: the price in red in the box right of
/// each top icon, the side's panel and bottom bar (`g_barres_bas` frame 0 fly, 1 farmer) over the
/// machine, owned counts as a coloured icon plus "xN" in the bottom boxes, and the money in the
/// panel. shortcut: open item O4, the purchase animation, the cockroaches, the tally art and the
/// level gauges are not decoded; revisit when `FUN_0042322f` is read.
pub fn goodies_shop(g: &mut Gfx, v: &View, m: &MatchState, s: &GoodiesShop) -> Result<(), Error> {
    g.spr("g_background", 0.0, 0.0, 0)?;
    let belt = if s.conveyor.is_some() {
        v.anim(100, 3)
    } else {
        0
    };
    g.spr("g_TapiRoulant", 346.0, 214.0, belt)?;
    let fly = s.side == ShopSide::Fly;
    if !fly {
        g.spr("g_barre_haut_tapette", 0.0, 0.0, 0)?;
    }
    g.spr("g_barres_tapis", 414.0, 67.0, 0)?;
    // the machine shakes inside its 4 × 4 box (SR §3.1 "wanders inside a small box")
    let shake = |n: u64| ((v.now / 40 + n) % 5) as f32 - 2.0;
    let (mx, my) = (7.0 + shake(0), 143.0 + shake(2));
    g.spr("g_Machine", mx, my, 0)?;
    g.spr("g_Machine_cache", mx + 46.0, my + 156.0, 0)?;
    let (_, can_h) = g.size("can1")?;
    g.spr("can1", 805.0, 239.0 - can_h, 0)?;
    g.spr("g_barres_bas", 0.0, 369.0, u32::from(!fly))?;

    let (icon0, item0) = if fly { (0, 5) } else { (5, 0) };
    if let Some(&x) = SHOP_SLOTS.get(s.cursor).filter(|_| s.cursor < 5) {
        g.spr("Goodies", x, 5.0, icon0 + s.cursor as u32)?;
    }
    if let Some(&x) = SHOP_SLOTS.get(s.cursor) {
        g.spr("g_Selection", x + 5.0, SHOP_SLOT_Y + 52.0, v.anim(30, 3))?;
    }
    let e = &m.economy;
    for (i, &x) in SHOP_SLOTS[..5].iter().enumerate() {
        let k = item0 + i;
        let price = format!("{} $", e.item_prices[k]);
        let w = g.r.text_width(ARIAL_BLACK, 18, &price)?;
        g.text(
            ARIAL_BLACK,
            18,
            &price,
            x + BOX_DX + BOX_W - 5.0 - w,
            12.0,
            PRICE_RED,
        )?;
        let owned = e.owned(ITEMS[k].item);
        // the level items show a gauge in the bar instead of a count box
        if ITEMS[k].increment > 0 || owned <= 0 {
            continue;
        }
        g.spr("Goodies", x, 541.0, icon0 + i as u32)?;
        let count = (x + BOX_DX + BOX_W / 2.0, 566.0);
        g.text_centred(ARIAL_BLACK, 18, &format!("x{owned}"), count, WHITE)?;
    }
    let money = match (&s.tally, fly) {
        (Some(t), _) if t.side == s.side => t.shown,
        (_, true) => e.fly.money,
        (_, false) => e.farmer.money,
    };
    g.text(JOINT, 43, &money.to_string(), 140.0, 470.0, WHITE)?;
    g.text(JOINT, 43, "$", 243.0, 470.0, WHITE)?;
    g.fade(shop_fade(v, s.fade.alpha))
}

/// Traps-shop items; the last is the done button (SR §3.2).
const TRAP_ITEMS: [(f32, f32); 4] = [
    (23.0, 281.0),
    (265.0, 281.0),
    (506.0, 281.0),
    (266.0, 530.0),
];

/// SR §3.2. shortcut: open item O4, the icons, the cross and the token text positions were not
/// read; only the backdrop and the cursor are drawn, with the token count bottom right.
pub fn traps_shop(g: &mut Gfx, v: &View, m: &MatchState, s: &TrapsShop) -> Result<(), Error> {
    g.spr("p_traps", 0.0, 0.0, 0)?;
    if let Some(&(x, y)) = TRAP_ITEMS.get(s.cursor) {
        g.spr("p_fleche", x, y, v.anim(30, 3))?;
    }
    let tokens = m.economy.farmer.tokens;
    g.text(ARIAL_BLACK, 30, &format!("x{tokens}"), 680.0, 540.0, GREEN)?;
    g.fade(shop_fade(v, s.fade.alpha))
}

const BAND_COLOURS: [Color; 3] = [
    Color::RGB(29, 222, 38),
    Color::RGB(255, 255, 0),
    Color::RGB(255, 0, 0),
];
const BAND: i32 = 100;

/// The bands of a bourse bar of `h` px, bottom up: `(offset from the baseline, height, colour)`.
/// Green, yellow and red per 100 px, black beyond (SR §3.3).
pub fn bar_bands(h: i32) -> Vec<(i32, i32, Color)> {
    const MAX_BANDS: i32 = 40;
    (0..MAX_BANDS)
        .map(|k| (k * BAND, (h - k * BAND).min(BAND), k))
        .take_while(|&(_, len, _)| len > 0)
        .map(|(off, len, k)| {
            let c = BAND_COLOURS.get(k as usize).copied().unwrap_or(BLACK);
            (off, len, c)
        })
        .collect()
}

/// `n` right-aligned the way SR §3.3 shifts the labels: by `three` px for 3 characters and by
/// `four` for 4 or more.
fn shift(s: &str, three: f32, four: f32) -> f32 {
    match s.chars().count() {
        0..=2 => 0.0,
        3 => three,
        _ => four,
    }
}

pub fn bourse(g: &mut Gfx, _v: &View, m: &MatchState, b: &Bourse) -> Result<(), Error> {
    g.spr("g_boursebg", 0.0, 0.0, 0)?;
    let e = &m.economy;
    let price = |obj: usize| e.food_prices.get(obj).map_or(0, |p| *p as i32);
    let in_play = |obj: usize| e.present.get(obj).copied().unwrap_or(false);
    let blinking = (0..COLUMNS)
        .filter(|&i| in_play(COLUMN_OBJECT[i]))
        .max_by_key(|&i| price(COLUMN_OBJECT[i]));
    for (i, &obj) in COLUMN_OBJECT.iter().enumerate() {
        let col = (i % 11) as f32;
        let (base, cover_y) = if i < 11 {
            (208.0, 98.0)
        } else {
            (392.0, 282.0)
        };
        let x = 50.0 + 69.0 * col;
        for (off, len, c) in bar_bands(b.bars[i]) {
            g.fill(x, base - (off + len) as f32, 9.0, len as f32, c)?;
        }
        let hidden = b.blink_on && blinking == Some(i) && b.bars[i] >= price(obj);
        if !hidden {
            let (frame, alpha) = if in_play(obj) { (1, 255.0) } else { (0, 150.0) };
            g.spr_ex(
                "g_bourse_cache",
                27.0 + 69.0 * col,
                cover_y,
                frame,
                alpha,
                false,
            )?;
        }
        let label = format!("{}$", price(obj));
        let lx = x - 5.0 - shift(&label, 5.0, 10.0);
        g.text(ARIAL_BLACK, 22, &label, lx, base + 5.0, WHITE)?;
    }
    let (left, right) = (e.farmer.score, e.fly.score);
    let right_leads = right >= left;
    g.spr("g_classe_bourse", 171.0, 484.0, u32::from(right_leads))?;
    g.spr("g_classe_bourse", 570.0, 484.0, u32::from(!right_leads))?;
    for (n, x) in [(left, 183.0), (right, 585.0)] {
        let s = format!("{n} $");
        let dx = match s.chars().count() {
            0..=3 => 0.0,
            4 => 8.0,
            _ => 15.0,
        };
        g.text(JOINT, 26, &s, x - dx, 554.0, WHITE)?;
    }
    Ok(())
}

fn pieces(
    g: &mut Gfx,
    v: &View,
    (name, frames): (&str, u32),
    cur: &[Piece],
    prev: Option<&[Piece]>,
) -> Result<(), Error> {
    for (i, p) in cur.iter().enumerate().filter(|(_, p)| p.active) {
        let pp = prev.and_then(|ps| ps.get(i)).filter(|p| p.active);
        let x = v.l.f(pp.map(|p| p.x), p.x);
        let y = v.l.f(pp.map(|p| p.y), p.y) + BONUS_Y;
        if name == "piecetap" {
            g.spr("ailesgoodies", x - 26.0, y - 8.0, v.anim(60, 3))?;
        }
        g.spr(name, x, y, (v.anim(50, frames) + i as u32) % frames)?;
    }
    Ok(())
}

/// The bonus field's screen y: `bckbonusfond` (800 × 440) between black bars, as the captures
/// show; the sim works in field coordinates (gaps §7.3's bgY taken as 0).
const BONUS_Y: f32 = 80.0;
/// `bckbonusbarre`'s y: its bottom on the field's bottom edge (observed).
const BARRE_Y: f32 = BONUS_Y + 440.0 - 317.0;

/// SR §6: the layout is open (open item O3). The field sits between black bars, coins are
/// `piece`, the farmer's targets the winged `piecetap` balloons (as captured). shortcut: O3, the
/// bonus swatter is drawn as the in-game hand `tapette-1` with its head on the mover box and the
/// fly as `Face`; the countdown in Joint and the tally in `boitedlg`. Revisit with O3.
pub fn bonus(g: &mut Gfx, v: &View, b: &BonusRound) -> Result<(), Error> {
    let prev = match v.prev_screen() {
        Some(MatchScreen::Bonus(p)) => Some(p.as_ref()),
        _ => None,
    };
    g.fill(0.0, 0.0, 800.0, 600.0, BLACK)?;
    g.spr("bckbonusfond", 0.0, BONUS_Y, 0)?;
    // the coins and the actors pass in front of the fence (65-bonus-*)
    g.spr("bckbonusbarre", 0.0, BARRE_Y, 0)?;
    let coins = prev.map(|p| p.coins.as_slice());
    pieces(g, v, ("piece", 15), &b.coins, coins)?;
    let fx = v.l.f(prev.map(|p| p.fly.x), b.fly.x);
    let fy = v.l.f(prev.map(|p| p.fly.y), b.fly.y) + BONUS_Y;
    g.spr("Face", fx, fy, v.anim(50, 10))?;
    let targets = prev.map(|p| p.targets.as_slice());
    pieces(g, v, ("piecetap", 1), &b.targets, targets)?;
    let sx = v.l.f(prev.map(|p| p.swatter.x), b.swatter.x);
    let sy = v.l.f(prev.map(|p| p.swatter.y), b.swatter.y) + BONUS_Y;
    g.spr("tapette-1", sx - 50.0, sy - 10.0, 0)?;
    match b.phase {
        BonusPhase::Countdown { count, .. } => {
            let s = if count >= 1 {
                count.to_string()
            } else {
                "Go!".to_owned()
            };
            let x = centred_x(s.chars().count(), 200.0);
            let colours = (Color::RGB(5, 55, 255), Color::RGB(255, 255, 0));
            g.text_outlined(JOINT, 200, &s, (x, 150.0), colours)
        }
        BonusPhase::Result => bonus_result(g, b),
        BonusPhase::Play { .. } | BonusPhase::End { .. } => Ok(()),
    }
}

/// The tally dialog as captured (59-bonus-result): a coloured caption and a white sum per side,
/// centred in `boitedlg`, and a green "OK". Observed positions.
fn bonus_result(g: &mut Gfx, b: &BonusRound) -> Result<(), Error> {
    const CX: f32 = 402.0;
    g.spr("boitedlg", 145.0, 100.0, 0)?;
    let fly = format!(
        "{} x {COIN_VALUE}$ = {} $",
        b.coins_taken,
        COIN_VALUE * b.coins_taken
    );
    let farmer = format!("{} x {HIT_VALUE}$ = {} $", b.hits, HIT_VALUE * b.hits);
    let lines = [
        ("Score the Fly :", 228.0, CAPTION_BLUE),
        (fly.as_str(), 258.0, WHITE),
        ("Score the Farmer :", 310.0, PRICE_RED),
        (farmer.as_str(), 340.0, WHITE),
        ("OK", 422.0, GREEN),
    ];
    for (s, y, c) in lines {
        g.text_centred(JOINT, 25, s, (CX, y), c)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_stack_green_yellow_red_then_black() {
        assert!(bar_bands(0).is_empty());
        let b = bar_bands(250);
        assert_eq!(b.len(), 3);
        assert_eq!((b[0].0, b[0].1, b[0].2), (0, 100, BAND_COLOURS[0]));
        assert_eq!((b[2].0, b[2].1, b[2].2), (200, 50, BAND_COLOURS[2]));
        assert_eq!(bar_bands(350)[3].2, BLACK);
    }

    #[test]
    fn labels_shift_left_with_length() {
        assert_eq!(shift("9$", 5.0, 10.0), 0.0);
        assert_eq!(shift("99$", 5.0, 10.0), 5.0);
        assert_eq!(shift("999$", 5.0, 10.0), 10.0);
    }
}
