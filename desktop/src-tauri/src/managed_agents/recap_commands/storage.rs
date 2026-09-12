use std::io::{Read, Write};

use sha2::{Digest, Sha256};
use tauri::AppHandle;

use super::{
    canonical_event_id, default_settings, valid_generation_id, RecapArtifact, RecapSettings,
    MAX_RECAP_BYTES, MAX_RECAP_ENTRIES, MAX_RECAP_TOTAL_BYTES, MAX_SETTINGS_BYTES,
    RECAPS_DIRECTORY, RECAP_PROMPT_VERSION, SETTINGS_FILENAME,
};

fn scope_digest(viewer_pubkey: &str, relay_origin: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{relay_origin}\0{viewer_pubkey}").as_bytes(),
    ))
}

fn settings_path<R: tauri::Runtime>(
    app: &AppHandle<R>,
    viewer_pubkey: &str,
    relay_origin: &str,
) -> Result<std::path::PathBuf, String> {
    Ok(
        crate::managed_agents::storage::managed_agents_base_dir(app)?.join(format!(
            "{SETTINGS_FILENAME}-{}.json",
            scope_digest(viewer_pubkey, relay_origin)
        )),
    )
}

fn read_private_json(path: &std::path::Path, limit: usize) -> Result<Option<Vec<u8>>, String> {
    // The metadata probe is only an absent-file fast path. The actual read is
    // an O_NOFOLLOW open followed by fstat in `private_read_file`, so a path
    // replacement between these operations cannot redirect the read.
    match std::fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("state_io".to_string()),
    }
    let mut file = crate::managed_agents::recap_state::private_read_file(path)
        .map_err(|_| "state_ownership".to_string())?;
    let initial = file.metadata().map_err(|_| "state_ownership".to_string())?;
    if initial.len() > limit as u64 {
        return Err("state_ownership".to_string());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "state_io".to_string())?;
    let final_metadata = file.metadata().map_err(|_| "state_ownership".to_string())?;
    if final_metadata.len() != initial.len()
        || bytes.len() as u64 != initial.len()
        || bytes.len() > limit
    {
        return Err("state_ownership".to_string());
    }
    Ok(Some(bytes))
}

pub(crate) struct LoadedRecapSettings {
    pub(crate) settings: RecapSettings,
    pub(crate) error: Option<String>,
}

pub(crate) fn decode_settings(bytes: &[u8]) -> LoadedRecapSettings {
    match serde_json::from_slice(bytes) {
        Ok(settings) => LoadedRecapSettings {
            settings,
            error: None,
        },
        Err(_) => LoadedRecapSettings {
            settings: default_settings(),
            error: Some("invalid_settings".to_string()),
        },
    }
}

pub(crate) fn load_settings<R: tauri::Runtime>(
    app: &AppHandle<R>,
    viewer_pubkey: &str,
    relay_origin: &str,
) -> Result<LoadedRecapSettings, String> {
    let path = settings_path(app, viewer_pubkey, relay_origin)?;
    let Some(bytes) = read_private_json(&path, MAX_SETTINGS_BYTES)? else {
        return Ok(LoadedRecapSettings {
            settings: default_settings(),
            error: None,
        });
    };
    // Corrupt owner-local settings fail closed to the safe default, but the
    // read result carries an explicit repair signal instead of silently
    // presenting that fallback as an authoritative user choice.
    Ok(decode_settings(&bytes))
}

pub(crate) fn write_settings<R: tauri::Runtime>(
    app: &AppHandle<R>,
    viewer_pubkey: &str,
    relay_origin: &str,
    settings: &RecapSettings,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(settings).map_err(|_| "invalid_settings".to_string())?;
    if bytes.len() > MAX_SETTINGS_BYTES {
        return Err("invalid_settings".to_string());
    }
    let path = settings_path(app, viewer_pubkey, relay_origin)?;
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            return Err("state_ownership".to_string());
        }
    }
    atomic_write_recap_json(&path, &bytes)
}

