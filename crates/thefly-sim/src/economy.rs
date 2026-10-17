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
    let _ = (price, rng);
    todo!("W3 ai-and-rules §7.1")
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
    /// Kill types (3/2/1), capped at [`crate::match_state::KILL_LOG_CAP`].
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
        let _ = item;
        todo!("W3 ai-and-rules §7.2")
    }

    /// `FUN_00423047`: the bought item reaches the bottom of the conveyor.
    pub fn grant(&mut self, item: Item) {
        let _ = item;
        todo!("W3 ai-and-rules §7.2 step 3")
    }

    pub fn owned(&self, item: Item) -> i32 {
        let _ = item;
        todo!("W3")
    }

    /// `FUN_0042ac27`'s economy part: present flags and saved counts for the new period.
    pub fn start_period(&mut self, present: &[bool]) {
        let _ = present;
        todo!("W3 ai-and-rules §7.1, §7.3")
    }

    /// The fly's payout (ai-and-rules §7.3): fluctuates every present type in order.
    pub fn pay_fly(&mut self, cx: &mut Ctx) -> i32 {
        let _ = cx;
        todo!("W3 ai-and-rules §7.3")
    }

    /// The farmer's payout: 42 × kills, then kills = 0.
    pub fn pay_farmer(&mut self) -> i32 {
        todo!("W3 ai-and-rules §7.3")
    }

    /// `FUN_0040bb54` + `FUN_0040bbe5`: the winner screen's top-up, no fluctuation.
    pub fn top_up(&mut self) {
        todo!("W3 ai-and-rules §8.4")
    }

    /// `FUN_0040c086`.
    pub fn meter(&self, side: Side) -> f64 {
        let _ = side;
        todo!("W3 ai-and-rules §8.2")
    }
}
