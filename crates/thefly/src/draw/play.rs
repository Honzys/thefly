//! The gameplay screen: loading, scene layers, actors, HUD and the period banner (SR §4, gaps §4).

use std::f32::consts::{FRAC_PI_2, TAU};

use thefly_platform::{Color, Error};
use thefly_sim::match_state::{MatchState, Meal, Scene};
use thefly_sim::screen::{GamePhase, Gameplay, LOADING_STEPS, MatchScreen};
use thefly_sim::world::camera::layer_images;
use thefly_sim::world::fly::{APPEAR_TICKS, Fly, Heading};
use thefly_sim::world::flying_goodies::{Flyer, MYSTERY_KIND};
use thefly_sim::world::midges::Midge;
use thefly_sim::world::{OBJECT_PARALLAX, W, World};

use super::View;
use crate::assets::{ARIAL_BLACK, JOINT};
use crate::gfx::{Gfx, WHITE, centred_x};
use crate::lerp::layer_src;

const SCREEN_W: f32 = 800.0;
const SCREEN_H: f32 = 600.0;
/// The table-object ring, trunc(4000 × 1.25) (fly-and-world §5).
const OBJECT_RING: f32 = W as f32 * OBJECT_PARALLAX as f32;

pub fn gameplay(g: &mut Gfx, v: &View, m: &MatchState, gp: &Gameplay) -> Result<(), Error> {
    let prev = match v.prev_screen() {
        Some(MatchScreen::Gameplay(p)) if !matches!(p.phase, GamePhase::Loading { .. }) => {
            Some(&p.world)
        }
        _ => None,
    };
    match gp.phase {
        GamePhase::Loading { step, .. } => loading(g, gp.loading_alt, step),
        GamePhase::Banner { count, .. } => {
            scene(g, v, m, &gp.world, prev)?;
            banner(g, m, count)
        }
        GamePhase::Playing => scene(g, v, m, &gp.world, prev),
    }
}

/// SR §4.1: the progress slice of `spriteload` after `step` of 9 steps. Up to step 4.5 it is a
/// slice of frame 2 over frame 1, then a slice of frame 3 over frame 2. Returns
/// `(base frame, slice frame, slice width)`.
pub fn loading_slice(step: u32, frame_w: f32) -> (u32, u32, f32) {
    let half = LOADING_STEPS as f32 / 2.0;
    let s = step.min(LOADING_STEPS) as f32;
    if s <= half {
        (0, 1, frame_w / half * s)
    } else {
        (1, 2, frame_w / half * (s - half))
    }
}

fn loading(g: &mut Gfx, alt: bool, step: u32) -> Result<(), Error> {
    const AT: (f32, f32) = (674.0, 497.0);
    g.spr(if alt { "LOADING2" } else { "LOADING" }, 0.0, 0.0, 0)?;
    let (w, h) = g.size("spriteload")?;
    let (base, slice, sw) = loading_slice(step, w);
    g.spr("spriteload", AT.0, AT.1, base)?;
    g.spr_part("spriteload", slice, (0.0, 0.0, sw, h), AT.0, AT.1)
}

/// Per scene: layer y positions (SR §4.2 table).
fn layer_ys(scene: Scene) -> [f32; 4] {
    match scene {
        Scene::Inside => [0.0, 205.0, 0.0, 525.0],
        Scene::Outside => [0.0, 280.0, 230.0, 525.0],
        Scene::Arena => [0.0, 280.0, 123.0, 525.0],
    }
}

/// gaps §4.3 step 6: [src, W) at x 0, then the start of the image after it; height cut at 600.
fn layer(g: &mut Gfx, name: &str, src: f32, y: f32) -> Result<(), Error> {
    let (w, h) = g.size(name)?;
    let h = h.min(SCREEN_H - y);
    let first = (w - src).min(SCREEN_W);
    g.spr_part(name, 0, (src, 0.0, first, h), 0.0, y)?;
    if first < SCREEN_W {
        g.spr_part(name, 0, (0.0, 0.0, SCREEN_W - first, h), first, y)?;
    }
    Ok(())
}

