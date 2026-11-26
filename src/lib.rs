pub mod app;
pub mod document;
pub mod importer;
pub mod mouseia;
pub mod package;
pub mod theme;
pub mod updater;
pub mod vault;

use std::{fmt, io, path::Path};

/// Set files and folders written by Skribi to owner-only access on Unix.
/// On other platforms the operating system's per-user profile provides the boundary.
#[cfg(unix)]
pub(crate) fn restrict_to_owner(path: &Path, directory: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if directory { 0o700 } else { 0o600 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn restrict_to_owner(_path: &Path, _directory: bool) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn restrict_tree(path: &Path) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            restrict_tree(&entry.path())?;
        } else {
            restrict_to_owner(&entry.path(), false)?;
        }
    }
    restrict_to_owner(path, true)
}

#[cfg(not(unix))]
pub(crate) fn restrict_tree(_path: &Path) -> Result<()> {
    Ok(())
}

/// Tighten the eframe persistence folder that holds saved workspace paths.
pub(crate) fn restrict_app_storage() {
    #[cfg(unix)]
    if let Some(directory) = eframe::storage_dir("Skribi")
        && directory.is_dir()
    {
        let _ = restrict_tree(&directory);
    }
}

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
