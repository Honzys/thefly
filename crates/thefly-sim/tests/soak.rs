//! Soak: whole matches from boot back to the menu under seeded pseudo-random keys, in every
//! mode. Asserts no panic, no stall (each screen moves on within a tick budget) and the economy
//! invariants on every tick.
//!
//! `cargo test --test soak` runs one match per mode. The long run:
//! `SOAK_MATCHES=250 cargo test --release --test soak -- --nocapture`.

use thefly_sim::economy::{ITEMS, Item};
use thefly_sim::match_state::{MatchConfig, Mode, Players};
use thefly_sim::menu::{Page, SlidePhase};
use thefly_sim::saves::Persisted;
use thefly_sim::screen::{MatchScreen, Screen};
use thefly_sim::sim::State;
use thefly_sim::world::ai::AiLevel;
use thefly_sim::world::objects::MAX_PLACED;
use thefly_sim::{GameData, KeyState, Scancode, Sim};

/// The cheap default: one match per mode.
const DEFAULT_MATCHES: u32 = MODES.len() as u32;
/// Ticks one screen may hold before the soak calls it a stall: 15 minutes. A 2-minute period is
/// 7 800 ticks; a random human walking a shop cursor onto Done has taken 20 073.
const STALL_TICKS: u64 = 60_000;
/// Ticks one whole match may take: 18 periods of a 6-day match plus every screen between.
const MATCH_TICKS: u64 = 600_000;
/// Taps the boot, logo and menu intro may need before the menu takes keys.
const MENU_TAPS: u32 = 2_000;
/// Idle ticks a menu page's boot slide may take to land.
const SLIDE_TICKS: u32 = 2_000;
/// One key toggle in this many is Escape: rare, since random keys then answer the quit dialog
/// Yes about half the time and end the match early.
const ESCAPE_ODDS: u64 = 100_000;
/// Each game key sits this many times in the pool.
const GAME_KEY_WEIGHT: usize = 4;
/// Ticks between key toggles, at most.
const TOGGLE_GAP: u64 = 8;

#[derive(Debug, Clone, Copy)]
enum ModeKind {
    OnePlayer(AiLevel),
    Highscore,
    TwoPlayer,
    Extreme,
    Tutorial,
}

const MODES: [ModeKind; 7] = [
    ModeKind::OnePlayer(AiLevel::Easy),
    ModeKind::OnePlayer(AiLevel::Normal),
    ModeKind::OnePlayer(AiLevel::Intelligent),
    ModeKind::Highscore,
    ModeKind::TwoPlayer,
    ModeKind::Extreme,
    ModeKind::Tutorial,
];

/// SplitMix64: the soak's own key stream, apart from the game's MSVC rand.
struct Keys(u64);

impl Keys {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Game keys, weighted so a random human leaves a shop in about a thousand ticks, then letters,
/// digits and Backspace for the name entry.
fn key_pool() -> Vec<Scancode> {
    let game = [
        Scancode::UP,
        Scancode::DOWN,
        Scancode::LEFT,
        Scancode::RIGHT,
        Scancode::RETURN,
        Scancode::RSHIFT,
        Scancode::T,
        Scancode::G,
        Scancode::F,
        Scancode::H,
        Scancode::TAB,
        Scancode::Q,
        Scancode(42),
    ];
    let mut pool: Vec<Scancode> = (0..GAME_KEY_WEIGHT).flat_map(|_| game).collect();
    pool.extend((4..=39).map(Scancode));
    pool
}

struct Driver {
    sim: Sim,
    prev: KeyState,
    ticks: u64,
}

impl Driver {
    fn step(&mut self, held: &[Scancode]) {
        let keys = KeyState::next(&self.prev, held.iter().copied());
        self.sim.tick(&keys);
        self.prev = keys;
        self.ticks += 1;
    }

    fn tap(&mut self, key: Scancode) {
        self.step(&[key]);
        self.step(&[]);
    }

