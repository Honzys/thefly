//! W3. The high-score table and its screen (0x6e): reveal, optional name entry, save
//! (ai-and-rules §8.5, screens-and-rendering §5.3, game-spec §7.1).

use crate::clock::Timer;
use crate::ctx::Ctx;
use crate::input::KeyState;
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
        let _ = score;
        todo!("W3 ai-and-rules §8.5")
    }

    /// `FUN_00410cbd`: shift down and insert at `row`.
    pub fn insert(&mut self, row: usize, entry: Row) {
        let _ = (row, entry);
        todo!("W3 game-spec §7.1")
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

impl HighScoreScreen {
    /// From the menu (None) or from the stats page with the qualifying row and score.
    pub fn new(entry: Option<(usize, i32)>, cx: &mut Ctx) -> Self {
        let _ = (entry, cx);
        todo!("W3 screens-and-rendering §5.3")
    }

    /// A key during the reveal skips it (slot 0x26); confirming a name saves (slot 0x33) and
    /// emits [`crate::Event::Save`]; Enter/Esc → menu. Typing: gaps §1.3 (a-z, 1..0, Enter, Backspace); open item O17 for Space and Esc.
    pub fn tick(&mut self, keys: &KeyState, table: &mut HighScoreTable, cx: &mut Ctx) -> Flow {
        let _ = (keys, table, cx);
        todo!("W3 screens-and-rendering §5.3")
    }
}
