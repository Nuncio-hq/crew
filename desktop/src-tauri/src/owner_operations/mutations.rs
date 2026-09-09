use sha2::{Digest, Sha256};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::storage::{kind_key, read, sql_error, validate_id, validate_scope};
use super::{
    CreateResult, Limits, NewOperation, Operation, OperationScope, OperationStatus, OperationStore,
    OperationUpdate, StoreError,
};

// Reject large IPC values with a counting writer before allocating a second
// serialized/canonical copy. The renderer cannot raise this native limit.
fn check_serialized_size(value: &impl serde::Serialize, limit: usize) -> Result<(), StoreError> {
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("record size limit"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(limit), value).map_err(|_| StoreError::Quota)
}

// Sort every object independently of serde_json's preserve_order feature.
fn canonical_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            serde_json::Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonical_value(value)))
                    .collect(),
            )
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonical_value).collect())
        }
        value => value.clone(),
    }
}

fn initial_digest(new: &NewOperation, limits: Limits) -> Result<Vec<u8>, StoreError> {
    check_serialized_size(&new.payload, super::record_byte_limit(limits, new.kind))?;
    let canonical = canonical_value(&serde_json::json!([
        1,
        kind_key(new.kind),
        new.resource_key,
        new.payload
    ]));
    let bytes = serde_json::to_vec(&canonical).map_err(|_| StoreError::Invalid)?;
    if bytes.len() > super::record_byte_limit(limits, new.kind) {
        return Err(StoreError::Quota);
    }
    Ok(Sha256::digest(bytes).to_vec())
}

fn trim(conn: &Connection, owner: &str, now: i64, limits: Limits) -> Result<(), StoreError> {
    conn.execute("DELETE FROM operations WHERE owner=?1 AND reconciled=1 AND (updated_at < ?2 OR rowid IN \
        (SELECT rowid FROM operations WHERE owner=?1 AND reconciled=1 ORDER BY updated_at DESC, rowid DESC LIMIT -1 OFFSET ?3))",
        params![owner, now.saturating_sub(limits.terminal_age_secs), limits.terminal_per_owner as i64]).map_err(sql_error)?;
    let (mut bytes, corrupt): (i64, bool) = conn.query_row(
        "SELECT coalesce(sum(length(CAST(record_json AS BLOB))),0), coalesce(max(bytes != length(CAST(record_json AS BLOB))),0) FROM operations WHERE owner=?1",
        [owner], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(sql_error)?;
    if corrupt {
        return Err(StoreError::Corrupt);
    }
    // Already bounded by terminal_per_owner. Admission has inserted its incoming
    // snapshot in this same transaction, so byte pressure includes its headroom.
    while bytes > limits.bytes_per_owner as i64 {
        let removed: Option<i64> = conn.query_row(
            "DELETE FROM operations WHERE rowid=(SELECT rowid FROM operations WHERE owner=?1 AND reconciled=1 ORDER BY updated_at, rowid LIMIT 1) RETURNING bytes",
            [owner], |row| row.get(0),
        ).optional().map_err(sql_error)?;
        match removed {
            Some(size) => bytes -= size,
            None => break,
        }
    }
    Ok(())
}

fn check_quota(conn: &Connection, owner: &str, limits: Limits) -> Result<(), StoreError> {
    let (pending, bytes, corrupt): (i64,i64,bool) = conn.query_row("SELECT count(*) FILTER (WHERE reconciled=0 AND kind != 'channel-crew-config'), coalesce(sum(length(CAST(record_json AS BLOB))),0), coalesce(max(bytes != length(CAST(record_json AS BLOB))),0) FROM operations WHERE owner=?1", [owner], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).map_err(sql_error)?;
    if corrupt {
        return Err(StoreError::Corrupt);
    }
    let crew_over_limit: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM operations WHERE owner=?1 AND kind='channel-crew-config' AND reconciled=0 GROUP BY community HAVING count(*) > 100)",
        [owner], |row|row.get(0)
    ).map_err(sql_error)?;
    if pending > limits.pending_per_owner as i64
        || bytes > limits.bytes_per_owner as i64
        || crew_over_limit
    {
        return Err(StoreError::Quota);
    }
    Ok(())
}

fn encode(op: &Operation, limits: Limits) -> Result<String, StoreError> {
    check_serialized_size(op, super::record_byte_limit(limits, op.kind))?;
    let json = serde_json::to_string(op).map_err(|_| StoreError::Invalid)?;
    if json.len() > super::record_byte_limit(limits, op.kind) {
        return Err(StoreError::Quota);
    }
    Ok(json)
}

