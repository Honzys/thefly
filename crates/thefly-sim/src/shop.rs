//! W3. The goodies shop (0x1e) with payout, purchases and the side switch, and the farmer's traps
//! shop (0x28) (ai-and-rules §7.2, §7.3, §4.2, §4.3; screens-and-rendering §3.1, §3.2).
//! A confirm pressed during the conveyor animation is dropped, not queued (gaps §9.2, §11.6 [I]).
//! Open item O4 (shop render). The conveyor delay between a purchase and its
//! grant is a named constant W3 must pick and record; the docs give no duration.

use crate::bourse::Bourse;
use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::economy::{ITEMS, Item, KILL_VALUE};
use crate::event::{Event, Music, SoundSlot};
use crate::input::{KeyState, Scancode};
use crate::match_state::{MatchState, PlaceError, Players};
use crate::screen::{FADE_MS, Fade, Flow, MatchFlow, MatchScreen};
use crate::shop_ai::{GoodiesShopAi, Press, SHOP_X_RANGE, TrapAction, TrapsShopAi};
use crate::world::lift::{ANCHOR_GAP, draw_anchors};
use crate::world::traps::TrapKind;

/// Item buttons 0..=4, then "done".
pub const DONE_BUTTON: usize = 5;
pub const BUTTONS: usize = 6;
/// The conveyor ride from purchase to grant, and one tally cycle. shortcut: open item O4, the
/// docs give no duration; 600 ms keeps it under the AI's 700 ms rhythm so no AI press is
/// dropped. Revisit when `FUN_00422571` is read.
pub const CONVEYOR_MS: u64 = 600;
/// Traps shop: the done button, the carried trap's start x and its speed cap.
pub const TRAPS_DONE: usize = 3;
pub const TRAPS_BUTTONS: usize = 4;
/// shortcut: open item O4, the human placement controls are not decoded (start x, speed ±1
/// per held tick up to ±10, x clamped to the shop range); revisit when the render is read.
pub const CARRY_START_X: i32 = 380;
pub const CARRY_MAX_SPEED: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShopSide {
    Farmer,
    Fly,
}

/// The payout tally: one type or one kill per conveyor cycle; a key skips to the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tally {
    pub side: ShopSide,
    /// Amounts still to credit, in order.
    pub pending: Vec<i32>,
    /// Per pending amount: the food type (fly) or the kill type 3/2/1 (farmer, the dead-fly
    /// frame).
    pub labels: Vec<usize>,
    pub cycle: Timer,
    /// The money shown: the payout is credited at once, this counts up to it.
    pub shown: i32,
}

impl Tally {
    fn new(side: ShopSide, pending: Vec<i32>, labels: Vec<usize>, before: i32, now: u64) -> Self {
        Self {
            side,
            pending,
            labels,
            cycle: Timer::armed(now, CONVEYOR_MS),
            shown: before,
        }
    }