fn layers(
    g: &mut Gfx,
    v: &View,
    scene: Scene,
    w: &World,
    p: Option<&World>,
    which: &[usize],
) -> Result<(), Error> {
    let names = layer_images(scene);
    let ys = layer_ys(scene);
    for &i in which {
        let l = &w.camera.layers[i];
        let prev = p.map(|p| p.camera.layers[i].acc);
        let src = layer_src(v.l, prev, l.acc, l.factor, l.width);
        layer(g, names[i], src.floor(), ys[i])?;
    }
    Ok(())
}

/// Screen x of a position on the 1.25 object ring (traps, lift), from the main layer's scroll.
/// shortcut: fly-and-world §5 gives the ring but not the sim's screen mapping for traps and the
/// lift; this assumes they ride the object layer like the table objects until the sim exposes
/// their screen x.
fn ring_x(x: f32, cam: f32, w: f32) -> f32 {
    let sx = (x - cam).rem_euclid(OBJECT_RING);
    if sx > OBJECT_RING - w {
        sx - OBJECT_RING
    } else {
        sx
    }
}

fn object_cam(v: &View, w: &World, p: Option<&World>) -> f32 {
    let main = &w.camera.layers[2];
    let prev = p.map(|p| p.camera.layers[2].acc);
    layer_src(
        v.l,
        prev,
        main.acc,
        main.factor * OBJECT_PARALLAX,
        OBJECT_RING as i32,
    )
}

fn scene(g: &mut Gfx, v: &View, m: &MatchState, w: &World, p: Option<&World>) -> Result<(), Error> {
    let sc = m.scene();
    layers(g, v, sc, w, p, &[0, 1, 2])?;
    tint(g, m)?;
    midges(g, v, &w.midges.slots, p.map(|p| p.midges.slots.as_slice()))?;
    midges(
        g,
        v,
        &w.ambient_midges.slots,
        p.map(|p| p.ambient_midges.slots.as_slice()),
    )?;
    // SR §4.3 step 6: the table front goes under the objects and actors, as the captures show;
    // §4.2's "layer 3 last" is the coarse pass list
    layers(g, v, sc, w, p, &[3])?;
    let cam = object_cam(v, w, p);
    objects(g, v, w, p)?;
    lift(g, v, w, p, cam)?;
    traps_back(g, w, cam)?;
    fly(g, v, &w.fly, p.map(|p| &p.fly))?;
    boot(g, v, w, p)?;
    hud(g, v, m, w)?;
    flyers(g, v, w, p)?;
    swatter(g, v, w, p)?;
    traps_front(g, v, w, cam)?;
    v.fx.draw_particles(g, v.l)?;
    bubbles(g, w)
}

/// SR §4.3 step 2: `color_shade` cell 3 or 4 by meal, inside 51/102, outside 25/63. Lunch has
/// no tint.
fn tint(g: &mut Gfx, m: &MatchState) -> Result<(), Error> {
    let inside = m.scene() == Scene::Inside;
    let (cell, alpha) = match (m.progress.meal, inside) {
        (Some(Meal::Breakfast), true) => (2, 51.0),
        (Some(Meal::Dinner), true) => (3, 102.0),
        (Some(Meal::Breakfast), false) => (2, 25.0),
        (Some(Meal::Dinner), false) => (3, 63.0),
        _ => return Ok(()),
    };
    g.shade(cell, alpha)
}

fn midges(g: &mut Gfx, v: &View, cur: &[Midge], prev: Option<&[Midge]>) -> Result<(), Error> {
    const FRAME_SIZE: f32 = 27.0;
    for (i, md) in cur.iter().enumerate().filter(|(_, md)| md.active) {
        let pm = prev.and_then(|p| p.get(i)).filter(|p| p.active);
        let x = v.l.i(pm.map(|p| p.x), md.x);
        let y = v.l.i(pm.map(|p| p.y), md.y);
        let frame = (v.anim(50, 6) + i as u32) % 6;
        let scale = if md.size > 0 {
            md.size as f32 / FRAME_SIZE
        } else {
            1.0
        };
        g.spr_scaled("moucheron", x, y, frame, scale, 255.0)?;
    }
    Ok(())
}

