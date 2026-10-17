//! `.btt` save files: raw struct dumps. `scoot.btt` = score table, `boite.btt` = two flags.

use crate::Error;

pub const SCORE_ENTRIES: usize = 10;
const RECORD_LEN: usize = 48;
const NAME_LEN: usize = 40;
/// 10 records plus a trailing f64.
pub const SCORES_FILE_LEN: usize = SCORE_ENTRIES * RECORD_LEN + 8;
pub const SETTINGS_FILE_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreEntry {
    /// Bytes before the first NUL in the 40-byte name field, one char per byte (Latin-1).
    pub name: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scores {
    pub entries: [ScoreEntry; SCORE_ENTRIES],
    /// Trailing f64; equals `entries[0].score` in the shipped file.
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
    let score = f64::from_le_bytes(
        rec[NAME_LEN..RECORD_LEN]
            .try_into()
            .map_err(|_| Error::Invalid("score field"))?,
    );
    Ok(ScoreEntry { name, score })
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

    fn scores_file(name: &[u8], first: f64) -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..SCORE_ENTRIES {
            let mut rec = [0xAAu8; RECORD_LEN]; // padding is uninitialised memory in the original
            rec[..name.len()].copy_from_slice(name);
            rec[name.len()] = 0;
            let s = if i == 0 { first } else { i as f64 };
            rec[NAME_LEN..].copy_from_slice(&s.to_le_bytes());
            v.extend(rec);
        }
        v.extend(first.to_le_bytes());
        v
    }

    #[test]
    fn parses_scores_ignoring_padding_after_nul() {
        let s = parse_scores(&scores_file(b"None", 12.5)).unwrap();
        assert_eq!(s.entries[0].name, "None");
        assert_eq!(s.entries[0].score, 12.5);
        assert_eq!(s.entries[9].score, 9.0);
        assert_eq!(s.trailer, 12.5);
    }

    #[test]
    fn name_decodes_latin1_bytes() {
        let s = parse_scores(&scores_file(&[0x4a, 0xe9], 0.0)).unwrap();
        assert_eq!(s.entries[0].name, "J\u{e9}");
    }

    #[test]
    fn rejects_unterminated_name() {
        let mut f = scores_file(b"x", 0.0);
        f[..NAME_LEN].fill(b'a');
        assert!(matches!(parse_scores(&f), Err(Error::Invalid(_))));
    }

    #[test]
    fn rejects_wrong_scores_length() {
        let f = scores_file(b"None", 1.0);
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
