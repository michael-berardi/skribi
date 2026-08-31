pub mod app;
pub mod document;
pub mod importer;
pub mod mouseia;
pub mod package;
pub mod theme;
pub mod vault;

use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Zip(zip::result::ZipError),
    Invalid(String),
    StaleDocument,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Zip(error) => write!(f, "{error}"),
            Self::Invalid(message) => f.write_str(message),
            Self::StaleDocument => {
                f.write_str("This document changed on disk. Reload it before saving.")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
