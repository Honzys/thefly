//! W3. The goodies shop (0x1e) with payout, purchases and the side switch, and the farmer's traps
//! shop (0x28) (ai-and-rules §7.2, §7.3, §4.2, §4.3; screens-and-rendering §3.1, §3.2;
//! answers.md "Goodies shop: purchase and tally are motion-driven", "Goodies shop side order and
//! exit", "Traps shop: carrying a trap", "Shop tally: target, food sizes and machine shake",
//! "Shop tally: kill count drops one per cycle").

use crate::bourse::Bourse;
use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::economy::{Economy, ITEMS, Item, KILL_VALUE};
use crate::event::{Event, Music, SoundSlot};
use crate::input::{KeyState, Scancode};
use crate::match_state::{MatchState, PlaceError, Players};
use crate::mover::{FpsRun, Wander};
use crate::screen::{FADE_MS, Fade, Flow, MatchFlow, MatchScreen};
use crate::shop_ai::{GoodiesShopAi, Press, TrapAction, TrapsShopAi};
use crate::world::lift::{ANCHOR_GAP, draw_anchors};
use crate::world::objects::{EXTREME_TYPES, NORMAL_TYPES, ObjectType};
use crate::world::traps::TrapKind;

/// Item buttons 0..=4, then "done".
pub const DONE_BUTTON: usize = 5;
pub const BUTTONS: usize = 6;
/// The item buttons' x (screens-and-rendering §3.1); a bought item drops from there.
pub const BUTTON_X: [i32; 5] = [44, 179, 309, 440, 570];
/// `FUN_00423047`: the drop starts at y 5 at 650 px/s, 10 px per tick, and is granted once
/// past y 541, where it snaps to 542.
pub const DROP_START_Y: i32 = 5;
pub const DROP_VY: i32 = 650;
pub const DROP_GRANT_Y: i32 = 541;
/// Traps shop: the done button, the carried trap's start x and its speed cap.
pub const TRAPS_DONE: usize = 3;
pub const TRAPS_BUTTONS: usize = 4;
pub const CARRY_START_X: i32 = 350;
pub const CARRY_MAX_SPEED: i32 = 10;
/// The carried trap's x stays in [20, 780 − w].
const CARRY_MIN_X: i32 = 20;
const CARRY_RIGHT: i32 = 780;

/// `FUN_0042322f`: a tally sprite enters at x 805 with its bottom at y 239, falls once its right
/// edge passes x 372 (+50 px/s per tick), and pays when it reaches the target's y.
pub const TALLY_START_X: i32 = 805;
pub const TALLY_BOTTOM: i32 = 239;
pub const DEAD_FLY_VX: i32 = -500;
pub const FOOD_VX: i32 = -250;
const FALL_X: i32 = 372;
const FALL_ACCEL: i32 = 50;
/// Sound 10 plays while 0 < vy < this.
const FALL_SOUND_VY: i32 = 80;
/// A food sprite stops when its centre passes x 588 and shows "x N" for 2000 ms.
const LABEL_X: i32 = 588;
pub const LABEL_MS: u64 = 2000;
/// `g_flymort` frames are 100×75.
pub const DEAD_FLY_SIZE: (i32, i32) = (100, 75);
const DEAD_FLY_FRAMES: i32 = 3;
/// A food image is scaled uniformly to fit 200 × 150: by 150 / h when h > 150, overridden by
/// 200 / w when w > 200 and w > h.
const FOOD_FIT: (i32, i32) = (200, 150);

/// `g_Machine` rests at (7, 143); `g_Machine_cache`, the target the tally falls onto, sits at
/// machine + (46, 156). Each landing shakes the machine for 4000 ms under the wander rule
/// (reroll 20 ms, speeds 140 / 150, runs 2 / 4, accel 5) with x ≤ 11 and y in 139..=147; the
/// left bound 3 is never applied (answers.md "Shop machine shake: only after a landing").
pub const MACHINE_REST: (i32, i32) = (7, 143);
pub const TARGET_OFFSET: (i32, i32) = (46, 156);
pub const SHAKE_MS: u64 = 4000;
const SHAKE_RIGHT_X: i32 = 11;
const SHAKE_Y: (i32, i32) = (139, 147);
const SHAKE_REROLL_MS: i32 = 20;
const SHAKE_SPEED: (i32, i32) = (140, 150);
const SHAKE_RUNS: (i32, i32) = (2, 4);
const SHAKE_ACCEL: (i32, i32) = (5, 5);

/// The shaking machine (+0xac8); its wander is on only while `shake` runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    pub x: i32,
    pub y: i32,
    /// Armed by each landing (+0x24a0); the dust effect (+0xca0) starts with it.
    pub shake: Timer,
    wander: Wander,
}

impl Default for Machine {
    fn default() -> Self {
        Self {
            x: MACHINE_REST.0,
            y: MACHINE_REST.1,
            shake: Timer::UNARMED,
            wander: Wander::new(SHAKE_REROLL_MS, SHAKE_SPEED, SHAKE_RUNS, SHAKE_ACCEL),
        }
    }
}

