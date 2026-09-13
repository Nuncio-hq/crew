//! Local source reads require an opaque root selected by a native user interaction.
use crate::source_snapshot::SourceReference;
use crate::WikiError;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::time::Instant;

/// An unforgeable in-process root handle. Only native picker output may open it.
/// This local grant does not prove signed membership or relay acceptance/access.
#[cfg(unix)]
mod unix;

/// Native-selected source root retained as descriptor capabilities.
pub struct SelectedSourceRoot {
    #[cfg(unix)]
    root: crate::source_folder_walk::Root,
    #[cfg(target_os = "macos")]
    git_helper: Option<PathBuf>,
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
            let root = crate::source_folder_walk::Root::open(_selected)?;
            #[cfg(target_os = "macos")]
            let git_helper = crate::source_git_command::bundled_helper_path().ok();
            Ok(Self {
                root,
                #[cfg(target_os = "macos")]
                git_helper,
            })
        }
        #[cfg(not(unix))]
        Err(unavailable())
    }

    /// Open a native-selected source root with an explicitly trusted bundled helper.
    ///
    /// This seam is used by native hosts and Rust integration tests that provide the
    /// exact helper artifact path. It never resolves a helper through `PATH`, a cache,
    /// or the selected source directory.
    #[cfg(target_os = "macos")]
    pub fn open_native_selection_with_helper(
        selected: &Path,
        helper: &Path,
    ) -> Result<Self, WikiError> {
        let root = crate::source_folder_walk::Root::open(selected)?;
        let git_helper = Some(crate::source_git_command::validate_helper_path(helper)?);
        Ok(Self { root, git_helper })
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
            #[cfg(target_os = "macos")]
            let git_helper = self.git_helper.as_deref();
            #[cfg(not(target_os = "macos"))]
            let git_helper: Option<&Path> = None;
            unix::read(&self.root, _revision, _reference, _deadline, git_helper)
        }
        #[cfg(not(unix))]
        Err(unavailable())
    }
}

fn unavailable() -> WikiError {
    WikiError::Git("selected-root source access is unavailable".into())
}