fn objects(g: &mut Gfx, v: &View, w: &World, p: Option<&World>) -> Result<(), Error> {
    for (i, o) in w.objects.list.iter().enumerate().filter(|(_, o)| o.active) {
        let po = p
            .and_then(|p| p.objects.list.get(i))
            .filter(|po| po.sprite == o.sprite);
        let x = v.l.i(po.map(|p| p.screen_x), o.screen_x);
        let y = v.l.i(po.map(|p| p.y), o.y);
        if x > SCREEN_W || x + (o.w as f32) < 0.0 {
            continue;
        }
        g.spr(o.sprite, x, y, 0)?;
    }
    Ok(())
}

/// fly-and-world §5.6: the hook and its rope, plus the two idle anchors at y = 50 − h.
fn lift(g: &mut Gfx, v: &View, w: &World, p: Option<&World>, cam: f32) -> Result<(), Error> {
    let lf = &w.lift;
    let (hook_w, hook_h) = g.size("treuil")?;
    let (rope_w, _) = g.size("treuil_corde")?;
    let rope_dx = (hook_w - rope_w) / 2.0;
    for (i, &ax) in lf.anchors.iter().enumerate() {
        let x = ring_x(ax as f32, cam, hook_w);
        let active = i == lf.anchor;
        let y = if active {
            v.l.i(p.map(|p| p.lift.y), lf.y)
        } else {
            50.0 - hook_h
        };
        let rope_y = if active {
            v.l.i(p.map(|p| p.lift.rope_y), lf.rope_y)
        } else {
            y - 380.0
        };
        g.spr("treuil_corde", x + rope_dx, rope_y, 0)?;
        g.spr("treuil", x, y, 0)?;
    }
    Ok(())
}

fn traps_back(g: &mut Gfx, w: &World, cam: f32) -> Result<(), Error> {
    for f in &w.traps.flypapers {
        let name = if f.variant2 {
            "colle_mouche2"
        } else {
            "colle_mouche1"
        };
        let (fw, _) = g.size(name)?;
        g.spr(name, ring_x(f.x as f32, cam, fw), f.y as f32, 0)?;
    }
    for z in &w.traps.zappers {
        let (zw, _) = g.size("tue_mouche")?;
        g.spr("tue_mouche", ring_x(z.x as f32, cam, zw), z.y as f32, 0)?;
    }
    Ok(())
}

/// Fans, their wind and the zapper arcs go over the fly.
fn traps_front(g: &mut Gfx, v: &View, w: &World, cam: f32) -> Result<(), Error> {
    for f in &w.traps.fans {
        let name = if f.facing_right {
            "ventilo_droite"
        } else {
            "ventilo_gauche"
        };
        let (fw, _) = g.size(name)?;
        g.spr(name, ring_x(f.x as f32, cam, fw), f.y as f32, 0)?;
    }
    for z in w.traps.zappers.iter().filter(|z| z.arcing) {
        let (zw, zh) = g.size("tue_mouche")?;
        let (ew, eh) = g.size("eclaires")?;
        let x = ring_x(z.x as f32, cam, zw) + (zw - ew) / 2.0;
        g.spr("eclaires", x, z.y as f32 + (zh - eh) / 2.0, v.anim(50, 4))?;
    }
    Ok(())
}

fn heading_sheet(h: Heading) -> &'static str {
    match h {
        Heading::Face => "Face",
        Heading::E => "transi-est",
        Heading::W => "transi-ouest",
        Heading::N => "transi-nord",
        Heading::S => "transi-sud",
        Heading::NE => "transi-nord-est",
        Heading::SE => "transi-sud-est",
        Heading::NW => "transi-nord-ouest",
        Heading::SW => "transi-sud-ouest",
    }
}

