//! Disposable, manifest-owned recap state. This module never opens an employee
//! profile, stores credentials, or follows a caller-supplied session path.

use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::recap_adapter::RECAP_INPUT_LIMIT;

const RECOVERY_AGE_SECONDS: u64 = 15 * 60;
const MANIFEST: &str = "owner.json";

/// Fixed state failure codes; no profile or prompt content reaches diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapStateFailure {
    Io,
    InputLimit,
    Ownership,
    RuntimeNotReady,
    ProcessPending,
    UnsupportedPlatform,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    ProcessMayBeRunning,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format_version: u8,
    run_id: String,
    created_at: u64,
    expires_at: u64,
    phase: Phase,
    /// Direct child PID persisted after the bounded owner has secured it.
    /// `None` means a crash may have happened before spawn identity could be
    /// recorded; startup recovery must preserve that root conservatively.
    #[serde(default)]
    process_pid: Option<u32>,
    directory: DirectoryIdentity,
}

/// One newly-created root; cleanup requires the same ownership generation.
pub(crate) struct OwnedRecapRun {
    path: PathBuf,
    manifest: Manifest,
    parent: DirectoryIdentity,
    base: DirectoryIdentity,
}

/// Recovery leaves unknown roots and uncertain process ownership untouched.
#[derive(Debug, Default)]
pub(crate) struct RecapRecovery {
    pub removed: usize,
    pub preserved_unknown: usize,
    pub pending_process: usize,
    /// More entries remain unexamined; this report does not mean recovery completed.
    pub scan_limited: bool,
}

impl OwnedRecapRun {
    /// Create a private generation below the managed-agents base, never home.
    pub(crate) fn create(base: &Path, now: u64) -> Result<Self, RecapStateFailure> {
        let base_identity = validate_owned_base(base)?;
        let parent = base.join("recap-runs");
        private_directory(&parent)?;
        let parent = parent.canonicalize().map_err(|_| RecapStateFailure::Io)?;
        let parent_identity = directory_identity(&parent)?;
        let run_id = uuid::Uuid::new_v4().to_string();
        let path = parent.join(&run_id);
        private_directory(&path)?;
        let run = Self {
            manifest: Manifest {
                format_version: 1,
                run_id,
                created_at: now,
                expires_at: now.saturating_add(RECOVERY_AGE_SECONDS),
                phase: Phase::Prepared,
                process_pid: None,
                directory: directory_identity(&path)?,
            },
            path,
            parent: parent_identity,
            base: base_identity,
        };
        run.persist(&run.manifest)?;
        Ok(run)
    }

    /// The only working/cache/config root supplied to the native recipe.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Create the fixed private runtime directories used by the native recipe.
    /// No caller-supplied path is accepted and every directory remains below
    /// the already-validated run generation.
    pub(crate) fn prepare_runtime_dirs(&self) -> Result<(), RecapStateFailure> {
        self.verify()?;
        for name in ["home", "config", "tmp", "cache", "data", "state"] {
            private_directory(&self.path.join(name))?;
        }
        self.verify()
    }

    /// Close the writer and return a private read-only stdin handle.
    pub(crate) fn input(&self, bytes: &[u8]) -> Result<File, RecapStateFailure> {
        if bytes.len() > RECAP_INPUT_LIMIT {
            return Err(RecapStateFailure::InputLimit);
        }
        self.verify()?;
        let path = self.path.join("input");
        let mut writer = private_new_file(&path)?;
        writer.write_all(bytes).map_err(|_| RecapStateFailure::Io)?;
        writer.sync_all().map_err(|_| RecapStateFailure::Io)?;
        drop(writer);
        let reader = private_read_file(&path)?;
        if reader.metadata().map_err(|_| RecapStateFailure::Io)?.len() != bytes.len() as u64 {
            return Err(RecapStateFailure::Ownership);
        }
        self.verify()?;
        std::fs::remove_file(path).map_err(|_| RecapStateFailure::Io)?;
        Ok(reader)
    }

    /// Mark before spawning; a crash must never make recovery infer no process.
    pub(crate) fn mark_process_pending(&mut self) -> Result<(), RecapStateFailure> {
        self.set_phase(Phase::ProcessMayBeRunning, None)
    }

    /// Persist the direct child PID once the bounded owner has secured it.
    /// Recovery may use a recorded, now-dead PID to finish cleanup; an absent
    /// PID remains conservatively pending because the process boundary is
    /// unknown after a crash.
    pub(crate) fn mark_process_started(&mut self, pid: u32) -> Result<(), RecapStateFailure> {
        if pid == 0 {
            return Err(RecapStateFailure::Ownership);
        }
        self.verify()?;
        if self.manifest.phase != Phase::ProcessMayBeRunning {
            return Err(RecapStateFailure::ProcessPending);
        }
        let mut next = self.manifest.clone();
        next.process_pid = Some(pid);
        self.persist(&next)?;
        self.manifest = next;
        Ok(())
    }

