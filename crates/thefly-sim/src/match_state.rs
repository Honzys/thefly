//! W3. Everything that lives for a whole match: the menu's configuration, period/day progression,
//! the economy, the statistics, the shop's trap layout and the winch slots (game-spec §2.4–§2.6,
//! ai-and-rules §9).

use crate::ctx::Ctx;
use crate::economy::{Economy, Setup};
use crate::stats::Stats;
use crate::world::ai::AiLevel;
use crate::world::lift::{ANCHOR_GAP, SHOP_SCALE, draw_anchors};
use crate::world::traps::TrapKind;

/// Period clock limit in minutes (data+0x26), and in tutorial mode.
pub const PERIOD_MINUTES: u64 = 2;
pub const TUTORIAL_PERIOD_MINUTES: u64 = 3;
/// Trap slots at data+0x2a2; the caps keep the count at 14 or fewer.
pub const TRAP_SLOTS: usize = 15;
/// The kill-type list at data+0x15c… holds 50 entries and is not bounds-checked; index 0 unused.
/// UB guard: kills past the end are counted but their type is not stored.
pub const KILL_LOG_CAP: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Players {
    /// The human is the fly; the AI plays the farmer.
    #[default]
    One,
    Two,
}

/// data+0x24: 1 normal; 2 is Highscore with one player and Extreme with two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Special,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchConfig {
    pub players: Players,
    pub mode: Mode,
    pub ai_level: AiLevel,
    /// 2, 4 or 6; 3 in Highscore mode.
    pub days: u8,
    /// Options toggle; makes periods 3 minutes and uses the tutorial inventories.
    pub tutorial: bool,
}

impl Default for MatchConfig {
    /// Menu defaults: 2 days, 1 player, mode 1, AI level 1 (game-spec §2.3).
    fn default() -> Self {
        Self {
            players: Players::One,
            mode: Mode::Normal,
            ai_level: AiLevel::Normal,
            days: 2,
            tutorial: false,
        }
    }
}

impl MatchConfig {
    /// data+0x28: two players and mode 2.
    pub fn extreme(&self) -> bool {
        self.players == Players::Two && self.mode == Mode::Special
    }

    /// `FUN_0040aac0` == 2: one player and mode 2.
    pub fn highscore(&self) -> bool {
        self.players == Players::One && self.mode == Mode::Special
    }
}

/// data+0x20, as meal bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meal {
    Breakfast = 1,
    Lunch = 2,
    Dinner = 4,
}

/// data+0x2c: 1 inside, 2 outside; extreme mode uses the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    Inside,
    Outside,
    Arena,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// None before the first period (data+0x20 == 0).
    pub meal: Option<Meal>,
    pub day: u8,
    /// None before the first period (data+0x2c == 0).
    pub outside: Option<bool>,
}

/// `FUN_0040b7b2`'s result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advance {
    Next,
    Over,
}

/// The period clock (`FUN_0044214e`, `FUN_004421dd`): elapsed = trunc((now − start − paused)
/// / 1000) s; the period ends when elapsed ≥ minutes × 60.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeriodClock {
    pub start_ms: u64,
    pub paused_ms: u64,
    pub pause_started: Option<u64>,
    pub limit_s: u64,
}

impl PeriodClock {
    pub fn new(now: u64, minutes: u64) -> Self {
        Self {
            start_ms: now,
            limit_s: minutes * 60,
            ..Self::default()
        }
    }

    /// While paused the clock stands still at the moment the pause began.
    pub fn elapsed_s(&self, now: u64) -> u64 {
        let end = self.pause_started.unwrap_or(now);
        end.saturating_sub(self.start_ms)
            .saturating_sub(self.paused_ms)
            / 1000
    }

    pub fn expired(&self, now: u64) -> bool {
        self.elapsed_s(now) >= self.limit_s
    }

    pub fn pause(&mut self, now: u64) {
        self.pause_started.get_or_insert(now);
    }

    pub fn resume(&mut self, now: u64) {
        if let Some(from) = self.pause_started.take() {
            self.paused_ms += now.saturating_sub(from);
        }
    }
}

/// One placed trap in the shop: type and stored x (shop px / 0.152).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrapSlot {
    pub kind: TrapKind,
    pub x: i32,
    pub y: i32,
}

