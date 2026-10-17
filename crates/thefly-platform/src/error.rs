use std::fmt;
use std::path::PathBuf;

use crate::data_dir::DataDirError;

/// Why the platform layer failed. SDL errors carry SDL's own message.
#[derive(Debug)]
pub enum Error {
    DataDir(DataDirError),
    Sdl(String),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Asset {
        path: PathBuf,
        source: thefly_assets::Error,
    },
    Midi {
        path: PathBuf,
        reason: String,
    },
}

/// Wraps any SDL-side error (the `sdl3` crate has several error types) into [`Error::Sdl`].
pub(crate) fn sdl(e: impl fmt::Display) -> Error {
    Error::Sdl(e.to_string())
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::DataDir(e) => e.fmt(f),
            Error::Sdl(msg) => write!(f, "SDL: {msg}"),
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Asset { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Midi { path, reason } => write!(f, "{}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

impl From<DataDirError> for Error {
    fn from(e: DataDirError) -> Self {
        Error::DataDir(e)
    }
}