const CRASH_FACES: [&str; 4] = [
    "crash-face0001",
    "crash-face0002",
    "crash-face0003",
    "crash-face0004",
];

/// fly-and-world §3: heading sheets, `Aspi` on a grab, a crash face while stunned. Dying and the
/// splat are drawn by the effects; the appear animation is the `onde_active` ring of gaps §2.3.
fn fly(g: &mut Gfx, v: &View, f: &Fly, p: Option<&Fly>) -> Result<(), Error> {
    if f.splatted || f.dying {
        return Ok(());
    }
    let x = v.l.i(p.map(|p| p.x), f.x);
    let y = v.l.i(p.map(|p| p.y), f.y);
    let frame = |i: i32| u32::try_from(i).unwrap_or(0);
    if f.appearing {
        let left = 1.0 - f.appear_ticks.min(APPEAR_TICKS) as f32 / APPEAR_TICKS as f32;
        let (cx, cy) = (x + f.size.w as f32 / 2.0, y + f.size.h as f32 / 2.0);
        g.spr_centred(
            "onde_active",
            cx,
            cy,
            0,
            0.1 + 2.9 * left,
            255.0 * (1.0 - left),
        )?;
        return g.spr_ex("Face", x, y, 0, 255.0 * (1.0 - left), false);
    }
    if f.stunned {
        let name = CRASH_FACES[frame(f.hit_frame).min(3) as usize];
        return g.spr(name, x, y, 0);
    }
    if f.grab_lock {
        return g.spr("Aspi", x, y, frame(f.grab_frame));
    }
    g.spr(heading_sheet(f.heading), x, y, frame(f.heading_frame))
}

fn boot(g: &mut Gfx, v: &View, w: &World, p: Option<&World>) -> Result<(), Error> {
    if !w.boot.active {
        return Ok(());
    }
    let pb = p.map(|p| &p.boot).filter(|b| b.active);
    // `x` is on the 5000 ring; `rect()` shifts it across the ring end (FW §4.4 step 6)
    let x = v.l.i(pb.map(|b| b.rect().x), w.boot.rect().x);
    let y = v.l.i(pb.map(|b| b.y), w.boot.y);
    g.spr("i_botte", x, y, 0)
}

pub(super) const SWATTERS: [&str; 5] = [
    "tapette-1",
    "tapette-3",
    "tapette-5",
    "tapette-7",
    "tapette-8",
];
/// `tapette-1`'s height; the hand sits at farmer y + this (ai-and-rules §1.1, yTop = 529 − 178).
const SWATTER_BASE_H: f32 = 178.0;

/// The swatter, frame by swing frame 0..4. shortcut: SR does not give the draw anchor; the frames
/// are bottom-aligned on the hand (farmer y + 178), and the sprite is drawn on both sides of the
/// 4000-px ring wrap.
fn swatter(g: &mut Gfx, v: &View, w: &World, p: Option<&World>) -> Result<(), Error> {
    let fm = &w.farmer;
    let name = SWATTERS[usize::try_from(fm.swing.frame).unwrap_or(0).min(4)];
    let (sw, sh) = g.size(name)?;
    let ring = W as f64;
    let x =
        v.l.ring(p.map(|p| f64::from(p.farmer.x)), f64::from(fm.x), ring);
    let y = v.l.i(p.map(|p| p.farmer.y), fm.y) + SWATTER_BASE_H - sh;
    for sx in [x, x - W as f32] {
        if sx < SCREEN_W && sx + sw > 0.0 {
            g.spr(name, sx, y, 0)?;
        }
    }
    Ok(())
}

