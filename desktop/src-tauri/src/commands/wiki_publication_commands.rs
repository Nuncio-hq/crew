//! Native Wiki publication prepare, dispatch, and recovery commands.

use super::owner_operations::{
    load_owner_operation_for_dispatch, owner_operation_create, owner_operation_update,
    replace_wiki_with_successor, ScopedOperationResult,
};
use super::wiki_publication_driver::drive;
use super::wiki_publication_record::WikiPublicationRecord;
use super::wiki_publication_runtime::{coordinate_parts, now, NativeWikiPublication};
use crate::app_state::owner_scope::{assert_current, OwnerScopeToken};
use crate::commands::resolve_wiki_runtime_selection;
use crate::managed_agents::wiki_runtime::WikiRuntimeSelection;
use crate::owner_operations::{
    NewOperation, Operation, OperationKind, OperationStatus, OperationUpdate,
};
use crate::wiki_worker::WikiGeneration;
use crew_wiki::snapshot_v1_build::{build_cadence_update, build_snapshot, SnapshotBuild};
use serde::Serialize;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;

const MAX_OPERATION_PAGES: usize = 10;
const OPERATION_PAGE_SIZE: usize = 100;

const MAX_NATIVE_GENERATIONS: usize = 2;

fn generation_admission() -> Arc<Semaphore> {
    static ADMISSION: OnceLock<Arc<Semaphore>> = OnceLock::new();
    ADMISSION
        .get_or_init(|| Arc::new(Semaphore::new(MAX_NATIVE_GENERATIONS)))
        .clone()
}

