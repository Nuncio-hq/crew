use crate::owner_operations::{Operation, OperationStatus};

use super::record::{Lease, Outcome, Payload, Progress};

pub(super) trait Backend {
    fn now(&self) -> Result<i64, String>;
    async fn check_scope(&self) -> Result<(), String>;
    async fn persist(
        &self,
        operation: &Operation,
        payload: &Payload,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String>;
    async fn latest(&self, payload: &Payload) -> Result<Option<String>, String>;
    async fn contains(&self, payload: &Payload, event: &nostr::Event) -> Result<bool, String>;
    async fn publish(&self, payload: &Payload, event: &nostr::Event) -> Result<(), String>;
}

pub(super) async fn resume<B: Backend>(
    backend: &B,
    mut operation: Operation,
    manual: bool,
) -> Result<Progress, String> {
    let mut payload: Payload = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "invalid canvas recovery record")?;
    super::record::validate(&operation, &payload)?;
    backend.check_scope().await?;
    if operation.reconciled {
        let current = backend.latest(&payload).await?;
        backend.check_scope().await?;
        if current.as_deref() != Some(&payload.canvas.id.to_hex()) {
            payload.outcome = Outcome::Superseded;
        }
        return Ok(payload.progress(&operation.id, current, operation.reconciled));
    }
    let now = backend.now()?;
    if payload
        .lease
        .as_ref()
        .is_some_and(|lease| lease.expires_at > now)
    {
        return Err("canvas recovery is already being processed".into());
    }
    if !manual && (payload.failures >= 5 || payload.next_retry_at.is_some_and(|due| due > now)) {
        return Err("canvas recovery is not due; use manual Retry when required".into());
    }
    if manual {
        payload.failures = 0;
    }
    payload.outcome = if payload.canvas_acknowledged {
        Outcome::CanvasCommittedAnnouncementPending
    } else if payload.canvas_attempted {
        Outcome::CommitUncertain
    } else {
        Outcome::NotCommitted
    };
    let worker = uuid::Uuid::new_v4().to_string();
    payload.lease = Some(Lease {
        worker: worker.clone(),
        expires_at: now.checked_add(60).ok_or("invalid retry clock")?,
    });
    operation = backend
        .persist(&operation, &payload, OperationStatus::Reconciling, false)
        .await?;
    match attempt(backend, &mut operation, &mut payload, &worker).await {
        Ok(current) => Ok(payload.progress(&operation.id, current, operation.reconciled)),
        Err(_) => {
            // Attempt intent was persisted before each send. If this final CAS
            // itself fails, the older intent and lease still survive restart.
            backend.check_scope().await?;
            payload.outcome = if payload.canvas_acknowledged {
                Outcome::CanvasCommittedAnnouncementPending
            } else if payload.canvas_attempted {
                Outcome::CommitUncertain
            } else {
                Outcome::NotCommitted
            };
            payload.failures = payload.failures.saturating_add(1).min(5);
            payload.next_retry_at = if payload.failures < 5 {
                let delay = (5_i64 << (payload.failures - 1)).min(300);
                Some(
                    backend
                        .now()?
                        .checked_add(delay)
                        .ok_or("invalid retry clock")?,
                )
            } else {
                None
            };
            payload.lease = None;
            backend
                .persist(&operation, &payload, OperationStatus::Failed, false)
                .await?;
            Ok(payload.progress(&operation.id, None, operation.reconciled))
        }
    }
}

async fn renew<B: Backend>(
    backend: &B,
    operation: &mut Operation,
    payload: &mut Payload,
    worker: &str,
) -> Result<(), String> {
    backend.check_scope().await?;
    if payload
        .lease
        .as_ref()
        .is_none_or(|lease| lease.worker != worker)
    {
        return Err("canvas recovery lease changed".into());
    }
    payload.lease = Some(Lease {
        worker: worker.to_string(),
        expires_at: backend
            .now()?
            .checked_add(60)
            .ok_or("invalid retry clock")?,
    });
    *operation = backend
        .persist(operation, payload, OperationStatus::Reconciling, false)
        .await?;
    backend.check_scope().await
}

async fn latest<B: Backend>(
    backend: &B,
    operation: &mut Operation,
    payload: &mut Payload,
    worker: &str,
) -> Result<Option<String>, String> {
    renew(backend, operation, payload, worker).await?;
    let value = backend.latest(payload).await?;
    backend.check_scope().await?;
    Ok(value)
}

