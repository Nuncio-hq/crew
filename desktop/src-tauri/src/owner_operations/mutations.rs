use sha2::{Digest, Sha256};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use super::storage::{kind_key, read, sql_error, validate_id, validate_scope};
use super::{
    CreateResult, Limits, NewOperation, Operation, OperationScope, OperationStatus, OperationStore,
    OperationUpdate, StoreError, WikiSuccessorResult,
};

const MAX_ACTIVE_WIKI_PINS: i64 = 16;

fn active_pin_count(conn: &Connection, owner: &str) -> Result<i64, StoreError> {
    conn.query_row(
        "SELECT count(*) FROM wiki_publication_successors link \
         JOIN operations successor ON successor.owner=link.owner \
           AND successor.community=link.community AND successor.id=link.successor_id \
         WHERE link.owner=?1 AND successor.reconciled=0",
        [owner],
        |row| row.get(0),
    )
    .map_err(sql_error)
}

pub(super) fn successor_link_bytes(conn: &Connection, owner: &str) -> Result<i64, StoreError> {
    // The fixed allowance covers JSON punctuation and the two signed integer
    // fields; text lengths account for the scoped IDs and resource key. Link
    // metadata is bounded by the same owner byte budget as operation records.
    conn.query_row(
        "SELECT coalesce(sum(length(CAST(owner AS BLOB)) \
                         + length(CAST(community AS BLOB)) \
                         + length(CAST(resource_key AS BLOB)) \
                         + length(CAST(predecessor_id AS BLOB)) \
                         + length(CAST(successor_id AS BLOB)) + 64), 0) \
         FROM wiki_publication_successors WHERE owner=?1",
        [owner],
        |row| row.get(0),
    )
    .map_err(sql_error)
}

fn cleanup_successor_links(conn: &Connection, owner: &str) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM wiki_publication_successors AS link \
         WHERE link.owner=?1 \
           AND (NOT EXISTS (SELECT 1 FROM operations successor \
                            WHERE successor.owner=link.owner \
                              AND successor.community=link.community \
                              AND successor.id=link.successor_id) \
                OR NOT EXISTS (SELECT 1 FROM operations predecessor \
                               WHERE predecessor.owner=link.owner \
                                 AND predecessor.community=link.community \
                                 AND predecessor.id=link.predecessor_id))",
        [owner],
    )
    .map(|_| ())
    .map_err(sql_error)
}

fn predecessor_is_active_pin(
    conn: &Connection,
    scope: &OperationScope,
    id: &str,
) -> Result<bool, StoreError> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM wiki_publication_successors link \
         JOIN operations successor ON successor.owner=link.owner \
           AND successor.community=link.community AND successor.id=link.successor_id \
         WHERE link.owner=?1 AND link.community=?2 AND link.predecessor_id=?3 \
           AND successor.reconciled=0)",
        params![scope.owner, scope.community, id],
        |row| row.get(0),
    )
    .map_err(sql_error)
}

