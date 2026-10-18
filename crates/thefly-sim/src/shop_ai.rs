//! W3. The AI farmer in the shops: the goodies-shop plan and its timed replay (ai-and-rules §3),
//! and trap planning, placement and replay (ai-and-rules §4.4).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::economy::{Item, KILL_VALUE};
use crate::match_state::{MatchState, TrapLayout, TrapSlot};
use crate::world::ai::AiLevel;
use crate::world::traps::TrapKind;

const LEVEL_CAP: i32 = 5;
const TOKEN_CAP: i32 = 14;

/// The AI presses one button per expiry of this period.
pub const STEP_MS: u64 = 700;
/// The goodie split loop's cap (the original's own bound).
pub const GOODIE_LOOPS: u32 = 1000;
pub const TRAP_X_ATTEMPTS: u32 = 200;
/// The shop's x range is 0..760.
pub const SHOP_X_RANGE: i32 = 760;
/// Replay timings: select, press, placed, next.
pub const REPLAY_SELECT_MS: u64 = 800;
pub const REPLAY_PRESS_MS: u64 = 500;
pub const REPLAY_PLACED_MS: u64 = 1000;
pub const REPLAY_NEXT_MS: u64 = 800;
/// UB guard: the type-pick retry in §4.4 step 2 ends by argument, not by a bound; the port caps
/// it (the argument says it is never reached).
pub const TRAP_PICK_CAP: u32 = 10_000;
/// UB guard: the order permutation draws `rand % 5` until an unused id comes up; capped.
pub const ORDER_DRAW_CAP: u32 = 10_000;

/// `FUN_00440ac9`'s profile bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShopProfile {
    pub reserve_pct: i32,
    pub reserve_jitter: i32,
    pub token_pct: i32,
    pub token_jitter: i32,
    /// Triple, Nuclear, Boot in %.
    pub split: [i32; 3],
    /// Trap gate `c`.
    pub trap_c: i32,
}

/// Indexed by [`AiLevel`] as usize.
pub const SHOP_PROFILES: [ShopProfile; 3] = [
    ShopProfile {
        reserve_pct: 70,
        reserve_jitter: 15,
        token_pct: 70,
        token_jitter: 15,
        split: [34, 33, 33],
        trap_c: 30,
    },
    ShopProfile {
        reserve_pct: 80,
        reserve_jitter: 10,
        token_pct: 80,
        token_jitter: 20,
        split: [40, 30, 30],
        trap_c: 50,
    },
    ShopProfile {
        reserve_pct: 95,
        reserve_jitter: 5,
        token_pct: 85,
        token_jitter: 5,
        split: [50, 25, 25],
        trap_c: 70,
    },
];

/// Planned counts by shop id 1..=5 (level, tokens, Triple, Nuclear, Boot), stored at index id − 1.
pub type Plan = [i32; 5];

/// A shop button press the AI makes; the shop executes it as if the farmer pressed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Move the cursor to this button and press it (0..=4 items, 5 done).
    Button(usize),
    /// Move the cursor to "done" without pressing it.
    SelectDone,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodiesShopAi {
    pub plan: Plan,
    /// Ids 1..=5 in visit order.
    pub order: [usize; 5],
    pub next: usize,
    pub timer: Timer,
    /// Paused until the farmer's payout tally finishes.
    pub paused: bool,
}

impl GoodiesShopAi {
    /// §3.1 plan, then the §3.2 order; the timer is armed at 700 ms.
    pub fn new(level: AiLevel, m: &MatchState, cx: &mut Ctx) -> Self {
        let plan = plan(&SHOP_PROFILES[level as usize], m, cx);
        Self {
            plan,
            order: visit_order(cx),
            next: 0,
            timer: Timer::armed(cx.now, STEP_MS),
            paused: false,
        }
    }

    /// The payout tally ended: the 700 ms rhythm starts now.
    pub fn unpause(&mut self, now: u64) {
        self.paused = false;
        self.timer.arm(now, STEP_MS);
    }

    pub fn tick(&mut self, cx: &mut Ctx) -> Option<Press> {
        if self.paused || !self.timer.expired_rearm(cx.now) {
            return None;
        }
        while self.next < self.order.len() && self.plan[self.order[self.next] - 1] < 1 {
            self.next += 1;
        }
        if let Some(&id) = self.order.get(self.next) {
            self.plan[id - 1] -= 1;
            return Some(Press::Button(id - 1));
        }
        // shortcut: the cursor moves to "done" on the expiry after the last press (§3.2 step 3
        // does not say which tick); revisit with a capture.
        if self.next == self.order.len() {
            self.next += 1;
            return Some(Press::SelectDone);
        }
        self.timer.disarm();
        Some(Press::Done)
    }
}

