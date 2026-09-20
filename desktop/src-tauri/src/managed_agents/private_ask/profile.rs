//! Private Hermes profile staging for one disposable Ask attempt.

use super::PrivateAskFailure;
use std::io::Read;
use std::path::{Path, PathBuf};

const PROFILE_FILE_LIMIT: usize = 1024;
const PROFILE_ENTRY_LIMIT: usize = 4096;
const PROFILE_DEPTH_LIMIT: usize = 32;
const PROFILE_BYTES_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Default)]
struct ProfileCopyBudget {
    entries: usize,
    files: usize,
    bytes: u64,
}

pub(super) fn canonical_profile_source(source: &Path) -> Result<PathBuf, PrivateAskFailure> {
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    let canonical = source
        .canonicalize()
        .map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    let canonical_metadata =
        std::fs::symlink_metadata(&canonical).map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    if canonical_metadata.file_type().is_symlink() || !canonical_metadata.is_dir() {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    super::super::recap_state::directory_identity(&canonical)
        .map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    Ok(canonical)
}

fn is_live_hermes_profile(source: &Path) -> bool {
    dirs::home_dir().is_some_and(|home| {
        let live = home.join(".hermes");
        source == live || source.starts_with(live)
    })
}

fn copy_profile_tree(
    source: &Path,
    destination: &Path,
    budget: &mut ProfileCopyBudget,
    depth: usize,
) -> Result<(), PrivateAskFailure> {
    if depth > PROFILE_DEPTH_LIMIT {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    budget.entries = budget
        .entries
        .checked_add(1)
        .ok_or(PrivateAskFailure::ProfileUnavailable)?;
    if budget.entries > PROFILE_ENTRY_LIMIT {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    if metadata.file_type().is_symlink() {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    if metadata.is_dir() {
        super::super::recap_state::directory_identity(source)
            .map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
        ensure_private_profile_directory(destination)?;
        let mut entries =
            std::fs::read_dir(source).map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
        for (index, entry) in entries.by_ref().take(PROFILE_ENTRY_LIMIT + 1).enumerate() {
            if index >= PROFILE_ENTRY_LIMIT {
                return Err(PrivateAskFailure::ProfileUnavailable);
            }
            let entry = entry.map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
            copy_profile_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                budget,
                depth + 1,
            )?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    budget.files = budget
        .files
        .checked_add(1)
        .ok_or(PrivateAskFailure::ProfileUnavailable)?;
    budget.bytes = budget
        .bytes
        .checked_add(metadata.len())
        .ok_or(PrivateAskFailure::ProfileUnavailable)?;
    if budget.files > PROFILE_FILE_LIMIT || budget.bytes > PROFILE_BYTES_LIMIT {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    let source_file =
        super::super::recap_state::private_read_file(source).map_err(PrivateAskFailure::State)?;
    let mut bytes = Vec::new();
    source_file
        .take(metadata.len().saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    if let Some(parent) = destination.parent() {
        ensure_private_profile_directory(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination)
            .and_then(|mut file| {
                use std::io::Write;
                file.write_all(&bytes)?;
                file.sync_all()
            })
            .map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(destination, &bytes).map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
    }
    Ok(())
}

pub(super) fn ensure_private_profile_directory(path: &Path) -> Result<(), PrivateAskFailure> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(PrivateAskFailure::ProfileUnavailable);
            }
            super::super::recap_state::directory_identity(path)
                .map(|_| ())
                .map_err(|_| PrivateAskFailure::ProfileUnavailable)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = std::fs::DirBuilder::new();
                builder.mode(0o700);
                builder
                    .create(path)
                    .map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
            }
            #[cfg(not(unix))]
            {
                std::fs::create_dir(path).map_err(|_| PrivateAskFailure::ProfileUnavailable)?;
            }
            super::super::recap_state::directory_identity(path)
                .map(|_| ())
                .map_err(|_| PrivateAskFailure::ProfileUnavailable)
        }
        Err(_) => Err(PrivateAskFailure::ProfileUnavailable),
    }
}

pub(super) fn copy_private_profile(
    source: &Path,
    destination: &Path,
) -> Result<(), PrivateAskFailure> {
    let source = canonical_profile_source(source)?;
    if is_live_hermes_profile(&source) {
        return Err(PrivateAskFailure::ProfileUnavailable);
    }
    let mut budget = ProfileCopyBudget::default();
    copy_profile_tree(&source, destination, &mut budget, 0)
}
