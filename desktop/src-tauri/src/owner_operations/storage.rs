#[cfg(unix)]
use std::fs::{self, OpenOptions};
use std::path::Path;
#[cfg(unix)]
use std::path::{Component, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior};

use super::{Limits, Operation, OperationKind, OperationScope, OperationStore, StoreError};

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
        OperationKind::ManagedAgentDelete => "managed-agent-delete",
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
        if !matches!(version, 0..=2) {
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
        } else if version == 1 {
            tx.execute_batch(include_str!("managed_delete_migration.sql"))
                .map_err(sql_error)?;
        } else if version != 2 {
            return Err(StoreError::Version);
        }
        let index_sql: Option<String> = tx.query_row(
            "SELECT sql FROM sqlite_master WHERE type='index' AND name='unresolved_managed_agent_delete'",
            [], |row| row.get(0),
        ).optional().map_err(sql_error)?;
        let canonical = |sql: &str| {
            sql.split_whitespace()
                .collect::<String>()
                .to_ascii_lowercase()
        };
        let expected = "CREATE UNIQUE INDEX unresolved_managed_agent_delete ON operations(resource_key) WHERE kind = 'managed-agent-delete' AND reconciled = 0";
        if index_sql.as_deref().map(canonical) != Some(canonical(expected)) {
            return Err(StoreError::Corrupt);
        }
        if version == 1 {
            super::managed_delete_claim::claim(&tx, &"0".repeat(64), limits.bytes_per_operation)?;
        }
        tx.commit().map_err(sql_error)?;
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
            "SELECT CASE WHEN length(CAST(id AS BLOB))=36 THEN id ELSE '' END, \
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
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, bool>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        rows.map(|row| {
            let (id, kind, resource_key, revision, status, reconciled, updated_at) =
                row.map_err(sql_error)?;
            validate_id(&id).map_err(|_| StoreError::Corrupt)?;
            if resource_key.is_empty() {
                return Err(StoreError::Corrupt);
            }
            let kind = serde_json::from_value(serde_json::Value::String(kind))
                .map_err(|_| StoreError::Corrupt)?;
            let status = serde_json::from_str(&status).map_err(|_| StoreError::Corrupt)?;
            Ok(super::OperationSummary {
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
}

pub(super) fn read(
    conn: &Connection,
    scope: &OperationScope,
    id: &str,
    max: usize,
) -> Result<Option<Operation>, StoreError> {
    let row = conn.query_row(
        "SELECT CASE WHEN bytes = length(CAST(record_json AS BLOB)) AND length(CAST(record_json AS BLOB)) <= CASE WHEN kind='channel-crew-config' THEN min(?4,1048576) WHEN kind='managed-agent-delete' THEN min(?4,65536) ELSE ?4 END THEN record_json END, revision, \
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