async fn contains<B: Backend>(
    backend: &B,
    operation: &mut Operation,
    payload: &mut Payload,
    worker: &str,
    announcement: bool,
) -> Result<bool, String> {
    renew(backend, operation, payload, worker).await?;
    let event = if announcement {
        &payload.announcement
    } else {
        &payload.canvas
    };
    let seen = backend.contains(payload, event).await?;
    backend.check_scope().await?;
    Ok(seen)
}

async fn finish<B: Backend>(
    backend: &B,
    operation: &mut Operation,
    payload: &mut Payload,
    outcome: Outcome,
    reconciled: bool,
) -> Result<(), String> {
    payload.outcome = outcome;
    payload.lease = None;
    payload.next_retry_at = None;
    // Ambiguous superseded sends remain unresolved and require explicit Retry.
    payload.failures = if reconciled { 0 } else { 5 };
    let status = if payload.outcome == Outcome::Applied {
        OperationStatus::Complete
    } else {
        OperationStatus::Superseded
    };
    *operation = backend
        .persist(operation, payload, status, reconciled)
        .await?;
    Ok(())
}

async fn superseded<B: Backend>(
    backend: &B,
    operation: &mut Operation,
    payload: &mut Payload,
    worker: &str,
) -> Result<(), String> {
    if payload.announcement_attempted && !payload.announcement_acknowledged {
        payload.announcement_acknowledged =
            contains(backend, operation, payload, worker, true).await?;
    }
    let reconciled = (!payload.canvas_attempted || payload.canvas_acknowledged)
        && (!payload.announcement_attempted || payload.announcement_acknowledged);
    finish(backend, operation, payload, Outcome::Superseded, reconciled).await
}

async fn attempt<B: Backend>(
    backend: &B,
    operation: &mut Operation,
    payload: &mut Payload,
    worker: &str,
) -> Result<Option<String>, String> {
    // Reconcile exact IDs before resending after restart, timeout or takeover.
    let canvas_seen = contains(backend, operation, payload, worker, false).await?;
    payload.canvas_acknowledged |= canvas_seen;
    if payload.canvas_acknowledged {
        payload.outcome = Outcome::CanvasCommittedAnnouncementPending;
    }
    let mut current = latest(backend, operation, payload, worker).await?;
    let canvas_id = payload.canvas.id.to_hex();
    if current.as_deref() != Some(&canvas_id)
        && (current != payload.expected_head || payload.canvas_acknowledged)
    {
        superseded(backend, operation, payload, worker).await?;
        return Ok(current);
    }
    if !canvas_seen {
        if payload.canvas_acknowledged || current.as_deref() == Some(&canvas_id) {
            return Err("canvas readback is inconsistent".into());
        }
        payload.canvas_attempted = true;
        payload.outcome = Outcome::CommitUncertain;
        renew(backend, operation, payload, worker).await?;
        backend.publish(payload, &payload.canvas).await?;
        backend.check_scope().await?;
        payload.canvas_acknowledged = true;
        payload.outcome = Outcome::CanvasCommittedAnnouncementPending;
        current = latest(backend, operation, payload, worker).await?;
        if current.as_deref() != Some(&canvas_id) {
            superseded(backend, operation, payload, worker).await?;
            return Ok(current);
        }
    }
    let notice_seen = contains(backend, operation, payload, worker, true).await?;
    payload.announcement_acknowledged |= notice_seen;
    // A remote write during reconciliation must not cause a stale notice send.
    current = latest(backend, operation, payload, worker).await?;
    if current.as_deref() != Some(&canvas_id) {
        superseded(backend, operation, payload, worker).await?;
        return Ok(current);
    }
    if !payload.announcement_acknowledged {
        payload.announcement_attempted = true;
        renew(backend, operation, payload, worker).await?;
        backend.publish(payload, &payload.announcement).await?;
        backend.check_scope().await?;
        payload.announcement_acknowledged = true;
    }
    current = latest(backend, operation, payload, worker).await?;
    if current.as_deref() == Some(&canvas_id) {
        finish(backend, operation, payload, Outcome::Applied, true).await?;
    } else {
        superseded(backend, operation, payload, worker).await?;
    }
    Ok(current)
}