/// One flying goodie: `Goodies` at its sheet frame, the mystery sheet for kind 15, with
/// `ailesgoodies` wings. shortcut: the wings' offset is not in the docs; centred above the icon.
fn flyer(g: &mut Gfx, v: &View, f: &Flyer, p: Option<&Flyer>, farmer: bool) -> Result<(), Error> {
    if !f.active {
        return Ok(());
    }
    let pf = p.filter(|p| p.active);
    let x = v.l.i(pf.map(|p| p.x), f.x);
    let y = v.l.i(pf.map(|p| p.y), f.y);
    let (ww, wh) = g.size("ailesgoodies")?;
    g.spr(
        "ailesgoodies",
        x + (f.w as f32 - ww) / 2.0,
        y - wh / 2.0,
        v.anim(50, 3),
    )?;
    if f.kind == MYSTERY_KIND {
        return g.spr("goodiesmystere", x, y, u32::from(farmer));
    }
    g.spr("Goodies", x, y, u32::try_from(f.frame).unwrap_or(0))
}

fn flyers(g: &mut Gfx, v: &View, w: &World, p: Option<&World>) -> Result<(), Error> {
    let fg = &w.flying_goodies;
    let pg = p.map(|p| &p.flying_goodies);
    flyer(g, v, &fg.fly, pg.map(|p| &p.fly), false)?;
    flyer(g, v, &fg.farmer, pg.map(|p| &p.farmer), true)
}

const HAND_RED: Color = Color::RGB(255, 0, 0);
const HAND_LEN: f32 = 15.0;
/// Non-selected goodie icons are drawn at 0.3 (SR §4.4).
const DIM_ALPHA: f32 = 0.3 * 255.0;

/// The clock hand angle after `elapsed_s` of a `period_s` period. shortcut: SR §4.4 leaves open
/// whether the quarter steps make one full turn per period; this turns once, clockwise from 12.
pub fn hand_angle(elapsed_s: u64, period_s: u64) -> f32 {
    let t = elapsed_s.min(period_s) as f32 / period_s.max(1) as f32;
    FRAC_PI_2 - TAU * t
}

fn hud(g: &mut Gfx, v: &View, m: &MatchState, w: &World) -> Result<(), Error> {
    g.spr("horloge", 3.0, 3.0, 0)?;
    let a = hand_angle(w.clock.elapsed_s(v.now), w.clock.limit_s);
    let c = (3.0 + 25.0, 3.0 + 26.0);
    g.line(
        c,
        (c.0 + HAND_LEN * a.cos(), c.1 - HAND_LEN * a.sin()),
        3,
        HAND_RED,
    )?;

    let fly = &m.economy.fly;
    if w.goodies.fly_selector > 0 {
        let (fx, fy) = (w.fly.x as f32, w.fly.y as f32);
        let counts = [fly.turbo, fly.superfly, fly.midges];
        let at = |i: usize| (fx + 12.0 + 36.0 * i as f32, fy - 10.0);
        icons(g, &counts, 2, w.goodies.fly_selector, at)?;
    }
    let farmer = &m.economy.farmer;
    if w.goodies.farmer_selector > 0 {
        let (pw, _) = g.size("plaque_tap")?;
        g.spr("plaque_tap", (SCREEN_W - pw) / 2.0, 350.0, 0)?;
        let counts = [farmer.triple, farmer.nuclear, farmer.boot];
        let at = |i: usize| (349.0 + 36.0 * i as f32, 568.0);
        icons(g, &counts, 7, w.goodies.farmer_selector, at)?;
    }
    Ok(())
}

/// SR §4.4 icon states: the selected one full, others at 0.3; a 0 count gets the cross, others
/// their count in Arial Black 22.
fn icons(
    g: &mut Gfx,
    counts: &[i32; 3],
    first_frame: u32,
    selector: u8,
    at: impl Fn(usize) -> (f32, f32),
) -> Result<(), Error> {
    for (i, &n) in counts.iter().enumerate() {
        let (x, y) = at(i);
        let selected = usize::from(selector) == i + 1;
        let alpha = if selected { 255.0 } else { DIM_ALPHA };
        g.spr_ex("Goodies", x, y, first_frame + i as u32, alpha, false)?;
        if n == 0 {
            g.spr("croix_goodies", x + 3.0, y + 3.0, 0)?;
        } else {
            g.text_fit(
                ARIAL_BLACK,
                22,
                &n.to_string(),
                (x + 4.0, y + 24.0),
                30.0,
                WHITE,
            )?;
        }
    }
    Ok(())
}

