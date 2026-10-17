//! Finds the original game's `data/` directory.

use std::fmt;
use std::path::{Path, PathBuf};

/// Where a candidate data path came from, so errors can say which setting was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Cli,
    Env,
    ExeDir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDirError {
    pub source: Source,
    pub path: PathBuf,
}

impl fmt::Display for DataDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let from = match self.source {
            Source::Cli => "--data",
            Source::Env => "THEFLY_DATA",
            Source::ExeDir => "default (./data next to the executable; set --data or THEFLY_DATA)",
        };
        write!(
            f,
            "game data not found: {from} = {} has no rsc/ directory (expected the original game's data/ dir or the release dir containing it)",
            self.path.display()
        )
    }
}

impl std::error::Error for DataDirError {}

/// Picks the first configured source in order `--data`, `THEFLY_DATA`, `<exe dir>/data`.
/// The chosen path may be the `data/` dir itself or the release dir that contains it.
/// A configured but wrong path is an error, not a fall-through, so typos stay visible.
pub fn resolve(
    cli: Option<PathBuf>,
    env: Option<PathBuf>,
    exe_dir: &Path,
    is_dir: impl Fn(&Path) -> bool,
) -> Result<PathBuf, DataDirError> {
    let (source, path) = match (cli, env) {
        (Some(p), _) => (Source::Cli, p),
        (None, Some(p)) => (Source::Env, p),
        (None, None) => (Source::ExeDir, exe_dir.join("data")),
    };
    if is_dir(&path.join("rsc")) {
        return Ok(path);
    }
    let nested = path.join("data");
    if is_dir(&nested.join("rsc")) {
        return Ok(nested);
    }
    Err(DataDirError { source, path })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs(dirs: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |p| dirs.iter().any(|d| Path::new(d) == p)
    }

    #[test]
    fn cli_wins_over_env_and_exe() {
        let got = resolve(
            Some("/cli".into()),
            Some("/env".into()),
            Path::new("/exe"),
            fs(&["/cli/rsc", "/env/rsc", "/exe/data/rsc"]),
        );
        assert_eq!(got, Ok(PathBuf::from("/cli")));
    }

    #[test]
    fn env_wins_over_exe_and_accepts_release_root() {
        let got = resolve(
            None,
            Some("/release".into()),
            Path::new("/exe"),
            fs(&["/release/data/rsc", "/exe/data/rsc"]),
        );
        assert_eq!(got, Ok(PathBuf::from("/release/data")));
    }

    #[test]
    fn falls_back_to_data_next_to_exe() {
        let got = resolve(None, None, Path::new("/exe"), fs(&["/exe/data/rsc"]));
        assert_eq!(got, Ok(PathBuf::from("/exe/data")));
    }

    #[test]
    fn wrong_explicit_path_is_an_error_naming_the_source() {
        let got = resolve(
            Some("/typo".into()),
            None,
            Path::new("/exe"),
            fs(&["/exe/data/rsc"]),
        );
        let err = got.unwrap_err();
        assert_eq!(err.source, Source::Cli);
        assert_eq!(err.path, PathBuf::from("/typo"));
        assert!(err.to_string().contains("--data"));
        assert!(err.to_string().contains("/typo"));
    }

    #[test]
    fn missing_default_names_exe_path() {
        let err = resolve(None, None, Path::new("/exe"), fs(&[])).unwrap_err();
        assert_eq!(err.source, Source::ExeDir);
        assert_eq!(err.path, PathBuf::from("/exe/data"));
    }
}