impl Machine {
    /// The target's top y: 299 at rest, 295..=303 while shaking.
    pub fn target_y(&self) -> i32 {
        self.y + TARGET_OFFSET.1
    }

    /// A tally landing (re-)arms the 4000 ms shake.
    fn land(&mut self, now: u64) {
        self.shake.arm(now, SHAKE_MS);
    }

    /// Wanders (drawing rand) only while the shake runs; at its end, back to rest with v 0.
    fn step(&mut self, cx: &mut Ctx) {
        if !self.shake.is_armed() {
            return;
        }
        if self.shake.expired(cx.now) {
            self.shake.disarm();
            (self.x, self.y) = MACHINE_REST;
            self.wander.v = [0, 0];
            return;
        }
        let (dx, dy) = self.wander.step(cx);
        self.x = (self.x + dx).min(SHAKE_RIGHT_X);
        self.y = (self.y + dy).clamp(SHAKE_Y.0, SHAKE_Y.1);
    }
}

/// The tally size of a food image of `w` × `h`: s is a float32, each side trunc(side × s) with
/// the product in extended precision.
pub fn tally_size(w: i32, h: i32) -> (i32, i32) {
    let (fit_w, fit_h) = FOOD_FIT;
    let mut s = 1.0_f32;
    if h > fit_h {
        s = fit_h as f32 / h as f32;
    }
    if w > fit_w && w > h {
        s = fit_w as f32 / w as f32;
    }
    let side = |n: i32| (f64::from(n) * f64::from(s)).trunc() as i32;
    (side(w), side(h))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopSide {
    Farmer,
    Fly,
}

/// A sprite moved by the shared FPS integrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mover {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    run: FpsRun,
}

impl Mover {
    fn at(x: i32, y: i32, vx: i32) -> Self {
        Self {
            x,
            y,
            vx,
            ..Self::default()
        }
    }

    fn step(&mut self) {
        let (dx, dy) = self.run.step([self.vx, self.vy]);
        self.x += dx;
        self.y += dy;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TallyItem {
    pub value: i32,
    /// The food type (fly), or the dead-fly frame 1..=3 drawn at its cycle start (farmer).
    pub label: usize,
    /// The saved count shown as "x N" (fly); 1 per kill.
    pub count: i32,
    /// The sprite's drawn size: [`DEAD_FLY_SIZE`], or the food image fitted by [`tally_size`].
    pub size: (i32, i32),
}

/// What one tally tick did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TallyStep {
    /// A sprite landed and was paid.
    pub landed: bool,
    /// Nothing is left to pay.
    pub done: bool,
}

/// The payout tally: one sprite per kill or saved type, paying when it lands; Enter skips.
#[derive(Debug, Clone, PartialEq)]
pub struct Tally {
    pub side: ShopSide,
    /// Still to credit; the first one is moving.
    pub pending: Vec<TallyItem>,
    pub sprite: Mover,
    /// The "x N" pause (fly), armed when the sprite stops.
    pub label: Timer,
    /// The money shown; each landing credits the economy and this alike.
    pub shown: i32,
}

impl Tally {
    fn new(side: ShopSide, pending: Vec<TallyItem>, before: i32, cx: &mut Ctx) -> Self {
        let mut t = Self {
            side,
            pending,
            sprite: Mover::default(),
            label: Timer::UNARMED,
            shown: before,
        };
        t.start(cx);
        t
    }

    fn start(&mut self, cx: &mut Ctx) {
        let Some(item) = self.pending.first_mut() else {
            return;
        };
        let vx = if self.side == ShopSide::Farmer {
            item.label = (cx.rng.r(DEAD_FLY_FRAMES) + 1) as usize;
            DEAD_FLY_VX
        } else {
            FOOD_VX
        };
        self.sprite = Mover::at(TALLY_START_X, TALLY_BOTTOM - item.size.1, vx);
        self.label = Timer::UNARMED;
    }

    /// `FUN_0042322f`: the sprite lands on the first tick its top y ≥ `target_y` once it is past
    /// x 372, and pays that cycle (a kill also drops the kill count by one). Skip pays the rest.
    fn tick(&mut self, skip: bool, target_y: i32, e: &mut Economy, cx: &mut Ctx) -> TallyStep {
        if skip {
            while !self.pending.is_empty() {
                self.pay(e);
            }
        }
        let Some(&item) = self.pending.first() else {
            return TallyStep {
                landed: false,
                done: true,
            };
        };
        let w = item.size.0;
        let s = &mut self.sprite;
        s.step();
        if self.side == ShopSide::Fly && !self.label.is_armed() && s.x + w / 2 < LABEL_X {
            s.vx = 0;
            self.label.arm(cx.now, LABEL_MS);
        }
        // [I] after the label the food moves on like a dead fly ("the same" in answers.md).
        if s.vx == 0 && self.label.expired(cx.now) {
            s.vx = FOOD_VX;
        }
        let moving = TallyStep {
            landed: false,
            done: false,
        };
        if s.x + w >= FALL_X {
            return moving;
        }
        s.vy += FALL_ACCEL;
        if s.vy < FALL_SOUND_VY {
            cx.emit(Event::play(SoundSlot::SHOP_MACHINE));
        }
        if s.y < target_y {
            return moving;
        }
        self.pay(e);
        self.start(cx);
        TallyStep {
            landed: true,
            done: self.pending.is_empty(),
        }
    }

