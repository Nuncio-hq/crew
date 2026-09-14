//! Pre-publication generation uses the existing Wiki owner-operation claim.
//! No signed graph or relay effects exist until this record is atomically
//! replaced with a validated WikiPublicationRecord. Older clients reject this
//! tagged payload instead of interpreting it as a publishable graph.

use super::owner_operations::{owner_operation_update_at_path, ScopedOperationResult};
use super::wiki_publication_commands::WikiPublicationJob;
use crate::app_state::owner_scope::OwnerScopeToken;
use crate::owner_operations::{
    NewOperation, Operation, OperationKind, OperationStatus, OperationUpdate,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const MAX_NATIVE_GENERATIONS: usize = 2;

fn generation_admission() -> Arc<Semaphore> {
    static ADMISSION: OnceLock<Arc<Semaphore>> = OnceLock::new();
    ADMISSION
        .get_or_init(|| Arc::new(Semaphore::new(MAX_NATIVE_GENERATIONS)))
        .clone()
}

pub(super) struct GenerationControl {
    pub(super) key: String,
    pub(super) cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) permit: Option<Arc<OwnedSemaphorePermit>>,
}

pub(super) async fn admit_generation() -> Result<Arc<OwnedSemaphorePermit>, String> {
    tokio::time::timeout(
        Duration::from_secs(5),
        generation_admission().acquire_owned(),
    )
    .await
    .map_err(|_| "Wiki generation worker admission timed out.".to_string())?
    .map(Arc::new)
    .map_err(|_| "Wiki generation worker admission is unavailable.".to_string())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WikiGenerationRecord {
    generation_version: u32,
    coordinate: String,
    operation_id: String,
    error: Option<String>,
}

impl WikiGenerationRecord {
    pub(super) fn new_operation(id: String, coordinate: String) -> Result<NewOperation, String> {
        let record = Self {
            generation_version: 1,
            coordinate: coordinate.clone(),
            operation_id: id.clone(),
            error: None,
        };
        Ok(NewOperation {
            id,
            kind: OperationKind::WikiPublication,
            resource_key: coordinate,
            payload: serde_json::to_value(record)
                .map_err(|_| "Wiki generation record serialization failed.")?,
        })
    }

    pub(super) fn read(operation: &Operation) -> Result<Option<Self>, String> {
        if operation.payload.get("generation_version").is_none() {
            return Ok(None);
        }
        let record: Self = serde_json::from_value(operation.payload.clone())
            .map_err(|_| "Invalid Wiki generation record.")?;
        let prefix = format!("30617:{}:", operation.scope.owner);
        if record.generation_version != 1
            || operation.kind != OperationKind::WikiPublication
            || record.coordinate != operation.resource_key
            || record.operation_id != operation.id
            || !record.coordinate.starts_with(&prefix)
            || record.coordinate.len() <= prefix.len()
            || record.coordinate.chars().any(char::is_control)
            || record
                .error
                .as_ref()
                .is_some_and(|error| error.len() > 4096)
            || !matches!(
                (operation.status, operation.reconciled),
                (OperationStatus::Preparing, false)
                    | (OperationStatus::Canceled | OperationStatus::Complete, true)
            )
        {
            return Err("Invalid Wiki generation record scope or phase.".into());
        }
        Ok(Some(record))
    }

    pub(super) fn job(&self, operation: &Operation) -> WikiPublicationJob {
        WikiPublicationJob {
            id: operation.id.clone(),
            revision: operation.revision,
            resource_key: self.coordinate.clone(),
            status: operation.status,
            reconciled: operation.reconciled,
            snapshot_id: String::new(),
            source_revision: String::new(),
            cadence: "manual".into(),
            pages: 0,
            attempts: 0,
            progress: "generation".into(),
            head_attempted: false,
            cancel_requested: operation.status == OperationStatus::Canceled,
            reconcile_only: false,
            retired_dependency_id: None,
            retry_at: 0,
            last_error: self.error.clone(),
        }
    }

    pub(super) fn terminal(mut self, error: Option<&str>) -> Result<OperationUpdate, String> {
        self.error = error.map(|error| error.chars().take(1000).collect());
        Ok(OperationUpdate {
            status: if error.is_some() {
                OperationStatus::Canceled
            } else {
                OperationStatus::Complete
            },
            reconciled: true,
            payload: serde_json::to_value(self)
                .map_err(|_| "Wiki generation record serialization failed.")?,
        })
    }
}

pub(super) async fn finish<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    expected: OwnerScopeToken,
    operation: &Operation,
    error: Option<&str>,
) -> Result<ScopedOperationResult<Operation>, String> {
    let record =
        WikiGenerationRecord::read(operation)?.ok_or("Operation is not a Wiki generation.")?;
    owner_operation_update_at_path(
        app,
        path,
        expected,
        operation.id.clone(),
        operation.revision,
        record.terminal(error)?,
        false,
    )
    .await
}

/// Persist cancellation before signaling the exact foreground process.
pub(super) async fn cancel<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    expected: OwnerScopeToken,
    operation: &Operation,
) -> Result<ScopedOperationResult<Operation>, String> {
    let key = crate::wiki_worker::generation_cancel_key(
        &expected.scope.community,
        &operation.resource_key,
        &operation.id,
        operation.revision,
    );
    let result = finish(
        app,
        path,
        expected,
        operation,
        Some("Wiki generation canceled before publication."),
    )
    .await;
    super::wiki_publication_commands::signal_generation_after_cancel(result, &key)
}

