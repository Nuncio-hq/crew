//! Bounded native dispatcher for one persisted Wiki snapshot publication.
//!
//! The dispatcher is deliberately generic over its runtime so tests exercise
//! the same ordering and recovery rules as the captured desktop runtime.

use super::wiki_publication_record::{
    WikiHeadRetirement, WikiPublicationLease, WikiPublicationProgress,
    WikiPublicationReconciliation, WikiPublicationRecord,
};
use crate::owner_operations::{Operation, OperationStatus};
use nostr::PublicKey;

/// Result of checking the exact persisted dependency IDs at the relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WikiDependencyState {
    /// Every page and the manifest are present and verify against the head.
    Verified,
    /// At least one immutable dependency is absent or soft-deleted.
    Missing,
}

/// Result of sending one persisted event.  A reserved immutable dependency
/// has a special, machine-readable relay refusal which is strong enough to
/// authorize the explicit successor flow.  Every other transport failure is
/// deliberately opaque to recovery: an ACK may have committed remotely.
/// Validated relay proof that the conditional head attempt itself, or its
/// exact expected predecessor, is permanently retired.
///
/// `current_head_id` is whatever different head the coordinate carries now,
/// retained as Superseded metadata. It is never the desired head: a live
/// desired head contradicts retirement and is rejected before this is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WikiHeadRetirementProof {
    pub(super) retirement: WikiHeadRetirement,
    pub(super) current_head_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WikiPublishError {
    /// The relay proved that this exact persisted immutable event is no longer
    /// live.  The event ID is retained as typed evidence in the journal.
    ImmutableDependencyRetired { event_id: String },
    /// The relay proved that this exact conditional head, or its exact
    /// expected predecessor, can never become live again. This settles the
    /// operation terminally; it is a different fact from a retired page or
    /// manifest dependency and is never conflated with one.
    HeadRetired(Box<WikiHeadRetirementProof>),
    /// The result is not strong enough to classify as permanent retirement.
    Unknown(String),
}

impl std::fmt::Display for WikiPublishError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ImmutableDependencyRetired { event_id } => write!(
                formatter,
                "Wiki immutable dependency retired: {event_id}. Regenerate the Wiki to create a fresh snapshot."
            ),
            Self::HeadRetired(proof) => match &proof.retirement {
                WikiHeadRetirement::Head { head_id } => write!(
                    formatter,
                    "Wiki head {head_id} is permanently retired; this publication can never be applied."
                ),
                WikiHeadRetirement::ExpectedHead {
                    expected_revision, ..
                } => write!(
                    formatter,
                    "The Wiki revision {expected_revision} this publication required is permanently retired."
                ),
            },
            Self::Unknown(reason) => formatter.write_str(reason),
        }
    }
}

/// Current authoritative `_toc` classification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WikiHead {
    /// No event exists at the expected `_toc` coordinate.
    Missing,
    /// The exact precondition head is still current.
    Original,
    /// The persisted head is current and its full graph was verified.
    Applied,
    /// A different valid head won the addressable coordinate.
    Conflict(String),
}