    /// Credits the first pending item.
    fn pay(&mut self, e: &mut Economy) {
        let item = self.pending.remove(0);
        self.shown += item.value;
        match self.side {
            ShopSide::Farmer => e.pay_kill(),
            ShopSide::Fly => e.pay_saved(item.value),
        }
    }
}

/// A bought item dropping out of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemDrop {
    pub item: Item,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoodiesShop {
    /// data+0x22fc; the fly shops first, then the farmer.
    pub side: ShopSide,
    pub cursor: usize,
    pub tally: Option<Tally>,
    /// The bought item dropping; granted once it passes y 541.
    pub conveyor: Option<ItemDrop>,
    /// A press made during a drop (the button), run on the first tick after it.
    pub latched: Option<usize>,
    /// One-player games, created on the farmer side.
    pub ai: Option<GoodiesShopAi>,
    pub fade: Fade,
    pub leaving: bool,
    pub machine: Machine,
}

impl GoodiesShop {
    /// Opens on the fly side with the fly's payout tally.
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        cx.emit(Event::Music(Music::Goodies));
        let types: &[ObjectType] = if m.config.extreme() {
            &EXTREME_TYPES
        } else {
            &NORMAL_TYPES
        };
        let e = &mut m.economy;
        let before = e.fly.money;
        let saved: Vec<(usize, i32)> = (0..e.saved.len())
            .filter_map(|t| e.saved[t].filter(|&n| n > 0).map(|n| (t, n)))
            .collect();
        // fly_payout returns the non-zero lines in type order: one per type with a saved count.
        let items: Vec<TallyItem> = e
            .fly_payout(cx)
            .into_iter()
            .zip(saved)
            .map(|(value, (label, count))| TallyItem {
                value,
                label,
                count,
                size: types.get(label).map_or(DEAD_FLY_SIZE, |t| {
                    let image = cx.data.size(t.name);
                    tally_size(image.w, image.h)
                }),
            })
            .collect();
        let tally = (!items.is_empty()).then(|| Tally::new(ShopSide::Fly, items, before, cx));
        Self {
            side: ShopSide::Fly,
            cursor: 0,
            tally,
            conveyor: None,
            latched: None,
            ai: None,
            fade: Fade::new(false, FADE_MS),
            leaving: false,
            machine: Machine::default(),
        }
    }

