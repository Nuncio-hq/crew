//! Versioned durable lifecycle records under the common Git directory.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::RecordError;
use crate::identity::validate_root_for_record;
use crate::paths::{lifecycle_record_path, lifecycle_records_dir, RECORD_SCHEMA_VERSION};

/// How the registry should treat a projected lifecycle binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleIdentity {
    /// Trusted ACP turn adopted or created this record.
    Verified,
    /// Worktree exists without a durable record.
    Legacy,
    /// Record exists but conflicts with Git/registry truth.
    Conflict,
}

/// V1 durable lifecycle record for one Project thread root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleRecord {
    /// Schema version.
    pub version: u32,
    /// Full 64-hex root event id.
    pub root_event_id: String,
    /// Real NIP-29 routing channel UUID.
    pub routing_channel_id: String,
    /// Optional normalized relay/community scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_scope: Option<String>,
    /// Deterministic managed branch name.
    pub branch: String,
    /// Canonical worktree checkout path.
    pub worktree_path: String,
    /// Optional named base branch used when this record was created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Unix seconds when the record was first created.
    pub created_at: i64,
    /// Unix seconds of last trusted ACP use (monotonic).
    pub last_used_at: i64,
    /// Advanced only after successful eviction.
    pub eviction_generation: u64,
}

impl LifecycleRecord {
    /// Build a new verified record for a trusted ACP adoption.
    pub fn new(
        root_event_id: String,
        routing_channel_id: String,
        community_scope: Option<String>,
        branch: String,
        worktree_path: String,
        now: i64,
    ) -> Self {
        Self::new_with_base(
            root_event_id,
            routing_channel_id,
            community_scope,
            branch,
            worktree_path,
            None,
            now,
        )
    }

    /// Build a verified record that records the requested base branch.
    pub fn new_with_base(
        root_event_id: String,
        routing_channel_id: String,
        community_scope: Option<String>,
        branch: String,
        worktree_path: String,
        base: Option<String>,
        now: i64,
    ) -> Self {
        Self {
            version: RECORD_SCHEMA_VERSION,
            root_event_id,
            routing_channel_id,
            community_scope,
            branch,
            worktree_path,
            base,
            created_at: now,
            last_used_at: now,
            eviction_generation: 0,
        }
    }
}

/// Read a record if present. Missing files return `Ok(None)`.
///
/// Fails closed when the record body's normalized `root_event_id` does not
/// equal the requested root (including a misnamed JSON file).
pub fn read_lifecycle_record(
    common_git: &Path,
    root_event_id: &str,
) -> Result<Option<LifecycleRecord>, RecordError> {
    let root = validate_root_for_record(root_event_id)?;
    let path = lifecycle_record_path(common_git, &root).map_err(RecordError::InvalidIdentity)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|source| RecordError::Io {
        path: path.clone(),
        source,
    })?;
    let record = parse_record_bytes(&path, &bytes)?;
    if record.root_event_id != root {
        return Err(RecordError::Conflict(
            "lifecycle record root does not match requested root".into(),
        ));
    }
    Ok(Some(record))
}

/// Atomically write a complete record (temp + sync + rename) under a lock.
pub fn write_lifecycle_record(
    common_git: &Path,
    record: &LifecycleRecord,
) -> Result<(), RecordError> {
    if record.version != RECORD_SCHEMA_VERSION {
        return Err(RecordError::UnsupportedVersion {
            found: record.version,
            supported: RECORD_SCHEMA_VERSION,
        });
    }
    let root = validate_root_for_record(&record.root_event_id)?;
    let path = lifecycle_record_path(common_git, &root).map_err(RecordError::InvalidIdentity)?;
    with_record_lock(common_git, &root, |_guard| atomic_write_json(&path, record))
}

/// Create or adopt a record from a trusted ACP turn.
///
/// Rejects conflicts on root, branch, worktree path, channel, or community scope.
pub fn adopt_or_create_record(
    common_git: &Path,
    root_event_id: &str,
    routing_channel_id: &str,
    community_scope: Option<&str>,
    branch: &str,
    worktree_path: &str,
    base: Option<&str>,
) -> Result<LifecycleRecord, RecordError> {
    let root = validate_root_for_record(root_event_id)?;
    let now = unix_now();
    with_record_lock(common_git, &root, |_guard| {
        let path =
            lifecycle_record_path(common_git, &root).map_err(RecordError::InvalidIdentity)?;
        if let Some(existing) = read_existing(&path)? {
            ensure_compatible(
                &existing,
                &root,
                routing_channel_id,
                community_scope,
                branch,
                worktree_path,
            )?;
            return Ok(existing);
        }
        let record = LifecycleRecord::new_with_base(
            root.clone(),
            routing_channel_id.to_string(),
            community_scope.map(str::to_string),
            branch.to_string(),
            worktree_path.to_string(),
            base.map(str::to_string).filter(|value| !value.is_empty()),
            now,
        );
        atomic_write_json(&path, &record)?;
        Ok(record)
    })
}

