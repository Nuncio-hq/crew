//! Bounded projection, history selection, and foreground/worker arbitration
//! for the native Wiki publication journal.
//!
//! Everything here is deliberately cheap: a status poll reads only journal
//! metadata and never re-verifies a complete signed graph. Full graph
//! verification stays on the dispatch seam, where an event is about to be
//! sent, and on the explicit successor seam, where a new claim is created.

use super::{
    WikiPublicationJob, WikiPublicationPrepareResult, MAX_OPERATION_PAGES, OPERATION_PAGE_SIZE,
};
use crate::app_state::owner_scope::{assert_current, OwnerScopeToken};
use crate::commands::wiki_publication_record::{
    WikiPublicationProgress, WikiPublicationReconciliation, WikiPublicationRecord,
};
use crate::owner_operations::{CreateResult, Operation, OperationKind, OperationSummary};
use tauri::AppHandle;

pub(super) fn job_from_operation(operation: &Operation) -> Result<WikiPublicationJob, String> {
    job_from_operation_with_validation(operation, true)
}

pub(in crate::commands) fn job_from_projection(
    operation: &Operation,
) -> Result<WikiPublicationJob, String> {
    job_from_operation_with_validation(operation, false)
}

fn job_from_operation_with_validation(
    operation: &Operation,
    verify_graph: bool,
) -> Result<WikiPublicationJob, String> {
    let owner = nostr::PublicKey::from_hex(&operation.scope.owner)
        .map_err(|_| "Wiki publication owner is invalid.".to_string())?;
    let record: WikiPublicationRecord = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "Invalid Wiki publication recovery payload.".to_string())?;
    if verify_graph {
        record.validate_intent(owner)?;
    } else {
        record.validate_projection(owner)?;
    }
    let progress = match record.progress {
        WikiPublicationProgress::Preparing => "preparing",
        WikiPublicationProgress::Pages { .. } => "pages",
        WikiPublicationProgress::Manifest => "manifest",
        WikiPublicationProgress::Head => "head",
    };
    Ok(WikiPublicationJob {
        id: operation.id.clone(),
        revision: operation.revision,
        resource_key: operation.resource_key.clone(),
        status: operation.status,
        reconciled: operation.reconciled,
        snapshot_id: record.snapshot_id,
        source_revision: record.source_revision,
        cadence: record.cadence,
        pages: record.pages.len(),
        attempts: record.attempts,
        progress: progress.into(),
        head_attempted: record.head_attempted,
        cancel_requested: record.cancel_requested,
        reconcile_only: record.reconcile_only,
        retired_dependency_id: match record.reconciliation {
            Some(WikiPublicationReconciliation::ImmutableDependencyRetired { dependency_id }) => {
                Some(dependency_id)
            }
            Some(WikiPublicationReconciliation::Superseded {
                retired_dependency_id: Some(dependency_id),
                ..
            }) => Some(dependency_id),
            _ => None,
        },
        retry_at: record.retry_at,
        last_error: record.last_error,
    })
}

pub(super) fn as_prepare_result(
    result: CreateResult,
) -> Result<WikiPublicationPrepareResult, String> {
    match result {
        CreateResult::Created(operation) | CreateResult::Existing(operation) => {
            Ok(WikiPublicationPrepareResult::Created {
                job: job_from_operation(&operation)?,
            })
        }
    }
}

/// Choose exactly one current Wiki row per repository coordinate from bounded
/// metadata pages. Only these selected IDs are loaded with their signed
/// payloads afterwards; the whole retained history is never hydrated.
pub(in crate::commands) async fn wiki_operation_summaries(
    app: AppHandle,
    expected: OwnerScopeToken,
    include_terminal: bool,
) -> Result<Vec<OperationSummary>, String> {
    let mut after = None;
    let mut latest = std::collections::HashMap::<String, OperationSummary>::new();
    let mut complete = false;
    for _ in 0..MAX_OPERATION_PAGES {
        let page = crate::commands::owner_operations::owner_operation_list(
            app.clone(),
            expected.clone(),
            after,
            OPERATION_PAGE_SIZE,
        )
        .await?
        .value;
        let done = page.len() < OPERATION_PAGE_SIZE;
        after = page.last().map(|summary| summary.id.clone());
        for summary in page {
            if summary.kind != OperationKind::WikiPublication
                || (!include_terminal && !is_unresolved(&summary))
            {
                continue;
            }
            let replace = latest
                .get(&summary.resource_key)
                .map(|current| prefer_operation(&summary, current))
                .unwrap_or(true);
            if replace {
                latest.insert(summary.resource_key.clone(), summary);
            }
        }
        if done {
            complete = true;
            break;
        }
    }
    if !complete {
        return Err("Wiki publication recovery list exceeded its bounded scan.".into());
    }
    let mut summaries = latest.into_values().collect::<Vec<_>>();
    summaries.sort_by(|left, right| left.resource_key.cmp(&right.resource_key));
    Ok(summaries)
}