    /// True when the tally has finished.
    fn tick(&mut self, skip: bool, now: u64) -> bool {
        if skip {
            self.shown += self.pending.iter().sum::<i32>();
            self.pending.clear();
        } else if self.cycle.expired_rearm(now) && !self.pending.is_empty() {
            self.shown += self.pending.remove(0);
            self.labels.remove(0);
        }
        self.pending.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoodiesShop {
    /// data+0x22fc; the farmer shops first.
    pub side: ShopSide,
    /// Second side's turn.
    pub second: bool,
    pub cursor: usize,
    pub tally: Option<Tally>,
    /// The bought item riding the conveyor; granted when it reaches the bottom.
    pub conveyor: Option<(Item, Timer)>,
    pub ai: Option<GoodiesShopAi>,
    pub fade: Fade,
    pub leaving: bool,
}

impl GoodiesShop {
    /// Fade in, payout tallies, the AI plan when one player.
    pub fn new(m: &mut MatchState, cx: &mut Ctx) -> Self {
        cx.emit(Event::Music(Music::Goodies));
        // AR §3.1 step 1: the plan counts the payout before it is paid.
        let mut ai = (m.config.players == Players::One)
            .then(|| GoodiesShopAi::new(m.config.ai_level, m, cx));
        let e = &mut m.economy;
        let kills = e.kills as usize;
        let labels = (0..kills)
            .map(|i| e.kill_log.get(i).map_or(3, |&k| usize::from(k)))
            .collect();
        let before = e.farmer.money;
        e.pay_farmer();
        let tally = (kills > 0).then(|| {
            Tally::new(
                ShopSide::Farmer,
                vec![KILL_VALUE; kills],
                labels,
                before,
                cx.now,
            )
        });
        if let Some(ai) = ai.as_mut() {
            ai.paused = tally.is_some();
        }
        Self {
            side: ShopSide::Farmer,
            second: false,
            cursor: 0,
            tally,
            conveyor: None,
            ai,
            fade: Fade::new(false, FADE_MS),
            leaving: false,
        }
    }

    /// Result 1 → traps shop, 2 → bourse, 0 → quit dialog (game-spec §2.2).
    pub fn tick(&mut self, keys: &KeyState, m: &mut MatchState, cx: &mut Ctx) -> MatchFlow {
        let faded = self.fade.step();
        if self.leaving {
            return if faded {
                MatchFlow::Next(after_goodies(m, cx))
            } else {
                MatchFlow::Stay
            };
        }
        if keys.pressed(Scancode::ESCAPE) {
            return MatchFlow::Leave(Flow::Quit);
        }
        if let Some(t) = &mut self.tally {
            let any_key = keys.skip() || keys.pressed(Scancode::TAB);
            if t.tick(any_key, cx.now) {
                self.tally = None;
                if let Some(ai) = self.ai.as_mut().filter(|_| self.side == ShopSide::Farmer) {
                    ai.unpause(cx.now);
                }
            }
            return MatchFlow::Stay;
        }
        if let Some((item, t)) = self.conveyor
            && t.expired(cx.now)
        {
            m.economy.grant(item);
            self.conveyor = None;
        }
        let ai_press = match (self.side, &mut self.ai) {
            (ShopSide::Farmer, Some(ai)) => Some(ai.tick(cx)),
            _ => None,
        };
        let confirm = match (self.side, ai_press) {
            (ShopSide::Farmer, Some(press)) => self.ai_press(press),
            (ShopSide::Farmer, None) => self.human(keys, Scancode::F, Scancode::H, Scancode::TAB),
            (ShopSide::Fly, _) => {
                self.human(keys, Scancode::LEFT, Scancode::RIGHT, Scancode::RETURN)
            }
        };
        // gaps §9.2, §11.6: a confirm during the conveyor animation is dropped.
        if confirm && self.conveyor.is_none() {
            self.confirm(m, cx);
        }
        MatchFlow::Stay
    }

    fn ai_press(&mut self, press: Option<Press>) -> bool {
        match press {
            None => false,
            Some(Press::Button(b)) => {
                self.cursor = b;
                true
            }
            Some(Press::SelectDone) => {
                self.cursor = DONE_BUTTON;
                false
            }
            Some(Press::Done) => {
                self.cursor = DONE_BUTTON;
                true
            }
        }
    }

    /// Cursor moves wrap over the six buttons; returns the confirm edge.
    fn human(&mut self, keys: &KeyState, left: Scancode, right: Scancode, ok: Scancode) -> bool {
        if keys.pressed(left) {
            self.cursor = (self.cursor + BUTTONS - 1) % BUTTONS;
        }
        if keys.pressed(right) {
            self.cursor = (self.cursor + 1) % BUTTONS;
        }
        keys.pressed(ok)
    }

    fn confirm(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        if self.cursor == DONE_BUTTON {
            return self.done(m, cx);
        }
        let first = if self.side == ShopSide::Farmer { 0 } else { 5 };
        let item = ITEMS[first + self.cursor].item;
        match m.economy.buy(item) {
            Ok(()) => {
                cx.emit(Event::play(SoundSlot::MONEY));
                self.conveyor = Some((item, Timer::armed(cx.now, CONVEYOR_MS)));
            }
            Err(_) => cx.emit(Event::play(SoundSlot::ERROR)),
        }
    }

    /// shortcut: SR §3.1's "no money left" exit is ambiguous; the plain order farmer → fly →
    /// leave is used. Revisit when `FUN_00421e6a` is re-read.
    fn done(&mut self, m: &mut MatchState, cx: &mut Ctx) {
        if self.second {
            self.leaving = true;
            self.fade = Fade::new(true, FADE_MS);
            return;
        }
        self.second = true;
        self.side = ShopSide::Fly;
        self.cursor = 0;
        let e = &mut m.economy;
        let before = e.fly.money;
        let types: Vec<usize> = (0..e.saved.len())
            .filter(|&t| e.saved[t].is_some())
            .collect();
        let values = e.pay_fly(cx);
        // pay_fly returns the non-zero lines in type order; label them with their types.
        let labels: Vec<usize> = types
            .into_iter()
            .filter(|&t| e.saved[t].is_some_and(|n| n > 0))
            .collect();
        if !values.is_empty() {
            self.tally = Some(Tally::new(ShopSide::Fly, values, labels, before, cx.now));
        }
    }
}

/// AR §4.2: the traps shop when the farmer has tokens, else the layout is cleared here.
fn after_goodies(m: &mut MatchState, cx: &mut Ctx) -> MatchScreen {
    if m.economy.farmer.tokens >= 1 {
        return MatchScreen::TrapsShop(Box::new(TrapsShop::new(m, cx)));
    }
    m.traps.slots.clear();
    m.winch = draw_anchors(ANCHOR_GAP, cx);
    MatchScreen::Bourse(Box::new(Bourse::new(cx)))
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
                MatchFlow::Next(MatchScreen::Bourse(Box::new(Bourse::new(cx))))
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
        let dir = i32::from(keys.held(Scancode::H)) - i32::from(keys.held(Scancode::F));
        self.speed = if dir == 0 {
            0
        } else {
            (self.speed + dir).clamp(-CARRY_MAX_SPEED, CARRY_MAX_SPEED)
        };
        let x = (x + self.speed).clamp(0, SHOP_X_RANGE - kind.spec().width - 1);
        // shortcut: open item O4, a fan faces the way it was last moved; revisit with the render.
        let kind = match (kind, dir) {
            (TrapKind::FanLeft | TrapKind::FanRight, -1) => TrapKind::FanLeft,
            (TrapKind::FanLeft | TrapKind::FanRight, 1) => TrapKind::FanRight,
            _ => kind,
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_state::MatchConfig;
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
        m.economy.farmer.money = money;
        m.economy.kills = kills;
        m
    }

    /// GS §5.6: slot 11 on purchase; the level counts only once the conveyor has run.
    #[test]
    fn a_purchase_plays_slot_11_and_counts_when_the_conveyor_ends() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 1000, 0);
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        let level = m.economy.owned(Item::FarmerLevel);
        rig.step(&[Scancode::TAB], |k, cx| shop.tick(k, &mut m, cx));
        assert!(rig.events.contains(&Event::play(SoundSlot::MONEY)));
        assert_eq!(m.economy.farmer.money, 800);
        assert_eq!(m.economy.owned(Item::FarmerLevel), level);
        let bought = rig.now();
        while rig.now() < bought + CONVEYOR_MS + 20 {
            rig.step(&[], |k, cx| shop.tick(k, &mut m, cx));
        }
        assert_eq!(m.economy.owned(Item::FarmerLevel), level + 1);
    }

