//! Scripted key input and screenshot ticks for headless runs (`--keys`, `--shot`).

use std::path::PathBuf;

use thefly_platform::Key;
use thefly_sim::input::Scancode;

/// One key held from tick `from` to tick `to` inclusive (a tap is `from == to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hold {
    pub from: u64,
    pub to: u64,
    pub key: Scancode,
}

#[derive(Debug, Clone, Default)]
pub struct Script {
    pub holds: Vec<Hold>,
    pub shots: Vec<(u64, PathBuf)>,
    pub exit_after_shots: bool,
    /// Run the sim faster than real time until this tick (`--warp`); key edges repeat while it does.
    pub warp_until: u64,
    /// Take the shots of the whole window, bars included (`--window`), not of the 800 × 600 frame.
    pub window_shots: bool,
}

impl Script {
    /// `(held, pressed, released)` for `key` at `tick`.
    pub fn at(&self, tick: u64, key: Scancode) -> (bool, bool, bool) {
        let held = |t: u64| {
            self.holds
                .iter()
                .any(|h| h.key == key && (h.from..=h.to).contains(&t))
        };
        let now = held(tick);
        let before = tick > 0 && held(tick - 1);
        (now, now && !before, !now && before)
    }
}

/// `"200:Return,400-420:Up"`: taps and holds by tick, keys by SDL scancode name.
pub fn parse_keys(s: &str) -> Result<Vec<Hold>, String> {
    s.split(',')
        .filter(|p| !p.is_empty())
        .map(parse_hold)
        .collect()
}

fn parse_hold(p: &str) -> Result<Hold, String> {
    let (ticks, name) = p
        .split_once(':')
        .ok_or_else(|| format!("bad key step {p:?}"))?;
    let (from, to) = match ticks.split_once('-') {
        Some((a, b)) => (tick(a)?, tick(b)?),
        None => (tick(ticks)?, tick(ticks)?),
    };
    let key = Key::from_name(name).ok_or_else(|| format!("unknown key {name:?}"))?;
    let code = u8::try_from(key as i32).map_err(|_| format!("key {name:?} out of range"))?;
    Ok(Hold {
        from,
        to: to.max(from),
        key: Scancode(code),
    })
}

fn tick(s: &str) -> Result<u64, String> {
    s.trim().parse().map_err(|_| format!("bad tick {s:?}"))
}