    /// Mark only after the runner has completed owned-process cleanup.
    pub(crate) fn mark_finished(&mut self) -> Result<(), RecapStateFailure> {
        self.set_phase(Phase::Finished, None)
    }

    fn set_phase(
        &mut self,
        phase: Phase,
        process_pid: Option<u32>,
    ) -> Result<(), RecapStateFailure> {
        self.verify()?;
        let mut next = self.manifest.clone();
        next.phase = phase;
        next.process_pid = process_pid;
        self.persist(&next)?;
        self.manifest = next;
        Ok(())
    }

    fn verify_directory(&self) -> Result<(), RecapStateFailure> {
        let parent = self.path.parent().ok_or(RecapStateFailure::Ownership)?;
        let base = parent.parent().ok_or(RecapStateFailure::Ownership)?;
        if validate_owned_base(base)? != self.base
            || directory_identity(parent)? != self.parent
            || directory_identity(&self.path)? != self.manifest.directory
        {
            return Err(RecapStateFailure::Ownership);
        }
        Ok(())
    }

    fn verify(&self) -> Result<(), RecapStateFailure> {
        self.verify_directory()?;
        if read_manifest(&self.path)? != self.manifest {
            return Err(RecapStateFailure::Ownership);
        }
        Ok(())
    }

    fn persist(&self, manifest: &Manifest) -> Result<(), RecapStateFailure> {
        self.verify_directory()?;
        let temporary = self
            .path
            .join(format!("owner-{}.tmp", uuid::Uuid::new_v4()));
        let mut file = private_new_file(&temporary)?;
        let bytes = serde_json::to_vec(manifest).map_err(|_| RecapStateFailure::Io)?;
        file.write_all(&bytes).map_err(|_| RecapStateFailure::Io)?;
        file.sync_all().map_err(|_| RecapStateFailure::Io)?;
        drop(file);
        self.verify_directory()?;
        std::fs::rename(temporary, self.path.join(MANIFEST)).map_err(|_| RecapStateFailure::Io)?;
        File::open(&self.path)
            .and_then(|file| file.sync_all())
            .map_err(|_| RecapStateFailure::Io)
    }

    /// Remove exactly this disposable generation, never a profile or parent.
    pub(crate) fn cleanup(self) -> Result<(), RecapStateFailure> {
        self.verify()?;
        if self.manifest.phase == Phase::ProcessMayBeRunning {
            return Err(RecapStateFailure::ProcessPending);
        }
        std::fs::remove_dir_all(&self.path).map_err(|_| RecapStateFailure::Io)
    }
}

#[cfg(unix)]
fn private_directory(path: &Path) -> Result<(), RecapStateFailure> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => directory_identity(path).map(|_| ()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            directory_identity(path).map(|_| ())
        }
        Err(_) => Err(RecapStateFailure::Io),
    }
}

#[cfg(not(unix))]
fn private_directory(_path: &Path) -> Result<(), RecapStateFailure> {
    Err(RecapStateFailure::UnsupportedPlatform)
}

/// Startup sweep: expiry is not permission to kill an unverified/reused PID.
pub(crate) fn recover_recap_runs(
    base: &Path,
    now: u64,
) -> Result<RecapRecovery, RecapStateFailure> {
    recover_with_cleanup(base, now, OwnedRecapRun::cleanup)
}

fn recover_with_cleanup(
    base: &Path,
    now: u64,
    mut cleanup: impl FnMut(OwnedRecapRun) -> Result<(), RecapStateFailure>,
) -> Result<RecapRecovery, RecapStateFailure> {
    let base_identity = validate_owned_base(base)?;
    let parent = base.join("recap-runs");
    if !parent.try_exists().map_err(|_| RecapStateFailure::Io)? {
        return Ok(RecapRecovery::default());
    }
    let parent_identity = directory_identity(&parent)?;
    let parent = parent.canonicalize().map_err(|_| RecapStateFailure::Io)?;
    let mut report = RecapRecovery::default();
    let mut first_failure = None;
    // Bound startup work even if an unrelated producer floods this namespace.
    for (index, entry) in std::fs::read_dir(&parent)
        .map_err(|_| RecapStateFailure::Io)?
        .take(1025)
        .enumerate()
    {
        if index == 1024 {
            report.scan_limited = true;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                first_failure.get_or_insert(RecapStateFailure::Io);
                continue;
            }
        };
        let path = entry.path();
        let manifest = match read_manifest(&path) {
            Ok(manifest)
                if directory_identity(&path).ok().as_ref() == Some(&manifest.directory) =>
            {
                manifest
            }
            _ => {
                report.preserved_unknown += 1;
                continue;
            }
        };
        if manifest.expires_at > now {
            continue;
        }
        let mut owned = OwnedRecapRun {
            path,
            manifest,
            parent: parent_identity.clone(),
            base: base_identity.clone(),
        };
        if owned.manifest.phase == Phase::ProcessMayBeRunning {
            // A PID that was durably recorded after ownership was secured can
            // be reclaimed once it is definitely gone. A missing or live PID
            // remains pending; expiry alone is never permission to remove it.
            let Some(pid) = owned.manifest.process_pid else {
                report.pending_process += 1;
                continue;
            };
            if process_is_running_for_recovery(pid) {
                report.pending_process += 1;
                continue;
            }
            if let Err(failure) = owned.mark_finished() {
                first_failure.get_or_insert(failure);
                continue;
            }
        }
        match cleanup(owned) {
            Ok(()) => report.removed += 1,
            Err(failure) => {
                first_failure.get_or_insert(failure);
            }
        }
    }
    match first_failure {
        Some(failure) => Err(failure),
        None => Ok(report),
    }
}