async fn generate_native_wiki(
    expected: &OwnerScopeToken,
    owner: &str,
    repo_d: &str,
    repo_path: Option<&str>,
    workspace_mode: Option<&str>,
    runtime_selection: WikiRuntimeSelection,
    generation_key: String,
) -> Result<WikiGeneration, String> {
    let generation_owner = owner.to_owned();
    let generation_repo = repo_d.to_owned();
    let generation_path = repo_path.map(str::to_owned);
    let generation_mode = workspace_mode.map(str::to_owned);
    let generation_scope = expected.scope.community.clone();
    let generation_permit: OwnedSemaphorePermit = tokio::time::timeout(
        Duration::from_secs(5),
        generation_admission().acquire_owned(),
    )
    .await
    .map_err(|_| "Wiki generation worker admission timed out.".to_string())?
    .map_err(|_| "Wiki generation worker admission is unavailable.".to_string())?;
    let result = tokio::task::spawn_blocking(move || {
        let _generation_permit = generation_permit;
        let _guard = super::super::wiki_worker::generate_lock()
            .acquire(&format!(
                "{generation_scope}:{generation_owner}:{generation_repo}"
            ))
            .map_err(|error| error.to_string())?;
        // Register only after the per-repository lock is held. Registering
        // before it would let a rejected concurrent request overwrite the
        // active request's token, making Cancel target the wrong process.
        let generation_cancel =
            super::super::wiki_worker::begin_generation_cancel(&generation_key)?;
        let result = catch_unwind(AssertUnwindSafe(|| {
            super::super::wiki_worker::generate_wiki_pages_with_runtime_and_cancel(
                &generation_owner,
                &generation_repo,
                generation_path.as_deref(),
                generation_mode.as_deref(),
                Some(runtime_selection),
                Some(generation_cancel.clone()),
            )
            .map_err(|error| error.to_string())
        }));
        super::super::wiki_worker::finish_generation_cancel(&generation_key, &generation_cancel);
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
    .await
    .map_err(|_| "Wiki generation worker failed.".to_string());
    result?
}

#[derive(Debug, Serialize)]
#[serde(tag = "result", rename_all = "kebab-case")]
pub(crate) enum WikiPublicationPrepareResult {
    Created {
        job: WikiPublicationJob,
    },
    Noop {
        #[serde(rename = "headId")]
        head_id: String,
        #[serde(rename = "sourceRevision")]
        source_revision: String,
    },
}

/// Bounded renderer projection of a journal row. Signed event bodies remain
/// native and are never copied into status or retry IPC responses.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WikiPublicationJob {
    pub id: String,
    pub revision: u64,
    pub resource_key: String,
    pub status: OperationStatus,
    pub reconciled: bool,
    pub snapshot_id: String,
    pub source_revision: String,
    pub cadence: String,
    pub pages: usize,
    pub attempts: u8,
    pub progress: String,
    pub head_attempted: bool,
    pub cancel_requested: bool,
    pub reconcile_only: bool,
    pub retired_dependency_id: Option<String>,
    pub retry_at: i64,
    pub last_error: Option<String>,
}

#[path = "wiki_publication_projection.rs"]
mod projection;

use projection::{
    as_prepare_result, committed_regeneration_job, current_row, fenced, foreground_decision,
    job_from_operation, job_from_projection, ForegroundDecision,
};
pub(super) use projection::{cancel_intent, wiki_operation_summaries, wiki_operations};

/// Admit a Cancel against the durable row before signaling its foreground
/// generation. Keeping this ordering in one production seam makes the
/// Regenerate-only refusal atomic with respect to the cancellation registry.
pub(super) fn admit_cancel_generation(
    operation: &Operation,
) -> Result<WikiPublicationRecord, String> {
    cancel_intent(operation)
}

/// Signal a foreground generation only after its cancellation CAS succeeded.
/// A failed durable update must leave the running generation untouched because
/// recovery still owns the original journal row.
pub(super) fn signal_generation_after_cancel<T>(
    result: Result<T, String>,
    generation_key: &str,
) -> Result<T, String> {
    let value = result?;
    super::super::wiki_worker::cancel_generation(generation_key);
    Ok(value)
}

// The Cancel guard's predicate and its exact refusal text are consumed by the
// recovery regression suite, not by the shipping library: `cancel_intent` is
// the only production entry point to that rule. Keep them out of the library's
// name surface so they cannot become an unused re-export.
#[cfg(test)]
pub(super) use projection::{
    job_from_projection as projected_job, may_cancel, RETIRED_CANCEL_REFUSAL,
};

#[cfg(test)]
#[path = "wiki_publication_commands_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
#[path = "wiki_publication_regenerate_tests.rs"]
mod regenerate_tests;

/// Hydrate durable Wiki publication status after a renderer or app restart.
#[tauri::command]
pub(crate) async fn wiki_publication_list(
    app: AppHandle,
    expected: OwnerScopeToken,
) -> Result<ScopedOperationResult<Vec<WikiPublicationJob>>, String> {
    let operations = wiki_operations(app.clone(), expected.clone(), true).await?;
    let mut jobs = operations
        .into_iter()
        .map(|operation| job_from_projection(&operation))
        .collect::<Result<Vec<_>, _>>()?;
    jobs.sort_by(|left, right| left.resource_key.cmp(&right.resource_key));
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: jobs,
    })
}

