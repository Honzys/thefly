//! W2. Trap entities on the table: flypapers, fans and zappers (game-spec §4.6, fly-and-world
//! §7.4). Built each period from the shop's [`TrapLayout`]. Ticks step 13.

use super::collision::{Rect, Shape, fly_outline, outlines_overlap};
use super::fly::{Fly, RawDamage};
use super::lift::SHOP_SCALE;
use super::{OBJECT_PARALLAX, SCREEN_W, W, World};
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::data::Size;
use crate::event::{Effect, Event, Pan, PlayParams, SoundOp, SoundSlot};
use crate::match_state::TrapLayout;

pub const MAX_FLYPAPERS: usize = 7;
pub const MAX_FANS: usize = 5;
pub const MAX_ZAPPERS: usize = 2;
/// Fans hit the fly at most once per this many ms.
pub const FAN_REHIT_MS: u64 = 100;
/// Flypaper divides both velocity components by this each tick of overlap.
pub const FLYPAPER_DRAG: f64 = 1.4;
/// Zapper arc on the fly: upward push and damage per arc.
pub const ARC_PUSH: i32 = 180;
pub const ARC_DAMAGE: i32 = 1;
/// The 1.25 layer's screen wrap, trunc(W × 1.25).
pub const SCREEN_WRAP: i32 = 5000;
/// Each flypaper built draws R(10); above 4 it uses this sprite (gaps §8.2).
pub const FLYPAPER_VARIANT_ROLL: i32 = 10;
pub const FLYPAPER_VARIANT_ABOVE: i32 = 4;
pub const FLYPAPER_VARIANT_SPRITE: &str = "colle_mouche2";
/// +0x91c (answers.md "Fan strength"): the hit adds ±500 to vx and subtracts 600 from vy.
pub const FAN_STRENGTH: i32 = 500;
/// The fan sheet's frame count: 666 / 3 = 222, the fan width +0x54 (GS §9.3, answers.md O22).
pub const FAN_FRAMES: i32 = 3;
/// A fan's top is at this y less its frame height (`FUN_0042699d`).
pub const FAN_FLOOR_Y: i32 = 570;
/// A fan hit: vy −= strength + 100; damage R(20) + 40, or R(10) + 20 with SuperFly; slot 45 at
/// 20000 + R(6000) Hz (FW §7.4, game-spec §4.6).
pub const FAN_LIFT: i32 = 100;
pub const FAN_DAMAGE_RAND: i32 = 20;
pub const FAN_DAMAGE_BASE: i32 = 40;
pub const FAN_DAMAGE_SUPERFLY_RAND: i32 = 10;
pub const FAN_DAMAGE_SUPERFLY_BASE: i32 = 20;
pub const FAN_HIT_HZ: i32 = 20_000;
pub const FAN_HIT_HZ_RAND: i32 = 6000;
/// The blow field `FUN_00429cbc` (answers.md "Fan strength"): the cone reaches 500 px past the
/// mouth, its rows slant up by 0.5 (top) and 0.35 (bottom) per px from the base; the push
/// P = trunc((1 − |mouth − x| / 500) × 200) goes 0.75 to vx and 0.25 to −vy.
pub const BLOW_REACH: i32 = 500;
pub const BLOW_PEAK: f64 = 200.0;
pub const BLOW_TOP_SLANT: f64 = 0.5;
pub const BLOW_BOTTOM_SLANT: f64 = 0.35;
pub const BLOW_X_SHARE: f64 = 0.75;
pub const BLOW_Y_SHARE: f64 = 0.25;
/// answers.md "Trap loop volumes": fans −100 (any on screen, or a single positional fan), else
/// −400; zappers −200 (any on screen), else −500, a single positional zapper −450.
pub const FAN_LOUD_CB: i32 = -100;
pub const FAN_QUIET_CB: i32 = -400;
pub const ZAPPER_SEEN_CB: i32 = -200;
pub const ZAPPER_IDLE_CB: i32 = -500;
pub const ZAPPER_SINGLE_CB: i32 = -450;
/// A fly whose top is within (zapper height − 80) of the zapper's top is grilled.
pub const GRILL_MARGIN: i32 = 80;
/// The arc test `FUN_00428b47` (answers.md "Zapper arc reach and schedule"): reach 250 px below
/// the zapper's bottom, chance trunc((1 − d / 250) × 100) against R(100), a 100 ms retry, an arc
/// of R(80) + 30 ms and slot 39 at R(800) + 11025 Hz.
pub const ARC_REACH: i32 = 250;
pub const ARC_ROLL: i32 = 100;
pub const ARC_RETRY_MS: u64 = 100;
pub const ARC_LEN_RAND: i32 = 80;
pub const ARC_LEN_BASE_MS: i32 = 30;
pub const ARC_HZ_RAND: i32 = 800;
pub const ARC_HZ: i32 = 11_025;
/// The grill column is the zapper's span less 60 px on each side (answers.md "Zapper arc on the
/// fly"); the fly's arc target sits 45 px below its top.
pub const GRILL_INSET: i32 = 60;
pub const ARC_TARGET_DY: i32 = 45;
/// Step 7, the arc fit: `j_eclaires` frames are 125 × 156 at y 78, scaled by
/// 1 + (target y − 234) / 156 (0.02 when that ratio is ≤ −1), and must sit in
/// [zapper x + 55, zapper x − 30 + zapper w]. Inside, the frame is R(4) + 1.
pub const ARC_Y: i32 = 78;
pub const ARC_FRAME_W: f64 = 125.0;
pub const ARC_FRAME_H: f64 = 156.0;
pub const ARC_FIT_BOTTOM: i32 = 234;
pub const ARC_MIN_SCALE: f64 = 0.02;
pub const ARC_LEFT_INSET: i32 = 55;
pub const ARC_RIGHT_INSET: i32 = 30;
pub const ARC_FRAME_ROLL: i32 = 4;
/// An arc on the fly throws up to 10 sparks (`FUN_00428ffc`) into the zapper's 30-slot pool,
/// one per free slot, each of 8 draws: R(5), R(5), frame R(3 + 1), R(10) < 6 negates vx,
/// R(181) + 70, R(10) < 4 negates vy, R(181) + 70, then R(60) + 100 ms life.
pub const ARC_SPARKS: usize = 10;
pub const SPARK_POOL: usize = 30;
pub const SPARK_JITTER: i32 = 5;
/// `etincelle` is a 3-frame sheet (screens-and-rendering §4.6).
pub const SPARK_FRAMES: i32 = 3;
pub const SPARK_NEG_X_BELOW: i32 = 6;
pub const SPARK_NEG_Y_BELOW: i32 = 4;
pub const SPARK_SPEED_RAND: i32 = 181;
pub const SPARK_SPEED_BASE: i32 = 70;
pub const SPARK_LIFE_RAND: i32 = 60;
pub const SPARK_LIFE_BASE_MS: i32 = 100;
/// The solid step `FUN_00428003`: at most this many 1 px steps out of a trap.
pub const SOLID_TRIES: u32 = 1000;
/// The flypaper's catch box is the fly frame inset by 50 left and right and 25 at the top.
pub const CATCH_INSET_X: i32 = 50;
pub const CATCH_INSET_TOP: i32 = 25;

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
    /// Layer x on the 1.25 layer, its screen x (see [`Traps::from_layout`]), and y.
    pub x: i32,
    pub screen_x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fan {
    /// Fan type 1 (+0x918), which pushes right; type 2 pushes left.
    pub facing_right: bool,
    /// Layer x and screen x, as for [`Flypaper`].
    pub x: i32,
    pub screen_x: i32,
    pub y: i32,
    /// +0x54, the frame width (222).
    pub w: i32,
    /// +0x91c.
    pub strength: i32,
    pub rehit: Timer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zapper {
    /// Layer x and screen x, as for [`Flypaper`].
    pub x: i32,
    pub screen_x: i32,
    pub y: i32,
    /// An arc is on (slot 39 played at its start), until `arc_timer` runs out.
    pub arcing: bool,
    pub arc_timer: Timer,
    /// +0x948: after an arc roll, no arc for 100 ms.
    pub retry: Timer,
    /// The last fitted arc (screen box, from y [`ARC_Y`]) and its frame 1..=4.
    pub arc: Rect,
    pub arc_frame: i32,
    /// The spark pool: a slot is free once its spark's life has run out.
    pub sparks: [Timer; SPARK_POOL],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Traps {
    pub flypapers: Vec<Flypaper>,
    pub fans: Vec<Fan>,
    pub zappers: Vec<Zapper>,
    /// The last volume set on the fan (44) and zapper (40) loops.
    pub fan_cb: i32,
    pub zapper_cb: i32,
}

/// One fan type's cone, as offsets from the fan's screen x and y (answers.md "Fan strength").
struct Cone {
    base: i32,
    mouth: i32,
    top: i32,
    bottom: i32,
}

const CONE_RIGHT: Cone = Cone {
    base: 0,
    mouth: 135,
    top: 42,
    bottom: 249,
};
const CONE_LEFT: Cone = Cone {
    base: 222,
    mouth: 93,
    top: 43,
    bottom: 279,
};

impl Fan {
    /// `FUN_00429cbc`: the (dvx, dvy) this fan's wind gives a point x..x + w wide at height py
    /// (the fly), tested at its centre. answers.md "Fan blow field: type-2 unwrap": a left fan
    /// whose base is within the reach of the screen's left edge sees a point in the ring's last
    /// point width at its centre moved by −wrap; a right fan never unwraps.
    fn blow(&self, x: i32, w: i32, py: i32) -> (i32, i32) {
        let (cone, dir) = if self.facing_right {
            (CONE_RIGHT, 1)
        } else {
            (CONE_LEFT, -1)
        };
        let (a, m) = (self.screen_x + cone.base, self.screen_x + cone.mouth);
        let unwrap = !self.facing_right && a <= BLOW_REACH && x >= SCREEN_WRAP - w;
        let px = if unwrap {
            w / 2 - (SCREEN_WRAP - x).abs()
        } else {
            x + w / 2
        };
        let inside = if self.facing_right {
            a < px && px < m + BLOW_REACH
        } else {
            m - BLOW_REACH < px && px < a
        };
        let d = f64::from((a - px).abs());
        let top = f64::from(self.y + cone.top) - BLOW_TOP_SLANT * d;
        let bottom = f64::from(self.y + cone.bottom) - BLOW_BOTTOM_SLANT * d;
        let py = f64::from(py);
        if !inside || py <= top || py >= bottom {
            return (0, 0);
        }
        let p = (1.0 - f64::from((m - px).abs()) / f64::from(BLOW_REACH)) * BLOW_PEAK;
        let p = f64::from(p as i32);
        (
            dir * (BLOW_X_SHARE * p) as i32,
            -((BLOW_Y_SHARE * p) as i32),
        )
    }

    /// The blade outline at the fan's screen x and y with its frame size (fan +0x9c).
    fn blades<'a>(&self, cx: &Ctx<'a>) -> Shape<'a> {
        Shape {
            x: self.screen_x,
            y: self.y,
            size: fan_frame(cx),
            outline: cx.data.outline(blades_outline(self.facing_right)),
        }
    }

    /// The screen x, minus the wrap when the fan straddles the ring end.
    pub fn visible_x(&self) -> i32 {
        visible_x(self.screen_x, self.w)
    }
}

