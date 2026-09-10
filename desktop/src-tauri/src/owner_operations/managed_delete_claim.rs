//! Native-only global instance claims. No foreign scope data leaves this API.
use rusqlite::Connection;

use super::storage::{read, sql_error, validate_id, validate_scope};
use super::{Operation, OperationKind, OperationScope, OperationStore, StoreError};

const MAX_DELETE_RECORDS: usize = 4096;

pub(super) fn validate_pubkey(pubkey: &str) -> Result<(), StoreError> {
    if pubkey.len() != 64
        || !pubkey
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StoreError::Invalid);
    }
    Ok(())
}

pub(super) fn claim(
    connection: &Connection,
    pubkey: &str,
    max_bytes: usize,
) -> Result<Option<Operation>, StoreError> {
    validate_pubkey(pubkey)?;
    // Include terminal rows: a corrupt SQL reconciled bit cannot silently
    // release a JSON-unresolved intent. Read each bounded record through the
    // shared coherence validator before inspecting its authoritative fields.
    let mut query = connection
        .prepare(
            "SELECT CASE WHEN length(CAST(owner AS BLOB))=64 THEN owner ELSE '' END, \
         CASE WHEN length(CAST(community AS BLOB)) BETWEEN 1 AND 2048 THEN community ELSE '' END, \
         CASE WHEN length(CAST(id AS BLOB))=36 THEN id ELSE '' END \
         FROM operations WHERE kind='managed-agent-delete' LIMIT ?1",
        )
        .map_err(sql_error)?;
    let mut rows = query
        .query([MAX_DELETE_RECORDS as i64 + 1])
        .map_err(sql_error)?;
    let mut count = 0;
    let mut found = None;
    while let Some(row) = rows.next().map_err(sql_error)? {
        count += 1;
        if count > MAX_DELETE_RECORDS {
            return Err(StoreError::Quota);
        }
        let scope = OperationScope {
            owner: row.get(0).map_err(sql_error)?,
            community: row.get(1).map_err(sql_error)?,
        };
        let id: String = row.get(2).map_err(sql_error)?;
        validate_scope(&scope).map_err(|_| StoreError::Corrupt)?;
        validate_id(&id).map_err(|_| StoreError::Corrupt)?;
        let op = read(connection, &scope, &id, max_bytes)?.ok_or(StoreError::Corrupt)?;
        validate_pubkey(&op.resource_key).map_err(|_| StoreError::Corrupt)?;
        if op.kind != OperationKind::ManagedAgentDelete {
            return Err(StoreError::Corrupt);
        }
        if op.resource_key == pubkey && !op.reconciled {
            if found.is_some() {
                return Err(StoreError::Corrupt);
            }
            found = Some(op);
        }
    }
    Ok(found)
}

impl OperationStore {
    /// Check whether an app-global keyed instance has unresolved offboarding.
    /// Call under the managed-store lock before local mutations or spawning.
    /// A failure is not availability; no foreign owner data is returned.
    pub fn managed_agent_delete_is_pending(&self, pubkey: &str) -> Result<bool, StoreError> {
        Ok(claim(&self.connection, pubkey, self.limits.bytes_per_operation)?.is_some())
    }
}
