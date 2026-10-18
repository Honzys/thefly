//! W3. The goodies shop (0x1e) with payout, purchases and the side switch, and the farmer's traps
//! shop (0x28) (ai-and-rules §7.2, §7.3, §4.2, §4.3; screens-and-rendering §3.1, §3.2;
//! answers.md "Goodies shop: purchase and tally are motion-driven", "Goodies shop side order and
//! exit", "Traps shop: carrying a trap").
//! shortcut: the shaking machine runs the shared wander mover every shop tick and draws rand
//! (answers.md "Goodies shop: purchase and tally are motion-driven"); not modelled until the
//! wander mover lands, so the shop's RNG stream lacks those draws.

use crate::bourse::Bourse;
use crate::clock::{FPS, Timer};
use crate::ctx::Ctx;
use crate::economy::{ITEMS, Item, KILL_VALUE};
use crate::event::{Event, Music, SoundSlot};
use crate::input::{KeyState, Scancode};
use crate::match_state::{MatchState, PlaceError, Players};
use crate::screen::{FADE_MS, Fade, Flow, MatchFlow, MatchScreen};
use crate::shop_ai::{GoodiesShopAi, Press, TrapAction, TrapsShopAi};
use crate::world::lift::{ANCHOR_GAP, draw_anchors};
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
/// shortcut: the target sprite the tally falls onto is not named in the docs; its y is taken as
/// the money panel's (470). Revisit when `FUN_0042322f`'s target is identified.
pub const TALLY_TARGET_Y: i32 = 470;
/// `g_flymort` frames are 100×75; also used for the food sprites. shortcut: the food sprites'
/// own sizes are not in the sim data; revisit if the fly tally's timing must match exactly.
const TALLY_SPRITE: (i32, i32) = (100, 75);
const DEAD_FLY_FRAMES: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopSide {
    Farmer,
    Fly,
}

/// `FUN_004180af`: exactly |v| px per run of FPS frames; the run restarts when v changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mover {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    run: (i32, i32),
    frames: i32,
    rem: (i32, i32),
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
        if (self.vx, self.vy) != self.run || self.frames >= FPS {
            (self.run, self.frames, self.rem) = ((self.vx, self.vy), 0, (0, 0));
        }
        self.frames += 1;
        self.x += fps_share(self.vx, &mut self.rem.0);
        self.y += fps_share(self.vy, &mut self.rem.1);
    }
}