    /// Fly done → farmer; farmer done → traps shop, or the bourse without tokens; Esc → quit
    /// dialog (game-spec §2.2). The machine steps after the tally, so the tally compares against
    /// the previous tick's target.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let flow = self.update(keys, m, cx);
        self.machine.step(cx);
        flow
    }

    fn update(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let faded = self.fade.step();
        if self.leaving {
            if !faded {
                return MatchFlow::Stay;
            }
            if self.side == ShopSide::Farmer {
                return MatchFlow::Next(after_goodies(m, cx));
            }
            self.switch_to_farmer(m, cx);
            return MatchFlow::Stay;
        }
        if keys.pressed(Scancode::ESCAPE) {
            return MatchFlow::Leave(Flow::Quit);
        }
        if let Some(t) = &mut self.tally {
            let target_y = self.machine.target_y();
            let step = t.tick(keys.pressed(Scancode::RETURN), target_y, &mut m.economy, cx);
            if step.landed {
                self.machine.land(cx.now);
            }
            if step.done {
                self.tally = None;
                if let Some(ai) = self.ai.as_mut() {
                    ai.unpause(cx.now);
                }
            }
            return MatchFlow::Stay;
        }
        let press = self.input(keys, cx);
        if let Some(d) = &mut self.conveyor {
            d.y += DROP_VY / FPS;
            if d.y > DROP_GRANT_Y {
                m.economy.grant(d.item);
                self.conveyor = None;
            }
            self.latched = press.or(self.latched);
            return MatchFlow::Stay;
        }
        let latched = self.latched.take();
        if let Some(button) = press.or(latched) {
            self.confirm(button, m, cx);
        }
        MatchFlow::Stay
    }

    /// The pressed button, if any; the cursor moves either way.
    fn input(&mut self, keys: &KeyState, cx: &mut Ctx) -> Option<usize> {
        if let Some(ai) = self.ai.as_mut() {
            let press = ai.tick(cx);
            return self.ai_press(press);
        }
        let (left, right, ok) = match self.side {
            ShopSide::Farmer => (Scancode::F, Scancode::H, Scancode::TAB),
            ShopSide::Fly => (Scancode::LEFT, Scancode::RIGHT, Scancode::RETURN),
        };
        if keys.pressed(left) {
            self.cursor = (self.cursor + BUTTONS - 1) % BUTTONS;
        }
        if keys.pressed(right) {
            self.cursor = (self.cursor + 1) % BUTTONS;
        }
        keys.pressed(ok).then_some(self.cursor)
    }

    fn ai_press(&mut self, press: Option<Press>) -> Option<usize> {
        let (cursor, pressed) = match press? {
            Press::Button(b) => (b, true),
            Press::SelectDone => (DONE_BUTTON, false),
            Press::Done => (DONE_BUTTON, true),
        };
        self.cursor = cursor;
        pressed.then_some(cursor)
    }

    fn confirm(&mut self, button: usize, m: &mut MatchState, cx: &mut Ctx) {
        let Some(&x) = BUTTON_X.get(button) else {
            return self.done(m, cx);
        };
        let first = if self.side == ShopSide::Farmer { 0 } else { 5 };
        let item = ITEMS[first + button].item;
        match m.economy.buy(item) {
            Ok(()) => {
                cx.emit(Event::play(SoundSlot::MONEY));
                self.conveyor = Some(ItemDrop {
                    item,
                    x,
                    y: DROP_START_Y,
                });
            }
            Err(_) => cx.emit(Event::play(SoundSlot::ERROR)),
        }
    }

    /// `FUN_00421e6a`: done fades out. Without trap tokens the farmer's press re-rolls the winch
    /// and clears the layout at once.
    fn done(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        self.leaving = true;
        self.fade = Fade::new(true, FADE_MS);
        if self.side == ShopSide::Farmer && m.economy.farmer.tokens < 1 {
            m.traps.slots.clear();
            m.winch = draw_anchors(ANCHOR_GAP, cx);
        }
    }

    /// `FUN_00421b51`: the farmer's side, with the AI plan (one player) and the kill tally.
    fn switch_to_farmer(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        self.side = ShopSide::Farmer;
        self.cursor = 0;
        self.leaving = false;
        self.fade = Fade::new(false, FADE_MS);
        // AR §3.1 step 1: the plan counts the payout before it is paid.
        self.ai = (m.config.players == Players::One)
            .then(|| GoodiesShopAi::new(m.config.ai_level, m, cx));
        // Each landing pays one kill and lowers the count (answers.md "Shop tally: kill count
        // drops one per cycle").
        let e = &m.economy;
        let kills = e.kills;
        let before = e.farmer.money;
        let kill = TallyItem {
            value: KILL_VALUE,
            label: 0,
            count: 1,
            size: DEAD_FLY_SIZE,
        };
        let items = vec![kill; usize::try_from(kills).unwrap_or(0)];
        self.tally = (kills > 0).then(|| Tally::new(ShopSide::Farmer, items, before, cx));
        if let Some(ai) = self.ai.as_mut() {
            ai.paused = self.tally.is_some();
        }
    }
}

/// AR §4.2: the traps shop when the farmer has tokens, else the bourse.
fn after_goodies(m: &mut MatchState, cx: &mut Ctx) -> MatchScreen {
    if m.economy.farmer.tokens >= 1 {
        return MatchScreen::TrapsShop(Box::new(TrapsShop::new(m, cx)));
    }
    MatchScreen::Bourse(Box::new(Bourse::new(m, cx)))
}
#[derive(Debug, Clone, PartialEq)]
pub struct TrapsShop {
    /// Buttons: flypaper, fan (left/right by placement), zapper, done.
    pub cursor: usize,
    /// The trap being positioned and its shop x.
    pub carrying: Option<(TrapKind, i32)>,
    /// The carried trap's speed, px per tick.
    pub speed: i32,
    pub ai: Option<TrapsShopAi>,
    pub fade: Fade,
    pub leaving: bool,
}

const BUTTON_KIND: [TrapKind; 3] = [TrapKind::Flypaper, TrapKind::FanLeft, TrapKind::Zapper];
const WINCH_ICON_GAP: i32 = 36;