fn signed(cx: &mut Ctx) -> i32 {
    if cx.rng.odd() { -1 } else { 1 }
}

/// §3.1 steps 1–6.
fn plan(p: &ShopProfile, m: &MatchState, cx: &mut Ctx) -> Plan {
    let e = &m.economy;
    let level = e.farmer.level;
    let last = m.last_shop();
    let mut b = e.farmer.money + KILL_VALUE * e.kills;
    let mut plan: Plan = [0; 5];
    let mut price = e.item_prices[Item::FarmerLevel as usize];
    for _ in 0..LEVEL_CAP {
        let ratio = f64::from(b) / f64::from(price);
        if ratio < 1.0 || level + plan[0] >= LEVEL_CAP || (plan[0] > 0 && ratio <= 1.0) {
            break;
        }
        plan[0] += 1;
        b -= price;
        price += 200;
    }
    if level + plan[0] < LEVEL_CAP && !last {
        let j = cx.rng.r(p.reserve_jitter + if plan[0] > 0 { 5 } else { 0 });
        let sgn = signed(cx);
        let base = b * p.reserve_pct / 100;
        let mut r = base + sgn * (j * base) / 100;
        if b < r {
            r = base;
        }
        b -= r;
    }
    let token_price = e.item_prices[Item::Tokens as usize];
    if e.farmer.tokens < TOKEN_CAP && token_price < b {
        let j = cx.rng.r(p.token_jitter);
        let sgn = signed(cx);
        let base = if last { b } else { b * p.token_pct / 100 };
        let mut spend = (base + sgn * (j * base) / 100).max(0);
        if level < LEVEL_CAP && spend > 0 {
            spend = (spend - spend / 5 * (LEVEL_CAP - level)).max(0);
        }
        let mut n = spend / token_price;
        if n >= TOKEN_CAP {
            n = TOKEN_CAP;
        } else if n + level > TOKEN_CAP {
            n = TOKEN_CAP - level;
        }
        plan[1] = n;
        b -= n * token_price;
    }
    let goodies = [Item::Triple, Item::Nuclear, Item::Boot].map(|i| e.item_prices[i as usize]);
    for _ in 0..GOODIE_LOOPS {
        if !goodies.iter().any(|&g| b >= g) {
            break;
        }
        let r = cx.rng.r(100);
        let pick = if r < p.split[0] {
            0
        } else if r < p.split[0] + p.split[1] {
            1
        } else {
            2
        };
        if b >= goodies[pick] {
            b -= goodies[pick];
            plan[2 + pick] += 1;
        }
    }
    plan
}

