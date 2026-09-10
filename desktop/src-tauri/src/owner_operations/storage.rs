#[cfg(unix)]
use std::fs::{self, OpenOptions};
use std::path::Path;
#[cfg(unix)]
use std::path::{Component, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior};

use super::{
    Limits, Operation, OperationKind, OperationScope, OperationStore, StoreError,
    WikiSuccessorResult,
};

/// Schema versions this binary can open. `0` is an empty file this opener
/// initializes. A build without the Wiki successor relation ends its set at
/// `1` and therefore refuses a migrated journal through the same branch,
/// rather than ignoring retention pins it cannot honor (D-079).
pub(super) const SUPPORTED_SCHEMA_VERSIONS: &[i64] = &[0, 1, 2];

/// Version written by the newest migration in this binary.
pub(super) const CURRENT_SCHEMA_VERSION: i64 = 2;

pub(super) fn sql_error(error: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(code, _) = &error {
        if matches!(
            code.extended_code,
            rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
        ) {
            return StoreError::Conflict;
        }
    }
    match error.sqlite_error_code() {
        Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => StoreError::Busy,
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => StoreError::Corrupt,
        _ => StoreError::Unavailable,
    }
}

pub(super) fn kind_key(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::WikiPublication => "wiki-publication",
        OperationKind::ProjectChange => "project-change",
        OperationKind::ThreadHandoff => "thread-handoff",
        OperationKind::ChannelCrewConfig => "channel-crew-config",
    }
}

pub(super) fn validate_scope(scope: &OperationScope) -> Result<(), StoreError> {
    if scope.owner.len() != 64
        || !scope
            .owner
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StoreError::Invalid);
    }
    let url = url::Url::parse(&scope.community).map_err(|_| StoreError::Invalid)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.origin().ascii_serialization() != scope.community
    {
        return Err(StoreError::Invalid);
    }
    Ok(())
}

pub(super) fn validate_id(id: &str) -> Result<(), StoreError> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| StoreError::Invalid)?;
    if parsed.hyphenated().to_string() != id {
        return Err(StoreError::Invalid);
    }
    Ok(())
}

#[cfg(not(unix))]
fn protect_path(_path: &Path) -> Result<(), StoreError> {
    // No equivalent private ACL/component-safe open has been implemented.
    Err(StoreError::UnsafePath)
}

#[cfg(unix)]
fn protect_path(path: &Path) -> Result<(), StoreError> {
    if !path.is_absolute() {
        return Err(StoreError::UnsafePath);
    }
    let parent = path.parent().ok_or(StoreError::UnsafePath)?;
    let mut current = PathBuf::new();
    for part in parent.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err(StoreError::UnsafePath);
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.is_symlink() || !meta.is_dir() => return Err(StoreError::UnsafePath),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder
                    .create(&current)
                    .map_err(|_| StoreError::UnsafePath)?;
            }
            Err(_) => return Err(StoreError::UnsafePath),
        }
    }
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-journal", path.display())),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        match fs::symlink_metadata(candidate) {
            Ok(meta) if meta.is_symlink() || !meta.is_file() => return Err(StoreError::UnsafePath),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(StoreError::UnsafePath),
        }
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| StoreError::UnsafePath)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| StoreError::UnsafePath)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| StoreError::UnsafePath)?;
    }
    Ok(())
}

