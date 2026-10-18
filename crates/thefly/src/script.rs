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

    /// Acceptance W4.2 in the sim: boot → menu → 2 Players → Normal → 2 days → keys page →
    /// loading → the "3, 2, 1, Go!" banner, driven by the same script the headless run uses.
    #[test]
    #[ignore = "needs W3"]
    fn a_scripted_two_player_run_reaches_the_banner() {
        use thefly_sim::data::GameData;
        use thefly_sim::input::KeyState;
        use thefly_sim::saves::Persisted;
        use thefly_sim::screen::{GamePhase, MatchScreen, Screen};
        use thefly_sim::sim::Sim;
        const MAX_TICKS: u64 = 65 * 60;
        let keys = "200:Return,400:Return,460:Down,480:Return,540:Return,600:Return,700:Return";
        let script = Script {
            holds: parse_keys(keys).unwrap(),
            ..Script::default()
        };
        let mut sim = Sim::new(1, GameData::synthetic(), Persisted::default());
        let codes = [Scancode::RETURN, Scancode::DOWN];
        let banner = (0..MAX_TICKS).any(|t| {
            let held = codes.iter().copied().filter(|&k| script.at(t, k).0);
            let pressed = codes.iter().copied().filter(|&k| script.at(t, k).1);
            let released = codes.iter().copied().filter(|&k| script.at(t, k).2);
            sim.tick(&KeyState::from_parts(held, pressed, released));
            matches!(&sim.state().screen, Screen::Match(m)
                if matches!(&m.screen, MatchScreen::Gameplay(g) if matches!(g.phase, GamePhase::Banner { .. })))
        });
        assert!(banner, "no banner within {MAX_TICKS} ticks");
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
