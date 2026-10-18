//! W2. Trap entities on the table: flypapers, fans and zappers (game-spec §4.6, fly-and-world
//! §7.4). Built each period from the shop's [`TrapLayout`]. Ticks step 13.

use super::collision::{Rect, Shape, fly_outline, outlines_overlap};
use super::fly::RawDamage;
use super::lift::SHOP_SCALE;
use super::{OBJECT_PARALLAX, W, World};
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::data::Size;
use crate::event::{Event, Pan, PlayParams, SoundOp, SoundSlot};
use crate::match_state::TrapLayout;

pub const MAX_FLYPAPERS: usize = 7;
pub const MAX_FANS: usize = 5;
pub const MAX_ZAPPERS: usize = 2;
/// Fans hit the fly at most once per this many ms.
pub const FAN_REHIT_MS: u64 = 100;
/// Flypaper divides both velocity components by this each tick of overlap.
pub const FLYPAPER_DRAG: f64 = 1.4;
/// Zapper arc: upward push and damage per tick.
pub const ARC_PUSH: i32 = 180;
pub const ARC_DAMAGE: i32 = 1;
/// The 1.25 layer's screen wrap, trunc(W × 1.25).
pub const SCREEN_WRAP: i32 = 5000;
/// Each flypaper built draws R(10); above 4 it uses this sprite (gaps §8.2).
pub const FLYPAPER_VARIANT_ROLL: i32 = 10;
pub const FLYPAPER_VARIANT_ABOVE: i32 = 4;
pub const FLYPAPER_VARIANT_SPRITE: &str = "colle_mouche2";
/// +0x91c. Open item O9: not in the docs; a guess of the order of the fly's speeds.
pub const FAN_STRENGTH: i32 = 400;
/// The fan sheet's frame count: 666 / 3 = 222, the 223 stored width (GS §9.3, AR §4.1).
pub const FAN_FRAMES: i32 = 3;
/// A fan hit: vy −= strength + 100; damage R(20) + 40, or R(10) + 20 with SuperFly; slot 45 at
/// 20000 + R(6000) Hz (FW §7.4, game-spec §4.6).
pub const FAN_LIFT: i32 = 100;
pub const FAN_DAMAGE_RAND: i32 = 20;
pub const FAN_DAMAGE_BASE: i32 = 40;
pub const FAN_DAMAGE_SUPERFLY_RAND: i32 = 10;
pub const FAN_DAMAGE_SUPERFLY_BASE: i32 = 20;
pub const FAN_HIT_HZ: i32 = 20_000;
pub const FAN_HIT_HZ_RAND: i32 = 6000;
/// The fan loop at −100 while blowing; fans always blow (O9), so the −400 idle level is unused.
pub const FAN_LOUD_CB: i32 = -100;
/// The zapper loop: −500 idle (−450 for a single zapper), −200 while arcing.
pub const ZAPPER_IDLE_CB: i32 = -500;
pub const ZAPPER_SINGLE_CB: i32 = -450;
pub const ZAPPER_ARC_CB: i32 = -200;
/// A fly whose top is within (zapper height − 80) of the zapper's top is grilled.
pub const GRILL_MARGIN: i32 = 80;
/// Open item O9: the arc schedule, idle then arcing.
pub const ARC_IDLE_MS: u64 = 3000;
pub const ARC_MS: u64 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapKind {
    Flypaper,
    FanLeft,
    FanRight,
    Zapper,
}

/// A row of the trap table (ai-and-rules §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrapSpec {
    pub kind: TrapKind,
    pub sprite: &'static str,
    pub cost: i32,
    /// Cap per type; the two fans share theirs.
    pub cap: usize,
    /// Width for the placement spacing, shop px.
    pub width: i32,
    pub stored_y: i32,
}