    #[test]
    fn a_purchase_without_the_money_plays_slot_12() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 10, 0);
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        rig.step(&[Scancode::TAB], |k, cx| shop.tick(k, &mut m, cx));
        assert!(rig.events.contains(&Event::play(SoundSlot::ERROR)));
        assert!(shop.conveyor.is_none());
    }

    /// AR §7.3: 3 kills pay 126 whether the tally runs or is skipped.
    #[test]
    fn the_animated_and_the_skipped_tally_give_the_same_total() {
        let shown = |skip: bool| {
            let mut rig = Rig::new(1);
            let mut m = two_player(&mut rig, 0, 3);
            let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
            for i in 0..1000 {
                let keys: &[Scancode] = if skip && i == 0 {
                    &[Scancode::RETURN]
                } else {
                    &[]
                };
                rig.step(keys, |k, cx| shop.tick(k, &mut m, cx));
                if shop.tally.is_none() {
                    break;
                }
            }
            (m.economy.farmer.money, m.economy.kills)
        };
        assert_eq!(shown(true).0, 126);
        assert_eq!(shown(false).0, 126);
        assert_eq!(shown(false).1, 0);
    }

    /// Farmer done, fly done → fade → bourse when the farmer has no tokens.
    #[test]
    fn both_sides_done_with_no_tokens_goes_to_the_bourse() {
        let mut rig = Rig::new(1);
        let mut m = two_player(&mut rig, 0, 0);
        m.economy.farmer.tokens = 0;
        let mut shop = rig.cx(|cx| GoodiesShop::new(&mut m, cx));
        for keys in [
            [Scancode::F],
            [Scancode::TAB],
            [Scancode::LEFT],
            [Scancode::RETURN],
        ] {
            rig.step(&keys, |k, cx| shop.tick(k, &mut m, cx));
            rig.step(&[], |k, cx| shop.tick(k, &mut m, cx));
        }
        assert!(shop.leaving);
        let mut next = None;
        for _ in 0..100 {
            if let MatchFlow::Next(s) = rig.step(&[], |k, cx| shop.tick(k, &mut m, cx)) {
                next = Some(s);
                break;
            }
        }
        assert!(matches!(next, Some(MatchScreen::Bourse(_))));
    }
}
