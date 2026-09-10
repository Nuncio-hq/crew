//! Whole-snapshot CAS and bounded worker admission on the existing owner store.
use super::record::{self, Lease, Payload, Phase};
use crate::owner_operations::{Operation, OperationStatus, OperationStore, OperationUpdate};

pub(super) fn decode(operation: &Operation) -> Result<Payload, String> {
    let payload: Payload = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "Invalid removal recovery data")?;
    record::validate(operation, &payload)?;
    Ok(payload)
}

pub(super) fn save(
    store: &mut OperationStore,
    operation: &Operation,
    payload: Payload,
    status: OperationStatus,
    reconciled: bool,
    now: i64,
) -> Result<Operation, String> {
    let mut candidate = operation.clone();
    candidate.payload =
        serde_json::to_value(&payload).map_err(|_| "Could not encode removal progress")?;
    candidate.status = status;
    candidate.reconciled = reconciled;
    record::validate(&candidate, &payload)?;
    store
        .compare_and_swap(
            &operation.scope,
            &operation.id,
            operation.revision,
            OperationUpdate {
                payload: candidate.payload,
                status,
                reconciled,
            },
            now,
        )
        .map_err(|e| e.to_string())
}

pub(super) fn acquire(
    store: &mut OperationStore,
    operation: &Operation,
    worker: &str,
    manual: bool,
    now: i64,
) -> Result<Operation, String> {
    let mut payload = decode(operation)?;
    if operation.reconciled {
        return Ok(operation.clone());
    }
    if payload
        .lease
        .as_ref()
        .is_some_and(|lease| lease.expires_at > now)
    {
        return Err("Removal is already being processed; reload its progress".into());
    }
    if !manual
        && (payload.failures >= 5
            || payload.phase == Phase::ReviewRequired
            || payload.next_retry_at.is_some_and(|time| time > now))
    {
        return Err("Removal is waiting for review or its retry window".into());
    }
    if manual {
        payload.failures = 0;
    }
    payload.lease = Some(Lease {
        worker: worker.into(),
        expires_at: now.saturating_add(90),
    });
    payload.last_error = None;
    payload.next_retry_at = None;
    save(
        store,
        operation,
        payload,
        OperationStatus::Reconciling,
        false,
        now,
    )
}

pub(super) fn assert_worker(payload: &Payload, worker: &str, now: i64) -> Result<(), String> {
    if !payload
        .lease
        .as_ref()
        .is_some_and(|lease| lease.worker == worker && lease.expires_at > now)
    {
        return Err("Removal worker expired or was replaced; reload progress".into());
    }
    Ok(())
}

pub(super) fn release_failure(
    store: &mut OperationStore,
    operation: &Operation,
    worker: &str,
    reason: &str,
    review: bool,
    now: i64,
) -> Result<Operation, String> {
    let mut payload = decode(operation)?;
    // An expired worker may record its own error only if no successor won CAS.
    if payload.lease.as_ref().map(|lease| lease.worker.as_str()) != Some(worker) {
        return Err("Removal worker was replaced; reload progress".into());
    }
    payload.lease = None;
    payload.failures = payload.failures.saturating_add(1).min(5);
    payload.last_error = Some(reason.chars().take(400).collect());
    if review {
        // Keep the actual resume phase; a review flag never discards progress.
        payload.next_retry_at = None;
    } else {
        payload.next_retry_at = Some(now.saturating_add(5_i64 << payload.failures));
    }
    if review {
        payload.failures = 5;
    }
    save(
        store,
        operation,
        payload,
        OperationStatus::Failed,
        false,
        now,
    )
}