/// This frame's share of |v| px over FPS frames (duplicates `world::flying_goodies::fps_share`).
fn fps_share(v: i32, rem: &mut i32) -> i32 {
    *rem += v.abs() % FPS;
    let carry = *rem >= FPS;
    if carry {
        *rem -= FPS;
    }
    v.signum() * (v.abs() / FPS + i32::from(carry))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TallyItem {
    pub value: i32,
    /// The food type (fly), or the dead-fly frame 1..=3 drawn at its cycle start (farmer).
    pub label: usize,
    /// The saved count shown as "x N" (fly); 1 per kill.
    pub count: i32,
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
    /// The money shown: the payout is credited at once, this counts up to it.
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
        self.sprite = Mover::at(TALLY_START_X, TALLY_BOTTOM - TALLY_SPRITE.1, vx);
        self.label = Timer::UNARMED;
    }

    /// True when the tally has finished.
    fn tick(&mut self, skip: bool, cx: &mut Ctx) -> bool {
        if skip {
            self.shown += self.pending.iter().map(|i| i.value).sum::<i32>();
            self.pending.clear();
        }
        let Some(&item) = self.pending.first() else {
            return true;
        };
        let w = TALLY_SPRITE.0;
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
        if s.x + w < FALL_X {
            s.vy += FALL_ACCEL;
            if s.vy < FALL_SOUND_VY {
                cx.emit(Event::play(SoundSlot::SHOP_MACHINE));
            }
        }
        if s.y < TALLY_TARGET_Y {
            return false;
        }
        self.shown += item.value;
        self.pending.remove(0);
        self.start(cx);
        self.pending.is_empty()
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
}

impl GoodiesShop {
    /// Opens on the fly side with the fly's payout tally.
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        cx.emit(Event::Music(Music::Goodies));
        let e = &mut m.economy;
        let before = e.fly.money;
        let saved: Vec<(usize, i32)> = (0..e.saved.len())
            .filter_map(|t| e.saved[t].filter(|&n| n > 0).map(|n| (t, n)))
            .collect();
        // pay_fly returns the non-zero lines in type order: one per type with a saved count.
        let items: Vec<TallyItem> = e
            .pay_fly(cx)
            .into_iter()
            .zip(saved)
            .map(|(value, (label, count))| TallyItem {
                value,
                label,
                count,
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
        }
    }

    /// Fly done → farmer; farmer done → traps shop, or the bourse without tokens; Esc → quit
    /// dialog (game-spec §2.2).
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
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
            if t.tick(keys.pressed(Scancode::RETURN), cx) {
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
        let e = &mut m.economy;
        let kills = e.kills;
        let before = e.farmer.money;
        e.pay_farmer();
        let kill = TallyItem {
            value: KILL_VALUE,
            label: 0,
            count: 1,
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

    /// Seed 1: the dead-fly frames are 41 % 3 + 1 = 3, then 18467 % 3 + 1 = 3. At −500 px/s the
    /// sprite covers 500 px in 65 ticks (x 305), then 7, 8, 8, 7, 8 → x 267 on tick 70, past
    /// 372 − 100: it falls (sound 10 once, at vy 50). From y 164 the drop on fall tick k is
    /// trunc(50k / 65): Σ 64 after 13, 258 after 26, 299 after 28 and 321 after 29, past the 306
    /// to y 470, so it pays on tick 99.
    #[test]
    fn a_dead_fly_pays_42_when_it_lands() {
        let mut rig = Rig::new(1);
        let kill = TallyItem {
            value: KILL_VALUE,
            label: 0,
            count: 1,
        };
        let mut t = rig.cx(|cx| Tally::new(ShopSide::Farmer, vec![kill; 2], 0, cx));
        assert_eq!(t.pending[0].label, 3);
        for tick in 1..=99 {
            let before = rig.events.len();
            let done = rig.step(&[], |_, cx| t.tick(false, cx));
            let rang = rig.events[before..].contains(&Event::play(SoundSlot::SHOP_MACHINE));
            assert_eq!(rang, tick == 70, "tick {tick}");
            assert!(!done);
            if tick == 70 {
                assert_eq!((t.sprite.x, t.sprite.y, t.sprite.vy), (267, 164, 50));
            }
            assert_eq!(t.shown, if tick < 99 { 0 } else { 42 }, "tick {tick}");
        }
        assert_eq!((t.pending.len(), t.pending[0].label), (1, 3));
        assert_eq!(t.sprite, Mover::at(805, 164, -500));
        let mut probe = MsvcRand::new(1);
        probe.rand();
        probe.rand();
        assert_eq!(rig.rng, probe);
    }

    /// A food sprite stops when its centre passes x 588 and shows "x N" for 2000 ms.
    #[test]
    fn a_food_sprite_stops_for_its_label() {
        let mut rig = Rig::new(1);
        let food = TallyItem {
            value: 90,
            label: 4,
            count: 3,
        };
        let mut t = rig.cx(|cx| Tally::new(ShopSide::Fly, vec![food], 0, cx));
        for _ in 0..200 {
            rig.step(&[], |_, cx| t.tick(false, cx));
            if t.label.is_armed() {
                break;
            }
        }
        let stop = (t.sprite.x, rig.now());
        assert!(stop.0 + 50 < 588 && stop.0 + 50 >= 588 - 4, "{stop:?}");
        while rig.now() < stop.1 + LABEL_MS - 20 {
            rig.step(&[], |_, cx| t.tick(false, cx));
        }
        assert_eq!(t.sprite.x, stop.0);
        for _ in 0..300 {
            if rig.step(&[], |_, cx| t.tick(false, cx)) {
                break;
            }
        }
        assert_eq!(t.shown, 90);
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