/// Return the bounded set of Wiki rows that still need a native recovery
/// decision.  The owner-operation journal is the durable source of truth; the
/// renderer gets only the small status projection above and loads no signed
/// envelopes itself.
pub(in crate::commands) async fn wiki_operations(
    app: AppHandle,
    expected: OwnerScopeToken,
    include_terminal: bool,
) -> Result<Vec<Operation>, String> {
    let summaries =
        wiki_operation_summaries(app.clone(), expected.clone(), include_terminal).await?;
    let mut result = Vec::with_capacity(summaries.len());
    for summary in summaries {
        result.push(
            crate::commands::owner_operations::owner_operation_load(
                app.clone(),
                expected.clone(),
                summary.id,
                None,
            )
            .await?
            .value,
        );
    }
    Ok(result)
}

/// Order two candidate rows for the same repository coordinate.
///
/// An unresolved claim always wins: it is the row whose side effects still
/// need a decision. Between two *different* terminal operations only the
/// native insertion sequence establishes which one came later. A revision is
/// local to a single row — a long-lived predecessor reaches revision 100 while
/// its freshly reserved successor is still at 0 — and `updated_at` is wall
/// clock, which a backward system clock or a slow retirement can invert.
/// Neither can order one operation against another.
pub(super) fn prefer_operation(candidate: &OperationSummary, current: &OperationSummary) -> bool {
    let candidate_active = is_unresolved(candidate);
    let current_active = is_unresolved(current);
    match candidate_active.cmp(&current_active) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            (candidate.sequence, candidate.id.as_str()) > (current.sequence, current.id.as_str())
        }
    }
}

pub(super) fn is_unresolved(operation: &OperationSummary) -> bool {
    // The durable store's claim is defined by `reconciled`, independently of
    // the visible phase. A superseded row can remain unresolved when a
    // conflict or retirement proof requires an explicit successor or
    // read-only reconciliation; dropping it here would hide the claim and
    // allow a newer history row to win the resource projection.
    !operation.reconciled
}

/// Exact refusal for a Cancel aimed at a typed retirement proof.
pub(in crate::commands) const RETIRED_CANCEL_REFUSAL: &str =
    "This Wiki publication is waiting on a retired immutable snapshot; regenerate it from source instead of canceling.";

/// Whether an explicit Cancel may act on this durable row at all.
///
/// Cancel exists to stop automatic attempts, but a typed
/// `ImmutableDependencyRetired` proof has already stopped them: the row is
/// read-only and its only way forward is the atomic successor command, which
/// needs *that exact proof* plus the predecessor's unresolved claim. Canceling
/// there would overwrite the proof with `CanceledBeforeHead` and reconcile the
/// row, silently releasing the claim and destroying the evidence Regenerate
/// depends on. This is the Cancel half of the same Regenerate-only contract
/// `may_resume` keeps for explicit retries.
pub(in crate::commands) fn may_cancel(record: &WikiPublicationRecord) -> bool {
    !matches!(
        record.reconciliation,
        Some(WikiPublicationReconciliation::ImmutableDependencyRetired { .. })
    )
}

/// The whole prelude of the Cancel command: validate the durable row against
/// the native owner and refuse the retirement contract.
///
/// This runs before the runtime is constructed, so a refused Cancel performs
/// no relay read and no durable mutation, and the exact typed proof, signed
/// graph and unresolved claim are left byte-for-byte in place.
pub(in crate::commands) fn cancel_intent(
    operation: &Operation,
) -> Result<WikiPublicationRecord, String> {
    let owner = nostr::PublicKey::from_hex(&operation.scope.owner)
        .map_err(|_| "Wiki publication owner is invalid.".to_string())?;
    let record = WikiPublicationRecord::from_operation(operation, owner)?;
    if !may_cancel(&record) {
        return Err(RETIRED_CANCEL_REFUSAL.into());
    }
    Ok(record)
}

