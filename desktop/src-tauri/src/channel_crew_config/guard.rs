use super::record::{self, Payload};
use crate::owner_operations::{Operation, OperationStatus};

#[derive(Clone)]
pub(super) struct DispatchStamp {
    pub id: String,
    pub revision: u64,
    pub worker: String,
}

pub(super) async fn reload_and_verify(
    load: impl std::future::Future<Output = Result<Operation, String>>,
    stamp: &DispatchStamp,
    now: impl FnOnce() -> Result<i64, String>,
) -> Result<(), String> {
    let operation = load.await?;
    verify_dispatch_record(&operation, &stamp.id, stamp.revision, &stamp.worker, now()?)
}

#[cfg(all(test, unix))]
#[path = "guard_tests.rs"]
mod tests;

/// Domain guard for the freshly reloaded record after HTTP admission.
pub(crate) fn verify_dispatch_record(
    operation: &Operation,
    id: &str,
    revision: u64,
    worker: &str,
    now: i64,
) -> Result<(), String> {
    let payload: Payload = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "invalid canvas recovery record")?;
    record::validate(operation, &payload)?;
    if operation.id != id
        || operation.revision != revision
        || operation.reconciled
        || operation.status != OperationStatus::Reconciling
        || payload
            .lease
            .as_ref()
            .is_none_or(|lease| lease.worker != worker || lease.expires_at <= now)
    {
        return Err("canvas recovery changed before dispatch; reload".into());
    }
    Ok(())
}