fn not_active_pin_sql(alias: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM wiki_publication_successors link \
         JOIN operations successor ON successor.owner=link.owner \
           AND successor.community=link.community AND successor.id=link.successor_id \
         WHERE link.owner={alias}.owner AND link.community={alias}.community \
           AND link.predecessor_id={alias}.id AND successor.reconciled=0)"
    )
}

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
    cleanup_successor_links(conn, owner)?;
    let pinned = not_active_pin_sql("operations");
    conn.execute(
        &format!(
            "DELETE FROM operations WHERE owner=?1 AND reconciled=1 AND {pinned} AND updated_at < ?2"
        ),
        params![owner, now.saturating_sub(limits.terminal_age_secs)],
    )
    .map_err(sql_error)?;
    // Keep the terminal cap inclusive of active pins. Delete the oldest
    // eligible terminal one row at a time; selecting by an OFFSET from the
    // newest rows would stop early when a pinned row sits inside that window.
    loop {
        let terminal_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM operations WHERE owner=?1 AND reconciled=1",
                [owner],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if terminal_count <= limits.terminal_per_owner as i64 {
            break;
        }
        let removed: Option<i64> = conn
            .query_row(
                &format!(
                    "DELETE FROM operations AS victim WHERE victim.owner=?1 \
                     AND victim.reconciled=1 AND {} \
                     AND victim.rowid IN (SELECT candidate.rowid FROM operations candidate \
                         WHERE candidate.owner=?1 AND candidate.reconciled=1 \
                           AND {} ORDER BY candidate.updated_at, candidate.rowid LIMIT 1) \
                     RETURNING bytes",
                    not_active_pin_sql("victim"),
                    not_active_pin_sql("candidate"),
                ),
                [owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if removed.is_none() {
            break;
        }
    }
    let (record_bytes, corrupt): (i64, bool) = conn.query_row(
        "SELECT coalesce(sum(length(CAST(record_json AS BLOB))),0), coalesce(max(bytes != length(CAST(record_json AS BLOB))),0) FROM operations WHERE owner=?1",
        [owner], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(sql_error)?;
    if corrupt {
        return Err(StoreError::Corrupt);
    }
    // Already bounded by terminal_per_owner. Admission has inserted its incoming
    // snapshot in this same transaction, so byte pressure includes its headroom
    // and the scoped successor metadata.
    let mut bytes = record_bytes.saturating_add(successor_link_bytes(conn, owner)?);
    while bytes > limits.bytes_per_owner as i64 {
        let removed: Option<i64> = conn
            .query_row(
                &format!(
                    "DELETE FROM operations AS victim WHERE victim.rowid=(SELECT candidate.rowid \
                 FROM operations candidate WHERE candidate.owner=?1 AND candidate.reconciled=1 \
                   AND {} ORDER BY candidate.updated_at, candidate.rowid LIMIT 1) \
                 RETURNING bytes",
                    not_active_pin_sql("candidate")
                ),
                [owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        match removed {
            Some(size) => bytes -= size,
            None => break,
        }
    }
    // A terminal row removed above may have been the last endpoint of a
    // resolved historical relation. Drop that now so its metadata does not
    // consume quota until the next unrelated mutation.
    cleanup_successor_links(conn, owner)?;
    Ok(())
}

fn check_quota(conn: &Connection, owner: &str, limits: Limits) -> Result<(), StoreError> {
    let (pending, record_bytes, corrupt): (i64, i64, bool) = conn
        .query_row(
            "SELECT count(*) FILTER (WHERE reconciled=0 AND kind != 'channel-crew-config'), \
                    coalesce(sum(length(CAST(record_json AS BLOB))),0), \
                    coalesce(max(bytes != length(CAST(record_json AS BLOB))),0) \
             FROM operations WHERE owner=?1",
            [owner],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    if corrupt {
        return Err(StoreError::Corrupt);
    }
    let crew_over_limit: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM operations WHERE owner=?1 AND kind='channel-crew-config' AND reconciled=0 GROUP BY community HAVING count(*) > 100)",
        [owner], |row|row.get(0)
    ).map_err(sql_error)?;
    let terminal_over_limit: bool = conn
        .query_row(
            "SELECT count(*) > ?2 FROM operations WHERE owner=?1 AND reconciled=1",
            params![owner, limits.terminal_per_owner as i64],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let active_pins = active_pin_count(conn, owner)?;
    let bytes = record_bytes.saturating_add(successor_link_bytes(conn, owner)?);
    if pending > limits.pending_per_owner as i64
        || bytes > limits.bytes_per_owner as i64
        || crew_over_limit
        || terminal_over_limit
        || active_pins > MAX_ACTIVE_WIKI_PINS
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

fn validate_wiki_retirement_payload(
    old: &serde_json::Value,
    replacement: &serde_json::Value,
) -> Result<(), StoreError> {
    let (Some(old), Some(replacement)) = (old.as_object(), replacement.as_object()) else {
        return Err(StoreError::Invalid);
    };
    let proof = replacement
        .get("reconciliation")
        .and_then(serde_json::Value::as_object)
        .and_then(|value| value.get("proof"))
        .and_then(serde_json::Value::as_str);
    if proof != Some("superseded") {
        return Err(StoreError::Invalid);
    }
    const RECOVERY_FIELDS: [&str; 5] = [
        "reconciliation",
        "reconcile_only",
        "lease",
        "retry_at",
        "last_error",
    ];
    // The native domain validator owns the graph semantics. The store still
    // fences accidental generic updates by requiring every non-recovery field
    // to remain byte-for-byte identical, including progress and head_attempted.
    for (key, value) in old {
        if !RECOVERY_FIELDS.contains(&key.as_str()) && replacement.get(key) != Some(value) {
            return Err(StoreError::Invalid);
        }
    }
    for key in replacement.keys() {
        if !RECOVERY_FIELDS.contains(&key.as_str()) && !old.contains_key(key) {
            return Err(StoreError::Invalid);
        }
    }
    Ok(())
}

fn successor_intent_matches(
    conn: &Connection,
    scope: &OperationScope,
    successor_id: &str,
    new: &NewOperation,
    digest: &[u8],
    limits: Limits,
) -> Result<Operation, StoreError> {
    let successor =
        read(conn, scope, successor_id, limits.bytes_per_operation)?.ok_or(StoreError::Corrupt)?;
    let stored_digest: Option<Vec<u8>> = conn
        .query_row(
            "SELECT CASE WHEN length(initial_digest)=32 THEN initial_digest END \
             FROM operations WHERE owner=?1 AND community=?2 AND id=?3",
            params![scope.owner, scope.community, successor_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if successor.kind != new.kind
        || successor.resource_key != new.resource_key
        || stored_digest.ok_or(StoreError::Corrupt)?.as_slice() != digest
    {
        return Err(StoreError::Conflict);
    }
    Ok(successor)
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
        if new.kind == super::OperationKind::ManagedAgentDelete {
            super::managed_delete_claim::validate_pubkey(&new.resource_key)?;
        }
        let limits = self.limits;
        let creation_digest = initial_digest(&new, limits)?;
        let candidate = Operation {
            version: 1,
            scope: scope.clone(),
            id: new.id.clone(),
            kind: new.kind,
            resource_key: new.resource_key.clone(),
            revision: 0,
            created_at: now,
            updated_at: now,
            status: OperationStatus::Preparing,
            reconciled: false,
            payload: new.payload.clone(),
        };
        if candidate.kind == super::OperationKind::ManagedAgentDelete {
            super::validate_managed_agent_delete_record(&candidate)?;
        }
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
            if existing.kind == super::OperationKind::ManagedAgentDelete {
                super::validate_managed_agent_delete_record(&existing)
                    .map_err(|_| StoreError::Corrupt)?;
            }
            return Ok(CreateResult::Existing(existing));
        }
        if new.kind == super::OperationKind::ManagedAgentDelete {
            if let Some(existing) = super::managed_delete_claim::claim(
                &tx,
                &new.resource_key,
                limits.bytes_per_operation,
            )? {
                if existing.scope != *scope {
                    return Err(StoreError::Busy);
                }
                let stored_digest: Option<Vec<u8>> = tx
                    .query_row(
                        "SELECT CASE WHEN length(initial_digest)=32 THEN initial_digest END FROM operations WHERE owner=?1 AND community=?2 AND id=?3",
                        params![scope.owner, scope.community, existing.id],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if stored_digest.ok_or(StoreError::Corrupt)? != creation_digest {
                    return Err(StoreError::Conflict);
                }
                return Ok(CreateResult::Existing(existing));
            }
        }
        let claimed: Option<String> = tx.query_row("SELECT id FROM operations WHERE owner=?1 AND community=?2 AND kind=?3 AND resource_key=?4 AND reconciled=0", params![scope.owner,scope.community,kind_key(new.kind),new.resource_key], |row|row.get(0)).optional().map_err(sql_error)?;
        if let Some(id) = claimed {
            let stored_digest: Option<Vec<u8>> = tx.query_row(
                "SELECT CASE WHEN length(initial_digest)=32 THEN initial_digest END FROM operations WHERE owner=?1 AND community=?2 AND id=?3",
                params![scope.owner, scope.community, id],
                |row| row.get(0),
            ).map_err(sql_error)?;
            if stored_digest.ok_or(StoreError::Corrupt)? != creation_digest {
                // A resource claim serializes unresolved work, but it does not
                // transfer ownership of a different draft to this request.
                return Err(StoreError::Conflict);
            }
            return Ok(CreateResult::Existing(
                read(&tx, scope, &id, limits.bytes_per_operation)?.ok_or(StoreError::Corrupt)?,
            ));
        }
        let op = candidate;
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
        if op.kind == super::OperationKind::ManagedAgentDelete {
            super::validate_managed_agent_delete_record(&op)?;
        }
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

    /// Atomically retire an unresolved Wiki operation and reserve its direct
    /// successor. No network or process work may occur inside this transaction.
    /// The link is deliberately one hop: an unresolved successor pins only its
    /// immediate predecessor, while older ancestors are retained only when a
    /// still-unresolved direct child independently pins them.
    pub fn replace_wiki_with_successor(
        &mut self,
        scope: &OperationScope,
        id: &str,
        revision: u64,
        retirement_update: OperationUpdate,
        new_operation: NewOperation,
        now: i64,
    ) -> Result<WikiSuccessorResult, StoreError> {
        validate_scope(scope)?;
        validate_id(id)?;
        validate_id(&new_operation.id)?;
        if id == new_operation.id
            || new_operation.kind != super::OperationKind::WikiPublication
            || new_operation.resource_key.is_empty()
            || new_operation.resource_key.len() > 512
            || now < 0
            || retirement_update.status != OperationStatus::Superseded
            || !retirement_update.reconciled
        {
            return Err(StoreError::Invalid);
        }
        let limits = self.limits;
        let successor_digest = initial_digest(&new_operation, limits)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;

        // A retry after a committed transaction may have lost its IPC result.
        // Resolve the exact direct relation before checking the old revision,
        // which has intentionally advanced as part of the prior retirement.
        let relation: Option<(String, String, i64)> = tx
            .query_row(
                "SELECT resource_key, successor_id, predecessor_revision \
                 FROM wiki_publication_successors \
                 WHERE owner=?1 AND community=?2 AND predecessor_id=?3",
                params![scope.owner, scope.community, id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some((relation_resource, successor_id, predecessor_revision)) = relation {
            let predecessor =
                read(&tx, scope, id, limits.bytes_per_operation)?.ok_or(StoreError::Corrupt)?;
            let revision_matches = u64::try_from(predecessor_revision)
                .ok()
                .is_some_and(|value| value == predecessor.revision);
            if relation_resource != predecessor.resource_key
                || !revision_matches
                || !predecessor.reconciled
            {
                return Err(StoreError::Corrupt);
            }
            let successor = successor_intent_matches(
                &tx,
                scope,
                &successor_id,
                &new_operation,
                &successor_digest,
                limits,
            )?;
            return Ok(WikiSuccessorResult {
                predecessor,
                successor,
            });
        }

        let old = read(&tx, scope, id, limits.bytes_per_operation)?.ok_or(StoreError::Missing)?;
        if old.kind != super::OperationKind::WikiPublication
            || old.resource_key != new_operation.resource_key
            || old.revision != revision
            || old.reconciled
        {
            return Err(
                if old.kind != super::OperationKind::WikiPublication
                    || old.resource_key != new_operation.resource_key
                {
                    StoreError::Invalid
                } else {
                    StoreError::Conflict
                },
            );
        }
        validate_wiki_retirement_payload(&old.payload, &retirement_update.payload)?;

        let successor_exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM operations WHERE owner=?1 AND community=?2 AND id=?3)",
                params![scope.owner, scope.community, new_operation.id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if successor_exists {
            return Err(StoreError::Conflict);
        }
        let claimed: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM operations WHERE owner=?1 AND community=?2 \
                 AND kind='wiki-publication' AND resource_key=?3 AND reconciled=0 AND id<>?4)",
                params![scope.owner, scope.community, new_operation.resource_key, id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if claimed {
            return Err(StoreError::Conflict);
        }

        let retired_at = now.max(old.updated_at);
        let mut predecessor = old.clone();
        predecessor.revision = old
            .revision
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or(StoreError::Invalid)?;
        predecessor.updated_at = retired_at;
        predecessor.status = retirement_update.status;
        predecessor.reconciled = true;
        predecessor.payload = retirement_update.payload;
        let predecessor_json = encode(&predecessor, limits)?;
        let predecessor_bytes = predecessor_json.len() as i64;
        tx.execute(
            "UPDATE operations SET revision=?4,updated_at=?5,status=?6,reconciled=1,record_json=?7,bytes=?8 \
             WHERE owner=?1 AND community=?2 AND id=?3",
            params![
                scope.owner,
                scope.community,
                id,
                predecessor.revision,
                retired_at,
                serde_json::to_string(&predecessor.status).map_err(|_| StoreError::Invalid)?,
                &predecessor_json,
                predecessor_bytes
            ],
        )
        .map_err(sql_error)?;

        let successor = Operation {
            version: 1,
            scope: scope.clone(),
            id: new_operation.id.clone(),
            kind: new_operation.kind,
            resource_key: new_operation.resource_key.clone(),
            revision: 0,
            created_at: retired_at,
            updated_at: retired_at,
            status: OperationStatus::Preparing,
            reconciled: false,
            payload: new_operation.payload,
        };
        let successor_json = encode(&successor, limits)?;
        let successor_bytes = successor_json.len() as i64;
        tx.execute(
            "INSERT INTO operations(owner,community,id,kind,resource_key,revision,created_at,updated_at,status,reconciled,record_json,bytes,initial_digest) \
             VALUES(?1,?2,?3,?4,?5,0,?6,?6,?7,0,?8,?9,?10)",
            params![
                scope.owner,
                scope.community,
                successor.id,
                kind_key(successor.kind),
                successor.resource_key,
                retired_at,
                serde_json::to_string(&successor.status).map_err(|_| StoreError::Invalid)?,
                &successor_json,
                successor_bytes,
                successor_digest
            ],
        )
        .map_err(sql_error)?;
        tx.execute(
            "INSERT INTO wiki_publication_successors(owner,community,resource_key,predecessor_id,predecessor_revision,successor_id,created_at) \
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                scope.owner,
                scope.community,
                predecessor.resource_key,
                id,
                predecessor.revision,
                successor.id,
                retired_at,
            ],
        )
        .map_err(sql_error)?;

        trim(&tx, &scope.owner, retired_at, limits)?;
        check_quota(&tx, &scope.owner, limits)?;
        if active_pin_count(&tx, &scope.owner)? > MAX_ACTIVE_WIKI_PINS {
            return Err(StoreError::Quota);
        }
        #[cfg(all(test, unix))]
        super::tests::crash_checkpoint("before-commit");
        tx.commit().map_err(sql_error)?;
        #[cfg(all(test, unix))]
        super::tests::crash_checkpoint("after-commit");
        Ok(WikiSuccessorResult {
            predecessor,
            successor,
        })
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
        if predecessor_is_active_pin(&tx, scope, id)? {
            return Err(StoreError::Pinned);
        }
        tx.execute(
            "DELETE FROM operations WHERE owner=?1 AND community=?2 AND id=?3",
            params![scope.owner, scope.community, id],
        )
        .map_err(sql_error)?;
        tx.execute(
            "DELETE FROM wiki_publication_successors WHERE owner=?1 AND community=?2 \
             AND (predecessor_id=?3 OR successor_id=?3)",
            params![scope.owner, scope.community, id],
        )
        .map_err(sql_error)?;
        tx.commit().map_err(sql_error)
    }
}