pub const TRAP_TABLE: [TrapSpec; 4] = [
    TrapSpec {
        kind: TrapKind::Flypaper,
        sprite: "colle_mouche1",
        cost: 1,
        cap: 7,
        width: 8,
        stored_y: 0,
    },
    TrapSpec {
        kind: TrapKind::FanLeft,
        sprite: "ventilo_gauche",
        cost: 2,
        cap: 5,
        width: 34,
        stored_y: 600,
    },
    TrapSpec {
        kind: TrapKind::FanRight,
        sprite: "ventilo_droite",
        cost: 2,
        cap: 5,
        width: 34,
        stored_y: 600,
    },
    TrapSpec {
        kind: TrapKind::Zapper,
        sprite: "tue_mouche",
        cost: 4,
        cap: 2,
        width: 97,
        stored_y: 0,
    },
];

impl TrapKind {
    pub fn spec(self) -> &'static TrapSpec {
        &TRAP_TABLE[self as usize]
    }
}

impl TrapSpec {
    /// The spacing width in stored units, trunc(width / 0.152): 52, 223 and 638, which match the
    /// sprite frame widths (53, 666 / 3, 635).
    pub fn stored_width(&self) -> i32 {
        (f64::from(self.width) / SHOP_SCALE) as i32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flypaper {
    pub variant2: bool,
    /// Layer x on the 1.25 layer, and y.
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fan {
    pub facing_right: bool,
    pub x: i32,
    pub y: i32,
    /// +0x91c. Open item O9: its value.
    pub strength: i32,
    /// Loud loop (−100) while blowing, else −400. Open item O9: the blow schedule.
    pub blowing: bool,
    pub rehit: Timer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zapper {
    pub x: i32,
    pub y: i32,
    /// Arc active (loop −200, slot 39). Open item O9: the arc schedule (`FUN_00428b47`).
    pub arcing: bool,
    pub arc_timer: Timer,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Traps {
    pub flypapers: Vec<Flypaper>,
    pub fans: Vec<Fan>,
    pub zappers: Vec<Zapper>,
}

impl Traps {
    /// The period's traps from the shop slots (stored x is trunc(shop px / 0.152), gaps §11.4).
    /// Caps 7/5/2 skip the rest (gaps §8.2); each flypaper built under its cap draws `R(10)` and
    /// uses the `colle_mouche2` sprite when the draw is above 4 (`variant2`), in slot order.
    /// Starts the fan (44) and zapper (40) loops.
    pub fn from_layout(layout: &TrapLayout, cx: &mut Ctx) -> Self {
        let mut t = Self::default();
        for slot in &layout.slots {
            let x = (f64::from(slot.x) / OBJECT_PARALLAX) as i32;
            let spec = slot.kind.spec();
            let h = cx.data.size(spec.sprite).h;
            let y = if spec.stored_y > 0 {
                spec.stored_y - h
            } else {
                spec.stored_y
            };
            match slot.kind {
                TrapKind::Flypaper if t.flypapers.len() < MAX_FLYPAPERS => {
                    let variant2 = cx.rng.r(FLYPAPER_VARIANT_ROLL) > FLYPAPER_VARIANT_ABOVE;
                    t.flypapers.push(Flypaper { variant2, x, y });
                }
                TrapKind::FanLeft | TrapKind::FanRight if t.fans.len() < MAX_FANS => {
                    t.fans.push(Fan {
                        facing_right: slot.kind == TrapKind::FanRight,
                        x,
                        y,
                        strength: FAN_STRENGTH,
                        blowing: true,
                        rehit: Timer::UNARMED,
                    });
                }
                TrapKind::Zapper if t.zappers.len() < MAX_ZAPPERS => {
                    t.zappers.push(Zapper {
                        x,
                        y,
                        arcing: false,
                        arc_timer: Timer::armed(cx.now, ARC_IDLE_MS),
                    });
                }
                _ => {}
            }
        }
        t.start_loops(cx);
        t
    }

    fn start_loops(&self, cx: &mut Ctx) {
        if !self.fans.is_empty() {
            let params = PlayParams {
                volume_cb: FAN_LOUD_CB,
                ..PlayParams::default()
            };
            cx.sound(SoundSlot::FAN, SoundOp::Loop(params));
        }
        if !self.zappers.is_empty() {
            let params = PlayParams {
                volume_cb: self.zapper_idle_cb(),
                ..PlayParams::default()
            };
            cx.sound(SoundSlot::ZAPPER_HUM, SoundOp::Loop(params));
        }
    }

    /// −500 idle, −450 when there is a single (positional) zapper (game-spec §4.6).
    fn zapper_idle_cb(&self) -> i32 {
        if self.zappers.len() == 1 {
            ZAPPER_SINGLE_CB
        } else {
            ZAPPER_IDLE_CB
        }
    }

    /// A zapper arc touches (x, y): the point is in an arcing zapper's column (the midge test
    /// `FUN_004278f3` at the midge's (x, y + 7)).
    pub fn arc_touches(&self, x: i32, y: i32, cx: &Ctx) -> bool {
        let w = TrapKind::Zapper.spec_size(cx).w;
        self.zappers
            .iter()
            .any(|z| z.arcing && in_span(x, visible_x(z.x, w), w) && y >= z.y)
    }

    /// (x, y) is inside a fan's frame, where its wind blows (`FUN_00427885`).
    pub fn fan_touches(&self, x: i32, y: i32, cx: &Ctx) -> bool {
        let size = fan_frame(cx);
        self.fans
            .iter()
            .any(|f| in_span(x, visible_x(f.x, size.w), size.w) && in_span(y, f.y, size.h))
    }

    /// `FUN_00427bb4`: may an object occupy [x, x + w] (object placement units, the 1.25 layer's
    /// screen x, FW §5.2 step 3)? Used by placement.
    // shortcut: O22, the exact test is not decoded. Only fans share the table with the objects
    // (flypapers and zappers hang from the top, stored y 0), so only a fan's span blocks, inclusive
    // like the object-object test. Revisit when FUN_00427bb4 is read.
    // A span past the 5000 ring end also blocks its wrapped part at the left (FW §4.4 step 6).
    pub fn span_free(&self, x: i32, w: i32) -> bool {
        let fan_w = TrapKind::FanLeft.spec().stored_width();
        let clear = |fx: i32| x + w < fx || fx + fan_w < x;
        self.fans.iter().all(|f| {
            let fx = screen_x(f.x);
            clear(fx) && clear(fx - SCREEN_WRAP)
        })
    }
}

/// A layer x on the 1.25 layer in placement units (FW §4.1 screen x before the wrap).
fn placement_x(layer_x: i32) -> i32 {
    (f64::from(layer_x) * OBJECT_PARALLAX) as i32
}

/// The wrapped screen x of a 1.25-layer x (FW §4.1).
fn screen_x(layer_x: i32) -> i32 {
    placement_x(layer_x).rem_euclid(SCREEN_WRAP)
}

/// The drawn x of a trap w wide: its screen x, minus the wrap width when it straddles the ring end
/// (FW §4.4 step 6), so a trap at 4990 reaches the left edge of the view.
fn visible_x(layer_x: i32, w: i32) -> i32 {
    let x = screen_x(layer_x);
    if x >= SCREEN_WRAP - w {
        x - SCREEN_WRAP
    } else {
        x
    }
}

fn in_span(v: i32, start: i32, len: i32) -> bool {
    (start..=start + len).contains(&v)
}

/// layerX += trunc(−delta), wrapped at W (FW §4.1, time-based integrator with v = 0).
fn scroll(layer_x: &mut i32, delta: f64) {
    *layer_x = (*layer_x + (-delta) as i32).rem_euclid(W);
}

impl TrapKind {
    fn spec_size(self, cx: &Ctx) -> Size {
        cx.data.size(self.spec().sprite)
    }
}

/// One frame of the fan sheet (`ventilo_*` holds FAN_FRAMES frames side by side).
fn fan_frame(cx: &Ctx) -> Size {
    let sheet = TrapKind::FanLeft.spec_size(cx);
    Size {
        w: sheet.w / FAN_FRAMES,
        h: sheet.h,
    }
}

impl World {
    /// Step 13 (`FUN_00427270`): flypaper drag, fan push and damage, zapper grill and arc, the
    /// loops (44, 40) and their positional single-trap variants.
    pub fn traps_step(&mut self, cx: &mut Ctx) {
        let delta = self.camera.delta;
        let t = &mut self.traps;
        for f in &mut t.flypapers {
            scroll(&mut f.x, delta);
        }
        for f in &mut t.fans {
            scroll(&mut f.x, delta);
        }
        for z in &mut t.zappers {
            scroll(&mut z.x, delta);
        }
        self.zapper_schedule(cx);
        self.single_trap_pan(delta, cx);
        if !self.fly.alive() {
            return;
        }
        self.flypaper_drag(cx);
        self.fan_hits(cx);
        self.zapper_hits(cx);
    }

    /// FW §7.4 flypaper: both velocity components / 1.4 per overlapping tick, no sound.
    fn flypaper_drag(&mut self, cx: &Ctx) {
        let fly = self.fly_rect();
        let touching = self.traps.flypapers.iter().any(|p| {
            let sprite = if p.variant2 {
                FLYPAPER_VARIANT_SPRITE
            } else {
                TrapKind::Flypaper.spec().sprite
            };
            let size = cx.data.size(sprite);
            Rect::new(visible_x(p.x, size.w), p.y, size.w, size.h).overlaps(&fly)
        });
        if touching {
            self.fly.vx = (f64::from(self.fly.vx) / FLYPAPER_DRAG) as i32;
            self.fly.vy = (f64::from(self.fly.vy) / FLYPAPER_DRAG) as i32;
        }
    }

    /// FW §7.4 fan: the blades (`palesventi_*`) knock the fly at most once per 100 ms.
    fn fan_hits(&mut self, cx: &mut Ctx) {
        let fly_outline = fly_outline();
        let fly = Shape {
            x: self.fly.x,
            y: self.fly.y,
            size: self.fly.size,
            outline: &fly_outline,
        };
        let size = fan_frame(cx);
        let mut hit = None;
        for (i, f) in self.traps.fans.iter().enumerate() {
            let rehit_running = f.rehit.is_armed() && !f.rehit.expired(cx.now);
            let blades = cx.data.outline(blades_outline(f.facing_right));
            let shape = Shape {
                x: screen_x(f.x),
                y: f.y,
                size,
                outline: blades,
            };
            if !rehit_running && outlines_overlap(&fly, &shape, SCREEN_WRAP) {
                hit = Some(i);
                break;
            }
        }
        let Some(i) = hit else {
            return;
        };
        let f = &mut self.traps.fans[i];
        f.rehit.arm(cx.now, FAN_REHIT_MS);
        let dir = if f.facing_right { 1 } else { -1 };
        self.fly.vx += dir * f.strength;
        self.fly.vy -= f.strength + FAN_LIFT;
        let damage = if self.fly.superfly {
            cx.rng.r(FAN_DAMAGE_SUPERFLY_RAND) + FAN_DAMAGE_SUPERFLY_BASE
        } else {
            cx.rng.r(FAN_DAMAGE_RAND) + FAN_DAMAGE_BASE
        };
        let freq = FAN_HIT_HZ + cx.rng.r(FAN_HIT_HZ_RAND);
        let params = PlayParams {
            freq_hz: u32::try_from(freq).ok(),
            ..PlayParams::default()
        };
        cx.sound(SoundSlot::FAN_HIT, SoundOp::Play(params));
        self.fly.add_raw_damage(damage, RawDamage::Fan { dir }, cx);
    }

    /// FW §7.4 zapper: grill near the top of the column, else an arc pushes up and adds 1.
    // shortcut: O9, the arc reach is not documented; the port lets an arc touch the whole column
    // below the zapper. Revisit with FUN_00428b47.
    fn zapper_hits(&mut self, cx: &mut Ctx) {
        let size = TrapKind::Zapper.spec_size(cx);
        let (fx, _) = self.fly.centre();
        let column = self
            .traps
            .zappers
            .iter()
            .find(|z| in_span(fx, visible_x(z.x, size.w), size.w));
        let Some(z) = column else {
            return;
        };
        if self.fly.y - z.y <= size.h - GRILL_MARGIN {
            self.fly.grill(cx);
            return;
        }
        if z.arcing {
            self.fly.vy -= ARC_PUSH;
            self.fly
                .add_raw_damage(ARC_DAMAGE, RawDamage::ZapperArc, cx);
        }
    }

    /// The arc schedule: idle ARC_IDLE_MS, arc ARC_MS (slot 39, louder hum).
    // shortcut: O9, FUN_00428b47's schedule is not decoded; a fixed idle/arc period with no rand
    // keeps the RNG stream untouched. Revisit when it is read.
    fn zapper_schedule(&mut self, cx: &mut Ctx) {
        let idle_cb = self.traps.zapper_idle_cb();
        for z in &mut self.traps.zappers {
            if !z.arc_timer.expired(cx.now) {
                continue;
            }
            z.arcing = !z.arcing;
            let (ms, cb) = if z.arcing {
                (ARC_MS, ZAPPER_ARC_CB)
            } else {
                (ARC_IDLE_MS, idle_cb)
            };
            z.arc_timer.arm(cx.now, ms);
            cx.sound(SoundSlot::ZAPPER_HUM, SoundOp::SetVolume(cb));
            if z.arcing {
                cx.emit(Event::play(SoundSlot::ZAPPER_ARC));
            }
        }
    }

    /// A single fan or zapper plays its loop positionally (game-spec §4.6).
    fn single_trap_pan(&self, delta: f64, cx: &mut Ctx) {
        if delta == 0.0 {
            return;
        }
        if let [f] = self.traps.fans.as_slice() {
            let w = fan_frame(cx).w;
            let pan = Pan::ScreenX {
                x: visible_x(f.x, w),
                w,
            };
            cx.sound(SoundSlot::FAN, SoundOp::Move(pan));
        }
        if let [z] = self.traps.zappers.as_slice() {
            let w = TrapKind::Zapper.spec_size(cx).w;
            let pan = Pan::ScreenX {
                x: visible_x(z.x, w),
                w,
            };
            cx.sound(SoundSlot::ZAPPER_HUM, SoundOp::Move(pan));
        }
    }

    fn fly_rect(&self) -> Rect {
        Rect::new(self.fly.x, self.fly.y, self.fly.size.w, self.fly.size.h)
    }
}

fn blades_outline(facing_right: bool) -> &'static str {
    if facing_right {
        "palesventi_dt"
    } else {
        "palesventi_gc"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fan_at(x: i32) -> Fan {
        Fan {
            facing_right: false,
            x,
            y: 600,
            strength: 0,
            blowing: false,
            rehit: Timer::UNARMED,
        }
    }

    #[test]
    fn stored_widths_scale_the_shop_widths() {
        let w = TRAP_TABLE.map(|t| t.stored_width());
        assert_eq!(w, [52, 223, 223, 638]);
    }

    /// A fan at layer x 800 sits at placement x 1000 and spans [1000, 1223].
    #[test]
    fn only_a_fan_span_blocks_an_object() {
        let traps = Traps {
            fans: vec![fan_at(800)],
            flypapers: vec![Flypaper {
                variant2: false,
                x: 2000,
                y: 0,
            }],
            ..Traps::default()
        };
        assert!(traps.span_free(899, 100) && traps.span_free(1224, 50));
        assert!(!traps.span_free(900, 100) && !traps.span_free(1223, 50));
        assert!(traps.span_free(2500, 100), "flypapers hang above the table");
        assert!(Traps::default().span_free(0, 5000));
    }

    /// A fan at layer 3920 (placement 4900) spans 4900..5123, which wraps onto 0..123 (FW §4.4).
    #[test]
    fn a_fan_span_across_the_ring_end_blocks_the_left_edge() {
        let traps = Traps {
            fans: vec![fan_at(3920)],
            ..Traps::default()
        };
        assert!(!traps.span_free(50, 100) && !traps.span_free(4850, 100));
        assert!(traps.span_free(124, 100));
    }
}

#[cfg(test)]
mod layout_tests {
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::match_state::{MatchConfig, MatchState, Players, TrapSlot};

    fn slot(kind: TrapKind, x: i32) -> TrapSlot {
        TrapSlot {
            kind,
            x,
            y: kind.spec().stored_y,
        }
    }

    /// gaps §8.2 on seed 1: R(10) per flypaper built under the cap, in slot order: 41 → 1,
    /// 18467 → 7, 6334 → 4, 26500 → 0, 19169 → 9, 15724 → 4, 11478 → 8; the eighth flypaper,
    /// the sixth fan and the third zapper are skipped and draw nothing.
    #[test]
    fn building_draws_one_variant_roll_per_flypaper_under_the_cap() {
        let mut slots = vec![slot(TrapKind::Zapper, 4000)];
        slots.extend((0..8).map(|i| slot(TrapKind::Flypaper, i * 100)));
        slots.extend((0..6).map(|i| slot(TrapKind::FanRight, 1000 + i * 250)));
        slots.extend((0..2).map(|i| slot(TrapKind::Zapper, 2500 + i * 700)));
        let mut rig = Rig::new(1);
        let traps = Traps::from_layout(&TrapLayout { slots }, &mut rig.cx(0));
        let variants: Vec<bool> = traps.flypapers.iter().map(|p| p.variant2).collect();
        assert_eq!(variants, [false, true, false, false, true, false, true]);
        assert_eq!((traps.fans.len(), traps.zappers.len()), (5, 2));
        assert_eq!(rig.rng.rand(), 29358);
        // Stored 1000 is layer x 800; fans stand on the floor (600 − 300).
        assert_eq!((traps.fans[0].x, traps.fans[0].y), (800, 300));
        assert!(traps.fans[0].facing_right && traps.fans[0].blowing);
    }

    fn world_with(slots: Vec<TrapSlot>, rig: &mut Rig) -> World {
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let m = MatchState::new(config, false, &mut rig.cx(0));
        let mut w = World::bare(&m, &mut rig.cx(0));
        w.traps = Traps::from_layout(&TrapLayout { slots }, &mut rig.cx(0));
        w
    }

    /// A flypaper at stored 500 (screen 500, 53 × 300 from y 0) divides v by 1.4 per tick.
    #[test]
    fn a_flypaper_divides_the_fly_speed_by_1_4() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Flypaper, 500)], &mut rig);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (480, 100, 300, -140);
        w.traps_step(&mut rig.at(1));
        assert_eq!((w.fly.vx, w.fly.vy), (214, -100));
        w.fly.x = 700;
        w.traps_step(&mut rig.at(2));
        assert_eq!((w.fly.vx, w.fly.vy), (214, -100));
    }

    /// The blades knock the fly at most once per 100 ms (tick 7 is 107 ms after tick 0).
    #[test]
    fn a_fan_hits_at_most_once_per_100_ms() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::FanLeft, 500)], &mut rig);
        let hits = |rig: &Rig| {
            rig.events
                .iter()
                .filter(|e| matches!(e, Event::Sound { slot, .. } if *slot == SoundSlot::FAN_HIT))
                .count()
        };
        for tick in 0..=7 {
            (w.fly.x, w.fly.y, w.fly.damage_total) = (500, 350, 0);
            w.traps_step(&mut rig.at(tick));
        }
        assert_eq!(hits(&rig), 2);
        // One frame is 222 wide: a fly 300 px right of the fan's x is clear of the blades.
        w.fly.x = 800;
        w.traps_step(&mut rig.at(100));
        assert_eq!(hits(&rig), 2);
    }

    /// A fly at the top of a zapper's column is grilled (slot 43, kind 13).
    #[test]
    fn a_zapper_grills_a_fly_at_the_top_of_its_column() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        (w.fly.x, w.fly.y) = (1000, 40);
        w.traps_step(&mut rig.at(1));
        assert!(w.fly.splatted);
        assert!(rig.events.contains(&Event::play(SoundSlot::ZAPPER_GRILL)));
    }

    /// FW §4.4 step 6: a trap whose screen x is within its width of the 5000 ring end is drawn
    /// at x − 5000, so a flypaper at stored 4990 (layer 3992, screen 4990) covers x 0..43.
    #[test]
    fn a_flypaper_at_the_ring_end_drags_a_fly_at_the_left_edge() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Flypaper, 4990)], &mut rig);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (0, 100, 300, -140);
        w.traps_step(&mut rig.at(1));
        assert_eq!((w.fly.vx, w.fly.vy), (214, -100));
    }

    /// A fan at stored 4900 (screen 4900, frame 222 wide) blows at x −100..122 (midge test).
    #[test]
    fn a_fan_at_the_ring_end_touches_a_point_at_the_left_edge() {
        let mut rig = Rig::new(1);
        let w = world_with(vec![slot(TrapKind::FanLeft, 4900)], &mut rig);
        let y = w.traps.fans[0].y + 10;
        assert!(w.traps.fan_touches(5, y, &rig.cx(0)));
        assert!(!w.traps.fan_touches(200, y, &rig.cx(0)));
    }

    /// A zapper at stored 4995 has its column across the ring end: a fly centred at x 20 is in it.
    #[test]
    fn a_zapper_at_the_ring_end_grills_a_fly_at_the_left_edge() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 4995)], &mut rig);
        (w.fly.x, w.fly.y) = (20 - w.fly.size.w / 2, 40);
        w.traps_step(&mut rig.at(1));
        assert!(w.fly.splatted);
    }
}