/// Sound 45 at 20000 + R(6000) Hz: a fan hit on the fly or an object.
pub fn play_fan_hit(cx: &mut Ctx) {
    let freq = FAN_HIT_HZ + cx.rng.r(FAN_HIT_HZ_RAND);
    let params = PlayParams {
        freq_hz: u32::try_from(freq).ok(),
        ..PlayParams::default()
    };
    cx.sound(SoundSlot::FAN_HIT, SoundOp::Play(params));
}

impl Zapper {
    /// `FUN_00428b47` for one target (answers.md "Zapper arc reach and schedule"): column, no
    /// arc on or retry running, reach, the R(100) chance, then the arc fit. True when an arc
    /// starts; a fit that fails starts nothing and leaves the retry unarmed.
    fn arc_at(&mut self, t: Rect, size: Size, cx: &mut Ctx) -> bool {
        let zx = visible_x(self.screen_x, size.w);
        let column = t.x - size.w <= zx && zx <= t.x + t.w;
        let retrying = !self.retry.expired(cx.now);
        let bottom = self.y + size.h;
        if !column || self.arcing || retrying || t.y > bottom + ARC_REACH {
            return false;
        }
        let d = f64::from((t.y - bottom).abs());
        let chance = ((1.0 - d / f64::from(ARC_REACH)) * 100.0) as i32;
        if cx.rng.r(ARC_ROLL) > chance {
            self.retry.arm(cx.now, ARC_RETRY_MS);
            return false;
        }
        let Some(frame) = fit_arc(&mut self.arc, t, zx, size.w, cx) else {
            return false;
        };
        self.arc_frame = frame;
        let ms = cx.rng.r(ARC_LEN_RAND) + ARC_LEN_BASE_MS;
        self.arc_timer.arm(cx.now, u64::try_from(ms).unwrap_or(0));
        self.retry.arm(cx.now, ARC_RETRY_MS);
        self.arcing = true;
        let freq = cx.rng.r(ARC_HZ_RAND) + ARC_HZ;
        let params = PlayParams {
            freq_hz: u32::try_from(freq).ok(),
            ..PlayParams::default()
        };
        cx.sound(SoundSlot::ZAPPER_ARC, SoundOp::Play(params));
        true
    }
}

/// Step 7 of "Zapper arc reach (O9)": fits the arc over target `t` into `arc` and returns its
/// frame, or None when it overhangs both insets by more than half its width. Compared in f64;
/// the original's float compares are [I].
fn fit_arc(arc: &mut Rect, t: Rect, zx: i32, zw: i32, cx: &mut Ctx) -> Option<i32> {
    let ratio = f64::from(t.y - ARC_FIT_BOTTOM) / ARC_FRAME_H;
    let scale = if ratio <= -1.0 {
        ARC_MIN_SCALE
    } else {
        1.0 + ratio
    };
    let w = ARC_FRAME_W * scale;
    let centre = (f64::from(t.x) + f64::from(t.w) / 2.0) as i32;
    let x = (f64::from(centre) - w / 2.0) as i32;
    let (left, right) = (zx + ARC_LEFT_INSET, zx - ARC_RIGHT_INSET + zw);
    let (x0, x1) = (f64::from(x), f64::from(x) + w);
    let frame = if x >= left && x1 <= f64::from(right) {
        cx.rng.r(ARC_FRAME_ROLL) + 1
    } else if x < left && f64::from(left) - x0 <= w / 2.0 {
        3
    } else if x1 > f64::from(right) && x1 - f64::from(right) <= w / 2.0 {
        2
    } else {
        return None;
    };
    *arc = Rect::new(x, ARC_Y, w as i32, (ARC_FRAME_H * scale) as i32);
    Some(frame)
}