/// What a foreground request must do with the durable row it reloaded inside
/// the serialized dispatch lock.
pub(super) enum ForegroundDecision {
    /// Drive this exact durable row.
    Drive(Box<Operation>),
    /// Startup recovery already resolved the requested operation; report its
    /// terminal projection instead of failing an action that has nothing left
    /// to do.
    Settled(Box<Operation>),
}

/// Decide what to do with the row that is durable *now*, given the revision
/// the renderer asked about.
///
/// The foreground request was queued behind the app-lifetime recovery worker,
/// which owns leases, attempt counts and progress on the same row. Failing
/// only because that worker claimed or finished the same operation would turn
/// a successful recovery into a user-visible error. An explicit user decision
/// on the row — cancellation — still produces a hard conflict, because the
/// action the renderer requested is no longer the one the user is looking at.
pub(super) fn foreground_decision(
    current: Operation,
    requested_revision: u64,
    cancel_requested: bool,
) -> Result<ForegroundDecision, String> {
    if current.kind != OperationKind::WikiPublication {
        return Err("Operation is not a Wiki publication.".into());
    }
    if current.revision == requested_revision {
        return Ok(ForegroundDecision::Drive(Box::new(current)));
    }
    if current.reconciled {
        return Ok(ForegroundDecision::Settled(Box::new(current)));
    }
    if cancel_requested {
        return Err("recovery operation changed; reload".into());
    }
    Ok(ForegroundDecision::Drive(Box::new(current)))
}

/// Load the durable row and its bounded metadata without verifying the whole
/// signed graph; `drive` performs that verification before any dispatch.
pub(super) async fn current_row(
    app: &AppHandle,
    expected: &OwnerScopeToken,
    id: &str,
) -> Result<(Operation, WikiPublicationRecord), String> {
    let current = crate::commands::owner_operations::owner_operation_load(
        app.clone(),
        expected.clone(),
        id.to_owned(),
        None,
    )
    .await?
    .value;
    if current.kind != OperationKind::WikiPublication {
        return Err("Operation is not a Wiki publication.".into());
    }
    let owner = nostr::PublicKey::from_hex(&current.scope.owner)
        .map_err(|_| "Wiki publication owner is invalid.".to_string())?;
    let record: WikiPublicationRecord = serde_json::from_value(current.payload.clone())
        .map_err(|_| "Invalid Wiki publication recovery payload.".to_string())?;
    record.validate_projection(owner)?;
    if current.resource_key != record.coordinate {
        return Err("Wiki publication resource does not match its signed coordinate.".into());
    }
    Ok((current, record))
}

/// Re-assert the owner/workspace fence on both the success and failure paths.
/// A result produced under a replaced identity must never be reported as the
/// current scope's outcome, and a transport failure is no exception.
pub(super) async fn fenced<T>(
    app: AppHandle,
    expected: &OwnerScopeToken,
    result: Result<T, String>,
) -> Result<T, String> {
    assert_current(app, expected).await?;
    result
}

/// Return the successor this exact regeneration request already committed.
///
/// Regeneration signs a fresh immutable graph, so a lost IPC response can
/// never be recovered by replaying the same intent: the store's creation
/// digest would differ and the predecessor revision has already advanced.
/// Resolving the recorded direct relation *before* capturing source is what
/// makes the command idempotent. The successor is returned even once it has
/// reconciled, and its stored graph is re-validated against the native owner
/// exactly as a freshly built one would be.
pub(super) async fn committed_regeneration_job<R: tauri::Runtime>(
    app: AppHandle<R>,
    path: std::path::PathBuf,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<Option<WikiPublicationJob>, String> {
    let Some(relation) = crate::commands::owner_operations::load_wiki_successor_at_path(
        app, path, expected, id, revision,
    )
    .await?
    .value
    else {
        return Ok(None);
    };
    if relation.successor.kind != OperationKind::WikiPublication
        || relation.successor.resource_key != relation.predecessor.resource_key
    {
        return Err("Wiki regeneration successor does not match its predecessor.".into());
    }
    Ok(Some(job_from_operation(&relation.successor)?))
}