#[cfg(test)]
mod unit_tests {
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::match_state::{MatchConfig, MatchState, Players, TrapSlot};

    const STORED: i32 = 1618;
    const SEEDS: u32 = 20;

    fn world_with_fan(seed: u32) -> World {
        let mut rig = Rig::new(seed);
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = MatchState::new(config, false, &mut rig.cx(0));
        m.traps.slots = vec![TrapSlot {
            kind: TrapKind::FanLeft,
            x: STORED,
            y: TrapKind::FanLeft.spec().stored_y,
        }];
        m.winch = [STORED; 3];
        m.advance(&mut rig.cx(0));
        World::new(&mut m, &mut rig.cx(0))
    }

    /// AR §4.3, FW §5.6: trap and winch x share the stored unit, the 1.25 layer's screen ring
    /// (0..5000). A fan and a winch at the same stored x meet on screen (±1 px for the trunc
    /// through the layer x), and so does the fan's stand.
    #[test]
    fn a_fan_and_a_winch_at_the_same_stored_x_share_a_screen_x() {
        let w = world_with_fan(1);
        let fan_x = screen_x(w.traps.fans[0].x);
        assert_eq!(w.lift.x, STORED);
        assert!((fan_x - STORED).abs() <= 1, "{fan_x}");
        let stand = w.objects.list.iter().find(|o| o.sprite == "piedventi_gc");
        assert_eq!(stand.map(|o| o.screen_x), Some(fan_x));
    }

    /// FW §5.2 step 3: placement passes screen-unit x to `span_free`, so no placed object
    /// overlaps the fan's stored span on any seed.
    #[test]
    fn no_placed_object_overlaps_a_fan_span() {
        let fan_w = TrapKind::FanLeft.spec().stored_width();
        for seed in 1..=SEEDS {
            let w = world_with_fan(seed);
            let fx = placement_x(w.traps.fans[0].x);
            let clash = w
                .objects
                .list
                .iter()
                .filter(|o| o.collectible)
                .find(|o| o.screen_x + o.w >= fx && o.screen_x <= fx + fan_w);
            assert!(
                clash.is_none(),
                "seed {seed}: {:?}",
                clash.map(|o| o.screen_x)
            );
        }
    }
}