/// `FUN_00428ffc`: up to [`ARC_SPARKS`] sparks from the bottom centre of the zapper's arc, one
/// per free pool slot. Which R(5) offsets x and which y is [I].
fn throw_sparks(z: &mut Zapper, cx: &mut Ctx) {
    let (x, y) = (z.arc.x + z.arc.w / 2, z.arc.y + z.arc.h);
    let now = cx.now;
    let free = z.sparks.iter_mut().filter(|s| s.expired(now));
    for slot in free.take(ARC_SPARKS) {
        let dx = cx.rng.r(SPARK_JITTER);
        let dy = cx.rng.r(SPARK_JITTER);
        let frame = cx.rng.r(SPARK_FRAMES + 1);
        let sx = if cx.rng.r(10) < SPARK_NEG_X_BELOW {
            -1
        } else {
            1
        };
        let vx = (cx.rng.r(SPARK_SPEED_RAND) + SPARK_SPEED_BASE) * sx;
        let sy = if cx.rng.r(10) < SPARK_NEG_Y_BELOW {
            -1
        } else {
            1
        };
        let vy = (cx.rng.r(SPARK_SPEED_RAND) + SPARK_SPEED_BASE) * sy;
        let life_ms = cx.rng.r(SPARK_LIFE_RAND) + SPARK_LIFE_BASE_MS;
        slot.arm(cx.now, u64::try_from(life_ms).unwrap_or(0));
        cx.emit(Event::Effect(Effect::Spark {
            x: x + dx,
            y: y + dy,
            frame,
            vx,
            vy,
            life_ms,
        }));
    }
}

/// `FUN_00428003` (answers.md "Zapper and flypaper: solid push-out"): when the trap's outline
/// meets the fly's, the fly goes back to its previous position and steps 1 px against vx, then
/// against vy while vy ≠ 0, until clear; then v = trunc(−v / 1.4). The trap also goes back to
/// its previous position, which is a no-op: traps are never collectible and stay in state 0, so
/// the object motion `FUN_0042ea74` right before leaves the previous position at the current one
/// [C] (answers.md "Trap restore in the push-out is a no-op").
fn push_out(fly: &mut Fly, trap: &Shape<'_>) {
    let outline = fly_outline();
    let touches = |fly: &Fly| {
        let f = Shape {
            x: fly.x,
            y: fly.y,
            size: fly.size,
            outline: &outline,
        };
        outlines_overlap(trap, &f, SCREEN_WRAP)
    };
    if !touches(fly) {
        return;
    }
    (fly.x, fly.y) = (fly.prev_x, fly.prev_y);
    for _ in 0..SOLID_TRIES {
        if !touches(fly) {
            break;
        }
        fly.x -= fly.vx.signum();
        if fly.vy != 0 && touches(fly) {
            fly.y -= fly.vy.signum();
        }
    }
    fly.vx = (f64::from(-fly.vx) / FLYPAPER_DRAG) as i32;
    fly.vy = (f64::from(-fly.vy) / FLYPAPER_DRAG) as i32;
}

/// `FUN_0042825c`: box overlap with both bounds inclusive.
fn boxes_touch(a: &Rect, b: &Rect) -> bool {
    a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h
}

impl Traps {
    /// The period's traps from the shop slots (stored x is trunc(shop px / 0.152), gaps §11.4).
    /// Caps 7/5/2 skip the rest (gaps §8.2); each flypaper built under its cap draws `R(10)` and
    /// uses the `colle_mouche2` sprite when the draw is above 4 (`variant2`), in slot order.
    /// Starts the fan (44) and zapper (40) loops.
    ///
    /// answers.md "Fan stands, fan y and trap x": the stored x goes to the screen-x setter, so
    /// screen x = stored and layer x = trunc(stored / 1.25) until the first [`scroll`]. Fan
    /// y = 570 − frame h, flypapers and zappers y 0; the stored y is never read.
    pub fn from_layout(layout: &TrapLayout, cx: &mut Ctx) -> Self {
        let mut t = Self::default();
        let fan = fan_frame(cx);
        for slot in &layout.slots {
            let screen_x = slot.x;
            let x = (f64::from(slot.x) / OBJECT_PARALLAX) as i32;
            match slot.kind {
                TrapKind::Flypaper if t.flypapers.len() < MAX_FLYPAPERS => {
                    let variant2 = cx.rng.r(FLYPAPER_VARIANT_ROLL) > FLYPAPER_VARIANT_ABOVE;
                    t.flypapers.push(Flypaper {
                        variant2,
                        x,
                        screen_x,
                        y: 0,
                    });
                }
                TrapKind::FanLeft | TrapKind::FanRight if t.fans.len() < MAX_FANS => {
                    t.fans.push(Fan {
                        facing_right: slot.kind == TrapKind::FanRight,
                        x,
                        screen_x,
                        y: FAN_FLOOR_Y - fan.h,
                        w: fan.w,
                        strength: FAN_STRENGTH,
                        rehit: Timer::UNARMED,
                    });
                }
                TrapKind::Zapper if t.zappers.len() < MAX_ZAPPERS => {
                    t.zappers.push(Zapper {
                        x,
                        screen_x,
                        y: 0,
                        arcing: false,
                        arc_timer: Timer::UNARMED,
                        retry: Timer::UNARMED,
                        arc: Rect::default(),
                        arc_frame: 0,
                        sparks: [Timer::UNARMED; SPARK_POOL],
                    });
                }
                _ => {}
            }
        }
        t.start_loops(cx);
        t
    }

    fn start_loops(&mut self, cx: &mut Ctx) {
        (self.fan_cb, self.zapper_cb) = self.loop_levels(cx);
        if !self.fans.is_empty() {
            let params = PlayParams {
                volume_cb: self.fan_cb,
                ..PlayParams::default()
            };
            cx.sound(SoundSlot::FAN, SoundOp::Loop(params));
        }
        if !self.zappers.is_empty() {
            let params = PlayParams {
                volume_cb: self.zapper_cb,
                ..PlayParams::default()
            };
            cx.sound(SoundSlot::ZAPPER_HUM, SoundOp::Loop(params));
        }
    }

    /// The fan and zapper loop volumes (answers.md "Trap loop volumes"): a single trap plays
    /// positionally at a fixed level, several follow "any on screen" (`FUN_00418b40`).
    fn loop_levels(&self, cx: &Ctx) -> (i32, i32) {
        let fan = match self.fans.as_slice() {
            [_] => FAN_LOUD_CB,
            fans if fans.iter().any(|f| on_screen(f.screen_x, f.w)) => FAN_LOUD_CB,
            _ => FAN_QUIET_CB,
        };
        let zw = TrapKind::Zapper.spec_size(cx).w;
        let zapper = match self.zappers.as_slice() {
            [_] => ZAPPER_SINGLE_CB,
            zs if zs.iter().any(|z| on_screen(z.screen_x, zw)) => ZAPPER_SEEN_CB,
            _ => ZAPPER_IDLE_CB,
        };
        (fan, zapper)
    }

    /// Per tick: set a loop's volume when its level changes.
    fn update_loops(&mut self, cx: &mut Ctx) {
        let (fan, zapper) = self.loop_levels(cx);
        if !self.fans.is_empty() && fan != self.fan_cb {
            self.fan_cb = fan;
            cx.sound(SoundSlot::FAN, SoundOp::SetVolume(fan));
        }
        if !self.zappers.is_empty() && zapper != self.zapper_cb {
            self.zapper_cb = zapper;
            cx.sound(SoundSlot::ZAPPER_HUM, SoundOp::SetVolume(zapper));
        }
    }