/// Captured native implementation seam. Every external step is fenced by the
/// operation revision, active owner/workspace scope, and current lease.
pub(super) trait WikiPublicationRuntime {
    fn now(&self) -> Result<i64, String>;
    async fn checkpoint(&self, operation: &Operation) -> Result<(), String>;
    async fn save(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String>;
    async fn capability(&self, operation: &Operation) -> Result<bool, String>;
    async fn inspect(
        &self,
        operation: &mut Operation,
        record: &mut WikiPublicationRecord,
        worker: &str,
    ) -> Result<WikiHead, String>;
    async fn dependencies(
        &self,
        operation: &mut Operation,
        record: &mut WikiPublicationRecord,
        worker: &str,
    ) -> Result<WikiDependencyState, String>;
    async fn publish(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        event: &nostr::Event,
    ) -> Result<(), WikiPublishError>;
}

fn renew(record: &mut WikiPublicationRecord, now: i64, worker: &str) -> Result<(), String> {
    record.lease = Some(WikiPublicationLease {
        worker_id: worker.to_owned(),
        expires_at: now
            .checked_add(60)
            .ok_or("Wiki publication lease clock overflow")?,
    });
    Ok(())
}

fn retry_delay(attempts: u8) -> i64 {
    5_i64 * (1_i64 << attempts.saturating_sub(1).min(5))
}

async fn fail<R: WikiPublicationRuntime>(
    runtime: &R,
    operation: &Operation,
    record: &mut WikiPublicationRecord,
    reason: String,
    read_only: bool,
    conflict: bool,
) -> Result<Operation, String> {
    runtime.checkpoint(operation).await?;
    record.lease = None;
    record.last_error = Some(truncate_error(&reason));
    record.retry_at = if read_only || conflict {
        0
    } else {
        runtime
            .now()?
            .checked_add(retry_delay(record.attempts).min(300))
            .ok_or("Wiki publication retry clock overflow")?
    };
    if conflict {
        record.reconcile_only = true;
    }
    let status = if conflict {
        OperationStatus::Superseded
    } else if read_only || record.cancel_requested {
        // A cancellation after a head attempt is still ambiguous until the
        // live coordinate is reconciled. Keep the journal visibly active.
        OperationStatus::Reconciling
    } else {
        OperationStatus::Failed
    };
    runtime.save(operation, record, status, false).await
}

fn truncate_error(reason: &str) -> String {
    // Sanitize first: replacing NUL with U+FFFD expands a byte, so truncating
    // the original string can produce a journal value larger than its bound.
    let sanitized = reason.replace('\0', "�");
    let mut end = sanitized.len().min(512);
    while end > 0 && !sanitized.is_char_boundary(end) {
        end -= 1;
    }
    let text = sanitized[..end].to_owned();
    if text.is_empty() {
        "Wiki publication failed.".into()
    } else {
        text
    }
}

/// Any immutable dependency retirement this record already proved.
///
/// A conflict resolution replaces the whole `reconciliation` value, so the
/// typed dependency proof must be carried forward explicitly or it is lost —
/// both from the durable row and from every later projection and reopen. It is
/// read from either the unresolved typed proof or an already terminal-shaped
/// Superseded metadata, so repeated reconciliation stays idempotent.
fn retained_dependency_id(record: &WikiPublicationRecord) -> Option<String> {
    match &record.reconciliation {
        Some(WikiPublicationReconciliation::ImmutableDependencyRetired { dependency_id }) => {
            Some(dependency_id.clone())
        }
        Some(WikiPublicationReconciliation::Superseded {
            retired_dependency_id: Some(dependency_id),
            ..
        }) => Some(dependency_id.clone()),
        _ => None,
    }
}

/// The single production constructor for a Superseded proof.
///
/// Both conflict arms and the head-retirement settle go through it, so the
/// dependency proof is preserved in exactly one place and a head-retirement
/// proof is carried independently rather than conflated with it.
fn superseded_proof(
    record: &WikiPublicationRecord,
    current_head_id: Option<String>,
    head_retirement: Option<WikiHeadRetirement>,
) -> WikiPublicationReconciliation {
    WikiPublicationReconciliation::Superseded {
        current_head_id,
        retired_dependency_id: retained_dependency_id(record),
        head_retirement,
    }
}

async fn resolve<R: WikiPublicationRuntime>(
    runtime: &R,
    operation: &Operation,
    record: &mut WikiPublicationRecord,
    proof: WikiPublicationReconciliation,
) -> Result<Operation, String> {
    if matches!(
        proof,
        WikiPublicationReconciliation::ImmutableDependencyRetired { .. }
    ) {
        return Err("Immutable dependency retirement requires an explicit successor.".into());
    }
    runtime.checkpoint(operation).await?;
    record.lease = None;
    record.last_error = None;
    record.retry_at = 0;
    record.reconciliation = Some(proof.clone());
    record.reconcile_only = true;
    let status = match proof {
        WikiPublicationReconciliation::Applied { .. } => OperationStatus::Complete,
        WikiPublicationReconciliation::Superseded { .. } => OperationStatus::Superseded,
        WikiPublicationReconciliation::CanceledBeforeHead { .. } => OperationStatus::Canceled,
        WikiPublicationReconciliation::ImmutableDependencyRetired { .. } => unreachable!(),
    };
    runtime.save(operation, record, status, true).await
}

async fn record_retirement<R: WikiPublicationRuntime>(
    runtime: &R,
    operation: &Operation,
    record: &mut WikiPublicationRecord,
    event_id: String,
) -> Result<Operation, String> {
    runtime.checkpoint(operation).await?;
    record.lease = None;
    record.reconcile_only = true;
    record.retry_at = 0;
    record.last_error = Some(
        WikiPublishError::ImmutableDependencyRetired {
            event_id: event_id.clone(),
        }
        .to_string(),
    );
    // Retirement is permanent evidence about one dependency, but it does not
    // prove that the old TOC write completed or that a successor is live. Keep
    // the predecessor unresolved/read-only until the atomic successor command
    // records the replacement claim.
    record.reconciliation = Some(WikiPublicationReconciliation::ImmutableDependencyRetired {
        dependency_id: event_id,
    });
    runtime
        .save(operation, record, OperationStatus::Reconciling, false)
        .await
}

/// Whether an explicit foreground action may durably revoke an ambiguous
/// cancellation for this exact attempt ("Resume publication").
///
/// Cancel stops automatic attempts, but a cancellation taken *after* an
/// ambiguous head send leaves the row unresolved and read-only forever: the
/// unique resource claim then blocks an ordinary Generate while nothing will
/// ever submit the saved graph again. An explicit, clearly labeled resume is
/// the way back, and only for a row that is:
///
/// * still unresolved — `from_operation` already refuses a reconciled row, so
///   a terminal cancellation stays terminal and a later Generate is separate;
/// * carrying **no** typed reconciliation proof — an
///   `ImmutableDependencyRetired` proof stays Regenerate-only, and a proven
///   `Superseded`/`Applied`/`CanceledBeforeHead` outcome is already decided;
/// * actually in the ambiguous cancellation / read-only recovery state.
///
/// A read-only reconciliation never revokes anything, even when the caller
/// also asks for an explicit retry, and an automatic restart never does.
pub(super) fn may_resume(
    record: &WikiPublicationRecord,
    explicit_retry: bool,
    force_read_only: bool,
) -> bool {
    explicit_retry
        && !force_read_only
        && record.reconciliation.is_none()
        && (record.cancel_requested || record.reconcile_only)
}

/// Claim, reconcile, and publish one exact graph in bounded attempts.
pub(super) async fn drive<R: WikiPublicationRuntime>(
    runtime: &R,
    mut operation: Operation,
    native_owner: PublicKey,
    explicit_retry: bool,
    force_read_only: bool,
) -> Result<Operation, String> {
    let mut record = WikiPublicationRecord::from_operation(&operation, native_owner)?;
    let now = runtime.now()?;
    // Evaluate the live lease before anything else may change this record: a
    // worker that is still inside its lease owns the row, and a resume must
    // not race it.
    if record
        .lease
        .as_ref()
        .is_some_and(|lease| lease.expires_at > now)
    {
        return Err("Wiki publication is being processed; retry after its lease expires.".into());
    }
    let resume = may_resume(&record, explicit_retry, force_read_only);
    if resume {
        // The revocation is part of the claimed-record CAS below, which lands
        // before `inspect` and before any send. The exact signed pages,
        // manifest and head, their IDs, the snapshot/source/expected-revision
        // CAS, `head_attempted` and `progress` are all untouched: this resumes
        // the *same* attempt rather than starting a new one.
        record.cancel_requested = false;
        record.reconcile_only = false;
    }
    let read_only = if resume {
        // An old unresolved Superseded/Canceled phase must not force read-only
        // after an approved resume; the live head is inspected first below and
        // still decides the outcome.
        false
    } else {
        force_read_only
            || record.reconcile_only
            || record.cancel_requested
            || matches!(
                operation.status,
                OperationStatus::Canceled | OperationStatus::Superseded | OperationStatus::Complete
            )
    };
    if !read_only && !explicit_retry && (record.attempts >= 5 || record.retry_at > now) {
        return Err("Wiki publication is waiting for its retry time or an explicit retry.".into());
    }
    if !read_only {
        if explicit_retry {
            record.attempts = 0;
        }
        record.attempts = record.attempts.saturating_add(1);
    }
    let worker = uuid::Uuid::new_v4().to_string();
    renew(&mut record, now, &worker)?;
    // The claimed-record CAS. It carries the resume revocation, so a lost CAS
    // or a refused write leaves the cancellation durably in place and this
    // attempt sends nothing at all.
    operation = runtime
        .save(&operation, &record, OperationStatus::Reconciling, false)
        .await?;

    // A desired head is complete only after the native runtime re-reads every
    // exact dependency and runs the v1 verifier. Missing/deleted dependencies
    // remain an explicit recovery failure and never become phantom success.
    match runtime.inspect(&mut operation, &mut record, &worker).await {
        Ok(WikiHead::Applied) => {
            let head_id = record.head.id.to_hex();
            return resolve(
                runtime,
                &operation,
                &mut record,
                WikiPublicationReconciliation::Applied { head_id },
            )
            .await;
        }
        Ok(WikiHead::Conflict(current_head_id)) => {
            let proof = superseded_proof(&record, Some(current_head_id), None);
            return resolve(runtime, &operation, &mut record, proof).await;
        }
        Ok(WikiHead::Original) if read_only => {
            if record.cancel_requested && !record.head_attempted {
                let current_head_id = record.expected_revision.clone();
                return resolve(
                    runtime,
                    &operation,
                    &mut record,
                    WikiPublicationReconciliation::CanceledBeforeHead {
                        current_head_id: Some(current_head_id),
                    },
                )
                .await;
            }
            return fail(
                runtime,
                &operation,
                &mut record,
                "The earlier Wiki publication is unresolved; this recovery check will not submit it again.".into(),
                true,
                false,
            )
            .await;
        }
        Ok(WikiHead::Missing) if read_only => {
            if record.cancel_requested && !record.head_attempted {
                return resolve(
                    runtime,
                    &operation,
                    &mut record,
                    WikiPublicationReconciliation::CanceledBeforeHead {
                        current_head_id: None,
                    },
                )
                .await;
            }
            return fail(
                runtime,
                &operation,
                &mut record,
                "The earlier Wiki publication is unresolved; this recovery check will not submit it again.".into(),
                true,
                false,
            )
            .await;
        }
        Err(error) if read_only => {
            return fail(runtime, &operation, &mut record, error, true, false).await;
        }
        Err(error) => {
            return fail(runtime, &operation, &mut record, error, false, false).await;
        }
        Ok(WikiHead::Missing | WikiHead::Original) => {}
    }

    runtime.checkpoint(&operation).await?;
    match runtime.capability(&operation).await {
        Ok(true) => runtime.checkpoint(&operation).await?,
        Ok(false) => {
            return fail(
                runtime,
                &operation,
                &mut record,
                "Relay Wiki conditional-publication capability is not confirmed.".into(),
                false,
                false,
            )
            .await;
        }
        Err(error) => return fail(runtime, &operation, &mut record, error, false, false).await,
    }

    // Progress is only a hint. Re-read the complete exact dependency set on
    // every resumed attempt; a relay may have soft-deleted an event after a
    // previous process recorded its page acknowledgement.
    let dependency_state = runtime
        .dependencies(&mut operation, &mut record, &worker)
        .await;
    match dependency_state {
        Ok(WikiDependencyState::Verified) => {}
        Ok(WikiDependencyState::Missing) => {
            // This can be a repair after an ambiguous head attempt. Keep the
            // phase representable with head_attempted=true; Preparing is only
            // valid before any dependency/head work has been recorded.
            record.progress = WikiPublicationProgress::Pages { confirmed: 0 };
            record.lease = Some(WikiPublicationLease {
                worker_id: worker.clone(),
                expires_at: runtime
                    .now()?
                    .checked_add(60)
                    .ok_or("Wiki publication lease clock overflow")?,
            });
            operation = runtime
                .save(&operation, &record, OperationStatus::Reconciling, false)
                .await?;
            for index in 0..record.pages.len() {
                let event = record.pages[index].clone();
                if let Err(error) = runtime.publish(&operation, &record, &event).await {
                    return match error {
                        WikiPublishError::ImmutableDependencyRetired { event_id } => {
                            record_retirement(runtime, &operation, &mut record, event_id).await
                        }
                        WikiPublishError::Unknown(reason) => {
                            fail(runtime, &operation, &mut record, reason, false, false).await
                        }
                    };
                }
                record.progress = WikiPublicationProgress::Pages {
                    confirmed: index + 1,
                };
                renew(&mut record, runtime.now()?, &worker)?;
                operation = runtime
                    .save(&operation, &record, OperationStatus::Reconciling, false)
                    .await?;
            }
            record.progress = WikiPublicationProgress::Manifest;
            renew(&mut record, runtime.now()?, &worker)?;
            operation = runtime
                .save(&operation, &record, OperationStatus::Reconciling, false)
                .await?;
            if let Err(error) = runtime.publish(&operation, &record, &record.manifest).await {
                return match error {
                    WikiPublishError::ImmutableDependencyRetired { event_id } => {
                        record_retirement(runtime, &operation, &mut record, event_id).await
                    }
                    WikiPublishError::Unknown(reason) => {
                        fail(runtime, &operation, &mut record, reason, false, false).await
                    }
                };
            }
            record.progress = WikiPublicationProgress::Head;
            renew(&mut record, runtime.now()?, &worker)?;
            operation = runtime
                .save(&operation, &record, OperationStatus::Reconciling, false)
                .await?;
        }
        Err(error) => return fail(runtime, &operation, &mut record, error, false, false).await,
    }

    // Even if all dependency IDs were present before this attempt, prove them
    // again immediately before the conditional head write.
    if !matches!(
        runtime
            .dependencies(&mut operation, &mut record, &worker)
            .await,
        Ok(WikiDependencyState::Verified)
    ) {
        return fail(
            runtime,
            &operation,
            &mut record,
            "Wiki immutable dependencies disappeared before head publication.".into(),
            false,
            false,
        )
        .await;
    }
    runtime.checkpoint(&operation).await?;
    // Every dependency has just verified, so the conditional head is the phase
    // this attempt is actually in. Record it in the *same* pre-send CAS as
    // `head_attempted`: an attempt that reached the head can never be described
    // as `Preparing`, and the repair branch above already arrived here with
    // `Head` set. Writing only `head_attempted` leaves the pair unrepresentable
    // and the durable save refuses it, so a snapshot whose dependencies were
    // already present could never reach its head write at all.
    record.progress = WikiPublicationProgress::Head;
    record.head_attempted = true;
    renew(&mut record, runtime.now()?, &worker)?;
    operation = runtime
        .save(&operation, &record, OperationStatus::Pending, false)
        .await?;
    let publish_result = runtime.publish(&operation, &record, &record.head).await;
    // A validated head/precondition retirement is terminal on its own: the
    // runtime already proved the exact retired event absent and read the
    // current head under the active fences. Settle it in ONE guarded CAS
    // before the ordinary post-send inspection, and take no further network
    // step — another read could only reintroduce ambiguity into a fact that is
    // already durable. If this save fails, nothing is released: the row keeps
    // its retryable claim and the same relay refusal can be obtained again.
    if let Err(WikiPublishError::HeadRetired(proof)) = &publish_result {
        let proof = superseded_proof(
            &record,
            proof.current_head_id.clone(),
            Some(proof.retirement.clone()),
        );
        return resolve(runtime, &operation, &mut record, proof).await;
    }
    runtime.checkpoint(&operation).await?;
    match runtime.inspect(&mut operation, &mut record, &worker).await {
        Ok(WikiHead::Applied) => {
            let head_id = record.head.id.to_hex();
            resolve(
                runtime,
                &operation,
                &mut record,
                WikiPublicationReconciliation::Applied { head_id },
            )
            .await
        }
        Ok(WikiHead::Conflict(current_head_id)) => {
            let proof = superseded_proof(&record, Some(current_head_id), None);
            resolve(runtime, &operation, &mut record, proof).await
        }
        Ok(WikiHead::Missing | WikiHead::Original) => {
            fail(
                runtime,
                &operation,
                &mut record,
                publish_result
                    .err()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "Relay did not retain the exact Wiki head.".into()),
                false,
                false,
            )
            .await
        }
        Err(error) => fail(runtime, &operation, &mut record, error, false, false).await,
    }
}