/// Keep the owner-local cache bounded. Access time is used as the read
/// watermark where the platform exposes it; modification time is a stable
/// fallback for filesystems that do not update atime.
fn prune_recap_artifacts(keep_path: &std::path::Path) -> Result<(), String> {
    let directory = keep_path.parent().ok_or_else(|| "state_io".to_string())?;
    let mut entries = Vec::new();
    let mut scanned = 0usize;
    for entry in std::fs::read_dir(directory).map_err(|_| "state_io".to_string())? {
        if scanned >= MAX_RECAP_ENTRIES.saturating_mul(3) {
            break;
        }
        scanned = scanned.saturating_add(1);
        let entry = entry.map_err(|_| "state_io".to_string())?;
        let path = entry.path();
        if path == keep_path || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| "state_io".to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != rustix::process::getuid().as_raw() || metadata.mode() & 0o077 != 0
            {
                continue;
            }
        }
        let watermark = metadata
            .accessed()
            .or_else(|_| metadata.modified())
            .map_err(|_| "state_io".to_string())?;
        entries.push((watermark, path, metadata.len()));
    }
    entries.sort_by_key(|entry| entry.0);
    let other_bytes = entries
        .iter()
        .fold(0u64, |total, (_, _, bytes)| total.saturating_add(*bytes));
    let keep_len = std::fs::symlink_metadata(keep_path)
        .map_err(|_| "state_io".to_string())?
        .len();
    let mut total_bytes = other_bytes.saturating_add(keep_len);
    let mut total_entries = entries.len().saturating_add(1);
    for (_, path, bytes) in entries {
        if total_entries <= MAX_RECAP_ENTRIES && total_bytes <= MAX_RECAP_TOTAL_BYTES {
            break;
        }
        std::fs::remove_file(path).map_err(|_| "state_io".to_string())?;
        total_entries = total_entries.saturating_sub(1);
        total_bytes = total_bytes.saturating_sub(bytes);
    }
    Ok(())
}

fn recap_directory<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<std::path::PathBuf, String> {
    let base = crate::managed_agents::storage::managed_agents_base_dir(app)?;
    let directory = base.join(RECAPS_DIRECTORY);
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("state_ownership".to_string());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&directory).map_err(|_| "state_io".to_string())?;
        }
        Err(_) => return Err("state_io".to_string()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = std::fs::symlink_metadata(&directory).map_err(|_| "state_io".to_string())?;
        if metadata.uid() != rustix::process::getuid().as_raw() {
            return Err("state_ownership".to_string());
        }
        if metadata.mode() & 0o777 != 0o700 {
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "state_ownership".to_string())?;
        }
    }
    let metadata = std::fs::symlink_metadata(&directory).map_err(|_| "state_io".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("state_ownership".to_string());
    }
    Ok(directory)
}

fn recap_path<R: tauri::Runtime>(
    app: &AppHandle<R>,
    viewer_pubkey: &str,
    relay_origin: &str,
    channel_id: &str,
    root_event_id: &str,
    source_manifest_hash: &str,
    settings_fingerprint: &str,
) -> Result<std::path::PathBuf, String> {
    let digest = recap_key_digest(
        relay_origin,
        viewer_pubkey,
        channel_id,
        root_event_id,
        source_manifest_hash,
        settings_fingerprint,
    );
    Ok(recap_directory(app)?.join(format!("{digest}.json")))
}

pub(crate) fn recap_key_digest(
    relay_origin: &str,
    viewer_pubkey: &str,
    channel_id: &str,
    root_event_id: &str,
    source_manifest_hash: &str,
    settings_fingerprint: &str,
) -> String {
    let key = format!(
        "v1\0{relay_origin}\0{viewer_pubkey}\0{channel_id}\0{root_event_id}\0{source_manifest_hash}\0{settings_fingerprint}\0prompt-{RECAP_PROMPT_VERSION}"
    );
    hex::encode(Sha256::digest(key.as_bytes()))
}

