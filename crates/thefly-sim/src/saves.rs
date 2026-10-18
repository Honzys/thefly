//! W3. Save files as bytes (game-spec §7). The sim never touches the disk: it decodes what the
//! frontend read at start-up and emits [`crate::Event::Save`] with the encoded bytes.
//!
//! The `scoot.btt` layout is game-spec §7.1 (`name[12]`, i32 score at +0x0c, f64 checksum at
//! +0x28), read by `thefly_assets::btt`. W3 adds the writer there; these types wrap both.

use crate::highscores::HighScoreTable;
use crate::rng::MsvcRand;

/// `boite.btt` (game-spec §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub music: bool,
    pub tutorial: bool,
}

impl Default for Options {
    /// Used when `boite.btt` is missing. Open item O7: the docs do not say what the original does.
    fn default() -> Self {
        Self {
            music: true,
            tutorial: false,
        }
    }
}

/// What the frontend loaded before the first tick.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Persisted {
    pub options: Options,
    pub scores: HighScoreTable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Length,
    /// A non-zero score or the trailer fails its checksum (`FUN_00410ad3`).
    Checksum,
}

pub fn decode_options(bytes: &[u8]) -> Result<Options, DecodeError> {
    let _ = bytes;
    todo!("W3 game-spec §7.2")
}

pub fn encode_options(options: &Options) -> Vec<u8> {
    let _ = options;
    todo!("W3 game-spec §7.2")
}

pub fn decode_scores(bytes: &[u8]) -> Result<HighScoreTable, DecodeError> {
    let _ = bytes;
    todo!("W3 game-spec §7.1")
}

/// Rewrites every checksum. Empty slots and a sub-1 trailer take random fillers, so writing the
/// file consumes the shared rand stream exactly as the original does.
pub fn encode_scores(table: &HighScoreTable, rng: &mut MsvcRand) -> Vec<u8> {
    let _ = (table, rng);
    todo!("W3 game-spec §7.1")
}

/// c(s) = sqrt(max(5.365799563·s − sqrt(5.365799563), else 0.1835)).
pub fn checksum(score: f64) -> f64 {
    let _ = score;
    todo!("W3 game-spec §7.1")
}
