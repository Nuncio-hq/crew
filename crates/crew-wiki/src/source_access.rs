//! Local source reads require an opaque root selected by a native user interaction.
use crate::source_snapshot::SourceReference;
use crate::WikiError;
use std::path::Path;
use std::time::Instant;

/// An unforgeable in-process root handle. Only native picker output may open it.
/// This local grant does not prove signed membership or relay acceptance/access.
#[cfg(unix)]
mod unix;

/// Native-selected source root retained as descriptor capabilities.
pub struct SelectedSourceRoot {
    #[cfg(unix)]
    root: crate::source_folder_walk::Root,
}

/// Complete authenticated source bytes and the requested inclusive LF range.
#[derive(Debug)]
pub struct VerifiedSourceFile {
    /// Complete UTF-8 file content; never substituted with the latest version.
    pub content: String,
    /// First requested line, one-based.
    pub start_line: u64,
    /// Last requested line, inclusive.
    pub end_line: u64,
}

impl SelectedSourceRoot {
    /// Open only an actual native picker result, never a renderer/relay pathname.
    pub fn open_native_selection(_selected: &Path) -> Result<Self, WikiError> {
        #[cfg(unix)]
        {
            return Ok(Self {
                root: crate::source_folder_walk::Root::open(_selected)?,
            });
        }
        #[cfg(not(unix))]
        Err(unavailable())
    }

    /// Read only after native signed-membership and current-access validation.
    /// The caller's remaining deadline is authoritative across all source IO.
    pub fn read_verified_reference(
        &self,
        _revision: &str,
        _reference: &SourceReference,
        _deadline: Instant,
    ) -> Result<VerifiedSourceFile, WikiError> {
        #[cfg(unix)]
        {
            return unix::read(&self.root, _revision, _reference, _deadline);
        }
        #[cfg(not(unix))]
        Err(unavailable())
    }
}

fn unavailable() -> WikiError {
    WikiError::Git("selected-root source access is unavailable".into())
}