/// Capture the source owner/repository and create one immutable publication
/// journal row before the first relay write.
#[tauri::command]
pub(crate) async fn wiki_publication_prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    coordinate: String,
    repo_path: Option<String>,
    workspace_mode: Option<String>,
    runtime_selection: Option<WikiRuntimeSelection>,
) -> Result<ScopedOperationResult<WikiPublicationPrepareResult>, String> {
    let (owner, repo_d) = coordinate_parts(&coordinate)?;
    let operation_id = uuid::Uuid::new_v4().to_string();
    let generation_key = super::super::wiki_worker::generation_cancel_key(
        &expected.scope.community,
        &coordinate,
        &operation_id,
        0,
    );
    let runtime_selection =
        resolve_wiki_runtime_selection(app.clone(), &expected, &coordinate, runtime_selection)
            .await?;
    let runtime = NativeWikiPublication::new(app.clone(), expected.clone(), &coordinate).await?;
    // The repository anchor is an explicit authenticated owner/d read. It is
    // intentionally separate from local source capture and does not require a
    // self-association tag that older repository announcements may omit.
    runtime.repository_head().await?;
    let before_head = runtime.current_head().await?;
    let generation = generate_native_wiki(
        &expected,
        owner,
        repo_d,
        repo_path.as_deref(),
        workspace_mode.as_deref(),
        runtime_selection,
        generation_key,
    )
    .await?;
    // Long local capture/generation must never publish against a head that
    // changed while it was running.
    let after_head = runtime.current_head().await?;
    if before_head.as_ref().map(|event| event.id) != after_head.as_ref().map(|event| event.id) {
        return Err("Wiki repository head changed while generating; retry.".into());
    }
    if let Some(head) = &after_head {
        if let Ok(Some(existing)) = runtime.current_publication().await {
            let existing_revision = existing.source_revision.clone();
            let cadence = head
                .tags
                .iter()
                .find(|tag| tag.as_slice().first().is_some_and(|name| name == "cadence"))
                .and_then(|tag| tag.as_slice().get(1))
                .map(String::as_str)
                .unwrap_or("manual");
            if existing_revision == generation.snapshot.source_revision
                && cadence == current_cadence(head)
            {
                let final_head = runtime.current_head().await?;
                if final_head.as_ref().map(|event| event.id) != Some(head.id) {
                    return Err("Wiki repository head changed while checking the no-op.".into());
                }
                assert_current(app.clone(), &expected).await?;
                return Ok(ScopedOperationResult {
                    token: expected,
                    value: WikiPublicationPrepareResult::Noop {
                        head_id: head.id.to_hex(),
                        source_revision: existing_revision,
                    },
                });
            }
        }
    }
    let current_time = u64::try_from(now()?).map_err(|_| "System clock is before the epoch")?;
    let created_at = after_head
        .as_ref()
        .map(|head| head.created_at.as_secs().saturating_add(1))
        .unwrap_or(0)
        .max(current_time);
    let cadence = after_head.as_ref().map(current_cadence).unwrap_or("manual");
    let expected_revision = after_head.as_ref().map(|event| event.id.to_hex());
    let publication = build_snapshot(SnapshotBuild {
        owner,
        repo_d,
        snapshot: &generation.snapshot,
        plan: &generation.plan,
        drafts: &generation.drafts,
        cadence,
        snapshot_id: None,
        expected_revision: expected_revision.as_deref(),
        created_at,
        keys: runtime.keys(),
    })
    .map_err(|error| error.to_string())?;
    let record = record_from_publication(publication, coordinate.clone(), cadence)?;
    let payload = serde_json::to_value(record)
        .map_err(|_| "Wiki publication recovery serialization failed.".to_string())?;
    super::wiki_publication_worker::start(app.clone());
    super::wiki_publication_worker::reserve(&app, &expected, &coordinate);
    let result = owner_operation_create(
        app.clone(),
        expected.clone(),
        NewOperation {
            id: operation_id,
            kind: OperationKind::WikiPublication,
            resource_key: coordinate.clone(),
            payload,
        },
    )
    .await;
    if result.is_err() {
        super::wiki_publication_worker::release(&app, &expected, &coordinate);
    }
    let result = result?;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: as_prepare_result(result.value)?,
    })
}