impl OperationStore {
    /// Open a versioned private SQLite journal; unknown schemas are never reset.
    pub fn open(path: &Path, limits: Limits) -> Result<Self, StoreError> {
        if limits.pending_per_owner == 0
            || limits.terminal_per_owner == 0
            || limits.bytes_per_operation == 0
            || limits.bytes_per_owner == 0
            || limits.terminal_age_secs <= 0
            || limits.busy_timeout_ms > 250
        {
            return Err(StoreError::Invalid);
        }
        protect_path(path)?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let mut connection = Connection::open_with_flags(path, flags).map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_millis(limits.busy_timeout_ms))
            .map_err(sql_error)?;
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(sql_error)?;
        if !SUPPORTED_SCHEMA_VERSIONS.contains(&version) {
            return Err(StoreError::Version);
        }
        connection
            .execute_batch(
                "PRAGMA temp_store=MEMORY; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;",
            )
            .map_err(sql_error)?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let version: i64 = tx
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(sql_error)?;
        if version == 0 {
            let tables: i64 = tx.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |row| row.get(0)).map_err(sql_error)?;
            if tables != 0 {
                return Err(StoreError::Version);
            }
            tx.execute_batch(include_str!("schema.sql"))
                .map_err(sql_error)?;
            tx.execute_batch(include_str!("migration_1_to_2.sql"))
                .map_err(sql_error)?;
        } else if version == 1 {
            tx.execute_batch(include_str!("migration_1_to_2.sql"))
                .map_err(sql_error)?;
        } else if version != CURRENT_SCHEMA_VERSION {
            return Err(StoreError::Version);
        }
        // The migration is one immediate transaction: an interrupted opener
        // leaves the previous version intact and the idempotent script runs
        // again on the next open.
        #[cfg(all(test, unix))]
        super::tests::crash_checkpoint("before-migration-commit");
        tx.commit().map_err(sql_error)?;
        #[cfg(all(test, unix))]
        super::tests::crash_checkpoint("after-migration-commit");
        Ok(Self { connection, limits })
    }

    /// List at most 100 metadata entries in ID order, without loading payloads.
    /// Pagination is a live view: callers restart it after recovery mutations.
    pub fn list(
        &self,
        scope: &OperationScope,
        after_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<super::OperationSummary>, StoreError> {
        validate_scope(scope)?;
        if let Some(id) = after_id {
            validate_id(id)?;
        }
        if limit == 0 || limit > 100 {
            return Err(StoreError::Invalid);
        }
        let mut query = self.connection.prepare(
            "SELECT rowid, CASE WHEN length(CAST(id AS BLOB))=36 THEN id ELSE '' END, \
             CASE WHEN length(CAST(kind AS BLOB)) BETWEEN 1 AND 32 THEN kind ELSE '' END, \
             CASE WHEN length(CAST(resource_key AS BLOB)) BETWEEN 1 AND 512 THEN resource_key ELSE '' END, revision, \
             CASE WHEN length(CAST(status AS BLOB)) BETWEEN 1 AND 32 THEN status ELSE '' END,reconciled,updated_at FROM operations \
             WHERE owner=?1 AND community=?2 AND id>?3 ORDER BY id LIMIT ?4"
        ).map_err(sql_error)?;
        let rows = query
            .query_map(
                params![
                    scope.owner,
                    scope.community,
                    after_id.unwrap_or(""),
                    limit as i64
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, u64>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, bool>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        rows.map(|row| {
            let (sequence, id, kind, resource_key, revision, status, reconciled, updated_at) =
                row.map_err(sql_error)?;
            validate_id(&id).map_err(|_| StoreError::Corrupt)?;
            if resource_key.is_empty() {
                return Err(StoreError::Corrupt);
            }
            let kind = serde_json::from_value(serde_json::Value::String(kind))
                .map_err(|_| StoreError::Corrupt)?;
            let status = serde_json::from_str(&status).map_err(|_| StoreError::Corrupt)?;
            Ok(super::OperationSummary {
                sequence,
                id,
                kind,
                resource_key,
                revision,
                status,
                reconciled,
                updated_at,
            })
        })
        .collect()
    }

    /// Load only the captured scope, rejecting malformed or oversized records.
    pub fn load(&self, scope: &OperationScope, id: &str) -> Result<Operation, StoreError> {
        validate_scope(scope)?;
        validate_id(id)?;
        read(&self.connection, scope, id, self.limits.bytes_per_operation)?
            .ok_or(StoreError::Missing)
    }

    /// Resolve the committed direct Wiki successor of one exact predecessor.
    ///
    /// `requested_revision` is the caller's **pre-retirement** revision. The
    /// link row records the predecessor revision the retirement CAS produced,
    /// so a request whose IPC response was lost proves it is asking about the
    /// same point in history by matching `requested_revision + 1`. Any other
    /// revision, owner or community resolves to nothing or to a conflict; this
    /// lookup never adopts an unrelated operation into a caller's request.
    ///
    /// The successor is returned even after it has reconciled, because a lost
    /// response must be recoverable after the worker finished the successor.
    pub fn wiki_successor(
        &self,
        scope: &OperationScope,
        predecessor_id: &str,
        requested_revision: u64,
    ) -> Result<Option<WikiSuccessorResult>, StoreError> {
        validate_scope(scope)?;
        validate_id(predecessor_id)?;
        let retired_revision = requested_revision
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or(StoreError::Invalid)?;
        let relation: Option<(String, String, i64)> = self
            .connection
            .query_row(
                "SELECT resource_key, successor_id, predecessor_revision \
                 FROM wiki_publication_successors \
                 WHERE owner=?1 AND community=?2 AND predecessor_id=?3",
                params![scope.owner, scope.community, predecessor_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((resource_key, successor_id, predecessor_revision)) = relation else {
            return Ok(None);
        };
        let max = self.limits.bytes_per_operation;
        let predecessor =
            read(&self.connection, scope, predecessor_id, max)?.ok_or(StoreError::Corrupt)?;
        if u64::try_from(predecessor_revision).ok() != Some(predecessor.revision)
            || resource_key != predecessor.resource_key
            || predecessor.kind != OperationKind::WikiPublication
            || !predecessor.reconciled
        {
            return Err(StoreError::Corrupt);
        }
        if predecessor.revision != retired_revision {
            return Err(StoreError::Conflict);
        }
        let successor =
            read(&self.connection, scope, &successor_id, max)?.ok_or(StoreError::Corrupt)?;
        if successor.kind != OperationKind::WikiPublication
            || successor.resource_key != predecessor.resource_key
            || successor.id == predecessor.id
        {
            return Err(StoreError::Corrupt);
        }
        Ok(Some(WikiSuccessorResult {
            predecessor,
            successor,
        }))
    }
}

pub(super) fn read(
    conn: &Connection,
    scope: &OperationScope,
    id: &str,
    max: usize,
) -> Result<Option<Operation>, StoreError> {
    let row = conn.query_row(
        "SELECT CASE WHEN bytes = length(CAST(record_json AS BLOB)) AND length(CAST(record_json AS BLOB)) <= CASE WHEN kind='channel-crew-config' THEN min(?4,1048576) ELSE ?4 END THEN record_json END, revision, \
         CASE WHEN length(CAST(kind AS BLOB)) BETWEEN 1 AND 32 THEN kind ELSE '' END, \
         CASE WHEN length(CAST(resource_key AS BLOB)) BETWEEN 1 AND 512 THEN resource_key ELSE '' END, \
         CASE WHEN length(CAST(status AS BLOB)) BETWEEN 1 AND 32 THEN status ELSE '' END, reconciled, created_at, updated_at \
         FROM operations WHERE owner=?1 AND community=?2 AND id=?3",
        params![scope.owner, scope.community, id, max as i64],
        |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, u64>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, bool>(5)?, row.get::<_, i64>(6)?, row.get::<_, i64>(7)?)),
    ).optional().map_err(sql_error)?;
    let Some((json, revision, kind, resource, status, reconciled, created_at, updated_at)) = row
    else {
        return Ok(None);
    };
    let json = json.ok_or(StoreError::Corrupt)?;
    let op: Operation = serde_json::from_str(&json).map_err(|_| StoreError::Corrupt)?;
    if op.version != 1 {
        return Err(StoreError::Version);
    }
    if &op.scope != scope
        || op.id != id
        || op.revision != revision
        || kind_key(op.kind) != kind
        || op.resource_key != resource
        || serde_json::to_string(&op.status).map_err(|_| StoreError::Corrupt)? != status
        || op.reconciled != reconciled
        || op.created_at != created_at
        || op.updated_at != updated_at
    {
        return Err(StoreError::Corrupt);
    }
    Ok(Some(op))
}
