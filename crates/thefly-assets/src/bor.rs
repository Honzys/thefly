//! `.bor` outline files: 500 rows of u16 x-coordinates, each row ended by 0xFFFF.

use crate::Error;

/// Rows in every file; row index = y in sprite pixels.
pub const ROWS: usize = 500;
const TERMINATOR: u16 = 0xFFFF;

/// Per-scanline outline pixels of a sprite silhouette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bor {
    /// Exactly [`ROWS`] rows; each lists x coordinates, descending.
    pub rows: Vec<Vec<u16>>,
}

impl Bor {
    /// Number of rows up to and including the last non-empty one.
    pub fn height(&self) -> usize {
        self.rows
            .iter()
            .rposition(|r| !r.is_empty())
            .map_or(0, |i| i + 1)
    }

    /// Largest x plus one, or 0 when the outline is empty.
    pub fn width(&self) -> usize {
        self.rows
            .iter()
            .flatten()
            .max()
            .map_or(0, |&x| usize::from(x) + 1)
    }

    /// All outline pixels as `(x, y)`.
    pub fn points(&self) -> impl Iterator<Item = (u16, usize)> + '_ {
        self.rows
            .iter()
            .enumerate()
            .flat_map(|(y, r)| r.iter().map(move |&x| (x, y)))
    }
}

pub fn parse(bytes: &[u8]) -> Result<Bor, Error> {
    if bytes.len() % 2 != 0 {
        return Err(Error::Invalid("odd byte length"));
    }
    let mut rows = Vec::new();
    let mut cur = Vec::new();
    for pair in bytes.chunks_exact(2) {
        let v = u16::from_le_bytes([pair[0], pair[1]]);
        if v == TERMINATOR {
            rows.push(std::mem::take(&mut cur));
        } else {
            cur.push(v);
        }
    }
    if !cur.is_empty() {
        return Err(Error::Invalid("data after last row terminator"));
    }
    if rows.len() != ROWS {
        return Err(Error::Invalid("row count is not 500"));
    }
    Ok(Bor { rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(rows: &[&[u16]]) -> Vec<u8> {
        let mut v = Vec::new();
        for i in 0..ROWS {
            for x in rows.get(i).copied().unwrap_or(&[]) {
                v.extend(x.to_le_bytes());
            }
            v.extend(TERMINATOR.to_le_bytes());
        }
        v
    }

    #[test]
    fn parses_rows_and_measures_extent() {
        let bor = parse(&encode(&[&[], &[], &[5], &[9, 3]])).unwrap();
        assert_eq!(bor.rows.len(), ROWS);
        assert_eq!(bor.rows[3], [9, 3]);
        assert_eq!(bor.height(), 4);
        assert_eq!(bor.width(), 10);
        assert_eq!(bor.points().collect::<Vec<_>>(), [(5, 2), (9, 3), (3, 3)]);
    }

    #[test]
    fn empty_outline_has_zero_extent() {
        let bor = parse(&encode(&[])).unwrap();
        assert_eq!((bor.height(), bor.width()), (0, 0));
    }

    #[test]
    fn rejects_odd_length() {
        let mut f = encode(&[]);
        f.push(0);
        assert!(matches!(parse(&f), Err(Error::Invalid(_))));
    }

    #[test]
    fn rejects_wrong_row_count() {
        let short = &encode(&[])[..(ROWS - 1) * 2];
        assert!(matches!(parse(short), Err(Error::Invalid(_))));
        let mut long = encode(&[]);
        long.extend(TERMINATOR.to_le_bytes());
        assert!(matches!(parse(&long), Err(Error::Invalid(_))));
    }

    #[test]
    fn rejects_data_after_last_terminator() {
        let mut f = encode(&[]);
        f.extend(7u16.to_le_bytes());
        assert!(matches!(parse(&f), Err(Error::Invalid(_))));
    }
}
