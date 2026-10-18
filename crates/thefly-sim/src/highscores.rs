//! W3. The high-score table and its screen (0x6e): reveal, optional name entry, save
//! (ai-and-rules §8.5, screens-and-rendering §5.3, game-spec §7.1).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, SaveFile, SoundSlot};
use crate::input::{KeyState, Scancode};
use crate::saves::encode_scores;
use crate::screen::Flow;

pub const ROWS: usize = 10;
/// Longest name: a character is stored only while the position is below 10 (`FUN_0041a8d9`).
pub const NAME_MAX: usize = 10;
pub const CURSOR_BLINK_MS: u64 = 200;
pub const ROW_SWEEP_MS: u64 = 200;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Row {
    pub name: String,
    pub score: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HighScoreTable {
    /// Best first; always [`ROWS`] long once decoded.
    pub rows: Vec<Row>,
}

impl HighScoreTable {
    /// `FUN_0040abb0`: the first row whose score ≤ `score` (ties rank above older entries).
    pub fn qualifies(&self, score: i32) -> Option<usize> {
        self.rows.iter().take(ROWS).position(|r| r.score <= score)
    }

    /// `FUN_00410cbd`: shift down and insert at `row`.
    pub fn insert(&mut self, row: usize, entry: Row) {
        self.rows.insert(row.min(self.rows.len()), entry);
        self.rows.truncate(ROWS);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEntry {
    pub row: usize,
    pub score: i32,
    pub name: String,
    pub cursor: Timer,
    pub cursor_on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighScoreScreen {
    /// Rows at or below this rank are drawn; counts down from 10.
    pub revealed: usize,
    pub sweep: Timer,
    pub entry: Option<NameEntry>,
}

/// Name-entry scancodes: a..z are 4..=29, the top-row 1..9 and 0 are 30..=39 (gaps §1.3).
const LETTER_A: u8 = 4;
const DIGIT_1: u8 = 30;
const DIGIT_0: u8 = 39;
const BACKSPACE: Scancode = Scancode(42);
const SPACE: Scancode = Scancode(44);

/// `FUN_0040f8d9` + `toupper`: names are stored in uppercase; Space types a space.
fn typed(code: u8) -> Option<char> {
    match code {
        LETTER_A..DIGIT_1 => Some(char::from(b'A' + (code - LETTER_A))),
        DIGIT_1..DIGIT_0 => Some(char::from(b'1' + (code - DIGIT_1))),
        DIGIT_0 => Some('0'),
        c if c == SPACE.0 => Some(' '),
        _ => None,
    }
}

impl HighScoreScreen {
    /// From the menu (None) or from the stats page with the qualifying row and score.
    pub fn new(entry: Option<(usize, i32)>, cx: &mut Ctx) -> Self {
        cx.emit(Event::play(SoundSlot::SUPERFLY));
        Self {
            revealed: ROWS,
            sweep: Timer::armed(cx.now, ROW_SWEEP_MS),
            entry: entry.map(|(row, score)| NameEntry {
                row,
                score,
                name: String::new(),
                cursor: Timer::armed(cx.now, CURSOR_BLINK_MS),
                cursor_on: true,
            }),
        }
    }

    /// A key during the reveal skips it (slot 0x26); confirming a name saves (slot 0x33) and
    /// emits [`crate::Event::Save`]; Esc → menu. Typing: gaps §1.3 and answers.md "Name entry"
    /// (A-Z, 1..0, Space, Enter, Backspace).
    pub fn tick(&mut self, keys: &KeyState, table: &mut HighScoreTable, cx: &mut Ctx) -> Flow {
        if self.revealed > 0 {
            self.reveal(keys, cx);
            return Flow::Stay;
        }
        // docs/re/observed.md: the table closes on Esc, not on Return.
        let Some(entry) = &mut self.entry else {
            return if keys.pressed(Scancode::ESCAPE) {
                Flow::Menu
            } else {
                Flow::Stay
            };
        };
        if entry.cursor.expired_rearm(cx.now) {
            entry.cursor_on = !entry.cursor_on;
        }
        // `FUN_0041a8d9`: Enter commits only a typed name; Esc maps to the ignore code.
        if keys.pressed(Scancode::RETURN) && !entry.name.is_empty() {
            confirm(entry, table, cx);
            self.entry = None;
            return Flow::Stay;
        }
        if keys.pressed(BACKSPACE) {
            entry.name.pop();
        }
        // One character per tick. shortcut: the original reads the last key event; with several
        // presses in one tick the lowest scancode wins here.
        let c = (LETTER_A..=SPACE.0)
            .filter(|&code| keys.pressed(Scancode(code)))
            .find_map(typed);
        if let Some(c) = c.filter(|_| entry.name.len() < NAME_MAX) {
            entry.name.push(c);
        }
        Flow::Stay
    }

    fn reveal(&mut self, keys: &KeyState, cx: &mut Ctx) {
        if keys.skip() {
            self.revealed = 0;
            cx.emit(Event::play(SoundSlot::GOODIE_FLY));
            return;
        }
        if self.sweep.expired_rearm(cx.now) {
            self.revealed -= 1;
        }
    }
}

/// ai-and-rules §8.5: insert at the recorded value's row; below all ten, nothing is stored.
fn confirm(entry: &NameEntry, table: &mut HighScoreTable, cx: &mut Ctx) {
    cx.emit(Event::play(SoundSlot::CHEER_FLY));
    let Some(row) = table.qualifies(entry.score) else {
        return;
    };
    table.insert(
        row,
        Row {
            name: entry.name.clone(),
            score: entry.score,
        },
    );
    let bytes = encode_scores(table, cx.rng);
    cx.emit(Event::Save {
        file: SaveFile::Scores,
        bytes,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(scores: &[i32]) -> HighScoreTable {
        let rows = scores
            .iter()
            .map(|&score| Row {
                name: "x".into(),
                score,
            })
            .collect();
        HighScoreTable { rows }
    }

    /// AR §8.5: the first row whose score ≤ the new one; ties rank above.
    #[test]
    fn a_score_qualifies_at_the_first_row_not_above_it() {
        let t = table(&[900, 800, 800, 500, 400, 300, 200, 100, 50, 10]);
        assert_eq!(t.qualifies(800), Some(1));
        assert_eq!(t.qualifies(9999), Some(0));
        assert_eq!(t.qualifies(10), Some(9));
        assert_eq!(t.qualifies(9), None);
    }

    #[test]
    fn insert_shifts_down_and_drops_the_last_row() {
        let mut t = table(&[900, 800, 700, 600, 500, 400, 300, 200, 100, 50]);
        t.insert(
            1,
            Row {
                name: "new".into(),
                score: 850,
            },
        );
        assert_eq!(t.rows.len(), ROWS);
        assert_eq!((t.rows[1].name.as_str(), t.rows[2].score), ("new", 800));
        assert_eq!(t.rows[9].score, 100);
    }

    /// GS §7.1, answers.md "Name entry": skip the reveal, at most 10 characters, typed in
    /// uppercase, Backspace, Enter saves the row.
    #[test]
    fn a_typed_name_is_capped_and_saved_at_its_row() {
        use crate::screen::rig::Rig;
        let mut rig = Rig::new(1);
        let mut t = table(&[900, 800, 700, 600, 500, 400, 300, 200, 100, 0]);
        let mut hs = rig.cx(|cx| HighScoreScreen::new(Some((2, 750)), cx));
        let tap = |rig: &mut Rig, hs: &mut HighScoreScreen, t: &mut HighScoreTable, k: u8| {
            let out = rig.step(&[Scancode(k)], |keys, cx| hs.tick(keys, t, cx));
            rig.step(&[], |keys, cx| hs.tick(keys, t, cx));
            out
        };
        tap(&mut rig, &mut hs, &mut t, Scancode::RETURN.0);
        assert_eq!(hs.revealed, 0);
        // "abcdefghijkl": twelve letters, the eleventh and twelfth are dropped.
        for code in 4..16 {
            tap(&mut rig, &mut hs, &mut t, code);
        }
        tap(&mut rig, &mut hs, &mut t, 42);
        tap(&mut rig, &mut hs, &mut t, 39);
        assert_eq!(
            hs.entry.as_ref().map(|e| e.name.as_str()),
            Some("ABCDEFGHI0")
        );
        tap(&mut rig, &mut hs, &mut t, Scancode::RETURN.0);
        assert_eq!(t.rows[2].name, "ABCDEFGHI0");
        assert_eq!(t.rows[2].score, 750);
        assert_eq!(t.rows.len(), ROWS);
        assert!(rig.events.iter().any(|e| matches!(
            e,
            Event::Save {
                file: SaveFile::Scores,
                ..
            }
        )));
        let out = tap(&mut rig, &mut hs, &mut t, Scancode::RETURN.0);
        assert!(matches!(out, Flow::Stay));
        let out = tap(&mut rig, &mut hs, &mut t, Scancode::ESCAPE.0);
        assert!(matches!(out, Flow::Menu));
    }

    /// answers.md "Name entry: Space, Esc, Enter, length": Enter on an empty name is ignored,
    /// Esc does nothing, Space types a space; one character per tick.
    #[test]
    fn enter_needs_a_character_and_space_types_one() {
        use crate::screen::rig::Rig;
        let mut rig = Rig::new(1);
        let mut t = table(&[900, 800, 700, 600, 500, 400, 300, 200, 100, 0]);
        let mut hs = rig.cx(|cx| HighScoreScreen::new(Some((0, 950)), cx));
        let mut tap = |hs: &mut HighScoreScreen, t: &mut HighScoreTable, k: &[Scancode]| {
            let out = rig.step(k, |keys, cx| hs.tick(keys, t, cx));
            rig.step(&[], |keys, cx| hs.tick(keys, t, cx));
            out
        };
        tap(&mut hs, &mut t, &[Scancode::RETURN]);
        tap(&mut hs, &mut t, &[Scancode::RETURN]);
        assert!(matches!(
            tap(&mut hs, &mut t, &[Scancode::ESCAPE]),
            Flow::Stay
        ));
        assert_eq!(hs.entry.as_ref().map(|e| e.name.as_str()), Some(""));
        tap(&mut hs, &mut t, &[SPACE]);
        tap(&mut hs, &mut t, &[Scancode(4), Scancode(5)]);
        assert_eq!(hs.entry.as_ref().map(|e| e.name.len()), Some(2));
        tap(&mut hs, &mut t, &[Scancode::RETURN]);
        assert!(hs.entry.is_none());
        assert!(t.rows[0].name.starts_with(' '));
    }
}
