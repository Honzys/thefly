//! W3. Money, scores, inventories, shop prices, food prices and their fluctuation, payout, kills
//! and the balance meter (game-spec §5.6, §5.7; ai-and-rules §7, §8.1, §8.2).

use crate::ctx::Ctx;
use crate::rng::MsvcRand;
use crate::world::flying_goodies::Side;
use crate::world::objects::NORMAL_TYPES;

/// Points per kill at payout and at the winner screen.
pub const KILL_VALUE: i32 = 42;
/// The meter's difference clamp.
pub const METER_CLAMP: i32 = 1000;
/// Food price floor after a fluctuation.
pub const PRICE_FLOOR: f32 = 1.0;
/// The extreme-mode food price table.
pub const EXTREME_PRICES: [f32; 3] = [80.0, 50.0, 20.0];

/// The shop items, in shop order per side (game-spec §5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    FarmerLevel,
    Tokens,
    Triple,
    Nuclear,
    Boot,
    FlyStrength,
    FlySpeed,
    Turbo,
    SuperFly,
    Midges,
}

impl Item {
    /// The farmer side's five shop items come first.
    pub fn farmer(self) -> bool {
        (self as usize) < 5
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemSpec {
    pub item: Item,
    pub price: i32,
    pub cap: i32,
    /// Rise per purchase; the farmer level's rise is one less when owned == cap − 2.
    pub increment: i32,
}

const fn spec(item: Item, price: i32, cap: i32, increment: i32) -> ItemSpec {
    ItemSpec {
        item,
        price,
        cap,
        increment,
    }
}

/// Indexed by [`Item`] as usize.
pub const ITEMS: [ItemSpec; 10] = [
    spec(Item::FarmerLevel, 200, 5, 200),
    spec(Item::Tokens, 55, 14, 0),
    spec(Item::Triple, 30, 99, 0),
    spec(Item::Nuclear, 40, 99, 0),
    spec(Item::Boot, 65, 99, 0),
    spec(Item::FlyStrength, 100, 5, 100),
    spec(Item::FlySpeed, 100, 5, 100),
    spec(Item::Turbo, 20, 99, 0),
    spec(Item::SuperFly, 55, 99, 0),
    spec(Item::Midges, 60, 99, 0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlySide {
    pub strength: i32,
    pub speed: i32,
    pub turbo: i32,
    pub superfly: i32,
    pub midges: i32,
    /// data+0x92.
    pub money: i32,
    /// data+0x30.
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FarmerSide {
    pub level: i32,
    /// Trap tokens, data+0x7e.
    pub tokens: i32,
    pub triple: i32,
    pub nuclear: i32,
    pub boot: i32,
    /// data+0x90.
    pub money: i32,
    /// data+0x38.
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    Normal,
    Extreme,
    Tutorial,
}

/// game-spec §5.7.
pub const fn starting_fly(setup: Setup) -> FlySide {
    let (strength, speed, turbo, superfly, midges, money, score) = match setup {
        Setup::Normal => (0, 0, 5, 3, 0, 0, 0),
        Setup::Extreme => (4, 4, 20, 20, 0, 0, 0),
        Setup::Tutorial => (3, 3, 50, 50, 50, 1000, 4200),
    };
    FlySide {
        strength,
        speed,
        turbo,
        superfly,
        midges,
        money,
        score,
    }
}

/// game-spec §5.7.
pub const fn starting_farmer(setup: Setup) -> FarmerSide {
    let (level, tokens, triple, nuclear, boot, money, score) = match setup {
        Setup::Normal => (0, 0, 3, 1, 0, 0, 0),
        Setup::Extreme => (4, 0, 20, 15, 0, 0, 0),
        Setup::Tutorial => (3, 6, 50, 50, 50, 400, 3000),
    };
    FarmerSide {
        level,
        tokens,
        triple,
        nuclear,
        boot,
        money,
        score,
    }
}

/// `Fluctuate(type)` (ai-and-rules §7.1): float32 throughout; returns trunc(old).
pub fn fluctuate(price: &mut f32, rng: &mut MsvcRand) -> i32 {
    let old = *price;
    let sgn: f32 = if rng.r(10) < 5 { -1.0 } else { 1.0 };
    let frac = rng.r(30) as f32 / 100.0;
    *price = (old + sgn * frac * old).max(PRICE_FLOOR);
    old as i32
}

/// Refusal reasons for [`Economy::buy`] (sound 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuyError {
    NoMoney,
    AtCap,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Economy {
    pub fly: FlySide,
    pub farmer: FarmerSide,
    /// Current shop prices, indexed by [`Item`].
    pub item_prices: [i32; 10],
    /// Food prices per object type (22 normal, or [`EXTREME_PRICES`]).
    pub food_prices: Vec<f32>,
    /// data+0x1c4: the type is present this period (≥ 0).
    pub present: Vec<bool>,
    /// data+0x1f0: deliveries this period; None for absent types (−1).
    pub saved: Vec<Option<i32>>,
    /// data+0x15c: kills since the last farmer payout.
    pub kills: i32,
    /// Kill types (3/2/1, 0 for none yet), capped at [`crate::match_state::KILL_LOG_CAP`].
    pub kill_log: Vec<u8>,
}

impl Economy {
    /// `FUN_0040b005` + `FUN_0040b3c7` + `FUN_0040b50c`: the starting inventories for `setup`,
    /// the base shop prices, and the food price table (22 types, or the extreme three when
    /// `extreme`). No type is present until the first period places its objects.
    pub fn new(setup: Setup, extreme: bool) -> Self {
        let food_prices: Vec<f32> = if extreme {
            EXTREME_PRICES.to_vec()
        } else {
            NORMAL_TYPES.iter().map(|t| t.price).collect()
        };
        Self {
            fly: starting_fly(setup),
            farmer: starting_farmer(setup),
            item_prices: ITEMS.map(|spec| spec.price),
            present: vec![false; food_prices.len()],
            saved: vec![None; food_prices.len()],
            food_prices,
            kills: 0,
            kill_log: Vec::new(),
        }
    }

    /// `FUN_00422223` steps 2–4: refuse, or deduct and raise the price. The owned count rises
    /// later, when the conveyor animation ends ([`Economy::grant`]).
    pub fn buy(&mut self, item: Item) -> Result<(), BuyError> {
        let spec = ITEMS[item as usize];
        let price = self.item_prices[item as usize];
        if *self.money_mut(item) < price {
            return Err(BuyError::NoMoney);
        }
        let owned = self.owned(item);
        if owned >= spec.cap {
            return Err(BuyError::AtCap);
        }
        *self.money_mut(item) -= price;
        // §7.2 step 4: the farmer level's rise two below the cap is one less (999, not 1000).
        if item == Item::FarmerLevel && owned == spec.cap - 2 {
            self.item_prices[item as usize] += spec.increment - 1;
        } else if owned < spec.cap - 1 {
            self.item_prices[item as usize] += spec.increment;
        }
        Ok(())
    }

    /// `FUN_00423047`: the bought item reaches the bottom of the conveyor.
    pub fn grant(&mut self, item: Item) {
        *self.owned_mut(item) += 1;
    }

    pub fn owned(&self, item: Item) -> i32 {
        let (fly, farmer) = (&self.fly, &self.farmer);
        match item {
            Item::FarmerLevel => farmer.level,
            Item::Tokens => farmer.tokens,
            Item::Triple => farmer.triple,
            Item::Nuclear => farmer.nuclear,
            Item::Boot => farmer.boot,
            Item::FlyStrength => fly.strength,
            Item::FlySpeed => fly.speed,
            Item::Turbo => fly.turbo,
            Item::SuperFly => fly.superfly,
            Item::Midges => fly.midges,
        }
    }

    fn owned_mut(&mut self, item: Item) -> &mut i32 {
        let (fly, farmer) = (&mut self.fly, &mut self.farmer);
        match item {
            Item::FarmerLevel => &mut farmer.level,
            Item::Tokens => &mut farmer.tokens,
            Item::Triple => &mut farmer.triple,
            Item::Nuclear => &mut farmer.nuclear,
            Item::Boot => &mut farmer.boot,
            Item::FlyStrength => &mut fly.strength,
            Item::FlySpeed => &mut fly.speed,
            Item::Turbo => &mut fly.turbo,
            Item::SuperFly => &mut fly.superfly,
            Item::Midges => &mut fly.midges,
        }
    }

    fn money_mut(&mut self, item: Item) -> &mut i32 {
        if item.farmer() {
            &mut self.farmer.money
        } else {
            &mut self.fly.money
        }
    }

    /// `FUN_0042ac27`'s economy part: present flags and saved counts for the new period.
    /// Saved is 0 for a present type and None (−1) for an absent one.
    pub fn start_period(&mut self, present: &[bool]) {
        self.present = present.to_vec();
        self.saved = present.iter().map(|&p| p.then_some(0)).collect();
    }

    /// The fly's payout (ai-and-rules §7.3): fluctuates every present type in order and pays
    /// the old price × saved. Returns the non-zero lines in type order, for the animated
    /// tally; their sum is already credited to money and score.
    pub fn pay_fly(&mut self, cx: &mut Ctx) -> Vec<i32> {
        let mut lines = Vec::new();
        for (price, saved) in self.food_prices.iter_mut().zip(&self.saved) {
            let Some(n) = *saved else { continue };
            let value = fluctuate(price, cx.rng) * n;
            if value != 0 {
                lines.push(value);
            }
        }
        let total: i32 = lines.iter().sum();
        self.fly.money += total;
        self.fly.score += total;
        lines
    }

    /// The farmer's payout: 42 × kills, then kills = 0. The kill-type list is not cleared.
    pub fn pay_farmer(&mut self) -> i32 {
        let total = KILL_VALUE * self.kills;
        self.farmer.money += total;
        self.farmer.score += total;
        self.kills = 0;
        total
    }

    /// Σ price × saved over saved > 0, accumulated as total = trunc(total + price·saved)
    /// (ai-and-rules §8.2; the top-up `FUN_0040bb54` sums the same way [I]).
    fn saved_value(&self) -> i32 {
        self.food_prices
            .iter()
            .zip(&self.saved)
            .filter_map(|(&p, s)| s.filter(|&n| n > 0).map(|n| (p, n)))
            .fold(0, |total, (p, n)| {
                (f64::from(total) + f64::from(p) * f64::from(n)) as i32
            })
    }

    /// `FUN_0040bb54` + `FUN_0040bbe5`: the winner screen's top-up, no fluctuation.
    pub fn top_up(&mut self) {
        self.fly.score += self.saved_value();
        self.farmer.score += KILL_VALUE * self.kills;
    }

    /// `FUN_0040c086`: 0.5 at a tie; the trailing side gets the value above 0.5.
    pub fn meter(&self, side: Side) -> f64 {
        let fly = self.fly.score + self.saved_value();
        let farmer = self.farmer.score + KILL_VALUE * self.kills;
        let diff = (fly - farmer).clamp(-METER_CLAMP, METER_CLAMP);
        if diff == 0 {
            return 0.5;
        }
        let r = (1.0 - f64::from(diff.abs()) / f64::from(METER_CLAMP)) / 2.0;
        let trailing = (diff > 0 && side == Side::Farmer) || (diff < 0 && side == Side::Fly);
        if trailing { 1.0 - r } else { r }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;

    fn with_cx<T>(seed: u32, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let (mut rng, data, mut events) = (MsvcRand::new(seed), GameData::synthetic(), Vec::new());
        f(&mut Ctx::new(0, &mut rng, &data, &mut events))
    }

    fn rich() -> Economy {
        let mut e = Economy::new(Setup::Normal, false);
        e.fly.money = 100_000;
        e.farmer.money = 100_000;
        e
    }

    /// Buys and grants `item` until refused; returns the prices paid.
    fn buy_all(e: &mut Economy, item: Item) -> (Vec<i32>, BuyError) {
        let mut paid = Vec::new();
        for _ in 0..100 {
            let before = e.item_prices[item as usize];
            if let Err(err) = e.buy(item) {
                return (paid, err);
            }
            paid.push(before);
            e.grant(item);
        }
        unreachable!("cap not reached")
    }

    /// AR §7.2 step 5.
    #[test]
    fn paid_sequences_follow_the_price_rises() {
        let mut e = rich();
        let (level, err) = buy_all(&mut e, Item::FarmerLevel);
        assert_eq!(level, [200, 400, 600, 800, 999]);
        assert_eq!(err, BuyError::AtCap);
        assert_eq!(e.farmer.money, 100_000 - 2999);
        let (strength, _) = buy_all(&mut e, Item::FlyStrength);
        assert_eq!(strength, [100, 200, 300, 400, 500]);
        let (tokens, _) = buy_all(&mut e, Item::Tokens);
        assert_eq!(tokens, [55; 14]);
        assert_eq!(e.farmer.tokens, 14);
    }

    #[test]
    fn a_purchase_needs_the_money_and_counts_only_when_granted() {
        let mut e = Economy::new(Setup::Normal, false);
        e.fly.money = 99;
        assert_eq!(e.buy(Item::FlySpeed), Err(BuyError::NoMoney));
        e.fly.money = 100;
        assert_eq!(e.buy(Item::FlySpeed), Ok(()));
        assert_eq!((e.fly.money, e.fly.speed), (0, 0));
        e.grant(Item::FlySpeed);
        assert_eq!(e.owned(Item::FlySpeed), 1);
        assert_eq!(e.item_prices[Item::FlySpeed as usize], 200);
    }

    /// AR §7.1 with seed 1: 41 % 10 = 1 < 5 gives −1; 18467 % 30 = 17.
    #[test]
    fn fluctuate_returns_the_old_price_and_moves_it_in_float32() {
        let mut rng = MsvcRand::new(1);
        let mut price = 60.0_f32;
        assert_eq!(fluctuate(&mut price, &mut rng), 60);
        assert_eq!(price, 60.0_f32 - (17.0_f32 / 100.0) * 60.0_f32);
        assert!((price - 49.8).abs() < 1e-5);
        let mut cheap = 1.0_f32;
        // seed 1 again: −0.17 × 1 = 0.83, floored to 1.0.
        assert_eq!(fluctuate(&mut cheap, &mut MsvcRand::new(1)), 1);
        assert_eq!(cheap, PRICE_FLOOR);
    }

    /// AR §7.3: old price × saved; present types with nothing saved still fluctuate.
    #[test]
    fn the_fly_payout_fluctuates_every_present_type() {
        let mut e = Economy::new(Setup::Extreme, true);
        e.start_period(&[true, true, false]);
        e.saved[0] = Some(2);
        let lines = with_cx(1, |cx| {
            let lines = e.pay_fly(cx);
            // Type 0 draws 41, 18467; type 1 draws 6334, 26500; type 2 is absent.
            assert_eq!(cx.rng.rand(), 19169);
            lines
        });
        assert_eq!(lines, [160]);
        assert_eq!((e.fly.money, e.fly.score), (160, 160));
        // Type 1: 6334 % 10 = 4 → −1, 26500 % 30 = 10 → 50 − 0.1 × 50.
        assert_eq!(e.food_prices[1], 50.0_f32 - (10.0_f32 / 100.0) * 50.0_f32);
        assert_eq!(e.food_prices[2], 20.0);
    }

    #[test]
    fn the_farmer_payout_is_42_per_kill_and_keeps_the_type_list() {
        let mut e = Economy::new(Setup::Normal, false);
        e.kills = 3;
        e.kill_log = vec![3, 2, 1];
        assert_eq!(e.pay_farmer(), 126);
        assert_eq!((e.farmer.money, e.farmer.score, e.kills), (126, 126, 0));
        assert_eq!(e.kill_log, [3, 2, 1]);
    }

    /// AR §8.2.
    #[test]
    fn the_meter_favours_the_trailing_side() {
        let mut e = Economy::new(Setup::Normal, false);
        assert_eq!((e.meter(Side::Fly), e.meter(Side::Farmer)), (0.5, 0.5));
        e.fly.score = 500;
        assert_eq!((e.meter(Side::Fly), e.meter(Side::Farmer)), (0.25, 0.75));
        e.fly.score = 0;
        e.kills = 100; // 4200 behind, clamped to 1000
        assert_eq!((e.meter(Side::Fly), e.meter(Side::Farmer)), (1.0, 0.0));
    }

    #[test]
    fn the_meter_and_top_up_count_saved_goods_and_kills() {
        let mut e = Economy::new(Setup::Extreme, true);
        e.start_period(&[true, true, true]);
        e.saved = vec![Some(1), Some(0), None];
        e.kills = 2;
        // fly 80, farmer 84: diff −4, r = (1 − 0.004)/2.
        assert_eq!(e.meter(Side::Fly), 1.0 - (1.0 - 4.0 / 1000.0) / 2.0);
        e.top_up();
        assert_eq!((e.fly.score, e.farmer.score), (80, 84));
    }

    #[test]
    fn start_period_marks_absent_types_unsaved() {
        let mut e = Economy::new(Setup::Normal, false);
        let mut present = vec![false; 22];
        present[3] = true;
        e.start_period(&present);
        assert_eq!(e.saved[3], Some(0));
        assert_eq!(e.saved.iter().filter(|s| s.is_none()).count(), 21);
    }
}