/// Create a cadence-only operation that retains the exact current manifest
/// and page events byte-for-byte.
#[tauri::command]
pub(crate) async fn wiki_publication_set_cadence(
    app: AppHandle,
    expected: OwnerScopeToken,
    coordinate: String,
    cadence: String,
) -> Result<ScopedOperationResult<WikiPublicationPrepareResult>, String> {
    let runtime = NativeWikiPublication::new(app.clone(), expected.clone(), &coordinate).await?;
    let Some(current) = runtime.current_publication().await? else {
        return Err("Wiki repository has no current snapshot.".into());
    };
    let current_cadence = current
        .head
        .tags
        .iter()
        .find(|tag| tag.as_slice().first().is_some_and(|name| name == "cadence"))
        .and_then(|tag| tag.as_slice().get(1))
        .map(String::as_str)
        .unwrap_or("manual");
    if current_cadence == cadence {
        let final_head = runtime.current_head().await?;
        if final_head.as_ref().map(|event| event.id) != Some(current.head.id) {
            return Err("Wiki head changed while checking the cadence no-op.".into());
        }
        assert_current(app.clone(), &expected).await?;
        return Ok(ScopedOperationResult {
            token: expected,
            value: WikiPublicationPrepareResult::Noop {
                head_id: current.head.id.to_hex(),
                source_revision: current.source_revision,
            },
        });
    }
    let timestamp = u64::try_from(now()?)
        .map_err(|_| "System clock is before the epoch")?
        .max(current.head.created_at.as_secs().saturating_add(1));
    let publication = build_cadence_update(
        &runtime.owner.to_hex(),
        &runtime.repo_d,
        &current.head,
        &current.manifest,
        &current.pages,
        &cadence,
        timestamp,
        runtime.keys(),
    )
    .map_err(|error| error.to_string())?;
    let record = record_from_publication(publication, coordinate.clone(), &cadence)?;
    let payload = serde_json::to_value(record)
        .map_err(|_| "Wiki publication recovery serialization failed.".to_string())?;
    super::wiki_publication_worker::start(app.clone());
    super::wiki_publication_worker::reserve(&app, &expected, &coordinate);
    let operation_id = uuid::Uuid::new_v4().to_string();
    let result = owner_operation_create(
        app.clone(),
        expected.clone(),
        NewOperation {
            id: operation_id,
            kind: OperationKind::WikiPublication,
            resource_key: coordinate.clone(),
            payload,
        },
    )
    .await;
    if result.is_err() {
        super::wiki_publication_worker::release(&app, &expected, &coordinate);
    }
    let result = result?;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: as_prepare_result(result.value)?,
    })
}