impl OperationStore {
    /// Reserve before external effects. Existing IDs cannot acquire new intent.
    pub fn create(
        &mut self,
        scope: &OperationScope,
        new: NewOperation,
        now: i64,
    ) -> Result<CreateResult, StoreError> {
        validate_scope(scope)?;
        validate_id(&new.id)?;
        if new.resource_key.is_empty() || new.resource_key.len() > 512 || now < 0 {
            return Err(StoreError::Invalid);
        }
        let limits = self.limits;
        let creation_digest = initial_digest(&new, limits)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        if let Some(existing) = read(&tx, scope, &new.id, limits.bytes_per_operation)? {
            let stored_digest: Option<Vec<u8>> = tx.query_row(
                "SELECT CASE WHEN length(initial_digest)=32 THEN initial_digest END FROM operations WHERE owner=?1 AND community=?2 AND id=?3",
                params![scope.owner, scope.community, new.id], |row| row.get(0)
            ).map_err(sql_error)?;
            let stored_digest = stored_digest.ok_or(StoreError::Corrupt)?;
            if existing.kind != new.kind
                || existing.resource_key != new.resource_key
                || stored_digest != creation_digest
            {
                return Err(StoreError::Conflict);
            }
            return Ok(CreateResult::Existing(existing));
        }
        let claimed: Option<String> = tx.query_row("SELECT id FROM operations WHERE owner=?1 AND community=?2 AND kind=?3 AND resource_key=?4 AND reconciled=0", params![scope.owner,scope.community,kind_key(new.kind),new.resource_key], |row|row.get(0)).optional().map_err(sql_error)?;
        if let Some(id) = claimed {
            return Ok(CreateResult::Existing(
                read(&tx, scope, &id, limits.bytes_per_operation)?.ok_or(StoreError::Corrupt)?,
            ));
        }
        let op = Operation {
            version: 1,
            scope: scope.clone(),
            id: new.id,
            kind: new.kind,
            resource_key: new.resource_key,
            revision: 0,
            created_at: now,
            updated_at: now,
            status: OperationStatus::Preparing,
            reconciled: false,
            payload: new.payload,
        };
        let json = encode(&op, limits)?;
        tx.execute("INSERT INTO operations(owner,community,id,kind,resource_key,revision,created_at,updated_at,status,reconciled,record_json,bytes,initial_digest) VALUES(?1,?2,?3,?4,?5,0,?6,?6,?7,0,?8,?9,?10)",
            params![scope.owner,scope.community,op.id,kind_key(op.kind),op.resource_key,now,serde_json::to_string(&op.status).map_err(|_|StoreError::Invalid)?,json,json.len() as i64,creation_digest]).map_err(sql_error)?;
        trim(&tx, &scope.owner, now, limits)?;
        check_quota(&tx, &scope.owner, limits)?;
        tx.commit().map_err(sql_error)?;
        Ok(CreateResult::Created(op))
    }

    /// CAS the complete snapshot inside a single SQLite immediate transaction.
    pub fn compare_and_swap(
        &mut self,
        scope: &OperationScope,
        id: &str,
        revision: u64,
        update: OperationUpdate,
        now: i64,
    ) -> Result<Operation, StoreError> {
        validate_scope(scope)?;
        validate_id(id)?;
        if update.reconciled
            && !matches!(
                update.status,
                OperationStatus::Complete | OperationStatus::Canceled | OperationStatus::Superseded
            )
        {
            return Err(StoreError::Invalid);
        }
        let limits = self.limits;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let mut op =
            read(&tx, scope, id, limits.bytes_per_operation)?.ok_or(StoreError::Missing)?;
        if op.revision != revision {
            return Err(StoreError::Conflict);
        }
        if op.reconciled {
            return Err(StoreError::Invalid);
        }
        let now = now.max(op.updated_at);
        op.revision = op
            .revision
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or(StoreError::Invalid)?;
        op.updated_at = now;
        op.status = update.status;
        op.reconciled = update.reconciled;
        op.payload = update.payload;
        let json = encode(&op, limits)?;
        tx.execute("UPDATE operations SET revision=?4,updated_at=?5,status=?6,reconciled=?7,record_json=?8,bytes=?9 WHERE owner=?1 AND community=?2 AND id=?3",
            params![scope.owner,scope.community,id,op.revision,now,serde_json::to_string(&op.status).map_err(|_|StoreError::Invalid)?,op.reconciled,json,json.len() as i64]).map_err(sql_error)?;
        trim(&tx, &scope.owner, now, limits)?;
        check_quota(&tx, &scope.owner, limits)?;
        #[cfg(all(test, unix))]
        super::tests::crash_checkpoint("before-commit");
        tx.commit().map_err(sql_error)?;
        #[cfg(all(test, unix))]
        super::tests::crash_checkpoint("after-commit");
        Ok(op)
    }

    /// Delete a reconciled snapshot; cancellation alone never authorizes removal.
    pub fn remove_reconciled(
        &mut self,
        scope: &OperationScope,
        id: &str,
        revision: u64,
    ) -> Result<(), StoreError> {
        validate_scope(scope)?;
        validate_id(id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let op =
            read(&tx, scope, id, self.limits.bytes_per_operation)?.ok_or(StoreError::Missing)?;
        if op.revision != revision {
            return Err(StoreError::Conflict);
        }
        if !op.reconciled {
            return Err(StoreError::Unreconciled);
        }
        tx.execute(
            "DELETE FROM operations WHERE owner=?1 AND community=?2 AND id=?3",
            params![scope.owner, scope.community, id],
        )
        .map_err(sql_error)?;
        tx.commit().map_err(sql_error)
    }
}