pub(crate) fn write_artifact<R: tauri::Runtime>(
    app: &AppHandle<R>,
    artifact: &RecapArtifact,
) -> Result<(), String> {
    let path = recap_path(
        app,
        &artifact.viewer_pubkey,
        &artifact.relay_origin,
        &artifact.channel_id,
        &artifact.root_event_id,
        &artifact.recap.source_manifest_hash,
        &artifact.settings_fingerprint,
    )?;
    let bytes = serde_json::to_vec(artifact).map_err(|_| "state_io".to_string())?;
    if bytes.len() > MAX_RECAP_BYTES {
        return Err("output_limit".to_string());
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            return Err("state_ownership".to_string());
        }
    }
    atomic_write_recap_json(&path, &bytes)?;
    prune_recap_artifacts(&path)
}

/// Atomically replace one recap-owned JSON file without following a target
/// symlink. The parent identity is checked before and after the temporary
/// write; rename replaces a raced target entry rather than opening it.
fn atomic_write_recap_json(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "state_io".to_string())?;
    let parent_identity = crate::managed_agents::recap_state::directory_identity(parent)
        .map_err(|_| "state_ownership".to_string())?;
    let temporary = parent.join(format!(".recap-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "state_io".to_string())?;
        file.write_all(bytes).map_err(|_| "state_io".to_string())?;
        file.sync_all().map_err(|_| "state_io".to_string())?;
        drop(file);
        if crate::managed_agents::recap_state::directory_identity(parent)
            .map_err(|_| "state_ownership".to_string())?
            != parent_identity
        {
            return Err("state_ownership".to_string());
        }
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() {
                return Err("state_ownership".to_string());
            }
        }
        std::fs::rename(&temporary, path).map_err(|_| "state_io".to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn load_artifact<R: tauri::Runtime>(
    app: &AppHandle<R>,
    viewer_pubkey: &str,
    relay_origin: &str,
    channel_id: &str,
    root_event_id: &str,
) -> Result<Option<RecapArtifact>, String> {
    let directory = recap_directory(app)?;
    let mut newest: Option<(u64, RecapArtifact)> = None;
    // Retention bounds the directory, and the scan has a separate hard cap so
    // a hostile or unrelated producer cannot turn a read into unbounded work.
    for entry in std::fs::read_dir(directory)
        .map_err(|_| "state_io".to_string())?
        .take(MAX_RECAP_ENTRIES.saturating_mul(3))
    {
        let entry = entry.map_err(|_| "state_io".to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(bytes) = read_private_json(&path, MAX_RECAP_BYTES)? else {
            continue;
        };
        let Ok(artifact) = serde_json::from_slice::<RecapArtifact>(&bytes) else {
            // A corrupt cache entry is never authoritative and must not hide
            // a valid entry for this exact owner/thread scope.
            continue;
        };
        if artifact.viewer_pubkey != viewer_pubkey
            || artifact.relay_origin != relay_origin
            || artifact.channel_id != channel_id
            || artifact.root_event_id != root_event_id
            || !valid_generation_id(&artifact.recap.generation_id)
            || !artifact
                .recap
                .source_event_ids
                .iter()
                .all(|event_id| canonical_event_id(event_id).is_ok())
            || artifact.settings_fingerprint.len() != 64
            || !artifact
                .settings_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || artifact.recap.source_manifest_hash.len() != 64
            || !artifact
                .recap
                .source_manifest_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            continue;
        }
        let generated_at = artifact.recap.generated_at;
        if newest
            .as_ref()
            .is_none_or(|(current, _)| generated_at > *current)
        {
            newest = Some((generated_at, artifact));
        }
    }
    Ok(newest.map(|(_, artifact)| artifact))
}