/// Monotonically update `last_used_at` (max of current and `now`).
pub fn touch_last_used_at(
    common_git: &Path,
    root_event_id: &str,
    now: i64,
) -> Result<LifecycleRecord, RecordError> {
    let root = validate_root_for_record(root_event_id)?;
    with_record_lock(common_git, &root, |_guard| {
        let path =
            lifecycle_record_path(common_git, &root).map_err(RecordError::InvalidIdentity)?;
        let mut record = read_existing(&path)?.ok_or_else(|| {
            RecordError::Conflict("lifecycle record missing for last-used update".to_string())
        })?;
        if now > record.last_used_at {
            record.last_used_at = now;
            atomic_write_json(&path, &record)?;
        }
        Ok(record)
    })
}

/// Advance eviction generation after a successful remove/prune.
pub fn advance_eviction_generation(
    common_git: &Path,
    root_event_id: &str,
) -> Result<LifecycleRecord, RecordError> {
    let root = validate_root_for_record(root_event_id)?;
    with_record_lock(common_git, &root, |_guard| {
        let path =
            lifecycle_record_path(common_git, &root).map_err(RecordError::InvalidIdentity)?;
        let mut record = read_existing(&path)?.ok_or_else(|| {
            RecordError::Conflict("lifecycle record missing for generation advance".to_string())
        })?;
        record.eviction_generation = record.eviction_generation.saturating_add(1);
        atomic_write_json(&path, &record)?;
        Ok(record)
    })
}

fn ensure_compatible(
    existing: &LifecycleRecord,
    root: &str,
    routing_channel_id: &str,
    community_scope: Option<&str>,
    branch: &str,
    worktree_path: &str,
) -> Result<(), RecordError> {
    if existing.version != RECORD_SCHEMA_VERSION {
        return Err(RecordError::UnsupportedVersion {
            found: existing.version,
            supported: RECORD_SCHEMA_VERSION,
        });
    }
    if !existing.root_event_id.eq_ignore_ascii_case(root) {
        return Err(RecordError::Conflict("root event id mismatch".into()));
    }
    if existing.routing_channel_id != routing_channel_id {
        return Err(RecordError::Conflict("routing channel mismatch".into()));
    }
    if existing.branch != branch {
        return Err(RecordError::Conflict("branch mismatch".into()));
    }
    if Path::new(&existing.worktree_path) != Path::new(worktree_path) {
        return Err(RecordError::Conflict("worktree path mismatch".into()));
    }
    match (&existing.community_scope, community_scope) {
        (None, None) => {}
        (Some(existing_scope), Some(requested)) if existing_scope == requested => {}
        (None, Some(_)) | (Some(_), None) | (Some(_), Some(_)) => {
            return Err(RecordError::Conflict("community scope mismatch".into()));
        }
    }
    Ok(())
}

fn read_existing(path: &Path) -> Result<Option<LifecycleRecord>, RecordError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|source| RecordError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    parse_record_bytes(path, &bytes).map(Some)
}

fn parse_record_bytes(path: &Path, bytes: &[u8]) -> Result<LifecycleRecord, RecordError> {
    let record: LifecycleRecord =
        serde_json::from_slice(bytes).map_err(|source| RecordError::Malformed {
            path: path.to_path_buf(),
            source,
        })?;
    if record.version != RECORD_SCHEMA_VERSION {
        return Err(RecordError::UnsupportedVersion {
            found: record.version,
            supported: RECORD_SCHEMA_VERSION,
        });
    }
    let normalized_root = validate_root_for_record(&record.root_event_id)?;
    // Fail closed when the on-disk filename/key does not match the record body.
    let path_root = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_ascii_lowercase);
    if path_root.as_deref() != Some(normalized_root.as_str()) {
        return Err(RecordError::Conflict(
            "lifecycle record root does not match record path".into(),
        ));
    }
    if !is_plausible_routing_channel(record.routing_channel_id.trim()) {
        return Err(RecordError::Conflict(
            "lifecycle record routing channel is invalid".into(),
        ));
    }
    if record.branch.trim().is_empty() || !is_acceptable_lifecycle_branch(&record.branch) {
        return Err(RecordError::Conflict(
            "lifecycle record branch is not a valid git branch".into(),
        ));
    }
    if record.worktree_path.trim().is_empty() {
        return Err(RecordError::Conflict(
            "lifecycle record missing worktree path".into(),
        ));
    }
    Ok(LifecycleRecord {
        root_event_id: normalized_root,
        ..record
    })
}

