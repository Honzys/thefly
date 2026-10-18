//! Font file lookup. A font name is `|`-separated candidates: every one but the last is a face
//! the game does not ship (looked up in `data/rsc/`, then the OS font directories), and the last
//! is the shipped stand-in in `data/rsc/`.

use std::fs;
use std::path::{Path, PathBuf};

/// How deep the OS font directories are searched (`/usr/share/fonts/truetype/msttcorefonts/`).
const MAX_DEPTH: u32 = 4;

/// Where the OS keeps fonts, per platform; missing directories are skipped.
pub(crate) fn os_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        r"C:\Windows\Fonts",
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
        "/usr/share/fonts",
    ]
    .map(PathBuf::from)
    .into();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(Path::new(&home).join(".local/share/fonts"));
    }
    dirs
}

/// The file to load for `name`: the first existing candidate, else the stand-in's `rsc` path.
pub(crate) fn resolve(rsc: &Path, os_dirs: &[PathBuf], name: &str) -> PathBuf {
    let mut names = name.split('|');
    let stand_in = names.next_back().unwrap_or(name);
    let wanted: Vec<&str> = names.collect();
    let found = wanted.iter().find_map(|w| find(rsc, w, 1)).or_else(|| {
        os_dirs
            .iter()
            .find_map(|d| wanted.iter().find_map(|w| find(d, w, MAX_DEPTH)))
    });
    found.unwrap_or_else(|| rsc.join(stand_in))
}

/// A file called `name` (any case) in `dir` or below it, at most `depth` directories deep.
fn find(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let mut subdirs = Vec::new();
    for e in fs::read_dir(dir).ok()?.flatten() {
        let path = e.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            return Some(path);
        }
    }
    let below = depth.checked_sub(1).filter(|d| *d > 0)?;
    subdirs.iter().find_map(|d| find(d, name, below))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "ariblk.ttf|Arial Black.ttf|casual.ttf";

    fn touch(dir: &Path, file: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let p = dir.join(file);
        fs::write(&p, b"").unwrap();
        p
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("thefly-fonts-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn user_font_in_rsc_beats_the_os_and_the_stand_in() {
        let t = tmp("rsc");
        let (rsc, os) = (t.join("rsc"), t.join("os"));
        touch(&rsc, "casual.ttf");
        touch(&os, "ARIBLK.TTF");
        let mine = touch(&rsc, "Arial Black.ttf");
        assert_eq!(resolve(&rsc, &[os], NAME), mine);
    }

    #[test]
    fn os_font_beats_the_stand_in_and_is_found_in_subdirectories() {
        let t = tmp("os");
        let (rsc, os) = (t.join("rsc"), t.join("os"));
        touch(&rsc, "casual.ttf");
        let sys = touch(&os.join("truetype/ms"), "ARIBLK.TTF");
        assert_eq!(resolve(&rsc, &[os], NAME), sys);
    }

    #[test]
    fn stand_in_is_the_last_resort_and_plain_names_stay_in_rsc() {
        let t = tmp("none");
        let rsc = t.join("rsc");
        assert_eq!(
            resolve(&rsc, &[t.join("none")], NAME),
            rsc.join("casual.ttf")
        );
        assert_eq!(resolve(&rsc, &[], "JOINPD__.TTF"), rsc.join("JOINPD__.TTF"));
    }
}
