use std::fmt;

/// Why a file could not be parsed. Parsers never panic on bad input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input ended before `needed` bytes were available.
    Truncated { needed: usize, have: usize },
    /// A fixed marker (for example `BM`) was missing.
    BadMagic,
    /// Valid structure that this crate does not decode (for example RLE bitmaps).
    Unsupported(&'static str),
    /// A field has a value the format does not allow.
    Invalid(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { needed, have } => {
                write!(f, "truncated input: need {needed} bytes, have {have}")
            }
            Error::BadMagic => write!(f, "bad magic marker"),
            Error::Unsupported(what) => write!(f, "unsupported: {what}"),
            Error::Invalid(what) => write!(f, "invalid: {what}"),
        }
    }
}

impl std::error::Error for Error {}