    /// The arc test of every zapper against one target (the fly, or a midge at (x, y + 7)).
    /// Returns how many arcs started; each draws rand.
    pub fn arcs_at(&mut self, target: Rect, cx: &mut Ctx) -> usize {
        let size = TrapKind::Zapper.spec_size(cx);
        let mut arcs = 0;
        for z in &mut self.zappers {
            arcs += usize::from(z.arc_at(target, size, cx));
        }
        arcs
    }

    /// The summed blow field of every fan at a point x..x + w wide at height y.
    pub fn blow(&self, x: i32, w: i32, y: i32) -> (i32, i32) {
        self.fans.iter().fold((0, 0), |(sx, sy), f| {
            let (dx, dy) = f.blow(x, w, y);
            (sx + dx, sy + dy)
        })
    }

    /// The first fan, in fan order, whose blades touch `s` (answers.md "Object fan push").
    pub fn blades_hit(&self, s: &Shape<'_>, cx: &Ctx) -> Option<&Fan> {
        self.fans
            .iter()
            .find(|f| outlines_overlap(s, &f.blades(cx), SCREEN_WRAP))
    }

    /// (x, y) is inside a fan's frame, where its wind blows (`FUN_00427885`).
    pub fn fan_touches(&self, x: i32, y: i32, cx: &Ctx) -> bool {
        let h = fan_frame(cx).h;
        self.fans
            .iter()
            .any(|f| in_span(x, visible_x(f.screen_x, f.w), f.w) && in_span(y, f.y, h))
    }

    /// `FUN_00427bb4` (answers.md "Placement test against fans (O22)"): may an object occupy
    /// [x, x + w] (the 1.25 layer's screen x, FW §5.2 step 3)? Blocked when
    /// fan x − w ≤ x ≤ fan x + fan width, inclusive; zappers and flypapers are not tested. The
    /// original's second clause (x + w ≥ 6250) never blocks inside the 5000 range.
    pub fn span_free(&self, x: i32, w: i32) -> bool {
        self.fans.iter().all(|f| {
            let fx = f.screen_x;
            !(fx - w <= x && x <= fx + f.w)
        })
    }
}

/// `FUN_00418b40`: a trap of a 1.25-layer x is on screen when it straddles the ring end or
/// starts within the view.
fn on_screen(screen_x: i32, w: i32) -> bool {
    screen_x + w >= SCREEN_WRAP || screen_x <= SCREEN_W
}

/// The wrapped screen x of a 1.25-layer x (FW §4.1).
fn screen_x(layer_x: i32) -> i32 {
    ((f64::from(layer_x) * OBJECT_PARALLAX) as i32).rem_euclid(SCREEN_WRAP)
}

/// The drawn x of a trap w wide: its screen x, minus the wrap width when it straddles the ring end
/// (FW §4.4 step 6), so a trap at 4990 reaches the left edge of the view.
fn visible_x(screen_x: i32, w: i32) -> i32 {
    if screen_x >= SCREEN_WRAP - w {
        screen_x - SCREEN_WRAP
    } else {
        screen_x
    }
}

fn in_span(v: i32, start: i32, len: i32) -> bool {
    (start..=start + len).contains(&v)
}

/// layerX += trunc(−delta), wrapped at W, and the screen x from it (FW §4.1, time-based
/// integrator with v = 0).
fn scroll(layer_x: &mut i32, screen: &mut i32, delta: f64) {
    *layer_x = (*layer_x + (-delta) as i32).rem_euclid(W);
    *screen = screen_x(*layer_x);
}

impl TrapKind {
    fn spec_size(self, cx: &Ctx) -> Size {
        cx.data.size(self.spec().sprite)
    }
}

/// One frame of the fan sheet (`ventilo_*` holds FAN_FRAMES frames side by side).
pub fn fan_frame(cx: &Ctx) -> Size {
    let sheet = TrapKind::FanLeft.spec_size(cx);
    Size {
        w: sheet.w / FAN_FRAMES,
        h: sheet.h,
    }
}

impl World {
    /// Step 13 (`FUN_00427270`): the loops (44, 40) and their positional single-trap variants,
    /// then flypaper drag, the fans' blow field and hit, the zappers' grill and arc.
    pub fn traps_step(&mut self, cx: &mut Ctx) {
        let delta = self.camera.delta;
        let t = &mut self.traps;
        for f in &mut t.flypapers {
            scroll(&mut f.x, &mut f.screen_x, delta);
        }
        for f in &mut t.fans {
            scroll(&mut f.x, &mut f.screen_x, delta);
        }
        for z in &mut t.zappers {
            scroll(&mut z.x, &mut z.screen_x, delta);
            z.arcing = !z.arc_timer.expired(cx.now);
        }
        t.update_loops(cx);
        self.single_trap_pan(delta, cx);
        // answers.md "Banner: the whole world update runs, paused": active = 0 skips the rest.
        if self.paused {
            return;
        }
        if self.fly.alive() {
            self.flypaper_step(cx);
        }
        // answers.md "Fan strength": the fan skips the fly only while it is splatted (+0x2a4).
        if !self.fly.splatted {
            self.fan_blow();
            self.fan_hits(cx);
        }
        self.zapper_hits(cx);
    }

    /// `FUN_0042663b` per flypaper (answers.md "Zapper and flypaper: solid push-out"): the solid
    /// step against the sprite's mask outline (answers.md "Flypaper push-out: mask outline"),
    /// then v / 1.4 when the fly's catch box touches the paper (inclusive), no sound.
    fn flypaper_step(&mut self, cx: &Ctx) {
        for i in 0..self.traps.flypapers.len() {
            let p = self.traps.flypapers[i];
            let sprite = if p.variant2 {
                FLYPAPER_VARIANT_SPRITE
            } else {
                TrapKind::Flypaper.spec().sprite
            };
            let size = cx.data.size(sprite);
            let paper = Rect::new(visible_x(p.screen_x, size.w), p.y, size.w, size.h);
            if paper.overlaps(&self.fly_rect()) {
                let shape = Shape {
                    x: paper.x,
                    y: paper.y,
                    size,
                    outline: cx.data.outline(sprite),
                };
                push_out(&mut self.fly, &shape);
            }
            let f = self.fly_rect();
            let catch = Rect::new(
                f.x + CATCH_INSET_X,
                f.y + CATCH_INSET_TOP,
                f.w - 2 * CATCH_INSET_X,
                f.h - CATCH_INSET_TOP,
            );
            if boxes_touch(&catch, &paper) {
                self.fly.vx = (f64::from(self.fly.vx) / FLYPAPER_DRAG) as i32;
                self.fly.vy = (f64::from(self.fly.vy) / FLYPAPER_DRAG) as i32;
            }
        }
    }

    /// `FUN_00429cbc`, before the hit test: the wind pushes the fly by its centre.
    fn fan_blow(&mut self) {
        let (_, cy) = self.fly.centre();
        let (dx, dy) = self.traps.blow(self.fly.x, self.fly.size.w, cy);
        self.fly.vx += dx;
        self.fly.vy += dy;
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
        let hit = self.traps.fans.iter().position(|f| {
            f.rehit.expired(cx.now) && outlines_overlap(&fly, &f.blades(cx), SCREEN_WRAP)
        });
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
        play_fan_hit(cx);
        self.fly.add_raw_damage(damage, RawDamage::Fan { dir }, cx);
    }

