//! W3. The high-score table and its screen (0x6e): reveal, optional name entry, save
//! (ai-and-rules §8.5, screens-and-rendering §5.3, game-spec §7.1).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::event::{Event, SaveFile, SoundSlot};
use crate::input::{KeyState, Scancode};
use crate::saves::encode_scores;
use crate::screen::Flow;

pub const ROWS: usize = 10;
/// Longest name: the field is `name[12]`, NUL-terminated (game-spec §7.1).
pub const NAME_MAX: usize = 11;
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

/// shortcut: letters are typed lowercase (the docs give no case rule); revisit with a capture.
fn typed(code: u8) -> Option<char> {
    match code {
        LETTER_A..DIGIT_1 => Some(char::from(b'a' + (code - LETTER_A))),
        DIGIT_1..DIGIT_0 => Some(char::from(b'1' + (code - DIGIT_1))),
        DIGIT_0 => Some('0'),
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
    /// emits [`crate::Event::Save`]; Enter/Esc → menu. Typing: gaps §1.3 (a-z, 1..0, Enter, Backspace); open item O17 for Space and Esc.
    pub fn tick(&mut self, keys: &KeyState, table: &mut HighScoreTable, cx: &mut Ctx) -> Flow {
        if self.revealed > 0 {
            self.reveal(keys, cx);
            return Flow::Stay;
        }
        let Some(entry) = &mut self.entry else {
            return if keys.skip() { Flow::Menu } else { Flow::Stay };
        };
        if entry.cursor.expired_rearm(cx.now) {
            entry.cursor_on = !entry.cursor_on;
        }
        if keys.pressed(Scancode::RETURN) {
            confirm(entry, table, cx);
            self.entry = None;
            return Flow::Stay;
        }
        if keys.pressed(BACKSPACE) {
            entry.name.pop();
        }
        // shortcut: open item O17, Space and Esc are ignored; revisit with a capture.
        for code in LETTER_A..=DIGIT_0 {
            let Some(c) = typed(code).filter(|_| keys.pressed(Scancode(code))) else {
                continue;
            };
            if entry.name.len() < NAME_MAX {
                entry.name.push(c);
            }
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
}