/// Build a fresh signed graph for a permanently retired Wiki dependency and
/// atomically transfer the predecessor's local claim to it.
///
/// Regeneration intentionally bypasses the ordinary unchanged-source no-op:
/// immutable page and manifest addresses include the new snapshot UUID, so a
/// retired graph can never be selected for replay again.
#[tauri::command]
pub(crate) async fn wiki_publication_regenerate(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
    repo_path: Option<String>,
    workspace_mode: Option<String>,
    runtime_selection: Option<WikiRuntimeSelection>,
) -> Result<ScopedOperationResult<WikiPublicationPrepareResult>, String> {
    // The requested revision is the *pre*-retirement one. Bind it to the
    // revision a committed retirement would have produced before doing any
    // work, so an overflowing request can never be silently accepted.
    revision
        .checked_add(1)
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or("Wiki publication revision overflow.".to_string())?;
    let journal = super::owner_operations::journal_path(&app)?;
    if let Some(job) = committed_regeneration_job(
        app.clone(),
        journal.clone(),
        expected.clone(),
        id.clone(),
        revision,
    )
    .await?
    {
        assert_current(app, &expected).await?;
        return Ok(ScopedOperationResult {
            token: expected,
            value: WikiPublicationPrepareResult::Created { job },
        });
    }
    let predecessor = match load_owner_operation_for_dispatch(
        app.clone(),
        expected.clone(),
        id.clone(),
        revision,
    )
    .await
    {
        Ok((_, predecessor)) => predecessor,
        Err(error) => {
            // A racing retry may have committed the retirement between the
            // lookup above and this load. Re-resolve the relation before
            // reporting a revision conflict the caller cannot act on.
            if let Some(job) = committed_regeneration_job(
                app.clone(),
                journal.clone(),
                expected.clone(),
                id.clone(),
                revision,
            )
            .await?
            {
                assert_current(app, &expected).await?;
                return Ok(ScopedOperationResult {
                    token: expected,
                    value: WikiPublicationPrepareResult::Created { job },
                });
            }
            return Err(error);
        }
    };
    if predecessor.kind != OperationKind::WikiPublication {
        return Err("Operation is not a Wiki publication.".into());
    }
    let owner = nostr::PublicKey::from_hex(&predecessor.scope.owner)
        .map_err(|_| "Wiki publication owner is invalid.".to_string())?;
    let predecessor_record = WikiPublicationRecord::from_operation(&predecessor, owner)?;
    let retired_dependency_id = match predecessor_record.reconciliation.as_ref() {
        Some(
            super::wiki_publication_record::WikiPublicationReconciliation::ImmutableDependencyRetired {
                dependency_id,
            },
        ) => dependency_id.clone(),
        _ => {
            return Err(
                "Wiki regeneration requires a typed immutable-dependency retirement proof.".into(),
            )
        }
    };
    if !predecessor_record.reconcile_only || predecessor.reconciled {
        return Err("Wiki regeneration requires an unresolved retired publication.".into());
    }
    let (owner_hex, repo_d) = {
        let (owner, repo_d) = coordinate_parts(&predecessor.resource_key)?;
        (owner.to_owned(), repo_d.to_owned())
    };
    if owner_hex != predecessor.scope.owner {
        return Err("Wiki publication owner scope does not match its coordinate.".into());
    }
    let runtime_selection = resolve_wiki_runtime_selection(
        app.clone(),
        &expected,
        &predecessor.resource_key,
        runtime_selection,
    )
    .await?;
    let runtime =
        NativeWikiPublication::new(app.clone(), expected.clone(), &predecessor.resource_key)
            .await?;
    let generation_key = super::super::wiki_worker::generation_cancel_key(
        &expected.scope.community,
        &predecessor.resource_key,
        &predecessor.id,
        predecessor.revision,
    );
    runtime.repository_head().await?;
    let before_head = runtime.current_head().await?;
    let generation = generate_native_wiki(
        &expected,
        &owner_hex,
        &repo_d,
        repo_path.as_deref(),
        workspace_mode.as_deref(),
        runtime_selection,
        generation_key,
    )
    .await?;
    let after_head = runtime.current_head().await?;
    if before_head.as_ref().map(|event| event.id) != after_head.as_ref().map(|event| event.id) {
        return Err("Wiki repository head changed while regenerating; retry.".into());
    }
    let cadence = after_head.as_ref().map(current_cadence).unwrap_or("manual");
    let expected_revision = after_head.as_ref().map(|event| event.id.to_hex());
    let current_time = u64::try_from(now()?).map_err(|_| "System clock is before the epoch")?;
    let created_at = after_head
        .as_ref()
        .map(|head| head.created_at.as_secs().saturating_add(1))
        .unwrap_or(0)
        .max(current_time);
    let publication = build_snapshot(SnapshotBuild {
        owner: &owner_hex,
        repo_d: &repo_d,
        snapshot: &generation.snapshot,
        plan: &generation.plan,
        drafts: &generation.drafts,
        cadence,
        snapshot_id: None,
        expected_revision: expected_revision.as_deref(),
        created_at,
        keys: runtime.keys(),
    })
    .map_err(|error| error.to_string())?;
    if publication.snapshot_id == predecessor_record.snapshot_id
        || publication.manifest.id == predecessor_record.manifest.id
        || publication.pages.iter().any(|event| {
            predecessor_record
                .pages
                .iter()
                .any(|old| old.id == event.id)
        })
    {
        return Err("Wiki regeneration did not produce a fresh immutable snapshot.".into());
    }
    let successor_record =
        record_from_publication(publication, predecessor.resource_key.clone(), cadence)?;
    let mut retired_record = predecessor_record;
    retired_record.reconcile_only = true;
    retired_record.reconciliation = Some(
        super::wiki_publication_record::WikiPublicationReconciliation::Superseded {
            current_head_id: after_head.map(|event| event.id.to_hex()),
            retired_dependency_id: Some(retired_dependency_id),
            // Regeneration retires an immutable dependency, never the head or
            // its precondition. Those are proven separately by the relay.
            head_retirement: None,
        },
    );
    retired_record.lease = None;
    retired_record.retry_at = 0;
    retired_record.last_error = None;
    retired_record.validate_intent(owner)?;
    successor_record.validate_intent(owner)?;
    let retirement_payload = serde_json::to_value(retired_record)
        .map_err(|_| "Wiki publication recovery serialization failed.".to_string())?;
    let successor_payload = serde_json::to_value(successor_record)
        .map_err(|_| "Wiki publication recovery serialization failed.".to_string())?;
    super::wiki_publication_worker::start(app.clone());
    super::wiki_publication_worker::reserve(&app, &expected, &predecessor.resource_key);
    let successor_id = uuid::Uuid::new_v4().to_string();
    let result = replace_wiki_with_successor(
        app.clone(),
        expected.clone(),
        predecessor.id.clone(),
        predecessor.revision,
        OperationUpdate {
            status: OperationStatus::Superseded,
            reconciled: true,
            payload: retirement_payload,
        },
        NewOperation {
            id: successor_id,
            kind: OperationKind::WikiPublication,
            resource_key: predecessor.resource_key.clone(),
            payload: successor_payload,
        },
    )
    .await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            super::wiki_publication_worker::release(&app, &expected, &predecessor.resource_key);
            // A concurrent same-action retry may have won the atomic
            // replacement. Its successor is the one valid outcome for this
            // request, so return it instead of a conflict that would tempt the
            // caller into signing a third graph.
            if let Some(job) = committed_regeneration_job(
                app.clone(),
                journal,
                expected.clone(),
                predecessor.id.clone(),
                revision,
            )
            .await?
            {
                assert_current(app, &expected).await?;
                return Ok(ScopedOperationResult {
                    token: expected,
                    value: WikiPublicationPrepareResult::Created { job },
                });
            }
            return Err(error);
        }
    };
    let job = job_from_operation(&result.value.successor)?;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: result.token,
        value: WikiPublicationPrepareResult::Created { job },
    })
}

