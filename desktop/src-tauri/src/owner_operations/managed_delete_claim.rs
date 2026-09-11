//! Native-only global instance claims. No foreign scope data leaves this API.
use rusqlite::Connection;
use serde::Deserialize;

use super::storage::{read, sql_error, validate_id, validate_scope};
use super::{
    ManagedAgentDeletionSummary, Operation, OperationKind, OperationScope, OperationStatus,
    OperationStore, StoreError,
};

const MAX_DELETE_RECORDS: usize = 4096;
const MAX_DELETE_CHANNELS: usize = 64;
const MAX_ERROR_BYTES: usize = 512;

#[derive(Deserialize)]
struct ManagedDeleteFence {
    pubkey: String,
    name: String,
    created_at: String,
    relay_url: String,
    backend_agent_id: Option<String>,
}

#[derive(Deserialize)]
struct ManagedDeleteChannel {
    operation_id: String,
    channel_id: String,
    members: Vec<String>,
    settled: bool,
    review_required: bool,
}

#[derive(Deserialize)]
struct ManagedDeletePayload {
    version: u32,
    fence: ManagedDeleteFence,
    channels: Vec<ManagedDeleteChannel>,
    local_removed: bool,
    key_removed: bool,
    #[serde(default)]
    tombstone_enqueued: bool,
    failures: u8,
    last_error: Option<String>,
}

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

/// Validate the complete managed-agent deletion snapshot at the storage seam.
///
/// This is deliberately kept in `owner_operations`, rather than only in the
/// async coordinator, because `owner_operation_update` is renderer-callable.
/// Every create and CAS therefore enforces the same progression and terminal
/// invariants before the journal can release its global instance claim.
pub(crate) fn validate_record(operation: &Operation) -> Result<(), StoreError> {
    if operation.kind != OperationKind::ManagedAgentDelete {
        return Err(StoreError::Invalid);
    }
    let payload: ManagedDeletePayload =
        serde_json::from_value(operation.payload.clone()).map_err(|_| StoreError::Invalid)?;
    if payload.version != 1
        || payload.fence.pubkey != operation.resource_key
        || validate_pubkey(&payload.fence.pubkey).is_err()
        || payload.fence.name.len() > 256
        || payload.fence.created_at.is_empty()
        || payload.fence.created_at.len() > 256
        || payload.fence.relay_url.is_empty()
        || payload.fence.relay_url.len() > 2048
        || payload
            .fence
            .backend_agent_id
            .as_ref()
            .is_some_and(|id| id.len() > 512)
        || payload.channels.len() > MAX_DELETE_CHANNELS
        || payload.key_removed && !payload.local_removed
        || payload.tombstone_enqueued && !payload.key_removed
        || payload.failures > 5
        || payload
            .last_error
            .as_ref()
            .is_some_and(|error| error.len() > MAX_ERROR_BYTES)
    {
        return Err(StoreError::Invalid);
    }

    let mut channels = std::collections::BTreeSet::new();
    let mut cleanup_operations = std::collections::BTreeSet::new();
    for cleanup in &payload.channels {
        let channel_id =
            uuid::Uuid::parse_str(&cleanup.channel_id).map_err(|_| StoreError::Invalid)?;
        let operation_id =
            uuid::Uuid::parse_str(&cleanup.operation_id).map_err(|_| StoreError::Invalid)?;
        if channel_id.to_string() != cleanup.channel_id
            || operation_id.is_nil()
            || operation_id.to_string() != cleanup.operation_id
            || cleanup.operation_id == operation.id
            || !channels.insert(&cleanup.channel_id)
            || !cleanup_operations.insert(&cleanup.operation_id)
            || cleanup.members != vec![payload.fence.pubkey.clone()]
            || cleanup.settled && cleanup.review_required
        {
            return Err(StoreError::Invalid);
        }
    }

    let all_settled = payload
        .channels
        .iter()
        .all(|cleanup| cleanup.settled && !cleanup.review_required);
    if operation.status == OperationStatus::Complete && !operation.reconciled {
        return Err(StoreError::Invalid);
    }
    if operation.reconciled
        && (operation.status != OperationStatus::Complete
            || !payload.local_removed
            || !payload.key_removed
            || !payload.tombstone_enqueued
            || !all_settled)
    {
        return Err(StoreError::Invalid);
    }
    Ok(())
}

fn scan(connection: &Connection, max_bytes: usize) -> Result<Vec<Operation>, StoreError> {
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
    let mut operations = Vec::new();
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
        validate_record(&op).map_err(|_| StoreError::Corrupt)?;
        operations.push(op);
    }
    Ok(operations)
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
    let mut found = None;
    for op in scan(connection, max_bytes)? {
        if op.reconciled
            && !matches!(
                op.status,
                OperationStatus::Complete | OperationStatus::Canceled | OperationStatus::Superseded
            )
        {
            // A SQL-level reconciled bit must agree with the operation's
            // terminal phase. Otherwise a damaged row could silently release
            // the app-global delete claim while its JSON still describes
            // unresolved work.
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

    /// List unresolved managed-agent deletions across every local scope.
    /// Payloads are deliberately omitted from this global recovery surface.
    pub fn list_managed_agent_deletions(
        &self,
    ) -> Result<Vec<ManagedAgentDeletionSummary>, StoreError> {
        let mut summaries: Vec<_> = scan(&self.connection, self.limits.bytes_per_operation)?
            .into_iter()
            .filter(|operation| !operation.reconciled)
            .map(|operation| ManagedAgentDeletionSummary {
                id: operation.id,
                owner: operation.scope.owner,
                community: operation.scope.community,
                resource_key: operation.resource_key,
                revision: operation.revision,
                status: operation.status,
                reconciled: operation.reconciled,
                updated_at: operation.updated_at,
            })
            .collect();
        summaries.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(summaries)
    }

    /// Load a managed-agent deletion by ID without assuming the active scope.
    /// Duplicate IDs across scopes are rejected rather than guessed.
    pub fn load_managed_agent_delete_any_scope(
        &self,
        operation_id: &str,
    ) -> Result<Operation, StoreError> {
        validate_id(operation_id)?;
        let mut matches = scan(&self.connection, self.limits.bytes_per_operation)?
            .into_iter()
            .filter(|operation| operation.id == operation_id);
        let Some(operation) = matches.next() else {
            return Err(StoreError::Missing);
        };
        if matches.next().is_some() {
            return Err(StoreError::Conflict);
        }
        Ok(operation)
    }
}

#[cfg(all(test, unix))]
#[path = "managed_delete_claim_tests.rs"]
mod tests;