/// The farmer's trap layout for the next period (data+0x2a2, trapCount data+0x2fc).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrapLayout {
    pub slots: Vec<TrapSlot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceError {
    /// Overlaps a trap or a winch slot.
    Blocked,
    /// The type's cap is reached, or tokens do not cover the cost.
    Refused,
}

impl TrapLayout {
    /// `FUN_0040bd61` + `FUN_00425456`: validate shop x against traps and winch slots, store,
    /// deduct the cost (floor 0). No 15-slot guard; the caps bound the count.
    pub fn place(
        &mut self,
        kind: TrapKind,
        shop_x: i32,
        winch: &[i32; 3],
        tokens: &mut i32,
    ) -> Result<(), PlaceError> {
        let spec = kind.spec();
        // FUN_00424db1: flypaper needs 1 token, a fan tokens/2 ≥ 1, a zapper tokens/4 ≥ 1.
        if *tokens < spec.cost || self.count(kind) >= spec.cap {
            return Err(PlaceError::Refused);
        }
        let w = spec.width;
        let on_trap = self.slots.iter().any(|s| {
            let sj = to_shop(s.x);
            (sj - w..=sj + s.kind.spec().width).contains(&shop_x)
        });
        let on_winch = winch.iter().any(|&stored| {
            let ws = to_shop(stored);
            (ws - w..=ws + ANCHOR_GAP).contains(&shop_x)
        });
        if on_trap || on_winch {
            return Err(PlaceError::Blocked);
        }
        self.slots.push(TrapSlot {
            kind,
            x: (f64::from(shop_x) / SHOP_SCALE) as i32,
            y: spec.stored_y,
        });
        *tokens = (*tokens - spec.cost).max(0);
        Ok(())
    }

    pub fn count(&self, kind: TrapKind) -> usize {
        let fan = |k: TrapKind| matches!(k, TrapKind::FanLeft | TrapKind::FanRight);
        self.slots
            .iter()
            .filter(|s| s.kind == kind || (fan(kind) && fan(s.kind)))
            .count()
    }
}