impl TrapsShop {
    /// Clears the trap slots on open (§4.2).
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        m.traps.slots.clear();
        let gap = cx
            .data
            .sizes
            .get("p_icone_treuil")
            .map_or(WINCH_ICON_GAP, |s| s.w);
        m.winch = draw_anchors(gap, cx);
        let ai =
            (m.config.players == Players::One).then(|| TrapsShopAi::new(m.config.ai_level, m, cx));
        Self {
            cursor: 0,
            carrying: None,
            speed: 0,
            ai,
            fade: Fade::new(false, FADE_MS),
            leaving: false,
        }
    }

    /// Result 1 → bourse.
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let faded = self.fade.step();
        if self.leaving {
            return if faded {
                MatchFlow::Next(MatchScreen::Bourse(Box::new(Bourse::new(m, cx))))
            } else {
                MatchFlow::Stay
            };
        }
        if keys.pressed(Scancode::ESCAPE) {
            return MatchFlow::Leave(Flow::Quit);
        }
        match self.ai.as_mut().map(|ai| ai.tick(cx)) {
            Some(action) => self.ai_action(action, m, cx),
            None if self.carrying.is_some() => self.carry(keys, m, cx),
            None => self.choose(keys, m, cx),
        }
        MatchFlow::Stay
    }

    fn ai_action(&mut self, action: Option<TrapAction>, m: &mut MatchState, cx: &mut Ctx) {
        match action {
            None => {}
            Some(TrapAction::Select(b)) => self.cursor = b,
            Some(TrapAction::Press) => {}
            Some(TrapAction::Place(slot)) => {
                let tokens = &mut m.economy.farmer.tokens;
                *tokens = (*tokens - slot.kind.spec().cost).max(0);
                m.traps.slots.push(slot);
                cx.emit(Event::play(SoundSlot::TRAP_BUILD));
            }
            Some(TrapAction::Done) => self.leave(),
        }
    }

    fn leave(&mut self) {
        self.cursor = TRAPS_DONE;
        self.leaving = true;
        self.fade = Fade::new(true, FADE_MS);
    }

    fn choose(&mut self, keys: &KeyState, m: &MatchState, cx: &mut Ctx) {
        if keys.pressed(Scancode::F) || keys.pressed(Scancode::T) {
            self.cursor = (self.cursor + TRAPS_BUTTONS - 1) % TRAPS_BUTTONS;
        }
        if keys.pressed(Scancode::H) || keys.pressed(Scancode::G) {
            self.cursor = (self.cursor + 1) % TRAPS_BUTTONS;
        }
        if !keys.pressed(Scancode::TAB) {
            return;
        }
        let Some(&kind) = BUTTON_KIND.get(self.cursor) else {
            return self.leave();
        };
        let spec = kind.spec();
        if m.economy.farmer.tokens < spec.cost || m.traps.count(kind) >= spec.cap {
            return cx.emit(Event::play(SoundSlot::ERROR));
        }
        self.carrying = Some((kind, CARRY_START_X));
        self.speed = 0;
    }

    fn carry(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) {
        let Some((kind, x)) = self.carrying else {
            return;
        };
        let fan = matches!(kind, TrapKind::FanLeft | TrapKind::FanRight);
        // `FUN_0042570b`: left wins when both are held; a reversal decelerates through 0.
        let (speed, kind) = if keys.held(Scancode::F) {
            let kind = if fan { TrapKind::FanLeft } else { kind };
            ((self.speed - 1).max(-CARRY_MAX_SPEED), kind)
        } else if keys.held(Scancode::H) {
            let kind = if fan { TrapKind::FanRight } else { kind };
            ((self.speed + 1).min(CARRY_MAX_SPEED), kind)
        } else {
            (0, kind)
        };
        self.speed = speed;
        let x = carry_clamp(x + speed, kind.spec().width);
        self.carrying = Some((kind, x));
        if !keys.pressed(Scancode::TAB) {
            return;
        }
        let winch = m.winch;
        match m.traps.place(kind, x, &winch, &mut m.economy.farmer.tokens) {
            Ok(()) => {
                self.carrying = None;
                cx.emit(Event::play(SoundSlot::TRAP_BUILD));
            }
            Err(PlaceError::Blocked) => cx.emit(Event::play(SoundSlot::ERROR)),
            Err(PlaceError::Refused) => {
                self.carrying = None;
                cx.emit(Event::play(SoundSlot::ERROR));
            }
        }
    }
}

