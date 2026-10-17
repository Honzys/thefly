//! Parsers for the original asset formats of The Fly (2002).
//! Format documentation lives in `docs/formats/`.

pub mod bor;
pub mod btt;
mod error;
pub mod irc;

pub use error::Error;

use std::path::{Path, PathBuf};

/// Files directly in `dir` whose extension equals `ext` ignoring ASCII case, sorted by name.
/// The release mixes `.bor` and `.BOR`, so callers must not match case-sensitively.
pub fn files_with_ext(dir: &Path, ext: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let hit = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext));
        if hit {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}