/// Stored units back to shop px: trunc(stored × 0.152) (ai-and-rules §4.3).
pub fn to_shop(stored: i32) -> i32 {
    (f64::from(stored) * SHOP_SCALE) as i32
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchState {
    pub config: MatchConfig,
    pub progress: Progress,
    pub economy: Economy,
    pub stats: Stats,
    pub traps: TrapLayout,
    /// The three winch slots (lift anchors), stored units.
    pub winch: [i32; 3],
    /// Picked per match when music is on.
    pub music_game2: bool,
}

impl MatchState {
    /// `FUN_00408113` steps 1–3: reset, starting inventories, extreme setup, music pick.
    ///
    /// Draw order (decisions in the plan): the winch slots first (`FUN_0040b994`, gap 36), then,
    /// only when music is on, one `rand % 2` for the track [I].
    pub fn new(config: MatchConfig, music_on: bool, cx: &mut Ctx) -> Self {
        let setup = if config.tutorial {
            Setup::Tutorial
        } else if config.extreme() {
            Setup::Extreme
        } else {
            Setup::Normal
        };
        let winch = draw_anchors(ANCHOR_GAP, cx);
        let music_game2 = music_on && cx.rng.odd();
        Self {
            config,
            progress: Progress::default(),
            economy: Economy::new(setup, config.extreme()),
            stats: Stats::default(),
            traps: TrapLayout::default(),
            winch,
            music_game2,
        }
    }

    /// `FUN_0040b7b2`.
    pub fn advance(&mut self, cx: &mut Ctx) -> Advance {
        if self.config.extreme() {
            return self.advance_extreme(cx);
        }
        let p = &mut self.progress;
        p.meal = Some(match p.meal {
            None => {
                p.day = 1;
                Meal::Breakfast
            }
            Some(Meal::Breakfast) => Meal::Lunch,
            Some(Meal::Lunch) => Meal::Dinner,
            Some(Meal::Dinner) => {
                p.day += 1;
                Meal::Breakfast
            }
        });
        p.outside = Some(p.outside.is_some_and(|o| !o));
        if p.day > self.config.days {
            return Advance::Over;
        }
        self.stats.periods += 1;
        Advance::Next
    }

    /// Extreme: one period at a random meal and side, then over. The draws follow the order
    /// AR §9.1 lists them (meal, side, winch). shortcut: the order inside `FUN_0040b7b2` is not
    /// stated; revisit if the binary is re-read.
    fn advance_extreme(&mut self, cx: &mut Ctx) -> Advance {
        const MEALS: [Meal; 3] = [Meal::Breakfast, Meal::Lunch, Meal::Dinner];
        if self.progress.meal.is_some() {
            return Advance::Over;
        }
        let meal = MEALS[cx.rng.r(3) as usize];
        let outside = cx.rng.odd();
        self.progress = Progress {
            meal: Some(meal),
            day: 1,
            outside: Some(outside),
        };
        self.winch = draw_anchors(ANCHOR_GAP, cx);
        self.traps.slots.clear();
        self.stats.periods += 1;
        Advance::Next
    }

    /// `FUN_0040b922`: a bonus round follows this period.
    pub fn bonus_due(&self) -> bool {
        self.config.extreme() || self.progress.meal == Some(Meal::Dinner)
    }

    /// `FUN_0040b94c`: the match ends after this period (and its bonus round, if due).
    pub fn over_after_period(&self) -> bool {
        self.config.extreme()
            || (self.progress.meal == Some(Meal::Dinner)
                && self.config.days < self.progress.day + 1)
    }

    /// AR §3.1 step 2: the shop before the final dinner.
    pub fn last_shop(&self) -> bool {
        self.progress.meal == Some(Meal::Lunch) && self.config.days <= self.progress.day
    }

    /// Extreme mode always plays in the arena; otherwise the period's inside/outside flag, with
    /// inside before the first period.
    pub fn scene(&self) -> Scene {
        if self.config.extreme() {
            return Scene::Arena;
        }
        match self.progress.outside {
            Some(true) => Scene::Outside,
            _ => Scene::Inside,
        }
    }

    /// Period clock limit in minutes.
    pub fn period_minutes(&self) -> u64 {
        if self.config.tutorial {
            TUTORIAL_PERIOD_MINUTES
        } else {
            PERIOD_MINUTES
        }
    }

    /// `FUN_0040bc5e`: kills + 1, stats kills + 1, kill type by this life's hits (3/2/1).
    pub fn record_kill(&mut self, hits: i32) {
        let e = &mut self.economy;
        e.kills += 1;
        self.stats.kills += 1;
        // A kill with no hit (boot, zapper) is taken as the one-hit case [I]: §7.6 lists 1, 2–3
        // and 4+ only.
        let kind = match hits {
            ..=1 => 3,
            2..=3 => 2,
            _ => 1,
        };
        // Index 0 of the original list is unused, so it holds KILL_LOG_CAP − 1 types.
        if e.kill_log.len() < KILL_LOG_CAP - 1 {
            e.kill_log.push(kind);
        }
    }

    /// Delivery (ai-and-rules §7.5): `saved[type] += 1`, stats delivered and price total. The
    /// sound (slot 11) is the caller's.
    pub fn record_delivery(&mut self, kind: usize) {
        let e = &mut self.economy;
        if let Some(Some(saved)) = e.saved.get_mut(kind) {
            *saved += 1;
        }
        self.stats.delivered += 1;
        self.stats.price_total += e.food_prices.get(kind).map_or(0, |&p| p as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::GameData;
    use crate::economy::{Setup, starting_farmer, starting_fly};
    use crate::rng::MsvcRand;

    fn build(config: MatchConfig, music: bool) -> (MatchState, MsvcRand) {
        let (mut rng, data, mut events) = (MsvcRand::new(1), GameData::synthetic(), Vec::new());
        let m = MatchState::new(
            config,
            music,
            &mut Ctx::new(0, &mut rng, &data, &mut events),
        );
        (m, rng)
    }

    #[test]
    fn a_default_match_starts_with_the_normal_inventories() {
        let (m, _) = build(MatchConfig::default(), false);
        assert_eq!(m.economy.fly, starting_fly(Setup::Normal));
        assert_eq!(m.economy.farmer, starting_farmer(Setup::Normal));
        assert_eq!(m.economy.food_prices.len(), 22);
        assert_eq!(m.progress, Progress::default());
        assert_eq!((m.scene(), m.period_minutes()), (Scene::Inside, 2));
    }

    #[test]
    fn extreme_uses_the_extreme_inventories_prices_and_arena() {
        let config = MatchConfig {
            players: Players::Two,
            mode: Mode::Special,
            ..MatchConfig::default()
        };
        let (m, _) = build(config, false);
        assert_eq!(m.economy.fly, starting_fly(Setup::Extreme));
        assert_eq!(m.economy.food_prices, crate::economy::EXTREME_PRICES);
        assert_eq!(m.scene(), Scene::Arena);
    }

    #[test]
    fn tutorial_wins_over_extreme_and_lasts_three_minutes() {
        let config = MatchConfig {
            players: Players::Two,
            mode: Mode::Special,
            tutorial: true,
            ..MatchConfig::default()
        };
        let (m, _) = build(config, false);
        assert_eq!(m.economy.fly, starting_fly(Setup::Tutorial));
        assert_eq!(m.period_minutes(), 3);
    }

    /// Seed 1: the winch takes the first three draws, the music coin the fourth, and only when
    /// music is on.
    #[test]
    fn the_music_coin_is_drawn_only_when_music_is_on() {
        let (off, rng_off) = build(MatchConfig::default(), false);
        let (on, rng_on) = build(MatchConfig::default(), true);
        assert!(!off.music_game2);
        assert_eq!(off.winch, on.winch);
        let mut probe = MsvcRand::new(1);
        (0..3).for_each(|_| _ = probe.rand());
        assert_eq!(rng_off, probe);
        assert_eq!(on.music_game2, probe.odd());
        assert_eq!(rng_on, probe);
    }

    fn cx_run<T>(rng: &mut MsvcRand, f: impl FnOnce(&mut Ctx) -> T) -> T {
        let (data, mut events) = (GameData::synthetic(), Vec::new());
        f(&mut Ctx::new(0, rng, &data, &mut events))
    }

    /// AR §9.1: normal 2 days is B1, L1, D1, B2, L2, D2, then over; sides alternate.
    #[test]
    fn a_normal_match_advances_through_the_meals_and_days() {
        let (mut m, mut rng) = build(MatchConfig::default(), false);
        let before = rng.clone();
        let mut seen = Vec::new();
        for _ in 0..6 {
            assert_eq!(cx_run(&mut rng, |cx| m.advance(cx)), Advance::Next);
            let p = m.progress;
            seen.push((p.meal.unwrap(), p.day, p.outside.unwrap()));
        }
        use Meal::*;
        assert_eq!(
            seen,
            [
                (Breakfast, 1, false),
                (Lunch, 1, true),
                (Dinner, 1, false),
                (Breakfast, 2, true),
                (Lunch, 2, false),
                (Dinner, 2, true),
            ]
        );
        assert_eq!(cx_run(&mut rng, |cx| m.advance(cx)), Advance::Over);
        assert_eq!(m.stats.periods, 6, "the over call is not counted");
        assert_eq!(rng, before, "a normal advance draws nothing");
    }

    /// AR §9.1 on a fresh seed 1: the meal, the side, then three winch draws.
    #[test]
    fn extreme_draws_one_period_and_rerolls_the_winch() {
        let config = MatchConfig {
            players: Players::Two,
            mode: Mode::Special,
            ..MatchConfig::default()
        };
        let (mut m, _) = build(config, false);
        let mut rng = MsvcRand::new(1);
        m.traps.slots.push(TrapSlot {
            kind: TrapKind::Zapper,
            x: 0,
            y: 0,
        });
        assert_eq!(cx_run(&mut rng, |cx| m.advance(cx)), Advance::Next);
        // Fresh seed 1: 41 % 3 = 2 → dinner; 18467 is odd → outside.
        assert_eq!(m.progress.meal, Some(Meal::Dinner));
        assert_eq!((m.progress.day, m.progress.outside), (1, Some(true)));
        assert!(m.traps.slots.is_empty());
        // The winch takes the next three draws: 6334 % 761, 26500 % 761, 19169 % 761.
        let shop = [6334 % 761, 26500 % 761, 19169 % 761];
        assert_eq!(shop, [246, 626, 144]);
        assert_eq!(m.winch, shop.map(|v| (f64::from(v) / 0.152) as i32));
        assert!(m.bonus_due() && m.over_after_period());
        assert_eq!(cx_run(&mut rng, |cx| m.advance(cx)), Advance::Over);
        assert_eq!(m.stats.periods, 1);
    }

    #[test]
    fn the_bonus_follows_dinner_and_the_match_ends_after_the_last_dinner() {
        let (mut m, mut rng) = build(MatchConfig::default(), false);
        let mut flags = Vec::new();
        for _ in 0..6 {
            cx_run(&mut rng, |cx| m.advance(cx));
            flags.push((m.bonus_due(), m.over_after_period(), m.last_shop()));
        }
        assert_eq!(
            flags,
            [
                (false, false, false),
                (false, false, false),
                (true, false, false),
                (false, false, false),
                (false, false, true),
                (true, true, false),
            ]
        );
    }

    /// AR §7.6.
    #[test]
    fn kills_count_and_log_their_type() {
        let (mut m, _) = build(MatchConfig::default(), false);
        for hits in [1, 2, 3, 4, 9] {
            m.record_kill(hits);
        }
        assert_eq!((m.economy.kills, m.stats.kills), (5, 5));
        assert_eq!(m.economy.kill_log, [3, 2, 2, 1, 1]);
        for _ in 0..100 {
            m.record_kill(1);
        }
        assert_eq!(m.economy.kills, 105);
        assert_eq!(m.economy.kill_log.len(), KILL_LOG_CAP - 1);
    }

    /// AR §7.5, gaps §11.3.
    #[test]
    fn a_delivery_counts_saved_and_the_truncated_price() {
        let (mut m, _) = build(MatchConfig::default(), false);
        let mut present = vec![false; 22];
        present[0] = true;
        m.economy.start_period(&present);
        m.economy.food_prices[0] = 49.8;
        m.record_delivery(0);
        m.record_delivery(0);
        assert_eq!(m.economy.saved[0], Some(2));
        assert_eq!((m.stats.delivered, m.stats.price_total), (2, 98));
    }

    /// AR §4.3: traps and winch block their spans, bounds inclusive; caps and tokens refuse.
    #[test]
    fn the_validator_rejects_spans_and_deducts_tokens() {
        let mut layout = TrapLayout::default();
        let winch = [(700.0 / 0.152) as i32; 3]; // shop x 699 or 700 after the round trip
        let ws = to_shop(winch[0]);
        let mut tokens = 14;
        assert_eq!(
            layout.place(TrapKind::Zapper, 100, &winch, &mut tokens),
            Ok(())
        );
        assert_eq!(tokens, 10);
        assert_eq!(layout.slots[0].x, (100.0 / 0.152) as i32);
        let zs = to_shop(layout.slots[0].x);
        // A flypaper (width 8) is blocked from zs − 8 to zs + 97.
        let fp = TrapKind::Flypaper;
        assert_eq!(
            layout.place(fp, zs - 8, &winch, &mut tokens),
            Err(PlaceError::Blocked)
        );
        assert_eq!(
            layout.place(fp, zs + 97, &winch, &mut tokens),
            Err(PlaceError::Blocked)
        );
        assert_eq!(
            layout.place(fp, ws + 36, &winch, &mut tokens),
            Err(PlaceError::Blocked)
        );
        assert_eq!(
            layout.place(fp, ws - 8, &winch, &mut tokens),
            Err(PlaceError::Blocked)
        );
        assert_eq!(layout.place(fp, zs + 98, &winch, &mut tokens), Ok(()));
        assert_eq!(
            layout.place(TrapKind::FanRight, 400, &winch, &mut tokens),
            Ok(())
        );
        assert_eq!((layout.slots[2].y, tokens), (600, 7));
        assert_eq!(
            layout.count(TrapKind::FanLeft),
            1,
            "the fans share their count"
        );
        let mut one = 1;
        assert_eq!(
            layout.place(TrapKind::FanLeft, 500, &winch, &mut one),
            Err(PlaceError::Refused)
        );
    }

    #[test]
    fn the_period_clock_stands_still_while_paused() {
        let mut clock = PeriodClock::new(1000, 2);
        assert_eq!(clock.elapsed_s(1000 + 61_999), 61);
        clock.pause(11_000);
        assert_eq!(clock.elapsed_s(500_000), 10);
        clock.resume(41_000);
        assert_eq!(clock.elapsed_s(41_000), 10);
        assert!(!clock.expired(1000 + 120_000 + 29_999) && clock.expired(1000 + 120_000 + 30_000));
    }
}
