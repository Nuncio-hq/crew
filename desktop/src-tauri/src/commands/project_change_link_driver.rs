//! One bounded attempt over the production native journal/transport seams.
use super::project_change_link_record::{
    ProjectLinkLease, ProjectLinkReconciliation, ProjectLinkRecord,
};
use crate::owner_operations::{Operation, OperationStatus};
use nostr::PublicKey;

pub(super) enum LinkHead {
    Original,
    Applied,
    Conflict(String),
}

/// Implemented by the native captured context; tests bind this same driver.
pub(super) trait ProjectLinkRuntime {
    fn now(&self) -> Result<i64, String>;
    async fn checkpoint(&self, operation: &Operation) -> Result<(), String>;
    async fn save(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String>;
    async fn capability(&self, operation: &Operation) -> Result<bool, String>;
    async fn eligible(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<(), String>;
    async fn inspect(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<LinkHead, String>;
    /// Confirm the original CAS precondition can no longer succeed on this relay.
    /// Only for this metadata-only Project patch, with no other planned effects.
    async fn prove_superseded(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<Option<LinkHead>, String>;
    async fn publish(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<(), String>;
}

fn renew(record: &mut ProjectLinkRecord, now: i64, worker: &str) -> Result<(), String> {
    record.lease = Some(ProjectLinkLease {
        worker_id: worker.into(),
        expires_at: now.checked_add(60).ok_or("Project lease clock overflow")?,
    });
    Ok(())
}

async fn fail<R: ProjectLinkRuntime>(
    runtime: &R,
    operation: &Operation,
    record: &mut ProjectLinkRecord,
    reason: String,
    conflict: bool,
) -> Result<Operation, String> {
    runtime.checkpoint(operation).await?;
    record.lease = None;
    record.reconcile_only |= conflict;
    record.last_error = Some(reason.chars().take(512).collect());
    let delay = 5_i64 * (1_i64 << record.attempts.saturating_sub(1).min(5));
    record.retry_at = runtime
        .now()?
        .checked_add(delay.min(300))
        .ok_or("Project retry clock overflow")?;
    runtime
        .save(
            operation,
            record,
            if conflict {
                OperationStatus::Superseded
            } else {
                OperationStatus::Failed
            },
            false,
        )
        .await
}

async fn resolve<R: ProjectLinkRuntime>(
    runtime: &R,
    operation: &Operation,
    record: &mut ProjectLinkRecord,
    proof: ProjectLinkReconciliation,
) -> Result<Operation, String> {
    runtime.checkpoint(operation).await?;
    let status = if matches!(proof, ProjectLinkReconciliation::Applied { .. }) {
        OperationStatus::Complete
    } else {
        OperationStatus::Superseded
    };
    record.lease = None;
    record.last_error = None;
    record.retry_at = 0;
    record.reconciliation = Some(proof);
    runtime.save(operation, record, status, true).await
}

async fn complete<R: ProjectLinkRuntime>(
    runtime: &R,
    operation: &Operation,
    record: &mut ProjectLinkRecord,
) -> Result<Operation, String> {
    let event_id = record.signed_patch.id.to_hex();
    resolve(
        runtime,
        operation,
        record,
        ProjectLinkReconciliation::Applied { event_id },
    )
    .await
}

/// Claim once, reconcile first, and publish only the persisted exact event.
/// Superseded/canceled records are readable for recovery but can never publish.
pub(super) async fn drive<R: ProjectLinkRuntime>(
    runtime: &R,
    mut operation: Operation,
    native_owner: PublicKey,
    explicit_retry: bool,
) -> Result<Operation, String> {
    let mut record = ProjectLinkRecord::from_operation(&operation, native_owner)?;
    let read_only = record.reconcile_only
        || matches!(
            operation.status,
            OperationStatus::Superseded | OperationStatus::Canceled
        );
    record.reconcile_only = read_only;
    let now = runtime.now()?;
    if record
        .lease
        .as_ref()
        .is_some_and(|lease| lease.expires_at > now)
    {
        return Err("Project operation is being processed; retry after its lease expires.".into());
    }
    if !read_only && !explicit_retry && (record.attempts >= 5 || record.retry_at > now) {
        return Err("Project operation is waiting for its retry time or an explicit retry.".into());
    }
    if !read_only {
        if explicit_retry {
            record.attempts = 0;
        }
        record.attempts += 1;
    }
    let worker = uuid::Uuid::new_v4().to_string();
    renew(&mut record, now, &worker)?;
    operation = runtime
        .save(&operation, &record, OperationStatus::Reconciling, false)
        .await?;
    // Reads that prove an already applied event do not require present-day write eligibility.
    match runtime.inspect(&operation, &record).await {
        Ok(LinkHead::Applied) => return complete(runtime, &operation, &mut record).await,
        Ok(LinkHead::Conflict(current_head_id)) if !record.publication_attempted => {
            return resolve(runtime, &operation, &mut record, ProjectLinkReconciliation::UnattemptedConflict { current_head_id }).await;
        }
        Ok(LinkHead::Conflict(_)) if read_only => {
            // A fresh conditional-protocol observation plus a fresh non-original head
            // proves any delayed exact CAS cannot apply. No other effects exist here.
            let proof = runtime.prove_superseded(&operation, &record).await;
            return match proof {
                Ok(Some(LinkHead::Applied)) => complete(runtime, &operation, &mut record).await,
                Ok(Some(LinkHead::Conflict(current_head_id))) => resolve(runtime, &operation, &mut record, ProjectLinkReconciliation::ConditionalConflict { current_head_id }).await,
                Ok(_) => fail(runtime, &operation, &mut record, "The earlier publication is unresolved. Retry read-only reconciliation when the relay can confirm its conditional protocol.".into(), true).await,
                Err(error) => fail(runtime, &operation, &mut record, error, true).await,
            };
        }
        Ok(LinkHead::Conflict(_)) => return fail(runtime, &operation, &mut record, "An earlier publication may have been attempted. Retry to reconcile it before preparing a successor.".into(), true).await,
        Ok(LinkHead::Original) if read_only => return fail(runtime, &operation, &mut record, "The earlier publication is unresolved; this recovery check will not submit it again.".into(), true).await,
        Ok(LinkHead::Original) => runtime.checkpoint(&operation).await?,
        Err(error) => return fail(runtime, &operation, &mut record, error, read_only).await,
    }
    renew(&mut record, runtime.now()?, &worker)?;
    operation = runtime
        .save(&operation, &record, OperationStatus::Reconciling, false)
        .await?;
    match runtime.capability(&operation).await {
        Ok(true) => runtime.checkpoint(&operation).await?,
        Ok(false) => {
            return fail(
                runtime,
                &operation,
                &mut record,
                "Relay Project channel-link publication capability is not confirmed.".into(),
                false,
            )
            .await
        }
        Err(error) => return fail(runtime, &operation, &mut record, error, false).await,
    }
    if let Err(error) = runtime.eligible(&operation, &record).await {
        return fail(runtime, &operation, &mut record, error, false).await;
    }
    record.publication_attempted = true;
    renew(&mut record, runtime.now()?, &worker)?;
    operation = runtime
        .save(&operation, &record, OperationStatus::Pending, false)
        .await?;
    let publish = runtime.publish(&operation, &record).await;
    runtime.checkpoint(&operation).await?;
    match runtime.inspect(&operation, &record).await {
        Ok(LinkHead::Applied) => complete(runtime, &operation, &mut record).await,
        Ok(LinkHead::Conflict(_)) => fail(runtime, &operation, &mut record, "Project publication may have been superseded. Retry read-only reconciliation; its claim is retained until proved safe.".into(), true).await,
        Ok(LinkHead::Original) => fail(runtime, &operation, &mut record, publish.err().unwrap_or_else(|| "Relay did not retain the exact Project publication.".into()), false).await,
        Err(error) => fail(runtime, &operation, &mut record, error, false).await,
    }
}

#[cfg(test)]
#[path = "project_change_link_driver_tests.rs"]
mod tests;