fn record_from_publication(
    publication: crew_wiki::snapshot_v1_build::SnapshotPublication,
    coordinate: String,
    cadence: &str,
) -> Result<WikiPublicationRecord, String> {
    let record = WikiPublicationRecord {
        version: 1,
        coordinate,
        snapshot_id: publication.snapshot_id,
        source_revision: publication.source_revision,
        expected_revision: publication.expected_revision,
        head: publication.head,
        manifest: publication.manifest,
        pages: publication.pages,
        cadence: cadence.to_owned(),
        attempts: 0,
        progress: super::wiki_publication_record::WikiPublicationProgress::Preparing,
        head_attempted: false,
        cancel_requested: false,
        reconcile_only: false,
        reconciliation: None,
        retry_at: 0,
        last_error: None,
        lease: None,
    };
    record.validate_intent(
        nostr::PublicKey::from_hex(coordinate_parts(&record.coordinate).map(|(owner, _)| owner)?)
            .map_err(|_| "Wiki publication owner is invalid.")?,
    )?;
    Ok(record)
}

fn current_cadence(event: &nostr::Event) -> &str {
    let value = event
        .tags
        .iter()
        .find(|tag| tag.as_slice().first().is_some_and(|name| name == "cadence"))
        .and_then(|tag| tag.as_slice().get(1))
        .map(String::as_str)
        .unwrap_or("manual");
    if buzz_core_pkg::wiki_page::WikiCadence::parse(value).is_ok() {
        value
    } else {
        "manual"
    }
}

/// Perform one automatic or explicit bounded publication attempt.
#[tauri::command]
pub(crate) async fn wiki_publication_dispatch(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
    explicit_retry: bool,
) -> Result<ScopedOperationResult<WikiPublicationJob>, String> {
    dispatch_row(app, expected, id, revision, explicit_retry, false).await
}

async fn dispatch_row(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
    explicit_retry: bool,
    force_read_only: bool,
) -> Result<ScopedOperationResult<WikiPublicationJob>, String> {
    // `resource_key` is immutable for a journal row, so reading it before the
    // lock is safe; every state that decides the action is reloaded inside it.
    let (preloaded, _) = current_row(&app, &expected, &id).await?;
    if preloaded.kind != OperationKind::WikiPublication {
        return Err("Operation is not a Wiki publication.".into());
    }
    let resource_key = preloaded.resource_key.clone();
    super::wiki_publication_worker::reserve(&app, &expected, &resource_key);
    let value = super::wiki_publication_worker::serialized(&app, async {
        let (current, record) = current_row(&app, &expected, &id).await?;
        if current.resource_key != resource_key {
            return Err("recovery operation changed; reload".into());
        }
        match foreground_decision(current, revision, record.cancel_requested)? {
            ForegroundDecision::Settled(operation) => Ok(*operation),
            ForegroundDecision::Drive(operation) => {
                let runtime =
                    NativeWikiPublication::new(app.clone(), expected.clone(), &resource_key)
                        .await?;
                drive(
                    &runtime,
                    *operation,
                    runtime.owner,
                    explicit_retry,
                    force_read_only,
                )
                .await
            }
        }
    })
    .await;
    super::wiki_publication_worker::release(&app, &expected, &resource_key);
    let value = fenced(app, &expected, value).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: job_from_projection(&value)?,
    })
}