    fn state(&self) -> &State {
        self.sim.state()
    }
}

/// The menu takes keys: the intro is over and a sub-page's boot has landed.
fn menu_ready(s: &State) -> bool {
    let Screen::Menu(m) = &s.screen else {
        return false;
    };
    let landed = m.page == Page::Top || m.boot.phase == SlidePhase::Kick;
    m.started && m.intro.step.is_none() && landed
}

fn in_match(s: &State) -> bool {
    matches!(s.screen, Screen::Match(_))
}

/// Taps Enter through the logo and intro, then walks `path` from the top page.
fn start_match(d: &mut Driver, path: &[Scancode]) -> MatchConfig {
    for _ in 0..MENU_TAPS {
        if menu_ready(d.state()) {
            break;
        }
        d.tap(Scancode::RETURN);
    }
    assert!(menu_ready(d.state()), "the menu never took keys");
    for &key in path {
        d.tap(key);
        for _ in 0..SLIDE_TICKS {
            if menu_ready(d.state()) || in_match(d.state()) {
                break;
            }
            d.step(&[]);
        }
    }
    let Screen::Match(im) = &d.state().screen else {
        panic!("{path:?} started no match");
    };
    im.m.config
}

fn menu_path(mode: ModeKind, days_index: usize) -> Vec<Scancode> {
    let (down, ok, up) = (Scancode::DOWN, Scancode::RETURN, Scancode::UP);
    let length = std::iter::repeat_n(down, days_index).chain([ok]);
    let path: Vec<Scancode> = match mode {
        ModeKind::OnePlayer(level) => {
            let pick = match level {
                AiLevel::Easy => vec![up],
                AiLevel::Normal => vec![],
                AiLevel::Intelligent => vec![down],
            };
            [ok, ok, ok]
                .into_iter()
                .chain(pick)
                .chain([ok])
                .chain(length)
                .collect()
        }
        ModeKind::Highscore => vec![ok, ok, down, ok],
        ModeKind::TwoPlayer | ModeKind::Tutorial => {
            [ok, down, ok, ok].into_iter().chain(length).collect()
        }
        ModeKind::Extreme => vec![ok, down, ok, down, ok],
    };
    path
}

fn check_config(mode: ModeKind, days_index: usize, c: &MatchConfig) {
    let days = [2, 4, 6][days_index];
    let expected = match mode {
        ModeKind::OnePlayer(level) => (Players::One, Mode::Normal, Some(level), days, false),
        ModeKind::Highscore => (Players::One, Mode::Special, Some(AiLevel::Normal), 3, false),
        ModeKind::TwoPlayer => (Players::Two, Mode::Normal, None, days, false),
        ModeKind::Tutorial => (Players::Two, Mode::Normal, None, days, true),
        ModeKind::Extreme => (Players::Two, Mode::Special, None, c.days, false),
    };
    let ai = (c.players == Players::One).then_some(c.ai_level);
    assert_eq!((c.players, c.mode, ai, c.days, c.tutorial), expected);
}

/// What counts as the game moving on: the screen, the in-match screen and the period.
fn progress_mark(s: &State) -> (std::mem::Discriminant<Screen>, Option<String>) {
    let inner = match &s.screen {
        Screen::Match(im) => Some(format!(
            "{:?} {:?}",
            std::mem::discriminant(&im.screen),
            im.m.progress
        )),
        _ => None,
    };
    (std::mem::discriminant(&s.screen), inner)
}

fn check_invariants(s: &State) {
    let Screen::Match(im) = &s.screen else {
        return;
    };
    let e = &im.m.economy;
    assert!(e.fly.money >= 0 && e.farmer.money >= 0, "money {e:?}");
    let owned = [
        (Item::FarmerLevel, e.farmer.level),
        (Item::Tokens, e.farmer.tokens),
        (Item::Triple, e.farmer.triple),
        (Item::Nuclear, e.farmer.nuclear),
        (Item::Boot, e.farmer.boot),
        (Item::FlyStrength, e.fly.strength),
        (Item::FlySpeed, e.fly.speed),
        (Item::Turbo, e.fly.turbo),
        (Item::SuperFly, e.fly.superfly),
        (Item::Midges, e.fly.midges),
    ];
    for (item, n) in owned {
        let cap = ITEMS[item as usize].cap;
        assert!((0..=cap).contains(&n), "{item:?} = {n}, cap {cap}");
    }
    if let MatchScreen::Gameplay(g) = &im.screen {
        let placed = g
            .world
            .objects
            .list
            .iter()
            .filter(|o| o.collectible)
            .count();
        assert!(placed <= MAX_PLACED, "{placed} objects");
    }
}

#[derive(Default)]
struct Totals {
    matches: u32,
    /// Matches that reached the stats screen rather than quitting early.
    finished: u32,
    ticks: u64,
    longest_screen: u64,
}

/// One match: menu → match under random keys → back at the menu.
fn soak_one(seed: u32, totals: &mut Totals) {
    let i = seed as usize;
    let mode = MODES[i % MODES.len()];
    let days_index = i / MODES.len() % 3;
    let mut persisted = Persisted::default();
    persisted.options.tutorial = matches!(mode, ModeKind::Tutorial);
    let mut d = Driver {
        sim: Sim::new(seed, GameData::synthetic(), persisted),
        prev: KeyState::NONE,
        ticks: 0,
    };
    let c = start_match(&mut d, &menu_path(mode, days_index));
    check_config(mode, days_index, &c);

    let pool = key_pool();
    let mut keys = Keys(u64::from(seed));
    let mut held: Vec<Scancode> = Vec::new();
    let mut next_toggle = 0;
    let (mut mark, mut since) = (progress_mark(d.state()), 0_u64);
    let start = d.ticks;
    let mut finished = false;
    while !matches!(d.state().screen, Screen::Menu(_)) {
        assert!(
            d.ticks - start < MATCH_TICKS,
            "seed {seed}: no menu after the match"
        );
        if next_toggle == 0 {
            // Escape is a tap: held, it would mask every later press of it.
            held.retain(|&k| k != Scancode::ESCAPE);
            let key = if keys.below(ESCAPE_ODDS) == 0 {
                Scancode::ESCAPE
            } else {
                pool[keys.below(pool.len() as u64) as usize]
            };
            match held.iter().position(|&k| k == key) {
                Some(at) => {
                    held.swap_remove(at);
                }
                None => held.push(key),
            }
            next_toggle = keys.below(TOGGLE_GAP) + 1;
        }
        next_toggle -= 1;
        d.step(&held);
        check_invariants(d.state());
        let now = progress_mark(d.state());
        if now == mark {
            since += 1;
            if since >= STALL_TICKS {
                let Screen::Match(im) = &d.state().screen else {
                    panic!("seed {seed} {mode:?}: stalled on {mark:?}");
                };
                panic!("seed {seed} {mode:?}: stalled in {:#?}", im.screen);
            }
            continue;
        }
        totals.longest_screen = totals.longest_screen.max(since);
        (mark, since) = (now, 0);
        finished |= matches!(&d.state().screen, Screen::Match(im) if matches!(im.screen, MatchScreen::Stats(_)));
    }
    totals.matches += 1;
    totals.finished += u32::from(finished);
    totals.ticks += d.ticks - start;
}

#[test]
fn random_keys_play_every_mode_from_boot_back_to_the_menu() {
    let env = |name: &str, default: u32| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let (first, count) = (env("SOAK_SEED", 1), env("SOAK_MATCHES", DEFAULT_MATCHES));
    let mut totals = Totals::default();
    for seed in first..first + count {
        soak_one(seed, &mut totals);
    }
    eprintln!(
        "soak: {} matches from seed {first} ({} to the stats), {} ticks, longest screen {} ticks",
        totals.matches, totals.finished, totals.ticks, totals.longest_screen
    );
    assert_eq!(totals.matches, count);
}
