//! Typed errors for Cowork shadow-git versioning.
//!
//! User-facing strings stay in the business register (Versions / Restore).
//! The word "git" never appears in those messages.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Failures from the Cowork shadow-history engine.
#[derive(Debug, Error)]
pub enum CoworkError {
    /// A restore or compact ran while another thread holds the folder.
    #[error("Can't restore while thread '{0}' is working")]
    Busy(String),
    /// Requested version is missing from history.
    #[error("That version is no longer available")]
    MissingVersion,
    /// Restore path escaped the folder.
    #[error("That file is not in this folder")]
    PathEscape,
    /// Folder path is missing or not a directory.
    #[error("This folder is not available")]
    MissingFolder,
    /// History directory env/path is unset or invalid.
    #[error("Version history location is not configured")]
    MissingHistoryDir,
    /// Underlying process or filesystem failure.
    #[error("{0}")]
    Operation(String),
    /// Filesystem I/O failure.
    #[error("Could not update Versions at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl CoworkError {
    pub(crate) fn operation(message: impl Into<String>) -> Self {
        Self::Operation(message.into())
    }
}