/// A post-restart PID check is only a safe positive signal on Unix. On other
/// platforms this module has no process identity primitive, so preserve the
/// durable root for an operator/owner recovery path instead of guessing.
#[cfg(unix)]
fn process_is_running_for_recovery(pid: u32) -> bool {
    crate::managed_agents::process_is_running(pid)
}

#[cfg(not(unix))]
fn process_is_running_for_recovery(_pid: u32) -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct DirectoryIdentity {
    device: u64,
    inode: u64,
    pub(super) owner: u32,
}

/// Existing canonical base only: do not follow an intermediate agents symlink
/// and then create or sweep a recap namespace outside the approved root.
pub(super) fn validate_owned_base(path: &Path) -> Result<DirectoryIdentity, RecapStateFailure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path).map_err(|_| RecapStateFailure::Ownership)?;
        if !path.is_absolute()
            || !metadata.is_dir()
            || metadata.mode() & 0o022 != 0
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || path
                .canonicalize()
                .map_err(|_| RecapStateFailure::Ownership)?
                != path
        {
            return Err(RecapStateFailure::Ownership);
        }
        Ok(DirectoryIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            owner: metadata.uid(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(RecapStateFailure::UnsupportedPlatform)
    }
}

pub(super) fn directory_identity(path: &Path) -> Result<DirectoryIdentity, RecapStateFailure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path).map_err(|_| RecapStateFailure::Ownership)?;
        if !metadata.is_dir()
            || metadata.mode() & 0o777 != 0o700
            || metadata.uid() != rustix::process::geteuid().as_raw()
        {
            return Err(RecapStateFailure::Ownership);
        }
        Ok(DirectoryIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            owner: metadata.uid(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(RecapStateFailure::UnsupportedPlatform)
    }
}

fn private_new_file(path: &Path) -> Result<File, RecapStateFailure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| RecapStateFailure::Io)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(RecapStateFailure::UnsupportedPlatform)
    }
}

pub(super) fn private_read_file(path: &Path) -> Result<File, RecapStateFailure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| RecapStateFailure::Ownership)?;
        let metadata = file.metadata().map_err(|_| RecapStateFailure::Ownership)?;
        if !metadata.is_file()
            || metadata.mode() & 0o777 != 0o600
            || metadata.nlink() != 1
            || metadata.uid() != rustix::process::geteuid().as_raw()
        {
            return Err(RecapStateFailure::Ownership);
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(RecapStateFailure::UnsupportedPlatform)
    }
}

fn read_manifest(path: &Path) -> Result<Manifest, RecapStateFailure> {
    let identity = directory_identity(path)?;
    let mut bytes = Vec::new();
    private_read_file(&path.join(MANIFEST))?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| RecapStateFailure::Ownership)?;
    if bytes.len() > 4096 {
        return Err(RecapStateFailure::Ownership);
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| RecapStateFailure::Ownership)?;
    if manifest.format_version != 1
        || manifest.directory != identity
        || path.file_name().and_then(|name| name.to_str()) != Some(manifest.run_id.as_str())
        || uuid::Uuid::parse_str(&manifest.run_id)
            .map(|id| id.to_string())
            .ok()
            .as_deref()
            != Some(manifest.run_id.as_str())
        || manifest.expires_at != manifest.created_at.saturating_add(RECOVERY_AGE_SECONDS)
        || manifest.process_pid == Some(0)
    {
        return Err(RecapStateFailure::Ownership);
    }
    Ok(manifest)
}

#[cfg(test)]
#[path = "recap_state/tests.rs"]
mod tests;