/// The range is [20, 780 − w]: past the right edge snaps to 780 − w, below 21 to 20.
fn carry_clamp(x: i32, w: i32) -> i32 {
    if x + w >= CARRY_RIGHT {
        CARRY_RIGHT - w
    } else if x < CARRY_MIN_X + 1 {
        CARRY_MIN_X
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::Setup;
    use crate::match_state::MatchConfig;
    use crate::rng::MsvcRand;
    use crate::screen::rig::Rig;

    fn two_player(rig: &mut Rig, money: i32, kills: i32) -> MatchState {
        let config = MatchConfig {
            players: Players::Two,
            ..MatchConfig::default()
        };
        let mut m = rig.cx(|cx| {
            let mut m = MatchState::new(config, false, cx);
            m.advance(cx);
            m
        });
        m.economy.fly.money = money;
        m.economy.farmer.money = money;
        m.economy.kills = kills;
        m
    }

    fn step(rig: &mut Rig, shop: &mut GoodiesShop, m: &mut MatchState, keys: &[Scancode]) {
        rig.step(keys, |k, cx| shop.tick(k, m, cx));
    }

    /// Fly done, then the fade to the farmer's side.
    fn to_farmer_side(rig: &mut Rig, shop: &mut GoodiesShop, m: &mut MatchState) {
        shop.cursor = DONE_BUTTON;
        step(rig, shop, m, &[Scancode::RETURN]);
        for _ in 0..100 {
            step(rig, shop, m, &[]);
            if shop.side == ShopSide::Farmer {
                return;
            }
        }
    }

    /// answers.md "Goodies shop side order and exit": the fly shops first, with its tally; done
    /// fades to the farmer's side.
    #[test]
    fn the_shop_opens_on_the_fly_side_then_fades_to_the_farmer() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 0, 0);
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        assert_eq!(shop.side, ShopSide::Fly);
        to_farmer_side(&mut rig, &mut shop, &mut m);
        assert_eq!(
            (shop.side, shop.leaving, shop.cursor),
            (ShopSide::Farmer, false, 0)
        );
    }

    /// answers.md "purchase and tally are motion-driven": the item drops 10 px per tick from
    /// y 5 and is granted once past y 541: 5 + 10 × 54 = 545, on the 54th tick after the press.
    #[test]
    fn a_purchase_drops_for_54_ticks_then_grants() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 1000, 0);
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        let level = m.economy.owned(Item::FlyStrength);
        step(&mut rig, &mut shop, &mut m, &[Scancode::RETURN]);
        assert!(rig.events.contains(&Event::play(SoundSlot::MONEY)));
        assert_eq!(shop.conveyor.map(|d| (d.x, d.y)), Some((44, 5)));
        for _ in 0..53 {
            step(&mut rig, &mut shop, &mut m, &[]);
        }
        assert_eq!(shop.conveyor.map(|d| d.y), Some(535));
        assert_eq!(m.economy.owned(Item::FlyStrength), level);
        step(&mut rig, &mut shop, &mut m, &[]);
        assert_eq!(shop.conveyor, None);
        assert_eq!(m.economy.owned(Item::FlyStrength), level + 1);
    }

    /// A press during the drop is latched (the later one wins) and runs the tick after the grant.
    #[test]
    fn a_press_during_the_drop_runs_the_tick_after_the_grant() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 10_000, 0);
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        for keys in [
            [Scancode::RETURN],
            [Scancode::F],
            [Scancode::RETURN],
            [Scancode::RIGHT],
        ] {
            step(&mut rig, &mut shop, &mut m, &keys);
        }
        assert_eq!(shop.latched, Some(0));
        step(&mut rig, &mut shop, &mut m, &[Scancode::RETURN]);
        assert_eq!(shop.latched, Some(1));
        for _ in 0..50 {
            step(&mut rig, &mut shop, &mut m, &[]);
        }
        assert!(shop.conveyor.is_none());
        let before = rig.events.len();
        step(&mut rig, &mut shop, &mut m, &[]);
        assert!(rig.events[before..].contains(&Event::play(SoundSlot::MONEY)));
        assert_eq!(shop.conveyor.map(|d| d.x), Some(179));
        assert_eq!(shop.latched, None);
    }

    #[test]
    fn a_purchase_without_the_money_plays_slot_12() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 10, 0);
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        step(&mut rig, &mut shop, &mut m, &[Scancode::RETURN]);
        assert!(rig.events.contains(&Event::play(SoundSlot::ERROR)));
        assert!(shop.conveyor.is_none());
    }

    fn kill() -> TallyItem {
        TallyItem {
            value: KILL_VALUE,
            label: 0,
            count: 1,
            size: DEAD_FLY_SIZE,
        }
    }

    /// Seed 1: the dead-fly frames are 41 % 3 + 1 = 3, then 18467 % 3 + 1 = 3. At −500 px/s the
    /// sprite covers 500 px in 65 ticks (x 305), then 7, 8, 8, 7, 8 → x 267 on tick 70, past
    /// 372 − 100: it falls (sound 10 once, at vy 50). From y 164 the drop on fall tick k is
    /// trunc(50k / 65): Σ 64 after 13, 123 after 18 (y 287) and 137 after 19 (y 301 ≥ the
    /// target's 299), so it lands on tick 89 and pays one kill.
    #[test]
    fn a_dead_fly_pays_42_when_it_lands_on_the_target() {
        let mut rig = Rig::new(1);
        let mut e = Economy::new(Setup::Normal, false);
        e.kills = 2;
        let mut t = rig.cx(|cx| Tally::new(ShopSide::Farmer, vec![kill(); 2], 0, cx));
        assert_eq!(t.pending[0].label, 3);
        let target = Machine::default().target_y();
        assert_eq!(target, 299);
        for tick in 1..=89 {
            let before = rig.events.len();
            let step = rig.step(&[], |_, cx| t.tick(false, target, &mut e, cx));
            let rang = rig.events[before..].contains(&Event::play(SoundSlot::SHOP_MACHINE));
            assert_eq!(rang, tick == 70, "tick {tick}");
            assert_eq!(step.landed, tick == 89, "tick {tick}");
            assert!(!step.done);
            if tick == 70 {
                assert_eq!((t.sprite.x, t.sprite.y, t.sprite.vy), (267, 164, 50));
            }
            let paid = if tick < 89 { 0 } else { 42 };
            assert_eq!((t.shown, e.farmer.money), (paid, paid), "tick {tick}");
        }
        assert_eq!((t.pending.len(), t.pending[0].label, e.kills), (1, 3, 1));
        assert_eq!(t.sprite, Mover::at(805, 164, -500));
        let mut probe = MsvcRand::new(1);
        probe.rand();
        probe.rand();
        assert_eq!(rig.rng, probe);
    }

    /// The landing test runs only once x + w < 372: a sprite already below the target but still
    /// right of that line does not land.
    #[test]
    fn the_landing_waits_for_the_fall_line() {
        let mut rig = Rig::new(1);
        let mut e = Economy::new(Setup::Normal, false);
        let mut t = rig.cx(|cx| Tally::new(ShopSide::Farmer, vec![kill()], 0, cx));
        t.sprite.y = 400;
        let step = rig.step(&[], |_, cx| t.tick(false, 299, &mut e, cx));
        assert!(!step.landed);
        t.sprite.x = 200;
        let step = rig.step(&[], |_, cx| t.tick(false, 299, &mut e, cx));
        assert_eq!((step.landed, step.done), (true, true));
    }

    /// A food sprite stops when its centre passes x 588 and shows "x N" for 2000 ms.
    #[test]
    fn a_food_sprite_stops_for_its_label() {
        let mut rig = Rig::new(1);
        let mut e = Economy::new(Setup::Normal, false);
        let food = TallyItem {
            value: 90,
            label: 4,
            count: 3,
            size: (100, 75),
        };
        let mut t = rig.cx(|cx| Tally::new(ShopSide::Fly, vec![food], 0, cx));
        for _ in 0..200 {
            rig.step(&[], |_, cx| t.tick(false, 299, &mut e, cx));
            if t.label.is_armed() {
                break;
            }
        }
        let stop = (t.sprite.x, rig.now());
        assert!(stop.0 + 50 < 588 && stop.0 + 50 >= 588 - 4, "{stop:?}");
        while rig.now() < stop.1 + LABEL_MS - 20 {
            rig.step(&[], |_, cx| t.tick(false, 299, &mut e, cx));
        }
        assert_eq!(t.sprite.x, stop.0);
        for _ in 0..300 {
            if rig.step(&[], |_, cx| t.tick(false, 299, &mut e, cx)).done {
                break;
            }
        }
        assert_eq!((t.shown, e.fly.money, e.fly.score), (90, 90, 90));
    }

    /// answers.md "Shop tally: target, food sizes and machine shake": the table rows, scaled to
    /// fit 200 × 150 with a float32 scale.
    #[test]
    fn food_sprites_are_scaled_to_fit_200_by_150() {
        let rows = [
            ((152, 217), (105, 150)),
            ((220, 135), (200, 122)),
            ((148, 107), (148, 107)),
            ((160, 48), (160, 48)),
            ((160, 49), (160, 49)),
            ((180, 43), (180, 43)),
            ((340, 157), (200, 92)),
            ((140, 60), (140, 60)),
            ((107, 53), (107, 53)),
            ((140, 160), (131, 150)),
            ((209, 300), (104, 150)),
            ((160, 132), (160, 132)),
            ((201, 182), (199, 181)),
            ((100, 50), (100, 50)),
            ((468, 270), (200, 115)),
            ((240, 76), (199, 63)),
            ((280, 109), (200, 77)),
            ((118, 171), (103, 149)),
            ((180, 165), (163, 150)),
            ((164, 154), (159, 149)),
            ((134, 61), (134, 61)),
            ((430, 130), (200, 60)),
        ];
        for ((w, h), want) in rows {
            assert_eq!(tally_size(w, h), want, "{w}×{h}");
        }
    }

    /// Seed 1, from rest: x timer 41 % 20, d 18467 % 10 = 7 (+1), 6334 % 140 + 65 = 99; y timer
    /// 26500 % 20, d 9 (+1), 15724 % 150 + 65 = 189; accel 5 → (104, 194), 1 and 2 px: (8, 145).
    /// Unshaken, the machine draws nothing; past 4000 ms it snaps back to (7, 143) with v 0.
    #[test]
    fn the_machine_shakes_only_for_4000_ms_after_a_landing() {
        let mut rig = Rig::new(1);
        let mut machine = Machine::default();
        rig.cx(|cx| machine.step(cx));
        assert_eq!(
            (machine.x, machine.y, rig.rng.clone()),
            (7, 143, MsvcRand::new(1))
        );
        let now = rig.now();
        machine.land(now);
        rig.cx(|cx| machine.step(cx));
        assert_eq!(
            (machine.x, machine.y, machine.wander.v),
            (8, 145, [104, 194])
        );
        assert_eq!(machine.target_y(), 301);
        let mut probe = MsvcRand::new(1);
        (0..6).for_each(|_| {
            probe.rand();
        });
        assert_eq!(rig.rng, probe);
        for _ in 0..1000 {
            rig.step(&[], |_, cx| machine.step(cx));
            assert!(machine.x <= 11 && (295..=303).contains(&machine.target_y()));
            if !machine.shake.is_armed() {
                break;
            }
        }
        assert!(rig.now() > now + SHAKE_MS);
        assert_eq!((machine.x, machine.y, machine.wander.v), (7, 143, [0, 0]));
        let still = rig.rng.clone();
        rig.step(&[], |_, cx| machine.step(cx));
        assert_eq!(rig.rng, still);
    }

    /// answers.md "Shop machine shake": the left bound 3 is never applied. Seed 1 with 41 spent:
    /// x timer 18467, d 6334 % 10 = 4 from v 0 (−1), 26500 % 140 + 65 = 105; y timer 19169, d 4
    /// (−1), 11478 % 150 + 65 = 143; accel → (−110, −148), 1 and 2 px: x 3 → 2.
    #[test]
    fn the_machine_left_bound_is_never_applied() {
        let mut rig = Rig::new(1);
        rig.rng.rand();
        let mut machine = Machine {
            x: 3,
            ..Machine::default()
        };
        machine.land(rig.now());
        rig.cx(|cx| machine.step(cx));
        assert_eq!(
            (machine.x, machine.y, machine.wander.v),
            (2, 141, [-110, -148])
        );
    }

    /// answers.md "Shop tally: kill count drops one per cycle": through the shop, each landing
    /// pays 42 and lowers the kill count by one, and shakes the machine.
    #[test]
    fn the_kill_tally_pays_and_counts_down_per_cycle() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 0, 3);
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        to_farmer_side(&mut rig, &mut shop, &mut m);
        assert_eq!((m.economy.kills, m.economy.farmer.money), (3, 0));
        let mut seen = Vec::new();
        for _ in 0..1000 {
            step(&mut rig, &mut shop, &mut m, &[]);
            let e = &m.economy;
            if seen.last() != Some(&(e.kills, e.farmer.money)) {
                seen.push((e.kills, e.farmer.money));
                assert!(e.kills == 3 || shop.machine.shake.is_armed());
            }
            if shop.tally.is_none() {
                break;
            }
        }
        assert_eq!(seen, [(3, 0), (2, 42), (1, 84), (0, 126)]);
    }

    /// AR §7.3: 3 kills pay 126 whether the tally runs or Enter skips it.
    #[test]
    fn the_animated_and_the_skipped_tally_give_the_same_total() {
        let shown = |skip: bool| {
            let mut rig = Rig::new(1);
            let mut m = two_player(&mut rig, 0, 3);
            let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
            to_farmer_side(&mut rig, &mut shop, &mut m);
            let mut shown = 0;
            for i in 0..1000 {
                let keys: &[Scancode] = if skip && i == 0 {
                    &[Scancode::RETURN]
                } else {
                    &[]
                };
                step(&mut rig, &mut shop, &mut m, keys);
                let Some(t) = &shop.tally else { break };
                shown = t.shown;
            }
            assert!(shop.tally.is_none());
            (shown, m.economy.farmer.money, m.economy.kills)
        };
        assert_eq!(shown(true).1, 126);
        assert_eq!(shown(false), (84, 126, 0));
    }

    /// Farmer done without tokens: the winch is re-rolled and the layout cleared on the press
    /// tick, then the fade leads to the bourse.
    #[test]
    fn the_farmer_done_without_tokens_rerolls_the_winch_then_goes_to_the_bourse() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 0, 0);
        m.economy.farmer.tokens = 0;
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        to_farmer_side(&mut rig, &mut shop, &mut m);
        step(&mut rig, &mut shop, &mut m, &[]);
        step(&mut rig, &mut shop, &mut m, &[Scancode::F]);
        let mut probe = Rig::new(1);
        probe.rng = rig.rng.clone();
        let expected = probe.cx(|cx| draw_anchors(ANCHOR_GAP, cx));
        step(&mut rig, &mut shop, &mut m, &[Scancode::TAB]);
        assert!(shop.leaving);
        assert_eq!(m.winch, expected);
        let mut next = None;
        for _ in 0..100 {
            if let MatchFlow::Next(s) = rig.step(&[], |k, cx| shop.tick(k, &mut m, cx)) {
                next = Some(s);
                break;
            }
        }
        assert!(matches!(next, Some(MatchScreen::Bourse(_))));
    }

    /// answers.md "Traps shop: carrying a trap": start at 350, left wins over right, a
    /// reversal decelerates through 0, and x stays in [20, 780 − w].
    #[test]
    fn the_carried_trap_starts_at_350_and_left_wins() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 0, 0);
        m.economy.farmer.tokens = 10;
        let mut shop = rig.cx(|cx| TrapsShop::new(&mut m, cx));
        let mut tap = |shop: &mut TrapsShop, m: &mut MatchState, keys: &[Scancode]| {
            rig.step(keys, |k, cx| shop.tick(k, m, cx));
        };
        tap(&mut shop, &mut m, &[Scancode::TAB]);
        assert_eq!(shop.carrying, Some((TrapKind::Flypaper, 350)));
        tap(&mut shop, &mut m, &[Scancode::H]);
        tap(&mut shop, &mut m, &[Scancode::H]);
        assert_eq!(
            (shop.carrying, shop.speed),
            (Some((TrapKind::Flypaper, 353)), 2)
        );
        tap(&mut shop, &mut m, &[Scancode::F, Scancode::H]);
        assert_eq!(
            (shop.carrying, shop.speed),
            (Some((TrapKind::Flypaper, 354)), 1)
        );
        for _ in 0..60 {
            tap(&mut shop, &mut m, &[Scancode::F]);
        }
        assert_eq!(
            (shop.carrying, shop.speed),
            (Some((TrapKind::Flypaper, 20)), -10)
        );
        tap(&mut shop, &mut m, &[]);
        assert_eq!(shop.speed, 0);
        assert_eq!(carry_clamp(780 - 53, 53), 727);
        assert_eq!(carry_clamp(726, 53), 726);
        assert_eq!(carry_clamp(21, 53), 21);
    }
}
