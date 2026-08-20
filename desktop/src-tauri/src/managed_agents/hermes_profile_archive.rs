//! Hermes-specific profile archive, restore, and permanent-delete mechanics.
//!
//! Archives are written copy → verify → remove. This module never operates on
//! the Hermes home root or the reserved `default` profile.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use tar::{Archive, Builder, EntryType, Header};

use super::{
    hermes_profile::{crew_may_mutate_hermes_profile, validate_hermes_profile_name},
    hermes_profile_lifecycle::{hermes_profile_dir, hermes_profiles_dir},
    nest::nest_dir,
    process_is_running, ManagedAgentPairRuntime, ManagedAgentRecord, ManagedAgentRuntimeKey,
};

pub const ARCHIVE_SCHEMA_VERSION: u32 = 1;
pub const ARCHIVE_EXCLUSIONS: &[&str] = &["audio_cache", "image_cache", "logs"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesProfileArchiveManifest {
    pub schema_version: u32,
    pub profile: String,
    pub archived_at: String,
    pub bound_agent_name: Option<String>,
    pub bound_agent_pubkey: Option<String>,
    pub offboard_reason: Option<String>,
    pub exclusions: Vec<String>,
    pub skipped_links: Vec<String>,
    pub entry_count: u64,
    pub included_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesProfileArchiveListing {
    pub id: String,
    pub archive_bytes: u64,
    pub manifest: HermesProfileArchiveManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesProfileArchiveEstimate {
    pub included_bytes: u64,
    pub excluded_bytes: u64,
    pub entry_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HermesProfileArchiveResult {
    Archived {
        id: String,
        profile: String,
        included_bytes: u64,
        archive_bytes: u64,
        skipped_link_count: u64,
    },
    Restored {
        id: String,
        profile: String,
    },
    PermanentlyDeleted {
        id: String,
        profile: String,
    },
    InvalidName {
        profile: String,
        message: String,
    },
    DoesNotExist {
        id: String,
        message: String,
    },
    Collision {
        profile: String,
        message: String,
    },
    AgentRunning {
        profile: String,
        agent_name: String,
        agent_pubkey: String,
        message: String,
    },
    ConfirmationMismatch {
        profile: String,
        message: String,
    },
    Failed {
        profile: Option<String>,
        id: Option<String>,
        message: String,
    },
}

impl HermesProfileArchiveResult {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Archived { id, .. } => format!("Hermes profile archived as '{id}'."),
            Self::Restored { profile, .. } => format!("Hermes profile '{profile}' restored."),
            Self::PermanentlyDeleted { profile, .. } => {
                format!("Hermes profile '{profile}' permanently deleted.")
            }
            Self::InvalidName { message, .. }
            | Self::DoesNotExist { message, .. }
            | Self::Collision { message, .. }
            | Self::AgentRunning { message, .. }
            | Self::ConfirmationMismatch { message, .. }
            | Self::Failed { message, .. } => message.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HermesProfileArchiveAgent {
    pub name: String,
    pub pubkey: String,
}

fn archive_root() -> Result<PathBuf, String> {
    nest_dir()
        .map(|root| root.join("profile-archives"))
        .ok_or_else(|| "could not resolve Crew nest directory".to_string())
}

fn ensure_archive_root(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| format!("failed to create archive directory: {e}"))?;
    if fs::symlink_metadata(root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("archive directory must not be a symlink".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("failed to restrict archive directory: {e}"))?;
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<String, HermesProfileArchiveResult> {
    let profile = name.trim().to_string();
    validate_hermes_profile_name(&profile).map_err(|message| {
        HermesProfileArchiveResult::InvalidName {
            profile: profile.clone(),
            message,
        }
    })?;
    if !crew_may_mutate_hermes_profile(&profile) {
        return Err(HermesProfileArchiveResult::InvalidName {
            profile,
            message: "the default Hermes profile is never managed by Crew".to_string(),
        });
    }
    Ok(profile)
}

fn archive_id(profile: &str, root: &Path) -> Result<String, String> {
    let timestamp = DateTime::<Utc>::from(SystemTime::now()).format("%Y%m%d-%H%M%S");
    let base = format!("{profile}-{timestamp}");
    if !root.join(format!("{base}.tar.gz")).exists() {
        return Ok(base);
    }
    for suffix in 1..10_000 {
        let id = format!("{base}-{suffix}");
        if !root.join(format!("{id}.tar.gz")).exists() {
            return Ok(id);
        }
    }
    Err("could not allocate a unique archive id".to_string())
}

fn walk_profile(
    root: &Path,
    path: &Path,
    estimate: &mut HermesProfileArchiveEstimate,
    entries: &mut Vec<PathBuf>,
    skipped_links: &mut Vec<String>,
) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if path != root {
        let relative = path.strip_prefix(root).unwrap_or(path);
        if let Some(Component::Normal(first)) = relative.components().next() {
            if ARCHIVE_EXCLUSIONS
                .iter()
                .any(|excluded| first == std::ffi::OsStr::new(excluded))
            {
                if metadata.is_file() {
                    estimate.excluded_bytes =
                        estimate.excluded_bytes.saturating_add(metadata.len());
                } else if metadata.is_dir() {
                    for child in fs::read_dir(path)? {
                        let child_path = child?.path();
                        let child_metadata = fs::symlink_metadata(&child_path)?;
                        if child_metadata.is_dir() {
                            let mut nested = HermesProfileArchiveEstimate {
                                included_bytes: 0,
                                excluded_bytes: 0,
                                entry_count: 0,
                            };
                            let mut ignored_entries = Vec::new();
                            let mut ignored_links = Vec::new();
                            walk_profile(
                                root,
                                &child_path,
                                &mut nested,
                                &mut ignored_entries,
                                &mut ignored_links,
                            )?;
                            estimate.excluded_bytes = estimate
                                .excluded_bytes
                                .saturating_add(nested.excluded_bytes)
                                .saturating_add(nested.included_bytes);
                        } else {
                            estimate.excluded_bytes =
                                estimate.excluded_bytes.saturating_add(child_metadata.len());
                        }
                    }
                }
                return Ok(());
            }
        }
        if metadata.file_type().is_symlink() {
            let relative = path.strip_prefix(root).unwrap_or(path);
            skipped_links.push(relative.to_string_lossy().into_owned());
            return Ok(());
        }
        estimate.entry_count += 1;
        if metadata.is_file() {
            estimate.included_bytes = estimate.included_bytes.saturating_add(metadata.len());
        }
        entries.push(path.to_path_buf());
    }
    if metadata.is_dir() {
        for child in fs::read_dir(path)? {
            walk_profile(root, &child?.path(), estimate, entries, skipped_links)?;
        }
    }
    Ok(())
}

fn profile_estimate(
    root: &Path,
) -> Result<(HermesProfileArchiveEstimate, Vec<PathBuf>, Vec<String>), String> {
    if !root.is_dir() {
        return Err("profile directory does not exist".to_string());
    }
    let mut estimate = HermesProfileArchiveEstimate {
        included_bytes: 0,
        excluded_bytes: 0,
        entry_count: 0,
    };
    let mut entries = Vec::new();
    let mut skipped_links = Vec::new();
    walk_profile(root, root, &mut estimate, &mut entries, &mut skipped_links)
        .map_err(|e| format!("failed to inspect profile: {e}"))?;
    Ok((estimate, entries, skipped_links))
}

fn append_manifest(
    builder: &mut Builder<GzEncoder<File>>,
    manifest: &HermesProfileArchiveManifest,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|e| format!("failed to serialize archive manifest: {e}"))?;
    let mut header = Header::new_gnu();
    header
        .set_path("manifest.json")
        .map_err(|e| e.to_string())?;
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    builder
        .append(&header, bytes.as_slice())
        .map_err(|e| format!("failed to append archive manifest: {e}"))
}

fn pack(
    archive_path: &Path,
    profile: &str,
    entries: &[PathBuf],
    manifest: &HermesProfileArchiveManifest,
    root: &Path,
) -> Result<(), String> {
    let file = File::create(archive_path).map_err(|e| format!("failed to create archive: {e}"))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = Builder::new(encoder);
    for entry in entries {
        let relative = entry
            .strip_prefix(root)
            .map_err(|e| format!("archive path error: {e}"))?;
        let archive_name = Path::new(profile).join(relative);
        let metadata =
            fs::symlink_metadata(entry).map_err(|e| format!("failed to inspect entry: {e}"))?;
        if metadata.is_dir() {
            builder
                .append_dir(&archive_name, entry)
                .map_err(|e| format!("failed to append directory: {e}"))?;
        } else if metadata.is_file() {
            builder
                .append_path_with_name(entry, &archive_name)
                .map_err(|e| format!("failed to append file: {e}"))?;
        }
    }
    append_manifest(&mut builder, manifest)?;
    let encoder = builder
        .into_inner()
        .map_err(|e| format!("failed to finish archive: {e}"))?;
    encoder
        .finish()
        .map_err(|e| format!("failed to compress archive: {e}"))?;
    Ok(())
}

fn read_manifest(path: &Path) -> Result<HermesProfileArchiveManifest, String> {
    let bytes = fs::read(path).map_err(|e| format!("failed to read manifest: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid archive manifest: {e}"))
}

pub fn estimate_profile(profile: &str) -> Result<HermesProfileArchiveEstimate, String> {
    let profile = validate_name(profile).map_err(|r| r.message())?;
    let root = hermes_profile_dir(&profile).ok_or("could not resolve Hermes profile directory")?;
    profile_estimate(&root).map(|(estimate, _, _)| estimate)
}

pub fn archive_profile(
    profile: &str,
    agent: Option<&HermesProfileArchiveAgent>,
    reason: Option<&str>,
) -> HermesProfileArchiveResult {
    let root = match archive_root() {
        Ok(root) => root,
        Err(message) => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(profile.trim().to_string()),
                id: None,
                message,
            }
        }
    };
    archive_profile_with(profile, &root, agent, reason)
}

pub fn archive_profile_with(
    profile: &str,
    archive_dir: &Path,
    agent: Option<&HermesProfileArchiveAgent>,
    reason: Option<&str>,
) -> HermesProfileArchiveResult {
    let profile = match validate_name(profile) {
        Ok(profile) => profile,
        Err(result) => return result,
    };
    let profile_dir = match hermes_profile_dir(&profile) {
        Some(path) => path,
        None => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(profile),
                id: None,
                message: "could not resolve Hermes profile directory".to_string(),
            }
        }
    };
    let (estimate, entries, skipped_links) = match profile_estimate(&profile_dir) {
        Ok(value) => value,
        Err(message) => {
            return HermesProfileArchiveResult::DoesNotExist {
                id: profile,
                message,
            }
        }
    };
    if let Err(message) = ensure_archive_root(archive_dir) {
        return HermesProfileArchiveResult::Failed {
            profile: Some(profile),
            id: None,
            message,
        };
    }
    let id = match archive_id(&profile, archive_dir) {
        Ok(id) => id,
        Err(message) => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(profile),
                id: None,
                message,
            }
        }
    };
    let archive_path = archive_dir.join(format!("{id}.tar.gz"));
    let manifest_path = archive_dir.join(format!("{id}.manifest.json"));
    let manifest = HermesProfileArchiveManifest {
        schema_version: ARCHIVE_SCHEMA_VERSION,
        profile: profile.clone(),
        archived_at: DateTime::<Utc>::from(SystemTime::now()).to_rfc3339(),
        bound_agent_name: agent.map(|a| a.name.clone()),
        bound_agent_pubkey: agent.map(|a| a.pubkey.clone()),
        offboard_reason: reason.map(str::to_string),
        exclusions: ARCHIVE_EXCLUSIONS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        skipped_links,
        entry_count: estimate.entry_count,
        included_bytes: estimate.included_bytes,
    };
    let temporary = archive_dir.join(format!(".{id}.tmp"));
    let result = (|| {
        pack(&temporary, &profile, &entries, &manifest, &profile_dir)?;
        let verify =
            File::open(&temporary).map_err(|e| format!("failed to reopen archive: {e}"))?;
        let mut archive = Archive::new(GzDecoder::new(verify));
        let mut found_manifest = false;
        for entry in archive
            .entries()
            .map_err(|e| format!("failed to read archive entries: {e}"))?
        {
            let mut entry = entry.map_err(|e| format!("failed to read archive entry: {e}"))?;
            if entry.path().map_err(|e| e.to_string())? == Path::new("manifest.json") {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                let embedded: HermesProfileArchiveManifest =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if embedded != manifest {
                    return Err("embedded manifest differs from sidecar manifest".to_string());
                }
                found_manifest = true;
            }
        }
        if !found_manifest {
            return Err("archive verification found no manifest".to_string());
        }
        fs::rename(&temporary, &archive_path)
            .map_err(|e| format!("failed to publish archive: {e}"))?;
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("failed to write archive manifest: {e}"))?;
        fs::remove_dir_all(&profile_dir)
            .map_err(|e| format!("failed to remove archived profile: {e}"))?;
        Ok::<(), String>(())
    })();
    let _ = fs::remove_file(&temporary);
    if let Err(message) = result {
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_file(&manifest_path);
        return HermesProfileArchiveResult::Failed {
            profile: Some(profile),
            id: Some(id),
            message,
        };
    }
    let archive_bytes = match fs::metadata(&archive_path) {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(profile),
                id: Some(id),
                message: format!("failed to stat published archive: {error}"),
            }
        }
    };
    HermesProfileArchiveResult::Archived {
        id,
        profile,
        included_bytes: manifest.included_bytes,
        archive_bytes,
        skipped_link_count: manifest.skipped_links.len() as u64,
    }
}
fn safe_archive_id(id: &str) -> Result<(), String> {
    if id.trim().is_empty()
        || id
            != Path::new(id)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
        || id.contains("..")
        || id.contains('/')
        || id.contains('\\')
    {
        return Err("invalid archive id".to_string());
    }
    Ok(())
}
pub fn list_archives() -> Result<Vec<HermesProfileArchiveListing>, String> {
    list_archives_with(&archive_root()?)
}
pub fn list_archives_with(root: &Path) -> Result<Vec<HermesProfileArchiveListing>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(manifest) = read_manifest(&path) {
                let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(id) = file_name.strip_suffix(".manifest.json") else {
                    continue;
                };
                let archive_bytes = fs::metadata(root.join(format!("{id}.tar.gz")))
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                manifests.push(HermesProfileArchiveListing {
                    id: id.to_string(),
                    archive_bytes,
                    manifest,
                });
            }
        }
    }
    manifests.sort_by(|a, b| b.manifest.archived_at.cmp(&a.manifest.archived_at));
    Ok(manifests)
}
fn validate_tar_path(path: &Path, profile: &str) -> Result<(), String> {
    if path == Path::new("manifest.json") {
        return Ok(());
    }
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == std::ffi::OsStr::new(profile) => {}
        _ => return Err("archive entry has an invalid profile root".to_string()),
    }
    for component in components {
        if !matches!(component, Component::Normal(_)) {
            return Err("archive entry contains path traversal".to_string());
        }
    }
    Ok(())
}
macro_rules! restore_failure {
    ($profile:expr, $id:expr, $message:expr) => {
        HermesProfileArchiveResult::Failed {
            profile: Some($profile.to_string()),
            id: Some($id.to_string()),
            message: $message.into(),
        }
    };
}
pub fn restore_archive(id: &str) -> HermesProfileArchiveResult {
    match archive_root() {
        Ok(root) => restore_archive_with(id, &root),
        Err(message) => HermesProfileArchiveResult::Failed {
            profile: None,
            id: Some(id.to_string()),
            message,
        },
    }
}
pub fn restore_archive_with(id: &str, archive_dir: &Path) -> HermesProfileArchiveResult {
    if let Err(message) = safe_archive_id(id) {
        return HermesProfileArchiveResult::Failed {
            profile: None,
            id: Some(id.to_string()),
            message,
        };
    }
    let manifest_path = archive_dir.join(format!("{id}.manifest.json"));
    let manifest = match read_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(message) => {
            return HermesProfileArchiveResult::DoesNotExist {
                id: id.to_string(),
                message,
            }
        }
    };
    if validate_hermes_profile_name(&manifest.profile).is_err() {
        return HermesProfileArchiveResult::InvalidName {
            profile: manifest.profile,
            message: "archive manifest contains an invalid profile name".to_string(),
        };
    }
    let profile_dir = match hermes_profile_dir(&manifest.profile) {
        Some(path) => path,
        None => {
            return restore_failure!(
                &manifest.profile,
                id,
                "could not resolve Hermes profile directory"
            );
        }
    };
    if fs::symlink_metadata(&profile_dir).is_ok() {
        return HermesProfileArchiveResult::Collision {
            profile: manifest.profile,
            message: "a live Hermes profile with this name already exists".to_string(),
        };
    }
    let profiles = match hermes_profiles_dir() {
        Some(path) => path,
        None => {
            return restore_failure!(
                &manifest.profile,
                id,
                "could not resolve Hermes profiles directory"
            );
        }
    };
    if let Err(error) = fs::create_dir_all(&profiles) {
        return restore_failure!(&manifest.profile, id, error.to_string());
    }
    let file = match File::open(archive_dir.join(format!("{id}.tar.gz"))) {
        Ok(file) => file,
        Err(error) => {
            return HermesProfileArchiveResult::DoesNotExist {
                id: id.to_string(),
                message: error.to_string(),
            }
        }
    };
    let mut archive = Archive::new(GzDecoder::new(file));
    let entries = match archive.entries() {
        Ok(entries) => entries,
        Err(error) => {
            return restore_failure!(&manifest.profile, id, error.to_string());
        }
    };
    for entry in entries {
        let mut entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let _ = fs::remove_dir_all(&profile_dir);
                return restore_failure!(&manifest.profile, id, error.to_string());
            }
        };
        let path = match entry.path() {
            Ok(path) => path.into_owned(),
            Err(error) => {
                let _ = fs::remove_dir_all(&profile_dir);
                return restore_failure!(&manifest.profile, id, error.to_string());
            }
        };
        if let Err(error) = validate_tar_path(&path, &manifest.profile) {
            let _ = fs::remove_dir_all(&profile_dir);
            return restore_failure!(&manifest.profile, id, error);
        }
        if matches!(
            entry.header().entry_type(),
            EntryType::Symlink | EntryType::Link
        ) {
            let _ = fs::remove_dir_all(&profile_dir);
            return restore_failure!(&manifest.profile, id, "archive contains a link entry");
        }
        if path == Path::new("manifest.json") {
            continue;
        }
        let destination = profiles.join(&path);
        if let Some(parent) = destination.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                let _ = fs::remove_dir_all(&profile_dir);
                return restore_failure!(&manifest.profile, id, error.to_string());
            }
        }
        if let Err(error) = entry.unpack(&destination) {
            let _ = fs::remove_dir_all(&profile_dir);
            return restore_failure!(&manifest.profile, id, error.to_string());
        }
    }
    HermesProfileArchiveResult::Restored {
        id: id.to_string(),
        profile: manifest.profile,
    }
}
pub fn permanently_delete_archive(id: &str, confirmation: &str) -> HermesProfileArchiveResult {
    match archive_root() {
        Ok(root) => permanently_delete_archive_with(id, confirmation, &root),
        Err(message) => HermesProfileArchiveResult::Failed {
            profile: None,
            id: Some(id.to_string()),
            message,
        },
    }
}
pub fn permanently_delete_archive_with(
    id: &str,
    confirmation: &str,
    archive_dir: &Path,
) -> HermesProfileArchiveResult {
    if let Err(message) = safe_archive_id(id) {
        return HermesProfileArchiveResult::Failed {
            profile: None,
            id: Some(id.to_string()),
            message,
        };
    }
    let manifest_path = archive_dir.join(format!("{id}.manifest.json"));
    let manifest = match read_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(message) => {
            return HermesProfileArchiveResult::DoesNotExist {
                id: id.to_string(),
                message,
            }
        }
    };
    if let Err(result) = validate_name(&manifest.profile) {
        return result;
    }
    if manifest.profile != confirmation {
        return HermesProfileArchiveResult::ConfirmationMismatch {
            profile: manifest.profile,
            message: "confirmation token must exactly match the profile name".to_string(),
        };
    }
    let archive_path = archive_dir.join(format!("{id}.tar.gz"));
    let archive_result = fs::remove_file(&archive_path);
    if let Err(error) = archive_result {
        if error.kind() != io::ErrorKind::NotFound {
            return HermesProfileArchiveResult::Failed {
                profile: Some(manifest.profile),
                id: Some(id.to_string()),
                message: error.to_string(),
            };
        }
    }
    if let Err(error) = fs::remove_file(&manifest_path) {
        if error.kind() == io::ErrorKind::NotFound {
            return HermesProfileArchiveResult::PermanentlyDeleted {
                id: id.to_string(),
                profile: manifest.profile,
            };
        }
        return HermesProfileArchiveResult::Failed {
            profile: Some(manifest.profile),
            id: Some(id.to_string()),
            message: error.to_string(),
        };
    }
    HermesProfileArchiveResult::PermanentlyDeleted {
        id: id.to_string(),
        profile: manifest.profile,
    }
}
pub fn running_agent_for_profile(
    profile: &str,
    records: &[ManagedAgentRecord],
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
) -> Option<HermesProfileArchiveAgent> {
    let running_keys = runtimes
        .iter_mut()
        .filter_map(|(key, runtime)| {
            runtime
                .child
                .try_wait()
                .ok()
                .flatten()
                .is_none()
                .then(|| key.clone())
        })
        .collect::<Vec<_>>();
    if let Some(agent) = running_agent_for_profile_with_keys(profile, records, &running_keys) {
        return Some(agent);
    }
    records.iter().find_map(|record| {
        let bound = record.hermes_profile.as_deref()?.trim();
        (bound == profile && record.runtime_pid.is_some_and(process_is_running)).then(|| {
            HermesProfileArchiveAgent {
                name: record.name.clone(),
                pubkey: record.pubkey.clone(),
            }
        })
    })
}
pub fn running_agent_for_profile_with_keys(
    profile: &str,
    records: &[ManagedAgentRecord],
    running_keys: &[ManagedAgentRuntimeKey],
) -> Option<HermesProfileArchiveAgent> {
    records.iter().find_map(|record| {
        let bound = record.hermes_profile.as_deref()?.trim();
        if bound != profile {
            return None;
        }
        let running = running_keys
            .iter()
            .any(|key| key.pubkey.eq_ignore_ascii_case(&record.pubkey));
        running.then(|| HermesProfileArchiveAgent {
            name: record.name.clone(),
            pubkey: record.pubkey.clone(),
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn with_home<F: FnOnce(&Path, &Path)>(f: F) {
        let _guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path().join("hermes");
        let profile = home.join("profiles/scout");
        fs::create_dir_all(&profile).expect("profile");
        fs::write(profile.join("config.yaml"), "model: scout\n").expect("config");
        fs::write(profile.join("notes.txt"), "hello").expect("notes");
        fs::create_dir(profile.join("audio_cache")).expect("audio cache");
        fs::write(profile.join("audio_cache/data.bin"), "excluded").expect("cache file");
        fs::create_dir(profile.join("logs")).expect("logs");
        fs::write(profile.join("logs/run.log"), "excluded").expect("log");
        #[cfg(unix)]
        std::os::unix::fs::symlink(profile.join("notes.txt"), profile.join("shortcut"))
            .expect("symlink");
        let original = std::env::var("HERMES_HOME").ok();
        std::env::set_var("HERMES_HOME", &home);
        f(&home, &temp.path().join("archives"));
        match original {
            Some(value) => std::env::set_var("HERMES_HOME", value),
            None => std::env::remove_var("HERMES_HOME"),
        }
    }
    #[test]
    fn archive_restore_round_trip_and_cache_exclusion() {
        with_home(|home, archives| {
            let estimate = estimate_profile("scout").expect("estimate");
            assert!(estimate.excluded_bytes >= b"excluded".len() as u64 * 2);
            let result = archive_profile_with(
                "scout",
                archives,
                Some(&HermesProfileArchiveAgent {
                    name: "Scout".to_string(),
                    pubkey: "a".repeat(64),
                }),
                Some("offboard"),
            );
            let HermesProfileArchiveResult::Archived { id, .. } = result else {
                panic!("unexpected archive result: {result:?}");
            };
            assert!(!home.join("profiles/scout").exists());
            let manifest: HermesProfileArchiveManifest = serde_json::from_slice(
                &fs::read(archives.join(format!("{id}.manifest.json"))).expect("manifest"),
            )
            .expect("parse manifest");
            assert_eq!(manifest.profile, "scout");
            assert_eq!(manifest.offboard_reason.as_deref(), Some("offboard"));
            assert_eq!(manifest.skipped_links, vec!["shortcut".to_string()]);
            let listing = list_archives_with(archives).expect("listing");
            assert_eq!(listing[0].id, id);
            assert_eq!(
                listing[0].archive_bytes,
                fs::metadata(archives.join(format!("{id}.tar.gz")))
                    .expect("archive stat")
                    .len()
            );
            assert!(restore_archive_with(&id, archives)
                .message()
                .contains("restored"));
            assert_eq!(
                fs::read(home.join("profiles/scout/notes.txt")).expect("notes"),
                b"hello"
            );
            let archive = File::open(archives.join(format!("{id}.tar.gz"))).expect("archive");
            let mut tar = Archive::new(GzDecoder::new(archive));
            let names = tar
                .entries()
                .expect("entries")
                .map(|entry| entry.expect("entry").path().expect("path").into_owned())
                .collect::<Vec<_>>();
            assert!(!names
                .iter()
                .any(|path| path.to_string_lossy().contains("logs")));
            assert!(!names
                .iter()
                .any(|path| path.to_string_lossy().contains("audio_cache")));
            assert!(matches!(
                permanently_delete_archive_with(&id, "scout", archives),
                HermesProfileArchiveResult::PermanentlyDeleted { .. }
            ));
        });
    }
    #[test]
    fn invalid_name_and_confirmation_are_rejected() {
        with_home(|_, archives| {
            assert!(matches!(
                archive_profile_with("../bad", archives, None, None),
                HermesProfileArchiveResult::InvalidName { .. }
            ));
            let manifest = HermesProfileArchiveManifest {
                schema_version: 1,
                profile: "scout".to_string(),
                archived_at: "2026-08-10T00:00:00Z".to_string(),
                bound_agent_name: None,
                bound_agent_pubkey: None,
                offboard_reason: None,
                exclusions: Vec::new(),
                skipped_links: Vec::new(),
                entry_count: 0,
                included_bytes: 0,
            };
            fs::create_dir_all(archives).expect("archives");
            fs::write(
                archives.join("scout-archive.manifest.json"),
                serde_json::to_vec(&manifest).expect("json"),
            )
            .expect("manifest");
            assert!(matches!(
                permanently_delete_archive_with("scout-archive", "scou", archives),
                HermesProfileArchiveResult::ConfirmationMismatch { .. }
            ));
            assert!(matches!(
                estimate_profile("default"),
                Err(message) if message.contains("default")
            ));
        });
    }
    #[test]
    fn estimate_matches_archive_and_collision_is_non_destructive() {
        with_home(|home, archives| {
            let estimate = estimate_profile("scout").expect("estimate");
            let result = archive_profile_with("scout", archives, None, None);
            let HermesProfileArchiveResult::Archived {
                id,
                included_bytes,
                archive_bytes,
                skipped_link_count,
                ..
            } = result
            else {
                panic!("unexpected archive result: {result:?}");
            };
            assert_eq!(included_bytes, estimate.included_bytes);
            assert!(archive_bytes > 0);
            assert_eq!(skipped_link_count, 1);
            fs::create_dir_all(home.join("profiles/scout")).expect("collision profile");
            fs::write(home.join("profiles/scout/keep.txt"), "keep").expect("keep");
            assert!(matches!(
                restore_archive_with(&id, archives),
                HermesProfileArchiveResult::Collision { .. }
            ));
            assert_eq!(
                fs::read(home.join("profiles/scout/keep.txt")).expect("keep"),
                b"keep"
            );
        });
    }
    #[test]
    fn archive_failure_leaves_live_profile_intact() {
        with_home(|home, temp_archives| {
            fs::write(temp_archives, "not a directory").expect("block archive root");
            assert!(matches!(
                archive_profile_with("scout", temp_archives, None, None),
                HermesProfileArchiveResult::Failed { .. }
            ));
            assert!(home.join("profiles/scout/notes.txt").exists());
        });
    }
    #[test]
    fn restore_rejects_traversal_entry() {
        for path in ["../escape.txt", "/tmp/escape.txt", "other/escape.txt"] {
            assert!(validate_tar_path(Path::new(path), "scout").is_err());
        }
    }
    #[test]
    fn running_agent_guard_matches_profile_and_runtime_pair() {
        let mut record: ManagedAgentRecord = serde_json::from_str(
            r#"{"pubkey":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","name":"Scout","private_key_nsec":"nsec1fake","relay_url":"","acp_command":"buzz-acp","agent_command":"hermes","agent_args":[],"hermes_profile":"scout","mcp_command":"","turn_timeout_seconds":320,"system_prompt":null,"model":null,"provider":null,"env_vars":{},"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","last_started_at":null,"last_stopped_at":null,"last_exit_code":null,"last_error":null}"#,
        )
        .expect("record");
        record.hermes_profile = Some("scout".to_string());
        let key = ManagedAgentRuntimeKey::new(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "wss://relay.example",
        )
        .expect("runtime key");
        let blocked = running_agent_for_profile_with_keys("scout", &[record.clone()], &[key]);
        assert_eq!(blocked.map(|agent| agent.name), Some("Scout".to_string()));
        assert!(running_agent_for_profile_with_keys("other", &[record.clone()], &[]).is_none());
        let mut adopted = record.clone();
        adopted.name = "Adopted".to_string();
        adopted.runtime_pid = Some(std::process::id());
        assert_eq!(
            running_agent_for_profile("scout", &[adopted], &mut HashMap::new())
                .map(|agent| agent.name),
            Some("Adopted".to_string())
        );
    }
    #[test]
    fn corrupt_sidecar_is_skipped_by_archive_listing() {
        with_home(|_, archives| {
            fs::create_dir_all(archives).expect("archives");
            fs::write(archives.join("broken.manifest.json"), b"{not json").expect("corrupt");
            assert!(list_archives_with(archives).expect("listing").is_empty());
        });
    }
}