    /// `FUN_004288fe` per zapper in index order (answers.md "Zapper arc on the fly"), each only
    /// while the fly is not dying, splatted or zapped. The zapper's x is unwrapped first (x −
    /// wrap when x ≥ wrap − w, round 3). Inside the column the grill near the top; outside it the
    /// solid step. Then that zapper's arc test on (x, y + 45, w): an arc pushes the fly up 180,
    /// throws its sparks and adds 1.
    fn zapper_hits(&mut self, cx: &mut Ctx) {
        let size = TrapKind::Zapper.spec_size(cx);
        for i in 0..self.traps.zappers.len() {
            let f = &self.fly;
            if f.dying || f.splatted || f.zapper_splat {
                return;
            }
            let z = &self.traps.zappers[i];
            let zx = visible_x(z.screen_x, size.w);
            let column = f.x <= zx + size.w - GRILL_INSET && f.x + f.size.w >= zx + GRILL_INSET;
            if !column {
                let shape = Shape {
                    x: zx,
                    y: z.y,
                    size,
                    outline: cx.data.outline(TrapKind::Zapper.spec().sprite),
                };
                push_out(&mut self.fly, &shape);
            } else if f.y <= z.y + size.h - GRILL_MARGIN {
                self.fly.grill(cx);
            }
            let f = &self.fly;
            let target = Rect::new(f.x, f.y + ARC_TARGET_DY, f.size.w, f.size.h);
            let z = &mut self.traps.zappers[i];
            if !z.arc_at(target, size, cx) {
                continue;
            }
            self.fly.vy -= ARC_PUSH;
            throw_sparks(z, cx);
            self.fly
                .add_raw_damage(ARC_DAMAGE, RawDamage::ZapperArc, cx);
        }
    }

    /// A single fan or zapper plays its loop positionally (game-spec §4.6).
    fn single_trap_pan(&self, delta: f64, cx: &mut Ctx) {
        if delta == 0.0 {
            return;
        }
        if let [f] = self.traps.fans.as_slice() {
            let pan = Pan::ScreenX {
                x: visible_x(f.screen_x, f.w),
                w: f.w,
                wrap: crate::event::PAN_WRAP_OBJECT,
            };
            cx.sound(SoundSlot::FAN, SoundOp::Move(pan));
        }
        if let [z] = self.traps.zappers.as_slice() {
            let w = TrapKind::Zapper.spec_size(cx).w;
            let pan = Pan::ScreenX {
                x: visible_x(z.screen_x, w),
                w,
                wrap: crate::event::PAN_WRAP_OBJECT,
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
            screen_x: screen_x(x),
            y: 300,
            w: 222,
            strength: FAN_STRENGTH,
            rehit: Timer::UNARMED,
        }
    }

    #[test]
    fn stored_widths_scale_the_shop_widths() {
        let w = TRAP_TABLE.map(|t| t.stored_width());
        assert_eq!(w, [52, 223, 223, 638]);
    }

    /// answers.md O22: a fan at layer x 800 sits at screen x 1000; a span [x, x + w] is blocked
    /// for 1000 − w ≤ x ≤ 1000 + 222 (the frame width, not the stored 223).
    #[test]
    fn only_a_fan_span_blocks_an_object() {
        let traps = Traps {
            fans: vec![fan_at(800)],
            flypapers: vec![Flypaper {
                variant2: false,
                x: 2000,
                screen_x: 2500,
                y: 0,
            }],
            ..Traps::default()
        };
        assert!(traps.span_free(899, 100) && traps.span_free(1223, 50));
        assert!(!traps.span_free(900, 100) && !traps.span_free(1222, 50));
        assert!(traps.span_free(2500, 100), "flypapers hang above the table");
        assert!(Traps::default().span_free(0, 5000));
    }

    /// answers.md O22: the original tests no wrapped copy (its x + w ≥ 6250 clause never fires),
    /// so a fan at screen 4900 does not block the left edge.
    #[test]
    fn a_fan_span_at_the_ring_end_leaves_the_left_edge_free() {
        let traps = Traps {
            fans: vec![fan_at(3920)],
            ..Traps::default()
        };
        assert!(traps.span_free(50, 100) && !traps.span_free(4850, 100));
    }

    /// Type 2 (left) at screen 500, y 300: base 722, mouth 593, rows 343..579. At (300, 400):
    /// d = 422, rows 132..431.3; P = trunc((1 − 293/500) × 200) = 82; vx −61, vy −20. At
    /// (300, 450) the point is under the slanted bottom; at 93 it is at the reach and outside.
    #[test]
    fn a_left_fan_blows_left_and_up_inside_its_slanted_cone() {
        let fan = fan_at(400);
        assert_eq!(centred(&fan, 300, 400), (-61, -20));
        assert_eq!(centred(&fan, 300, 450), (0, 0));
        assert_eq!(centred(&fan, 93, 400), (0, 0));
        assert_eq!(centred(&fan, 722, 400), (0, 0));
    }

    /// The blow at a 120-wide point centred on (cx, cy).
    fn centred(fan: &Fan, cx: i32, cy: i32) -> (i32, i32) {
        fan.blow(cx - 60, 120, cy)
    }

    /// Type 1 (right) at screen 500, y 300: base 500, mouth 635, rows 342..549. At (801, 350):
    /// d = 301, rows 191.5..443.65; P = trunc((1 − 166/500) × 200) = 133; vx +99, vy −33.
    #[test]
    fn a_right_fan_blows_right_and_up() {
        let fan = Fan {
            facing_right: true,
            ..fan_at(400)
        };
        assert_eq!(centred(&fan, 801, 350), (99, -33));
        assert_eq!(centred(&fan, 500, 400), (0, 0));
    }

    /// answers.md "Fan blow field: type-2 unwrap": a left fan with base x + 222 ≤ 500 sees a
    /// point at x ≥ 5000 − w at cx = w/2 − |5000 − x|. Fan at screen 200 (base 422, mouth 293):
    /// x 4950 → cx 10, d 412, rows 137..434.8, P = trunc((1 − 283/500) × 200) = 86: (−64, −21).
    /// x 4880 → cx −60, d 482, P = trunc(58.8) = 58: (−43, −14); x 4879 is not unwrapped.
    /// Screen 278 (base 500) still unwraps: cx 10, P = trunc((1 − 361/500) × 200) = 55: (−41,
    /// −13); screen 280 (base 502) does not.
    #[test]
    fn a_left_fan_near_the_left_edge_sees_a_point_at_the_ring_end() {
        let fan = fan_at(160);
        assert_eq!(fan.screen_x, 200);
        assert_eq!(fan.blow(4950, 120, 300), (-64, -21));
        assert_eq!(fan.blow(4880, 120, 300), (-43, -14));
        assert_eq!(fan.blow(4879, 120, 300), (0, 0));
        assert_eq!(fan_at(223).blow(4950, 120, 300), (-41, -13));
        assert_eq!(fan_at(224).blow(4950, 120, 300), (0, 0));
        let right = Fan {
            facing_right: true,
            ..fan
        };
        assert_eq!(right.blow(4950, 120, 300), (0, 0), "type 1 has no unwrap");
        // The base is never moved: a left fan at screen 4900 (base 5122) misses a point at 50.
        assert_eq!(centred(&fan_at(3920), 50, 300), (0, 0));
    }
}
#[cfg(test)]
mod layout_tests {
    use super::super::farmer::rig::Rig;
    use super::*;
    use crate::data::Outline;
    use crate::event::Effect;
    use crate::event::Event;
    use crate::match_state::{MatchConfig, MatchState, Players, TrapSlot};
    use crate::rng::MsvcRand;

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
        // answers.md "Fan stands, fan y and trap x": stored 1000 is the screen x and layer x
        // trunc(1000 / 1.25) = 800; fan y = 570 − 300, the others 0.
        let f = &traps.fans[0];
        assert_eq!((f.x, f.screen_x, f.y), (800, 1000, 270));
        let z = &traps.zappers[0];
        assert_eq!((z.x, z.screen_x, z.y), (3200, 4000, 0));
        let p = &traps.flypapers[1];
        assert_eq!((p.x, p.screen_x, p.y), (80, 100, 0));
        assert!(traps.fans[0].facing_right);
        assert_eq!((traps.fans[0].w, traps.fans[0].strength), (222, 500));
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

