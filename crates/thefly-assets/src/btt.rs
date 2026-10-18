//! `.btt` save files: raw struct dumps. `scoot.btt` = score table, `boite.btt` = two flags.
//! Layouts: `docs/formats/btt.md`; field meaning: `docs/re/game-spec.md` §7.

use crate::Error;

pub const SCORE_ENTRIES: usize = 10;
const RECORD_LEN: usize = 48;
/// Name field, NUL included (game-spec §7.1).
const NAME_LEN: usize = 12;
const SCORE_AT: usize = 12;
/// The checksum f64 sits at +0x28, after the run statistics (+0x10..+0x21) and unwritten bytes.
const CHECKSUM_AT: usize = 40;
/// 10 records plus a trailing f64.
pub const SCORES_FILE_LEN: usize = SCORE_ENTRIES * RECORD_LEN + 8;
pub const SETTINGS_FILE_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreEntry {
    /// Bytes before the first NUL in the 12-byte name field, one char per byte (Latin-1).
    pub name: String,
    /// The fly score minus 10 × kills (game-spec §7.1).
    pub score: i32,
    /// `c(score)`, or a random filler on an empty slot. The run statistics at +0x10 are not read.
    pub checksum: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scores {
    pub entries: [ScoreEntry; SCORE_ENTRIES],
    /// Trailing f64: the checksum of the sum of all scores. Equals `entries[0].checksum` in the
    /// shipped file, whose scores sum to 1.
    pub trailer: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub values: [u32; 2],
}

fn parse_entry(rec: &[u8]) -> Result<ScoreEntry, Error> {
    let name_field = rec.get(..NAME_LEN).ok_or(Error::Truncated {
        needed: NAME_LEN,
        have: rec.len(),
    })?;
    let len = name_field
        .iter()
        .position(|&b| b == 0)
        .ok_or(Error::Invalid("name is not NUL-terminated"))?;
    let name = name_field[..len].iter().map(|&b| char::from(b)).collect();
    let score = i32::from_le_bytes(
        rec[SCORE_AT..SCORE_AT + 4]
            .try_into()
            .map_err(|_| Error::Invalid("score field"))?,
    );
    let checksum = f64::from_le_bytes(
        rec[CHECKSUM_AT..RECORD_LEN]
            .try_into()
            .map_err(|_| Error::Invalid("checksum field"))?,
    );
    Ok(ScoreEntry {
        name,
        score,
        checksum,
    })
}

fn check_len(bytes: &[u8], want: usize) -> Result<(), Error> {
    if bytes.len() < want {
        return Err(Error::Truncated {
            needed: want,
            have: bytes.len(),
        });
    }
    if bytes.len() > want {
        return Err(Error::Invalid("unexpected trailing bytes"));
    }
    Ok(())
}

pub fn parse_scores(bytes: &[u8]) -> Result<Scores, Error> {
    check_len(bytes, SCORES_FILE_LEN)?;
    let entries = bytes
        .chunks_exact(RECORD_LEN)
        .take(SCORE_ENTRIES)
        .map(parse_entry)
        .collect::<Result<Vec<_>, _>>()?;
    let entries: [ScoreEntry; SCORE_ENTRIES] = entries
        .try_into()
        .map_err(|_| Error::Invalid("entry count"))?;
    let tail = &bytes[SCORE_ENTRIES * RECORD_LEN..];
    let trailer = f64::from_le_bytes(tail.try_into().map_err(|_| Error::Invalid("trailer"))?);
    Ok(Scores { entries, trailer })
}

/// Writes the table back. Bytes +0x10..+0x27 (the run statistics) are zero-filled, which
/// docs/formats/btt.md allows. shortcut: statistics not written, revisit if anything reads them.
pub fn write_scores(scores: &Scores) -> Vec<u8> {
    let mut out = Vec::with_capacity(SCORES_FILE_LEN);
    for e in &scores.entries {
        let mut rec = [0u8; RECORD_LEN];
        let name = e.name.as_bytes();
        let len = name.len().min(NAME_LEN - 1);
        rec[..len].copy_from_slice(&name[..len]);
        rec[SCORE_AT..SCORE_AT + 4].copy_from_slice(&e.score.to_le_bytes());
        rec[CHECKSUM_AT..].copy_from_slice(&e.checksum.to_le_bytes());
        out.extend(rec);
    }
    out.extend(scores.trailer.to_le_bytes());
    out
}

pub fn write_settings(settings: &Settings) -> Vec<u8> {
    settings
        .values
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

pub fn parse_settings(bytes: &[u8]) -> Result<Settings, Error> {
    check_len(bytes, SETTINGS_FILE_LEN)?;
    let word = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    Ok(Settings {
        values: [word(0), word(4)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Record `i` has score `i` (`first` for record 0) and checksum `i + 0.5`.
    fn scores_file(name: &[u8], first: i32) -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..SCORE_ENTRIES {
            let mut rec = [0xAAu8; RECORD_LEN]; // padding is uninitialised memory in the original
            rec[..name.len()].copy_from_slice(name);
            rec[name.len()] = 0;
            let score = if i == 0 { first } else { i as i32 };
            rec[SCORE_AT..SCORE_AT + 4].copy_from_slice(&score.to_le_bytes());
            rec[CHECKSUM_AT..].copy_from_slice(&(i as f64 + 0.5).to_le_bytes());
            v.extend(rec);
        }
        v.extend(0.5f64.to_le_bytes());
        v
    }

    #[test]
    fn the_writer_round_trips_and_zero_fills_the_statistics() {
        let s = parse_scores(&scores_file(b"abcdefghijk", 7)).unwrap();
        let bytes = write_scores(&s);
        assert_eq!(bytes.len(), SCORES_FILE_LEN);
        assert_eq!(parse_scores(&bytes).unwrap(), s);
        assert!(bytes[0x10..CHECKSUM_AT].iter().all(|&b| b == 0));
        let settings = Settings { values: [1, 0] };
        assert_eq!(
            parse_settings(&write_settings(&settings)).unwrap(),
            settings
        );
    }

    #[test]
    fn parses_score_and_checksum_at_their_offsets() {
        let s = parse_scores(&scores_file(b"None", -12)).unwrap();
        assert_eq!(s.entries[0].name, "None");
        assert_eq!(s.entries[0].score, -12);
        assert_eq!(s.entries[0].checksum, 0.5);
        assert_eq!(s.entries[9].score, 9);
        assert_eq!(s.entries[9].checksum, 9.5);
        assert_eq!(s.trailer, 0.5);
    }

    #[test]
    fn name_decodes_latin1_bytes() {
        let s = parse_scores(&scores_file(&[0x4a, 0xe9], 0)).unwrap();
        assert_eq!(s.entries[0].name, "J\u{e9}");
    }

    #[test]
    fn rejects_unterminated_name() {
        let mut f = scores_file(b"x", 0);
        f[..NAME_LEN].fill(b'a');
        assert!(matches!(parse_scores(&f), Err(Error::Invalid(_))));
    }

    #[test]
    fn a_name_may_use_all_eleven_bytes_before_the_nul() {
        let s = parse_scores(&scores_file(b"abcdefghijk", 0)).unwrap();
        assert_eq!(s.entries[0].name, "abcdefghijk");
    }

    #[test]
    fn rejects_wrong_scores_length() {
        let f = scores_file(b"None", 1);
        assert!(matches!(
            parse_scores(&f[..100]),
            Err(Error::Truncated { .. })
        ));
        let mut long = f;
        long.push(0);
        assert!(matches!(parse_scores(&long), Err(Error::Invalid(_))));
    }

    #[test]
    fn parses_settings() {
        let mut f = Vec::new();
        f.extend(1u32.to_le_bytes());
        f.extend(7u32.to_le_bytes());
        assert_eq!(parse_settings(&f).unwrap().values, [1, 7]);
    }

    #[test]
    fn rejects_wrong_settings_length() {
        assert!(matches!(
            parse_settings(&[0; 4]),
            Err(Error::Truncated { .. })
        ));
        assert!(matches!(parse_settings(&[0; 9]), Err(Error::Invalid(_))));
    }
}