/// §3.2 step 1: draw `rand % 5` until an unused id comes up, for each of the five slots.
fn visit_order(cx: &mut Ctx) -> [usize; 5] {
    let mut order = [0; 5];
    let mut used = [false; 5];
    for slot in &mut order {
        for _ in 0..ORDER_DRAW_CAP {
            let id = cx.rng.r(5) as usize;
            if !used[id] {
                used[id] = true;
                *slot = id + 1;
                break;
            }
        }
    }
    order
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayStep {
    Select,
    Press,
    Placed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrapsShopAi {
    /// Accepted traps to replay; empty when the gate said "keep the tokens".
    pub queue: Vec<TrapSlot>,
    pub next: usize,
    pub step: ReplayStep,
    pub timer: Timer,
}

/// What the traps shop does on behalf of the AI this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapAction {
    Select(usize),
    Press,
    Place(TrapSlot),
    Done,
}

/// Plan order and the placement order (§4.4 steps 2 and 3).
const PICKS: [TrapKind; 4] = [
    TrapKind::Flypaper,
    TrapKind::FanLeft,
    TrapKind::FanRight,
    TrapKind::Zapper,
];
const PLACE_ORDER: [TrapKind; 4] = [
    TrapKind::Zapper,
    TrapKind::FanRight,
    TrapKind::FanLeft,
    TrapKind::Flypaper,
];

/// The traps-shop button of a trap: both fans share button 1.
pub fn trap_button(kind: TrapKind) -> usize {
    match kind {
        TrapKind::Flypaper => 0,
        TrapKind::FanLeft | TrapKind::FanRight => 1,
        TrapKind::Zapper => 2,
    }
}

impl TrapsShopAi {
    /// Gate, plan types, place with up to 200 attempts each, copy out and clear (§4.4 1–4).
    pub fn new(level: AiLevel, m: &mut MatchState, cx: &mut Ctx) -> Self {
        let c = SHOP_PROFILES[level as usize].trap_c;
        let tokens = m.economy.farmer.tokens;
        let threshold = c - tokens * (c - c / 2) / 14;
        let gate = cx.rng.r(100) > threshold;
        let queue = if gate || m.last_shop() {
            place_all(&plan_traps(tokens, cx), &m.winch, tokens, cx)
        } else {
            Vec::new()
        };
        m.traps.slots.clear();
        Self {
            queue,
            next: 0,
            step: ReplayStep::Select,
            timer: Timer::armed(cx.now, REPLAY_SELECT_MS),
        }
    }

    pub fn tick(&mut self, cx: &mut Ctx) -> Option<TrapAction> {
        if !self.timer.expired(cx.now) {
            return None;
        }
        let Some(&slot) = self.queue.get(self.next) else {
            self.timer.disarm();
            return Some(TrapAction::Done);
        };
        let (action, step, ms) = match self.step {
            ReplayStep::Select => (
                TrapAction::Select(trap_button(slot.kind)),
                ReplayStep::Press,
                REPLAY_PRESS_MS,
            ),
            ReplayStep::Press => (TrapAction::Press, ReplayStep::Placed, REPLAY_PLACED_MS),
            ReplayStep::Placed => {
                self.next += 1;
                (TrapAction::Place(slot), ReplayStep::Select, REPLAY_NEXT_MS)
            }
        };
        self.step = step;
        self.timer.arm(cx.now, ms);
        Some(action)
    }
}

/// §4.4 step 2: planned counts indexed like [`PICKS`].
fn plan_traps(tokens: i32, cx: &mut Ctx) -> [usize; 4] {
    let mut left = tokens;
    let mut counts = [0usize; 4];
    for _ in 0..TRAP_PICK_CAP {
        let affordable = PICKS.iter().filter(|k| k.spec().cost <= left).count();
        if left <= 0 || affordable == 0 {
            break;
        }
        let pick = cx.rng.r(affordable as i32) as usize;
        let spec = PICKS[pick].spec();
        let placed = if pick == 1 || pick == 2 {
            counts[1] + counts[2]
        } else {
            counts[pick]
        };
        if placed < spec.cap {
            counts[pick] += 1;
            left -= spec.cost;
        } else if affordable == 1 {
            break;
        }
    }
    counts
}

/// §4.4 step 3: random x per planned trap, zappers first; 200 failures abandon the rest.
fn place_all(counts: &[usize; 4], winch: &[i32; 3], tokens: i32, cx: &mut Ctx) -> Vec<TrapSlot> {
    let mut layout = TrapLayout::default();
    let mut scratch = tokens;
    for kind in PLACE_ORDER {
        let n = counts[PICKS.iter().position(|&k| k == kind).unwrap_or(0)];
        for _ in 0..n {
            if !place_one(&mut layout, kind, winch, &mut scratch, cx) {
                return layout.slots;
            }
        }
    }
    layout.slots
}

fn place_one(
    layout: &mut TrapLayout,
    kind: TrapKind,
    winch: &[i32; 3],
    tokens: &mut i32,
    cx: &mut Ctx,
) -> bool {
    let range = SHOP_X_RANGE - kind.spec().width;
    (0..TRAP_X_ATTEMPTS).any(|_| {
        let x = cx.rng.r(range);
        layout.place(kind, x, winch, tokens).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::match_state::MatchConfig;
    use crate::rng::MsvcRand;

    fn run<T>(now: u64, rng: &mut MsvcRand, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let (data, mut events) = (GameData::synthetic(), Vec::new());
        f(&mut Ctx::new(now, rng, &data, &mut events))
    }

    /// A breakfast-of-day-1 match whose rand stream then restarts at seed 1.
    fn shop_match(money: i32, tokens: i32) -> (MatchState, MsvcRand) {
        let mut rng = MsvcRand::new(1);
        let mut m = run(0, &mut rng, |cx| {
            let mut m = MatchState::new(MatchConfig::default(), false, cx);
            m.advance(cx);
            m
        });
        m.economy.farmer.money = money;
        m.economy.farmer.tokens = tokens;
        (m, MsvcRand::new(1))
    }

    /// AR §3.1 by hand, Normal profile, B = 1000, level 0:
    /// levels 200 + 400 → B 400; reserve j = 41 % 15 = 11, 18467 odd → −1, base 320,
    /// R = 320 − 35 → B 115; tokens j = 6334 % 20 = 14, 26500 even → +1, base 92, spend 104,
    /// 104 − 20 × 5 = 4 → 0 tokens; goodies 19169 → 69 Nuclear, 15724 → 24 Triple,
    /// 11478 → 78 Boot (65 > 45), 29358 → 58 Nuclear, B 5.
    /// Order: 26962 % 5 = 2, 24464 → 4, 5705 → 0, then 1 at 23281, then 3 at 153.
    #[test]
    fn the_seed_1_plan_matches_the_hand_computation() {
        let (m, mut rng) = shop_match(1000, 0);
        let ai = run(0, &mut rng, |cx| {
            GoodiesShopAi::new(AiLevel::Normal, &m, cx)
        });
        assert_eq!(ai.plan, [2, 0, 1, 2, 0]);
        assert_eq!(ai.order, [3, 5, 1, 2, 4]);
        assert_eq!(rng.rand(), 292);
    }

    #[test]
    fn presses_come_700_ms_apart_in_the_drawn_order() {
        let (m, mut rng) = shop_match(1000, 0);
        let mut ai = run(0, &mut rng, |cx| {
            GoodiesShopAi::new(AiLevel::Normal, &m, cx)
        });
        let mut presses = Vec::new();
        for now in 0..10_000 {
            if let Some(p) = run(now, &mut rng, |cx| ai.tick(cx)) {
                presses.push((now, p));
            }
        }
        use Press::*;
        assert_eq!(
            presses,
            [
                (700, Button(2)),
                (1400, Button(0)),
                (2100, Button(0)),
                (2800, Button(3)),
                (3500, Button(3)),
                (4200, SelectDone),
                (4900, Done),
            ]
        );
    }

    /// AR §4.4 with 14 tokens, Normal: threshold 50 − 25 = 25 < 41. Picks (rand % 4, then % 3,
    /// then % 1): zapper, fan right, flypaper, fan left, flypaper, fan right, flypaper, flypaper.
    /// Placement x = rand % (760 − w); 5705 % 726 = 623 and 11942 % 752 = 662 hit the zapper.
    #[test]
    fn the_traps_ai_plans_places_zappers_first_and_validates() {
        let (mut m, mut rng) = shop_match(0, 14);
        m.winch = [0; 3];
        let ai = run(0, &mut rng, |cx| {
            TrapsShopAi::new(AiLevel::Normal, &mut m, cx)
        });
        use TrapKind::*;
        let kinds: Vec<_> = ai.queue.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                Zapper, FanRight, FanRight, FanLeft, Flypaper, Flypaper, Flypaper, Flypaper
            ]
        );
        let xs: Vec<_> = ai.queue.iter().map(|s| s.x).collect();
        let shop = [596, 557, 49, 129, 185, 491, 739, 315];
        assert_eq!(xs, shop.map(|x| (f64::from(x) / 0.152) as i32));
        assert_eq!(rng.rand(), 5436);
        assert!(m.traps.slots.is_empty());
        assert_eq!(
            m.economy.farmer.tokens, 14,
            "tokens go only as the replay places"
        );
    }

    #[test]
    fn a_low_gate_draw_keeps_the_tokens() {
        let (mut m, mut rng) = shop_match(0, 0);
        // threshold 50 with 0 tokens; 41 ≤ 50.
        let mut ai = run(0, &mut rng, |cx| {
            TrapsShopAi::new(AiLevel::Normal, &mut m, cx)
        });
        assert!(ai.queue.is_empty());
        assert_eq!(run(800, &mut rng, |cx| ai.tick(cx)), Some(TrapAction::Done));
    }

    #[test]
    fn the_replay_runs_at_800_500_1000_800_ms() {
        let slot = TrapSlot {
            kind: TrapKind::FanLeft,
            x: 100,
            y: 600,
        };
        let mut ai = TrapsShopAi {
            queue: vec![slot, slot],
            next: 0,
            step: ReplayStep::Select,
            timer: Timer::armed(0, REPLAY_SELECT_MS),
        };
        let mut rng = MsvcRand::new(1);
        let mut got = Vec::new();
        for now in 0..8000 {
            if let Some(a) = run(now, &mut rng, |cx| ai.tick(cx)) {
                got.push((now, a));
            }
        }
        use TrapAction::*;
        assert_eq!(
            got,
            [
                (800, Select(1)),
                (1300, Press),
                (2300, Place(slot)),
                (3100, Select(1)),
                (3600, Press),
                (4600, Place(slot)),
                (5400, Done),
            ]
        );
    }
}