/// List every readable lifecycle record under `common_git`.
pub fn list_lifecycle_records(common_git: &Path) -> Result<Vec<LifecycleRecord>, RecordError> {
    let dir = lifecycle_records_dir(common_git);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&dir).map_err(|source| RecordError::Io {
        path: dir.clone(),
        source,
    })?;
    let mut records = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| RecordError::Io {
            path: dir.clone(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        match fs::read(&path) {
            Ok(bytes) => match parse_record_bytes(&path, &bytes) {
                Ok(record) => records.push(record),
                Err(_) => continue,
            },
            Err(_) => continue,
        }
    }
    Ok(records)
}

/// Shared-worktree idle input: max `last_used_at` across records on `worktree_path`.
pub fn max_last_used_at_for_path(common_git: &Path, worktree_path: &Path) -> Option<i64> {
    let records = list_lifecycle_records(common_git).ok()?;
    records
        .into_iter()
        .filter(|record| Path::new(&record.worktree_path) == worktree_path)
        .map(|record| record.last_used_at)
        .max()
}

fn is_acceptable_lifecycle_branch(branch: &str) -> bool {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') || branch.contains("..") {
        return false;
    }
    !branch.contains('\0') && !branch.contains(' ')
}

fn is_plausible_routing_channel(value: &str) -> bool {
    // NIP-29 routing channels are UUIDs. Reject empty/whitespace/garbage without
    // pulling uuid into this crate — hyphenated 36-char hex is enough to fail closed.
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    let positions = [8usize, 13, 18, 23];
    for (index, byte) in bytes.iter().enumerate() {
        if positions.contains(&index) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn atomic_write_json(path: &Path, record: &LifecycleRecord) -> Result<(), RecordError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| RecordError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = File::create(&tmp).map_err(|source| RecordError::Io {
            path: tmp.clone(),
            source,
        })?;
        let payload =
            serde_json::to_vec_pretty(record).map_err(|source| RecordError::Malformed {
                path: tmp.clone(),
                source,
            })?;
        file.write_all(&payload).map_err(|source| RecordError::Io {
            path: tmp.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| RecordError::Io {
            path: tmp.clone(),
            source,
        })?;
    }
    fs::rename(&tmp, path).map_err(|source| RecordError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

struct RecordLockGuard {
    _file: File,
}

fn with_record_lock<T>(
    common_git: &Path,
    root: &str,
    body: impl FnOnce(&RecordLockGuard) -> Result<T, RecordError>,
) -> Result<T, RecordError> {
    use fs4::fs_std::FileExt;

    let dir = lifecycle_records_dir(common_git);
    fs::create_dir_all(&dir).map_err(|source| RecordError::Io {
        path: dir.clone(),
        source,
    })?;
    let lock_path = dir.join(format!("{root}.lock"));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| RecordError::Io {
            path: lock_path.clone(),
            source,
        })?;
    FileExt::lock_exclusive(&file).map_err(|source| RecordError::Io {
        path: lock_path,
        source,
    })?;
    let guard = RecordLockGuard { _file: file };
    body(&guard)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use tempfile::TempDir;

    fn root_a() -> String {
        format!("{}{}", "a".repeat(12), "1".repeat(52))
    }

    fn root_b() -> String {
        format!("{}{}", "a".repeat(12), "2".repeat(52))
    }

    #[test]
    fn atomic_round_trip() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let record = LifecycleRecord::new(
            root_a(),
            "11111111-1111-1111-1111-111111111111".into(),
            Some("relay.example".into()),
            "buzz/aaaaaaaaaaaa".into(),
            "/tmp/.buzz-worktrees/crew-aaaaaaaaaaaa".into(),
            100,
        );
        write_lifecycle_record(common, &record).unwrap();
        let loaded = read_lifecycle_record(common, &root_a()).unwrap().unwrap();
        assert_eq!(loaded, record);
    }

    #[test]
    fn version_rejection() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let path = lifecycle_record_path(common, &root_a()).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"version":99,"rootEventId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","routingChannelId":"x","branch":"b","worktreePath":"/p","createdAt":1,"lastUsedAt":1,"evictionGeneration":0}"#).unwrap();
        let err = read_lifecycle_record(common, &root_a()).unwrap_err();
        assert!(matches!(
            err,
            RecordError::UnsupportedVersion { found: 99, .. }
        ));
    }

    #[test]
    fn full_root_prefix_collision_does_not_alias() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let a = LifecycleRecord::new(
            root_a(),
            "11111111-1111-1111-1111-111111111111".into(),
            None,
            "buzz/aaaaaaaaaaaa".into(),
            "/a".into(),
            1,
        );
        let b = LifecycleRecord::new(
            root_b(),
            "22222222-2222-2222-2222-222222222222".into(),
            None,
            "buzz/aaaaaaaaaaaa".into(),
            "/b".into(),
            1,
        );
        write_lifecycle_record(common, &a).unwrap();
        write_lifecycle_record(common, &b).unwrap();
        assert_eq!(
            read_lifecycle_record(common, &root_a())
                .unwrap()
                .unwrap()
                .routing_channel_id,
            "11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(
            read_lifecycle_record(common, &root_b())
                .unwrap()
                .unwrap()
                .routing_channel_id,
            "22222222-2222-2222-2222-222222222222"
        );
    }

    #[test]
    fn conflicting_channel_adoption_fails_closed() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        adopt_or_create_record(
            common,
            &root_a(),
            "11111111-1111-1111-1111-111111111111",
            None,
            "buzz/aaaaaaaaaaaa",
            "/wt",
            None,
        )
        .unwrap();
        let err = adopt_or_create_record(
            common,
            &root_a(),
            "22222222-2222-2222-2222-222222222222",
            None,
            "buzz/aaaaaaaaaaaa",
            "/wt",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, RecordError::Conflict(_)));
    }

    #[test]
    fn misnamed_record_body_root_fails_closed() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let path = lifecycle_record_path(common, &root_a()).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // File named for root_a but body claims root_b.
        let forged = format!(
            r#"{{"version":1,"rootEventId":"{}","routingChannelId":"11111111-1111-1111-1111-111111111111","branch":"buzz/aaaaaaaaaaaa","worktreePath":"/p","createdAt":1,"lastUsedAt":1,"evictionGeneration":0}}"#,
            root_b()
        );
        fs::write(&path, forged).unwrap();
        let err = read_lifecycle_record(common, &root_a()).unwrap_err();
        assert!(matches!(err, RecordError::Conflict(_)));
    }

    #[test]
    fn invalid_routing_channel_fails_closed() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let path = lifecycle_record_path(common, &root_a()).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            format!(
                r#"{{"version":1,"rootEventId":"{}","routingChannelId":"not-a-uuid","branch":"buzz/aaaaaaaaaaaa","worktreePath":"/p","createdAt":1,"lastUsedAt":1,"evictionGeneration":0}}"#,
                root_a()
            ),
        )
        .unwrap();
        let err = read_lifecycle_record(common, &root_a()).unwrap_err();
        assert!(matches!(err, RecordError::Conflict(_)));
    }

    #[test]
    fn concurrent_monotonic_last_use_updates() {
        let temp = TempDir::new().unwrap();
        let common = temp.path().to_path_buf();
        let mut seed = adopt_or_create_record(
            &common,
            &root_a(),
            "11111111-1111-1111-1111-111111111111",
            None,
            "buzz/aaaaaaaaaaaa",
            "/wt",
            None,
        )
        .unwrap();
        // Force a low baseline so concurrent touches exercise monotonic max.
        seed.last_used_at = 10;
        write_lifecycle_record(&common, &seed).unwrap();
        let common_a = common.clone();
        let common_b = common.clone();
        let root = root_a();
        let t1 = thread::spawn(move || touch_last_used_at(&common_a, &root, 50).unwrap());
        let root = root_a();
        let t2 = thread::spawn(move || touch_last_used_at(&common_b, &root, 80).unwrap());
        t1.join().unwrap();
        t2.join().unwrap();
        // A stale write must never move last_used_at backwards.
        touch_last_used_at(&common, &root_a(), 40).unwrap();
        let record = read_lifecycle_record(&common, &root_a()).unwrap().unwrap();
        assert_eq!(record.last_used_at, 80);
    }

    #[test]
    fn records_optional_base_and_shared_path_idle_is_max() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let shared = "/tmp/.buzz-worktrees/crew-ws-feature";
        let first = adopt_or_create_record(
            common,
            &root_a(),
            "11111111-1111-1111-1111-111111111111",
            None,
            "feature/x",
            shared,
            Some("main"),
        )
        .unwrap();
        assert_eq!(first.base.as_deref(), Some("main"));
        let first_used = first.last_used_at + 10;
        touch_last_used_at(common, &root_a(), first_used).unwrap();
        let second = adopt_or_create_record(
            common,
            &root_b(),
            "22222222-2222-2222-2222-222222222222",
            None,
            "feature/x",
            shared,
            None,
        )
        .unwrap();
        let second_used = second.last_used_at.max(first_used) + 30;
        touch_last_used_at(common, &root_b(), second_used).unwrap();
        assert_eq!(
            max_last_used_at_for_path(common, Path::new(shared)),
            Some(second_used)
        );
    }

    #[test]
    fn malformed_record_never_authorizes() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let path = lifecycle_record_path(common, &root_a()).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not-json").unwrap();
        assert!(read_lifecycle_record(common, &root_a()).is_err());
    }
}
