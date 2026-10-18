//! W3. Save files as bytes (game-spec §7). The sim never touches the disk: it decodes what the
//! frontend read at start-up and emits [`crate::Event::Save`] with the encoded bytes.
//!
//! The `scoot.btt` layout is game-spec §7.1 (`name[12]`, i32 score at +0x0c, f64 checksum at
//! +0x28), read by `thefly_assets::btt`. W3 adds the writer there; these types wrap both.

use crate::highscores::{HighScoreTable, Row};
use crate::rng::MsvcRand;
use thefly_assets::btt;

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
    let s = btt::parse_settings(bytes).map_err(|_| DecodeError::Length)?;
    Ok(Options {
        music: s.values[0] != 0,
        tutorial: s.values[1] != 0,
    })
}

pub fn encode_options(options: &Options) -> Vec<u8> {
    btt::write_settings(&btt::Settings {
        values: [u32::from(options.music), u32::from(options.tutorial)],
    })
}

/// Checksums are recomputed here; a relative 1e-9 absorbs the x87 → f64 rounding [I].
fn matches(stored: f64, score: i32) -> bool {
    let want = checksum(f64::from(score));
    (stored - want).abs() <= 1e-9 * want.abs().max(1.0)
}

pub fn decode_scores(bytes: &[u8]) -> Result<HighScoreTable, DecodeError> {
    let s = btt::parse_scores(bytes).map_err(|_| DecodeError::Length)?;
    let bad = s
        .entries
        .iter()
        .any(|e| e.score != 0 && !matches(e.checksum, e.score));
    let sum: i32 = s.entries.iter().map(|e| e.score).sum();
    // shortcut: a sum below 1 carries a random trailer, so it is not checked (game-spec §7.1
    // does not say how the original validates it); revisit if a capture shows otherwise.
    if bad || (sum >= 1 && !matches(s.trailer, sum)) {
        return Err(DecodeError::Checksum);
    }
    let rows = s
        .entries
        .into_iter()
        .map(|e| Row {
            name: e.name,
            score: e.score,
        })
        .collect();
    Ok(HighScoreTable { rows })
}

/// Rewrites every checksum. Empty slots and a sub-1 trailer take random fillers, so writing the
/// file consumes the shared rand stream exactly as the original does.
pub fn encode_scores(table: &HighScoreTable, rng: &mut MsvcRand) -> Vec<u8> {
    let empty = Row::default();
    let entries = std::array::from_fn(|i| {
        let row = table.rows.get(i).unwrap_or(&empty);
        let checksum = if row.score == 0 {
            f64::from(rng.r(8000)) / 5.63
        } else {
            checksum(f64::from(row.score))
        };
        btt::ScoreEntry {
            name: row.name.clone(),
            score: row.score,
            checksum,
        }
    });
    let sum: i32 = table.rows.iter().map(|r| r.score).sum();
    let trailer = if sum < 1 {
        f64::from(rng.r(8000)) / 6.8899
    } else {
        checksum(f64::from(sum))
    };
    btt::write_scores(&btt::Scores { entries, trailer })
}

/// c(s) = sqrt(max(5.365799563·s − sqrt(5.365799563), else 0.1835)).
pub fn checksum(score: f64) -> f64 {
    const K: f64 = 5.365799563;
    let x = K * score - K.sqrt();
    if x < 0.0 { 0.1835_f64.sqrt() } else { x.sqrt() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> Option<std::path::PathBuf> {
        std::env::var_os("THEFLY_DATA").map(Into::into)
    }

    #[test]
    fn the_checksum_of_1_is_the_shipped_value() {
        assert!((checksum(1.0) - 1.74625).abs() < 5e-6);
        assert_eq!(checksum(0.0), 0.1835_f64.sqrt());
    }

    #[test]
    fn options_are_two_words() {
        let o = Options {
            music: false,
            tutorial: true,
        };
        assert_eq!(encode_options(&o), [0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(decode_options(&encode_options(&o)), Ok(o));
        assert_eq!(decode_options(&[1, 2]), Err(DecodeError::Length));
    }

    /// Nine empty slots draw 41 % 8000, 18467 % 8000, … in record order; the sum is 1.
    #[test]
    fn empty_slots_take_fillers_from_the_rand_stream() {
        let table = HighScoreTable {
            rows: vec![Row {
                name: "None".into(),
                score: 1,
            }],
        };
        let mut rng = MsvcRand::new(1);
        let bytes = encode_scores(&table, &mut rng);
        let s = btt::parse_scores(&bytes).unwrap();
        assert_eq!(s.entries[1].checksum, 41.0 / 5.63);
        assert_eq!(s.entries[2].checksum, f64::from(18467 % 8000) / 5.63);
        assert_eq!(s.entries[9].checksum, f64::from(26962 % 8000) / 5.63);
        assert_eq!(s.trailer, checksum(1.0));
        assert_eq!(rng.rand(), 24464, "nine draws, no trailer draw");
        assert_eq!(decode_scores(&bytes).unwrap().rows[0].score, 1);
    }

    #[test]
    fn an_all_zero_table_draws_a_trailer_and_a_bad_checksum_is_refused() {
        let mut rng = MsvcRand::new(1);
        let bytes = encode_scores(&HighScoreTable::default(), &mut rng);
        let s = btt::parse_scores(&bytes).unwrap();
        assert_eq!(s.trailer, 5705.0 / 6.8899);
        let mut broken = bytes.clone();
        broken[12] = 5; // score 5 with a filler checksum
        assert_eq!(decode_scores(&broken), Err(DecodeError::Checksum));
    }

    /// The shipped `scoot.btt` decodes and re-encodes to the same scores and checksums.
    #[test]
    fn the_shipped_table_round_trips() {
        let Some(root) = release() else { return };
        let bytes = std::fs::read(root.join("scoot.btt")).unwrap();
        let table = decode_scores(&bytes).unwrap();
        assert_eq!(
            (table.rows[0].name.as_str(), table.rows[0].score),
            ("None", 1)
        );
        let again = encode_scores(&table, &mut MsvcRand::new(1));
        let (a, b) = (
            btt::parse_scores(&bytes).unwrap(),
            btt::parse_scores(&again).unwrap(),
        );
        for (x, y) in a.entries.iter().zip(&b.entries) {
            assert_eq!((&x.name, x.score), (&y.name, y.score));
            assert!(x.score == 0 || matches(y.checksum, x.score));
        }
        assert!(matches(b.trailer, table.rows.iter().map(|r| r.score).sum()));
        assert_eq!(decode_scores(&again), Ok(table));
    }
}