/// `"200:/tmp/a.png"`.
pub fn parse_shot(s: &str) -> Result<(u64, PathBuf), String> {
    let (t, path) = s.split_once(':').ok_or_else(|| format!("bad shot {s:?}"))?;
    Ok((tick(t)?, PathBuf::from(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_press_and_release_on_successive_ticks() {
        let s = Script {
            holds: parse_keys("5:Return").unwrap(),
            ..Script::default()
        };
        assert_eq!(s.at(4, Scancode::RETURN), (false, false, false));
        assert_eq!(s.at(5, Scancode::RETURN), (true, true, false));
        assert_eq!(s.at(6, Scancode::RETURN), (false, false, true));
    }

    #[test]
    fn holds_span_their_range() {
        let s = Script {
            holds: parse_keys("10-12:Up").unwrap(),
            ..Script::default()
        };
        assert_eq!(s.at(11, Scancode::UP), (true, false, false));
        assert!(s.at(13, Scancode::UP).2);
        assert_eq!(s.at(11, Scancode::DOWN), (false, false, false));
    }

    /// The 2P match script of the headless run: 2 Players → Normal → 2 days, then in each shop
    /// skip the fly tally, buy on the fly side and leave, then skip the kill tally, try a buy on
    /// the farmer side and leave, leave a traps shop at once, and go on from
    /// the bourse, bonus, winner and stats. Tick numbers follow the 2 min period (7800 ticks).
    fn two_player_match_keys() -> String {
        const MENU: &str =
            "200:Return,400:Return,460:Return,600:Down,620:Return,760:Return,900:Return";
        // shop entry → fly tally skip (a buy that fails when nothing was saved), turbo (button 2,
        // the one item the bonus money pays for), fly done; the fade to the farmer; kill tally
        // skip, farmer buy, farmer done; traps done; bourse. A buy drops for 54 ticks, so the
        // next press waits at least 55.
        const SHOP: [(u64, &str); 15] = [
            (160, "Return"),
            (200, "Right"),
            (220, "Right"),
            (260, "Return"),
            (330, "Right"),
            (350, "Right"),
            (370, "Right"),
            (400, "Return"),
            (520, "Return"),
            (600, "Tab"),
            (680, "F"),
            (740, "Tab"),
            (860, "F"),
            (920, "Tab"),
            (1060, "Return"),
        ];
        const SHOPS_AT: [u64; 5] = [9140, 18440, 30740, 40040, 49340];
        const AFTER: &str = "30740:Return,61640:Return,62100:Return,62400:Return";
        let shops = SHOPS_AT
            .iter()
            .flat_map(|p| SHOP.iter().map(move |(dt, k)| format!("{}:{k}", p + dt)));
        let mut keys: Vec<String> = vec![MENU.into()];
        keys.extend(shops);
        keys.push(AFTER.into());
        keys.join(",")
    }

    fn screen_kind(s: &thefly_sim::sim::State) -> &'static str {
        use thefly_sim::screen::{GamePhase, MatchScreen, Screen};
        let Screen::Match(m) = &s.screen else {
            return if matches!(s.screen, Screen::Menu(_)) {
                "Menu"
            } else {
                "Front"
            };
        };
        match &m.screen {
            MatchScreen::Gameplay(g) => match g.phase {
                GamePhase::Loading { .. } => "Loading",
                GamePhase::Banner { .. } => "Banner",
                GamePhase::Playing => "Playing",
            },
            MatchScreen::GoodiesShop(_) => "GoodiesShop",
            MatchScreen::TrapsShop(_) => "TrapsShop",
            MatchScreen::Bourse(_) => "Bourse",
            MatchScreen::Bonus(_) => "Bonus",
            MatchScreen::Winner(_) => "Winner",
            MatchScreen::Stats(_) => "Stats",
            MatchScreen::KeysPage(_) | MatchScreen::TutorialPage => "Pages",
        }
    }

    /// What the scripted 2P match did.
    struct Run {
        /// The screens it passed through, each with the tick it started on.
        seen: Vec<(&'static str, u64)>,
        /// Sounds played under the banner.
        banner_sounds: usize,
        /// Per goodies shop, the sides it showed in order.
        shop_sides: Vec<Vec<thefly_sim::shop::ShopSide>>,
        /// Goodies-shop purchases: the side of each drop.
        buys: Vec<thefly_sim::shop::ShopSide>,
    }

    fn goodies_shop(s: &thefly_sim::sim::State) -> Option<&thefly_sim::shop::GoodiesShop> {
        use thefly_sim::screen::{MatchScreen, Screen};
        let Screen::Match(im) = &s.screen else {
            return None;
        };
        match &im.screen {
            MatchScreen::GoodiesShop(s) => Some(s),
            _ => None,
        }
    }

    fn run_script(data: thefly_sim::data::GameData) -> Run {
        use thefly_sim::event::Event;
        use thefly_sim::input::KeyState;
        use thefly_sim::saves::Persisted;
        use thefly_sim::sim::Sim;
        const TICKS: u64 = 62_500;
        let script = Script {
            holds: parse_keys(&two_player_match_keys()).unwrap(),
            ..Script::default()
        };
        let mut codes: Vec<Scancode> = script.holds.iter().map(|h| h.key).collect();
        codes.sort_by_key(|k| k.0);
        codes.dedup();
        let mut sim = Sim::new(1, data, Persisted::default());
        let mut run = Run {
            seen: Vec::new(),
            banner_sounds: 0,
            shop_sides: Vec::new(),
            buys: Vec::new(),
        };
        let mut dropping = false;
        for t in 0..TICKS {
            let at: Vec<_> = codes.iter().map(|&k| (k, script.at(t, k))).collect();
            let keys = |pick: fn((bool, bool, bool)) -> bool| {
                at.iter().filter(move |(_, s)| pick(*s)).map(|(k, _)| *k)
            };
            let events = sim.tick(&KeyState::from_parts(
                keys(|s| s.0),
                keys(|s| s.1),
                keys(|s| s.2),
            ));
            let kind = screen_kind(sim.state());
            if kind == "Banner" {
                run.banner_sounds += events
                    .iter()
                    .filter(|e| matches!(e, Event::Sound { .. }))
                    .count();
            }
            let entered = run.seen.last().map(|s| s.0) != Some(kind);
            if entered {
                run.seen.push((kind, t));
            }
            let Some(shop) = goodies_shop(sim.state()) else {
                continue;
            };
            if entered {
                run.shop_sides.push(Vec::new());
            }
            if let Some(sides) = run
                .shop_sides
                .last_mut()
                .filter(|v| v.last() != Some(&shop.side))
            {
                sides.push(shop.side);
            }
            if shop.conveyor.is_some() && !dropping {
                run.buys.push(shop.side);
            }
            dropping = shop.conveyor.is_some();
        }
        run
    }

    /// The match reaches the winner, the stats and the menu, in that order; each goodies shop
    /// shows the fly side and then the farmer side, and the fly buys something once it has money.
    fn assert_full_run(run: &Run) {
        use thefly_sim::shop::ShopSide;
        const SHOPS: usize = 5;
        let kinds: Vec<&str> = run.seen.iter().map(|s| s.0).collect();
        for want in ["Winner", "Stats", "Menu"] {
            assert!(
                kinds.contains(&want),
                "never reached {want}: {:?}",
                run.seen
            );
        }
        let tail = &kinds[kinds.len().saturating_sub(3)..];
        assert_eq!(tail, ["Winner", "Stats", "Menu"], "{:?}", run.seen);
        assert_eq!(
            run.shop_sides,
            vec![vec![ShopSide::Fly, ShopSide::Farmer]; SHOPS]
        );
        assert!(run.buys.contains(&ShopSide::Fly), "buys {:?}", run.buys);
        assert!(run.banner_sounds > 0, "no sound under the banner");
    }

    /// Acceptance W4.2 in the sim: the headless run's script plays a 2P, 2-day match from boot
    /// through the banner, six periods, the shops, bourses and bonus rounds, to the winner, the
    /// stats and back to the menu, with sounds under the banner.
    #[test]
    fn a_scripted_two_player_match_runs_to_the_stats() {
        let run = run_script(thefly_sim::data::GameData::synthetic());
        let period = ["Loading", "Banner", "Playing"];
        let shop = ["GoodiesShop", "Bourse"];
        let day: Vec<&str> = [&period[..], &shop, &period, &shop, &period, &["Bonus"]].concat();
        let expected: Vec<&str> = [
            &["Front", "Menu"][..],
            &day,
            &shop,
            &day,
            &["Winner", "Stats", "Menu"],
        ]
        .concat();
        let kinds: Vec<&str> = run.seen.iter().map(|s| s.0).collect();
        assert_eq!(kinds, expected, "{:?}", run.seen);
        assert_full_run(&run);
    }

    /// The same script on the real data also reaches the menu the same way.
    #[test]
    #[ignore = "needs THEFLY_DATA; run with --ignored"]
    fn the_scripted_match_reaches_the_menu_on_real_data() {
        use std::path::Path;
        let Some(env) = std::env::var_os("THEFLY_DATA").map(PathBuf::from) else {
            eprintln!("THEFLY_DATA not set, skipping");
            return;
        };
        let dir = thefly_platform::data_dir::resolve(None, Some(env), Path::new("."), Path::is_dir)
            .expect("THEFLY_DATA");
        let data = crate::assets::game_data(&dir).expect("real game data");
        assert_full_run(&run_script(data));
    }

    #[test]
    fn bad_input_is_rejected() {
        assert!(parse_keys("x:Return").is_err());
        assert!(parse_keys("1:NoSuchKey").is_err());
        assert_eq!(
            parse_shot("7:/tmp/a.png").unwrap(),
            (7, PathBuf::from("/tmp/a.png"))
        );
    }
}
