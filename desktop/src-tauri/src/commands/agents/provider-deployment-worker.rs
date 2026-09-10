//! Keep the pending witness and provider result durable across caller cancellation.
use crate::managed_agents::ManagedAgentRecord;
use tokio::{sync::OwnedMutexGuard, task::JoinHandle};

/// Each update is one locked load/change/save transaction. A failed update
/// must leave its previously persisted snapshot available for reconciliation.
pub(super) trait RecordStore: Send + 'static {
    fn update(
        &mut self,
        change: impl FnOnce(&mut ManagedAgentRecord) -> Result<(), String>,
    ) -> Result<(), String>;
}

pub(super) fn spawn(
    mut store: impl RecordStore,
    captured: ManagedAgentRecord,
    guard: OwnedMutexGuard<()>,
    invoke: impl FnOnce() -> Result<String, String> + Send + 'static,
) -> JoinHandle<Result<(), String>> {
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        store.update(|record| {
            super::validate_attempt(record, &captured)?;
            if record.respond_to != captured.respond_to
                || record.respond_to_allowlist != captured.respond_to_allowlist
            {
                return Err("Provider policy changed before deployment; retry Start".into());
            }
            record.provider_policy_pending = true;
            Ok(())
        })?;
        let result = invoke();
        store.update(|record| {
            super::validate_attempt(record, &captured)?;
            match &result {
                Ok(handle) => {
                    record.backend_agent_id = Some(handle.clone());
                    if record.respond_to == captured.respond_to
                        && record.respond_to_allowlist == captured.respond_to_allowlist
                    {
                        record.provider_policy_pending = false;
                    }
                    record.last_started_at = Some(crate::util::now_iso());
                    record.last_error = None;
                }
                Err(error) => record.last_error = Some(error.clone()),
            }
            record.updated_at = crate::util::now_iso();
            Ok(())
        })?;
        result.map(|_| ())
    })
}

#[cfg(test)]
#[path = "provider-deployment-worker-tests.rs"]
mod tests;