/// Tutorial bubbles (open item O12: placement not decoded). shortcut: the fly bubble at the top
/// right and the farmer bubble at the bottom left.
fn bubbles(g: &mut Gfx, w: &World) -> Result<(), Error> {
    let Some(b) = &w.ambient.tutorial else {
        return Ok(());
    };
    let frame = u32::try_from(b.frame).unwrap_or(0);
    if b.farmer_side {
        g.spr("tutoBulFarmer", 20.0, 400.0, frame)
    } else {
        g.spr("tutoBulFly", 546.0, 20.0, frame)
    }
}

const DIGIT_OUTLINE: Color = Color::RGB(255, 255, 0);
const DIGIT_FILL: Color = Color::RGB(5, 55, 255);
const CAPTION_OUTLINE: Color = Color::RGB(130, 130, 130);

fn meal_name(m: Option<Meal>) -> &'static str {
    match m {
        Some(Meal::Breakfast) | None => "Breakfast",
        Some(Meal::Lunch) => "Lunch",
        Some(Meal::Dinner) => "Dinner",
    }
}

/// SR §4.5 countdown and caption. shortcut: the caption's italic style is not applied (the
/// platform text has no style switch).
fn banner(g: &mut Gfx, m: &MatchState, count: i32) -> Result<(), Error> {
    const DIGIT: f32 = 200.0;
    const CAPTION: f32 = 40.0;
    let s = if count >= 1 {
        count.to_string()
    } else {
        "Go!".to_owned()
    };
    let len = s.chars().count() as f32;
    let y = if count >= 1 {
        (SCREEN_H - len * DIGIT / 0.9) / 2.0
    } else {
        (SCREEN_H - len * DIGIT / 2.0) / 2.0 + 25.0
    };
    let x = centred_x(s.chars().count(), DIGIT);
    g.text_outlined(JOINT, 200, &s, (x, y), (DIGIT_FILL, DIGIT_OUTLINE))?;
    let place = if m.scene() == Scene::Inside {
        "Inside..."
    } else {
        "Outside..."
    };
    let caption = format!(
        "{} {} Day {}",
        meal_name(m.progress.meal),
        place,
        m.progress.day
    );
    let cx = centred_x(caption.chars().count(), CAPTION) + 20.0;
    g.text_outlined(JOINT, 40, &caption, (cx, 380.0), (WHITE, CAPTION_OUTLINE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_fills_across_two_frames() {
        assert_eq!(loading_slice(0, 65.0), (0, 1, 0.0));
        let (b, s, w) = loading_slice(4, 65.0);
        assert_eq!((b, s), (0, 1));
        assert!((w - 65.0 / 4.5 * 4.0).abs() < 1e-3);
        let (b, s, w) = loading_slice(9, 65.0);
        assert_eq!((b, s), (1, 2));
        assert!((w - 65.0).abs() < 1e-3);
    }

    #[test]
    fn clock_hand_starts_at_twelve_and_turns_clockwise() {
        assert_eq!(hand_angle(0, 120), FRAC_PI_2);
        assert!((hand_angle(30, 120) - 0.0).abs() < 1e-6);
        assert!((hand_angle(500, 120) - (FRAC_PI_2 - TAU)).abs() < 1e-6);
    }

    #[test]
    fn ring_x_keeps_things_just_left_of_the_screen_negative() {
        assert_eq!(ring_x(100.0, 0.0, 50.0), 100.0);
        assert_eq!(ring_x(4990.0, 0.0, 50.0), -10.0);
        assert_eq!(ring_x(10.0, 4990.0, 50.0), 20.0);
    }

    #[test]
    fn scene_layer_order_matches_the_sim_names() {
        assert_eq!(layer_images(Scene::Inside)[3], "Table");
        assert_eq!(layer_ys(Scene::Arena)[2], 123.0);
    }
}