/// Register before the journal row becomes visible. The restart worker can
/// distinguish a live foreground request from an interrupted one without a
/// timer lease that could expire during a legitimate long runtime call.
pub(super) struct GenerationRegistration {
    pub(super) key: String,
    pub(super) token: Arc<AtomicBool>,
}
impl GenerationRegistration {
    pub(super) fn begin(
        expected: &OwnerScopeToken,
        id: &str,
        coordinate: &str,
    ) -> Result<Self, String> {
        let key =
            crate::wiki_worker::generation_cancel_key(&expected.scope.community, coordinate, id, 0);
        let token = crate::wiki_worker::begin_generation_cancel(&key)?;
        Ok(Self { key, token })
    }
}
impl Drop for GenerationRegistration {
    fn drop(&mut self) {
        self.token.store(true, Ordering::Release);
        crate::wiki_worker::finish_generation_cancel(&self.key, &self.token);
    }
}

/// Reserve the Wiki generation claim before invoking the installed runtime.
pub(super) async fn prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    coordinate: String,
    repo_path: Option<String>,
    workspace_mode: Option<String>,
    runtime_selection: Option<crate::managed_agents::wiki_runtime::WikiRuntimeSelection>,
) -> Result<
    ScopedOperationResult<super::wiki_publication_commands::WikiPublicationPrepareResult>,
    String,
> {
    use super::owner_operations::owner_operation_create_at_path;
    use super::wiki_publication_commands::{prepare_registered, WikiPublicationPrepareResult};
    use super::wiki_publication_runtime::coordinate_parts;
    use crate::commands::resolve_wiki_runtime_selection;
    use crate::owner_operations::CreateResult;
    coordinate_parts(&coordinate)?;
    let selection =
        resolve_wiki_runtime_selection(app.clone(), &expected, &coordinate, runtime_selection)
            .await?;
    let operation_id = uuid::Uuid::new_v4().to_string();
    let permit = admit_generation().await?;
    let registration = GenerationRegistration::begin(&expected, &operation_id, &coordinate)?;
    let journal = super::owner_operations::journal_path(&app)?;
    let created = owner_operation_create_at_path(
        app.clone(),
        journal.clone(),
        expected.clone(),
        WikiGenerationRecord::new_operation(operation_id, coordinate.clone())?,
    )
    .await?;
    let operation = match created.value {
        CreateResult::Created(operation) => operation,
        CreateResult::Existing(_) => {
            return Err("Wiki generation already owns this repository; reload its status.".into())
        }
    };
    super::wiki_publication_worker::start(app.clone());
    let result = prepare_registered(
        app.clone(),
        expected.clone(),
        repo_path,
        workspace_mode,
        Some(selection),
        operation.clone(),
        GenerationControl {
            key: registration.key.clone(),
            cancel: Some(registration.token.clone()),
            permit: Some(permit.clone()),
        },
    )
    .await;
    let terminal = match &result {
        Ok(value) if matches!(value.value, WikiPublicationPrepareResult::Noop { .. }) => Some(None),
        Err(error) => Some(Some(error.as_str())),
        _ => None,
    };
    if let Some(error) = terminal {
        if let Err(finish_error) = super::wiki_generation_record::finish(
            app.clone(),
            journal.clone(),
            expected.clone(),
            &operation,
            error,
        )
        .await
        {
            // Cancel or a completed graph transition may have won the CAS.
            // Only that durable evidence permits leaving the original row alone.
            let current = super::owner_operations::owner_operation_load_at_path(
                app.clone(),
                journal,
                expected.clone(),
                operation.id.clone(),
                None,
            )
            .await?;
            if !current.value.reconciled && WikiGenerationRecord::read(&current.value)?.is_some() {
                return Err(format!(
                    "Wiki generation failed to record its terminal state: {finish_error}"
                ));
            }
        }
    }
    result
}

/// Atomically replace the generation claim with its complete signed graph.
/// Cancel and restart settlement compete on the same journal revision.
pub(super) async fn complete_generation_at_path<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    expected: OwnerScopeToken,
    operation: &Operation,
    input: super::wiki_publication_commands::WikiPublicationBuildInput,
) -> Result<ScopedOperationResult<Operation>, String> {
    WikiGenerationRecord::read(operation)?.ok_or("Operation is not a Wiki generation.")?;
    use super::wiki_publication_commands::{
        build_publication_from_generation, record_from_publication,
    };
    let publication = build_publication_from_generation(&input)?;
    let record =
        record_from_publication(publication, operation.resource_key.clone(), &input.cadence)?;
    let payload = serde_json::to_value(record)
        .map_err(|_| "Wiki publication recovery serialization failed.")?;
    super::owner_operations::owner_operation_update_at_path(
        app,
        path,
        expected,
        operation.id.clone(),
        operation.revision,
        OperationUpdate {
            status: OperationStatus::Preparing,
            reconciled: false,
            payload,
        },
        false,
    )
    .await
}

#[cfg(test)]
#[path = "wiki_generation_record_tests.rs"]
mod tests;