/// Explicit retry resets the bounded automatic-attempt window.
#[tauri::command]
pub(crate) async fn wiki_publication_retry(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<ScopedOperationResult<WikiPublicationJob>, String> {
    wiki_publication_dispatch(app, expected, id, revision, true).await
}

/// Mark a publication canceled while retaining its unresolved recovery row.
///
/// A typed immutable-retirement proof is refused here, before any relay read
/// or durable write: that row is already read-only and Regenerate-only, and
/// canceling it would erase the proof the atomic successor command needs.
#[tauri::command]
pub(crate) async fn wiki_publication_cancel(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<ScopedOperationResult<WikiPublicationJob>, String> {
    let (_, operation) =
        load_owner_operation_for_dispatch(app.clone(), expected.clone(), id, revision).await?;
    let generation_key = super::super::wiki_worker::generation_cancel_key(
        &expected.scope.community,
        &operation.resource_key,
        &operation.id,
        operation.revision,
    );
    // A prepare/regenerate call may still own a local runtime while the
    // renderer submits Cancel. This flag only reaches the process registered
    // for this exact owner/community/repository; it never touches an employee
    // runtime or another community. The production seam validates the durable
    // row first, so a stale Cancel cannot stop an active Regenerate.
    let mut record = admit_cancel_generation(&operation)?;
    let runtime =
        NativeWikiPublication::new(app.clone(), expected.clone(), &operation.resource_key).await?;
    let current_head = runtime.current_head().await?;
    let expected_head = if record.expected_revision == "absent" {
        None
    } else {
        Some(record.expected_revision.as_str())
    };
    let safe_before_head = !record.head_attempted
        && current_head
            .as_ref()
            .map(|event| Some(event.id.to_hex()) == expected_head.map(str::to_owned))
            .unwrap_or(expected_head.is_none());
    record.cancel_requested = true;
    record.reconcile_only = true;
    record.lease = None;
    if safe_before_head {
        record.reconciliation = Some(
            super::wiki_publication_record::WikiPublicationReconciliation::CanceledBeforeHead {
                current_head_id: current_head.map(|event| event.id.to_hex()),
            },
        );
    }
    let payload = serde_json::to_value(record)
        .map_err(|_| "Wiki publication recovery serialization failed.".to_string())?;
    let result = owner_operation_update(
        app.clone(),
        expected.clone(),
        operation.id,
        operation.revision,
        OperationUpdate {
            status: if safe_before_head {
                OperationStatus::Canceled
            } else {
                OperationStatus::Reconciling
            },
            reconciled: safe_before_head,
            payload,
        },
    )
    .await;
    let result = signal_generation_after_cancel(result, &generation_key)?;
    super::wiki_publication_worker::wake(&app);
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: job_from_operation(&result.value)?,
    })
}

/// Reconcile an unresolved row without enabling new publication writes.
#[tauri::command]
pub(crate) async fn wiki_publication_reconcile(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<ScopedOperationResult<WikiPublicationJob>, String> {
    dispatch_row(app, expected, id, revision, false, true).await
}

/// Return one bounded status projection for a publication recovery row.
#[tauri::command]
pub(crate) async fn wiki_publication_status(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
) -> Result<ScopedOperationResult<WikiPublicationJob>, String> {
    let result =
        super::owner_operations::owner_operation_load(app, expected.clone(), id, None).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: job_from_projection(&result.value)?,
    })
}