    /// answers.md "Fan type mapping": kind 1 is `j_ventilo_gauche`, built as internal type 2 with
    /// the `palesventi_gc` blades, and blows left: at stored 500 (screen 500, y 270) a point
    /// centred on (300, 370) gets (−61, −20). Kind 2 is `ventilo_droite`, which blows right.
    #[test]
    fn kind_1_is_ventilo_gauche_and_blows_left() {
        assert_eq!(
            (TrapKind::FanLeft as usize, TRAP_TABLE[1].kind),
            (1, TrapKind::FanLeft)
        );
        assert_eq!(
            TRAP_TABLE.map(|t| t.sprite)[1..3],
            ["ventilo_gauche", "ventilo_droite"]
        );
        let mut rig = Rig::new(1);
        let w = world_with(vec![slot(TrapKind::FanLeft, 500)], &mut rig);
        let f = w.traps.fans[0];
        assert!(!f.facing_right);
        assert_eq!(blades_outline(f.facing_right), "palesventi_gc");
        assert_eq!(f.blow(240, 120, 370), (-61, -20));
        let w = world_with(vec![slot(TrapKind::FanRight, 500)], &mut rig);
        assert!(w.traps.fans[0].facing_right);
    }

    /// answers.md "Zapper and flypaper: solid push-out": the drag tests the fly box (x + 50,
    /// y + 25, w − 100, h − 25) inclusively. A flypaper at stored 500 covers 500..553 × 0..300; a
    /// fly at y 272 has its outline rows (301..356) below the paper, so no solid hit. At x 430 the
    /// box's right edge is 500: v / 1.4. At x 429 it is 499: nothing.
    #[test]
    fn a_flypaper_drags_through_the_inset_catch_box() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Flypaper, 500)], &mut rig);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (430, 272, 300, -140);
        (w.fly.prev_x, w.fly.prev_y) = (430, 272);
        w.traps_step(&mut rig.at(1));
        assert_eq!(
            (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy),
            (430, 272, 214, -100)
        );
        w.fly.x = 429;
        w.traps_step(&mut rig.at(2));
        assert_eq!((w.fly.vx, w.fly.vy), (214, -100));
    }

    /// The flypaper's mask outline (the synthetic 53 × 300 rect: half-x 250 and 275, rows
    /// 0..=85) meets the fly's right side (half-x x / 2 + 45) at x 410. The fly goes back to its
    /// previous position: from 409 it is clear, so it stays there; from 411 it steps x −1 (still
    /// touching), y +1 against vy −140, then x −1 to 409, clear (its bottom row 85 at half-x
    /// 219..249 misses row 85's 250..275). Then v = trunc(−v / 1.4), and the inset box (459..479)
    /// misses the paper, so no drag.
    #[test]
    fn a_flypaper_pushes_the_fly_back_out_of_its_outline() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Flypaper, 500)], &mut rig);
        for (prev_x, want_y) in [(409, 0), (411, 1)] {
            (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (410, 0, 300, -140);
            (w.fly.prev_x, w.fly.prev_y) = (prev_x, 0);
            w.traps_step(&mut rig.at(1));
            assert_eq!(
                (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy),
                (409, want_y, -214, 100)
            );
        }
    }

    /// answers.md "Flypaper push-out: mask outline": the outline ends at row 85, so the same fly
    /// at y 100 (outline rows 129..184) passes through the paper's side untouched.
    #[test]
    fn a_fly_below_row_85_of_a_flypaper_is_not_pushed_out() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Flypaper, 500)], &mut rig);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (410, 100, 300, -140);
        (w.fly.prev_x, w.fly.prev_y) = (411, 100);
        w.traps_step(&mut rig.at(1));
        assert_eq!(
            (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy),
            (410, 100, 300, -140)
        );
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
            (w.fly.x, w.fly.y, w.fly.damage_total) = (500, 320, 0);
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
    /// at x − 5000. A flypaper at stored 4999 (layer 3999, screen trunc(3999 × 1.25) = 4998)
    /// covers x −2..51, so a fly at (0, 272) drags (box 50..70).
    #[test]
    fn a_flypaper_at_the_ring_end_drags_a_fly_at_the_left_edge() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Flypaper, 4999)], &mut rig);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (0, 272, 300, -140);
        (w.fly.prev_x, w.fly.prev_y) = (0, 272);
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

    fn sounds(rig: &Rig, want: SoundSlot) -> Vec<SoundOp> {
        rig.events
            .iter()
            .filter_map(|e| match e {
                Event::Sound { slot, op } if *slot == want => Some(*op),
                _ => None,
            })
            .collect()
    }

    /// Left fan at stored 500 (screen 500, y 570 − 300 = 270). A fly centred at (300, 370) is in the cone
    /// and clear of the blades: vx −61, vy −20 (see the unit test), no hit. The blow runs on a
    /// dying fly, not on a splatted one (answers.md "Fan strength").
    #[test]
    fn the_blow_field_pushes_the_fly_unless_it_is_splatted() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::FanLeft, 500)], &mut rig);
        let place = |w: &mut World| (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (240, 323, 0, 0);
        place(&mut w);
        w.fly.dying = true;
        w.traps_step(&mut rig.at(1));
        assert_eq!((w.fly.vx, w.fly.vy), (-61, -20));
        assert!(sounds(&rig, SoundSlot::FAN_HIT).is_empty());
        place(&mut w);
        w.fly.splatted = true;
        w.traps_step(&mut rig.at(2));
        assert_eq!((w.fly.vx, w.fly.vy), (0, 0));
    }

    /// Right fan at stored 500: a fly at (500, 320), centre (560, 367), is on the blades and
    /// in the cone (d = 60, rows 282..498; P = trunc((1 − 75/500) × 200) = 170: +127, −42).
    /// The hit adds +500 and −600 (answers.md "Fan strength").
    #[test]
    fn a_fan_hit_adds_500_and_lifts_600() {
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::FanRight, 500)], &mut rig);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (500, 320, 0, 0);
        w.traps_step(&mut rig.at(1));
        assert_eq!(sounds(&rig, SoundSlot::FAN_HIT).len(), 1);
        assert_eq!((w.fly.vx, w.fly.vy), (127 + 500, -42 - 600));
    }

    /// Zapper at stored 1000 (screen 1000, 635 × 131 from y 0); a fly at (1000, 86) is below
    /// the grill zone (y ≤ 51) and its arc target y 86 + 45 = 131 is at d = 0, so the chance is
    /// 100. Seed 1: R(100) = 41 ≤ 100 arcs, 18467 % 80 + 30 = 97 ms, slot 39 at
    /// 6334 % 800 + 11025 = 11759 Hz (the arc, x 1038, overhangs L = 1055 by less than half its
    /// 42 px: frame 3, no draw); vy −180, then 10 sparks of 8 draws (rands 4..=83), then
    /// damage 1. The next tick the arc is still on and nothing is drawn: rand 84 is 19954.
    #[test]
    fn an_arc_starts_on_the_chance_roll_and_lasts_r80_plus_30_ms() {
        let mut rig = Rig::new(7);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y, w.fly.vy) = (1000, 86, 0);
        w.traps_step(&mut rig.at(1));
        let z = w.traps.zappers[0];
        assert!(z.arcing && !w.fly.splatted);
        assert_eq!(z.arc_frame, 3);
        assert_eq!(z.arc_timer.remaining_ms(rig.at(1).now), 97);
        assert_eq!((w.fly.vy, w.fly.damage_total), (-180, 1));
        let freq = PlayParams {
            freq_hz: Some(11_759),
            ..PlayParams::default()
        };
        assert_eq!(sounds(&rig, SoundSlot::ZAPPER_ARC), [SoundOp::Play(freq)]);
        w.traps_step(&mut rig.at(2));
        assert_eq!(w.fly.vy, -180);
        assert_eq!(rig.rng.rand(), 19_954);
    }

    /// Target d = 200 (fly y 286): chance trunc((1 − 0.8) × 100) = trunc(19.99…) = 19 < 41, so a
    /// 100 ms retry and no draw on the next tick; past 250 below the bottom nothing is drawn.
    #[test]
    fn a_failed_roll_waits_100_ms_and_the_reach_is_250() {
        let mut rig = Rig::new(7);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y) = (1000, 286);
        w.traps_step(&mut rig.at(1));
        w.traps_step(&mut rig.at(2));
        assert!(!w.traps.zappers[0].arcing);
        assert_eq!(rig.rng.rand(), 18467);
        w.traps.zappers[0].retry = Timer::UNARMED;
        w.fly.y = 131 + 251 - 45;
        w.traps_step(&mut rig.at(3));
        assert_eq!(rig.rng.rand(), 6334);
    }

    /// The fly is not tested by the zapper while dying (+0x288), but is while stunned.
    #[test]
    fn a_dying_fly_is_not_zapped() {
        let mut rig = Rig::new(7);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y, w.fly.dying) = (1000, 86, true);
        w.traps_step(&mut rig.at(1));
        assert_eq!(rig.rng.rand(), 41);
        (w.fly.dying, w.fly.stunned) = (false, true);
        w.traps_step(&mut rig.at(2));
        assert!(w.traps.zappers[0].arcing);
    }

    /// answers.md "Zapper arc on the fly": the grill does not return, so the same zapper's arc
    /// test still runs (it arcs or arms its retry), but the entry gate is tested again at the
    /// next zapper, which a grilled fly no longer reaches.
    #[test]
    fn a_grilled_fly_meets_the_same_zappers_arc_but_not_the_next_zapper() {
        let mut rig = Rig::new(7);
        let z = slot(TrapKind::Zapper, 1000);
        let mut w = world_with(vec![z, z], &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y) = (1000, 40);
        let at = rig.at(1).now;
        w.traps_step(&mut rig.at(1));
        assert!(w.fly.zapper_splat);
        let [first, second] = [w.traps.zappers[0], w.traps.zappers[1]];
        assert!(first.arcing || !first.retry.expired(at));
        assert!(!second.arcing && second.retry.expired(at));
    }

    /// Outside the grill column (fly x < 1060 − 120) the zapper is solid. Its outline (a 100 ×
    /// 100 rect here, half-x 500 and 549) meets the fly's top row (half-x 470..500) at x 910;
    /// from the previous x 909 (470..499 → 469..499) it is clear: the fly stays there and
    /// v = trunc(−v / 1.4). The retry timer keeps the arc off.
    #[test]
    fn a_zapper_outside_its_column_pushes_the_fly_back() {
        let mut rig = Rig::new(7);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        let mut rig = Rig::new(1);
        w.traps.zappers[0].retry.arm(rig.at(1).now, ARC_RETRY_MS);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (910, 0, 300, 0);
        (w.fly.prev_x, w.fly.prev_y) = (909, 0);
        w.traps_step(&mut rig.at(1));
        assert_eq!((w.fly.x, w.fly.y, w.fly.vx, w.fly.vy), (909, 0, -214, 0));
        assert!(!w.fly.splatted);
        assert_eq!(rig.rng.rand(), 41);
    }

    /// answers.md round 3: before the column test a zapper with x ≥ wrap − w moves left by the
    /// wrap. At stored 4995 (screen 4995) it sits at −5, so with its full 635 × 131 rect outline
    /// its right side is half-x trunc(−5 / 2) + 316 = 314, which a fly at 598 (half-x 299 + 15)
    /// meets. Unshifted (4995 / 2 + 316 = 2813 against 2799 + 15) it would miss.
    #[test]
    fn a_zapper_at_the_ring_end_is_solid_at_its_shifted_x() {
        let mut rig = Rig::new(7);
        let size = Size { w: 635, h: 131 };
        rig.data
            .outlines
            .insert("tue_mouche".to_owned(), Outline::rect(size));
        let mut w = world_with(vec![slot(TrapKind::Zapper, 4995)], &mut rig);
        w.traps.zappers[0].retry.arm(rig.at(1).now, ARC_RETRY_MS);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (598, 0, -300, 0);
        (w.fly.prev_x, w.fly.prev_y) = (600, 0);
        w.traps_step(&mut rig.at(1));
        assert_eq!((w.fly.x, w.fly.vx), (600, 214));
    }

    /// answers.md "Zapper arc reach (O9)" step 7 with the fly target (1017, 131, 120): scale
    /// 1 + (131 − 234) / 156, arc w 42.47, x = trunc(1077 − 21.23) = 1055 = L, inside
    /// [1055, 1605]: frame R(4) + 1 = 18467 % 4 + 1 = 4, then R(80) = 14 (44 ms), R(800) = 100
    /// (11125 Hz), and the retry is armed for 100 ms. The sparks start at (1055 + 21, 78 + 53);
    /// the first takes rands 5..=12: R(5) 4, R(5) 4, frame 2, 8 ≥ 6 so +, 174 + 70, 4 ≥ 4 so +,
    /// 94 + 70, life 5 + 100. Ten sparks use rands 5..=84; rand 85 is 18756.
    #[test]
    fn an_arc_inside_the_zapper_draws_its_frame_then_ten_sparks() {
        let mut rig = Rig::new(7);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y, w.fly.vy) = (1017, 86, 0);
        (w.fly.prev_x, w.fly.prev_y) = (1017, 86);
        let now = rig.at(1).now;
        w.traps_step(&mut rig.at(1));
        let z = w.traps.zappers[0];
        assert!(z.arcing);
        assert_eq!(z.arc_frame, 4);
        assert_eq!(z.arc_timer.remaining_ms(now), 44);
        assert_eq!(z.retry.remaining_ms(now), 100);
        let freq = PlayParams {
            freq_hz: Some(11_125),
            ..PlayParams::default()
        };
        assert_eq!(sounds(&rig, SoundSlot::ZAPPER_ARC), [SoundOp::Play(freq)]);
        let sparks: Vec<Effect> = rig
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Effect(s @ Effect::Spark { .. }) => Some(*s),
                _ => None,
            })
            .collect();
        assert_eq!(sparks.len(), 10);
        let first = Effect::Spark {
            x: 1080,
            y: 135,
            frame: 2,
            vx: 244,
            vy: 164,
            life_ms: 105,
        };
        assert_eq!(sparks[0], first);
        assert_eq!(rig.rng.rand(), 18756);
    }

    /// The spark pool holds 30: with 25 slots still alive an arc throws only 5 sparks (40
    /// draws, rands 5..=44, so rand 45 is next); with all 30 alive it throws none.
    #[test]
    fn an_arc_throws_only_as_many_sparks_as_the_pool_has_free_slots() {
        for (alive, next) in [(25, 45), (30, 5)] {
            let mut rig = Rig::new(7);
            let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
            let mut rig = Rig::new(1);
            let now = rig.at(1).now;
            for s in &mut w.traps.zappers[0].sparks[..alive] {
                s.arm(now, 200);
            }
            (w.fly.x, w.fly.y, w.fly.vy) = (1017, 86, 0);
            w.traps_step(&mut rig.at(1));
            let thrown = rig
                .events
                .iter()
                .filter(|e| matches!(e, Event::Effect(Effect::Spark { .. })))
                .count();
            assert_eq!(thrown, (30 - alive).min(10));
            let mut probe = MsvcRand::new(1);
            (1..next).for_each(|_| _ = probe.rand());
            assert_eq!(rig.rng, probe);
        }
    }

    /// Step 7's no-start case: at fly x 900 the arc (x 938, 42 wide) overhangs L = 1055 by more
    /// than half its width and stays inside Rt. R(100) is drawn and nothing else, the retry is
    /// not armed, so the next tick rolls R(100) again (rand 2), and rand 3 is next.
    #[test]
    fn an_arc_that_does_not_fit_starts_nothing_and_rolls_again() {
        let mut rig = Rig::new(7);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y) = (900, 86);
        (w.fly.prev_x, w.fly.prev_y) = (900, 86);
        w.traps_step(&mut rig.at(1));
        let z = w.traps.zappers[0];
        assert!(!z.arcing && z.retry.expired(rig.at(1).now));
        w.traps_step(&mut rig.at(2));
        assert_eq!(rig.rng.rand(), 6334);
    }

    /// An arc that takes the total past 100 runs the zapper burst, without sound 43.
    #[test]
    fn an_arc_over_100_runs_the_zapper_burst() {
        let mut rig = Rig::new(7);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 1000)], &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y, w.fly.vy, w.fly.damage_total) = (1000, 86, 0, 100);
        w.traps_step(&mut rig.at(1));
        assert!(w.fly.zapper_splat && w.fly.splatted);
        assert_eq!(w.fly.damage_total, 101);
        assert!(!rig.events.contains(&Event::play(SoundSlot::ZAPPER_GRILL)));
    }

    /// answers.md "Banner: the whole world update runs, paused": traps still move with the
    /// camera, but the flypaper, the fans and the zappers leave the fly alone and draw nothing.
    #[test]
    fn paused_traps_move_but_leave_the_fly_alone() {
        let mut rig = Rig::new(7);
        let slots = vec![slot(TrapKind::Flypaper, 1000), slot(TrapKind::Zapper, 1000)];
        let mut w = world_with(slots, &mut rig);
        let mut rig = Rig::new(1);
        (w.fly.x, w.fly.y, w.fly.vx, w.fly.vy) = (1000, 86, 300, -140);
        (w.paused, w.camera.delta) = (true, 10.0);
        let x = w.traps.zappers[0].x;
        w.traps_step(&mut rig.at(1));
        assert_eq!((w.fly.vx, w.fly.vy, w.fly.damage_total), (300, -140, 0));
        assert_eq!(w.traps.zappers[0].x, x - 10);
        assert_eq!(rig.rng.rand(), 41);
    }

    /// answers.md "Trap loop volumes": two fans, one on screen, start at −100; both off screen
    /// (screen 2000 and 3000) set −400 once. Two zappers off screen start at −500; one at screen
    /// 4500 (4500 + 635 ≥ 5000) sets −200. A single zapper stays at −450 with no volume change.
    #[test]
    fn several_traps_set_the_loop_volume_from_visibility() {
        let fans = vec![slot(TrapKind::FanLeft, 500), slot(TrapKind::FanLeft, 3000)];
        let zappers = vec![slot(TrapKind::Zapper, 2000), slot(TrapKind::Zapper, 3000)];
        let mut rig = Rig::new(1);
        let mut w = world_with([fans, zappers].concat(), &mut rig);
        let looped = |cb| {
            SoundOp::Loop(PlayParams {
                volume_cb: cb,
                ..PlayParams::default()
            })
        };
        assert_eq!(sounds(&rig, SoundSlot::FAN), [looped(-100)]);
        assert_eq!(sounds(&rig, SoundSlot::ZAPPER_HUM), [looped(-500)]);
        let mut rig = Rig::new(1);
        w.traps.fans[0].x = 1600;
        w.traps.zappers[0].x = 3600;
        w.traps_step(&mut rig.at(1));
        w.traps_step(&mut rig.at(2));
        let set = |slot| -> Vec<SoundOp> {
            let ops = sounds(&rig, slot);
            ops.into_iter()
                .filter(|op| matches!(op, SoundOp::SetVolume(_)))
                .collect()
        };
        assert_eq!(set(SoundSlot::FAN), [SoundOp::SetVolume(-400)]);
        assert_eq!(set(SoundSlot::ZAPPER_HUM), [SoundOp::SetVolume(-200)]);
        let mut rig = Rig::new(1);
        let mut w = world_with(vec![slot(TrapKind::Zapper, 2000)], &mut rig);
        assert_eq!(sounds(&rig, SoundSlot::ZAPPER_HUM), [looped(-450)]);
        let mut rig = Rig::new(1);
        w.traps_step(&mut rig.at(1));
        let set = sounds(&rig, SoundSlot::ZAPPER_HUM);
        assert!(!set.iter().any(|op| matches!(op, SoundOp::SetVolume(_))));
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
    /// (0..5000). The stored x goes straight to the screen x (answers.md "Fan stands, fan y and
    /// trap x"), so a fan, its stand and a winch at the same stored x meet exactly on screen.
    #[test]
    fn a_fan_and_a_winch_at_the_same_stored_x_share_a_screen_x() {
        let w = world_with_fan(1);
        assert_eq!((w.lift.x, w.traps.fans[0].screen_x), (STORED, STORED));
        let stand = w.objects.list.iter().find(|o| o.sprite == "piedventi_gc");
        let stand = stand.map(|o| (o.screen_x, o.layer_x, o.w, o.h, o.y, o.collectible));
        // Layer trunc(1618 / 1.25) = 1294; the fan frame 666 / 3 × 300 at y 570 − 300.
        assert_eq!(stand, Some((STORED, 1294, 222, 300, 270, false)));
    }

    /// FW §5.2 step 3: placement passes screen-unit x to `span_free`, so no placed object
    /// overlaps the fan's span (its 222 frame width, answers.md O22) on any seed.
    #[test]
    fn no_placed_object_overlaps_a_fan_span() {
        for seed in 1..=SEEDS {
            let w = world_with_fan(seed);
            let fan_w = w.traps.fans[0].w;
            let fx = w.traps.fans[0].screen_x;
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
