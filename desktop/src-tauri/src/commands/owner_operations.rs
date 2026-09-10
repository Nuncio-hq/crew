//! Recovery IPC binds native scope before IO and fences its returned result.
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::app_state::owner_scope::{
    assert_current, capture, CapturedOwnerScope, OwnerScopeToken, OWNER_SCOPE_STALE as STALE,
};
use crate::owner_operations::{
    CreateResult, Limits, NewOperation, Operation, OperationScope, OperationStore,
    OperationSummary, OperationUpdate, WikiSuccessorResult,
};

/// Consumer must retain and compare this token before applying queued results.
#[derive(Serialize)]
pub(crate) struct ScopedOperationResult<T: Serialize> {
    pub token: OwnerScopeToken,
    pub value: T,
}

#[deny(clippy::await_holding_lock)]
async fn run_at_path<R, T, F>(
    app: AppHandle<R>,
    path: PathBuf,
    expected: OwnerScopeToken,
    action: F,
) -> Result<ScopedOperationResult<T>, String>
where
    R: Runtime,
    T: Serialize + Send + 'static,
    F: FnOnce(&mut OperationStore, &OperationScope) -> Result<T, String> + Send + 'static,
{
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(STALE.into());
    }
    let token = captured.token;
    // Captured keys/transport are not handed to the generic metadata consumer.
    drop(captured.keys);
    drop(captured.relay_url);
    let scope = token.scope.clone();
    let value = tokio::task::spawn_blocking(move || {
        let mut store =
            OperationStore::open(&path, Limits::default()).map_err(|error| error.to_string())?;
        action(&mut store, &scope)
    })
    .await
    .map_err(|_| "recovery operation failed".to_string())??;
    assert_current(app, &token).await?;
    Ok(ScopedOperationResult { token, value })
}

pub(crate) fn journal_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|_| "recovery storage unavailable")?;
    journal_path_from_app_data(&base)
}

// Only the trusted platform app-data anchor may resolve aliases. The store
// still rejects symlinks inside the owner-operations directory and database.
fn journal_path_from_app_data(base: &std::path::Path) -> Result<PathBuf, String> {
    let base = base
        .canonicalize()
        .map_err(|_| "recovery storage unavailable")?;
    Ok(base.join("owner-operations").join("recovery.db"))
}

/// Native domain dispatch loads only opaque identity/revision from the caller.
/// Domain validates the stored signed payload, endpoint and lease before send.
/// Never substitute the current endpoint for the endpoint recorded by prepare.
pub(crate) async fn load_owner_operation_for_dispatch<R: Runtime>(
    app: AppHandle<R>,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<(CapturedOwnerScope, Operation), String> {
    let path = journal_path(&app)?;
    load_owner_operation_for_dispatch_at_path(app, path, expected, id, revision).await
}

// Widened to sibling command modules only so the native guarded-read context
// can fence on an explicitly supplied journal path. No IPC or store API change.
pub(super) async fn load_owner_operation_for_dispatch_at_path<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<(CapturedOwnerScope, Operation), String> {
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(STALE.into());
    }
    let result = run_at_path(app.clone(), path, expected, move |store, scope| {
        let operation = store.load(scope, &id).map_err(|error| error.to_string())?;
        if operation.revision != revision {
            return Err("recovery operation changed; reload".into());
        }
        Ok(operation)
    })
    .await?;
    Ok((captured, result.value))
}

fn now() -> Result<i64, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock unavailable")?
        .as_secs();
    i64::try_from(seconds).map_err(|_| "system clock unavailable".into())
}

/// Capture the active native owner/community and generation fence.
#[tauri::command]
pub(crate) async fn owner_operation_scope(app: AppHandle) -> Result<OwnerScopeToken, String> {
    Ok(capture(app).await?.token)
}

/// Reserve or recover an immutable creation intent before external effects.
#[tauri::command]
pub(crate) async fn owner_operation_create(
    app: AppHandle,
    expected: OwnerScopeToken,
    operation: NewOperation,
) -> Result<ScopedOperationResult<CreateResult>, String> {
    run_at_path(
        app.clone(),
        journal_path(&app)?,
        expected,
        move |store, scope| {
            store
                .create(scope, operation, now()?)
                .map_err(|error| error.to_string())
        },
    )
    .await
}

/// Load an exact record from the currently captured native scope.
#[tauri::command]
pub(crate) async fn owner_operation_load(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: Option<u64>,
) -> Result<ScopedOperationResult<Operation>, String> {
    if let Some(revision) = revision {
        let (captured, operation) =
            load_owner_operation_for_dispatch(app, expected, id, revision).await?;
        return Ok(ScopedOperationResult {
            token: captured.token,
            value: operation,
        });
    }
    run_at_path(
        app.clone(),
        journal_path(&app)?,
        expected,
        move |store, scope| store.load(scope, &id).map_err(|error| error.to_string()),
    )
    .await
}

/// List bounded metadata; payloads require an explicit scoped load.
#[tauri::command]
pub(crate) async fn owner_operation_list(
    app: AppHandle,
    expected: OwnerScopeToken,
    after_id: Option<String>,
    limit: usize,
) -> Result<ScopedOperationResult<Vec<OperationSummary>>, String> {
    run_at_path(
        app.clone(),
        journal_path(&app)?,
        expected,
        move |store, scope| {
            store
                .list(scope, after_id.as_deref(), limit)
                .map_err(|error| error.to_string())
        },
    )
    .await
}

/// Atomically advance one whole recovery snapshot at its expected local revision.
#[tauri::command]
pub(crate) async fn owner_operation_update(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
    update: OperationUpdate,
) -> Result<ScopedOperationResult<Operation>, String> {
    run_at_path(
        app.clone(),
        journal_path(&app)?,
        expected,
        move |store, scope| {
            store
                .compare_and_swap(scope, &id, revision, update, now()?)
                .map_err(|error| error.to_string())
        },
    )
    .await
}

/// Atomically retire one unresolved Wiki operation and reserve its successor.
///
/// The store validates the predecessor's exact retirement snapshot and the
/// successor's immutable creation intent inside one SQLite transaction. No
/// network or source-generation work belongs in this helper.
pub(crate) async fn replace_wiki_with_successor(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
    retirement_update: OperationUpdate,
    new_operation: NewOperation,
) -> Result<ScopedOperationResult<WikiSuccessorResult>, String> {
    run_at_path(
        app.clone(),
        journal_path(&app)?,
        expected,
        move |store, scope| {
            store
                .replace_wiki_with_successor(
                    scope,
                    &id,
                    revision,
                    retirement_update,
                    new_operation,
                    now()?,
                )
                .map_err(|error| error.to_string())
        },
    )
    .await
}

/// Resolve an already-committed Wiki successor for one exact predecessor.
///
/// This is deliberately not a Tauri command: it exists so a native regenerate
/// request whose IPC response was lost recovers the successor it already
/// committed instead of capturing fresh source and signing a second graph.
/// `revision` is the caller's pre-retirement revision; the store binds it to
/// the recorded post-retirement revision.
pub(crate) async fn load_wiki_successor_at_path<R: Runtime>(
    app: AppHandle<R>,
    path: PathBuf,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<ScopedOperationResult<Option<WikiSuccessorResult>>, String> {
    run_at_path(app, path, expected, move |store, scope| {
        store
            .wiki_successor(scope, &id, revision)
            .map_err(|error| error.to_string())
    })
    .await
}

/// Remove only a record whose domain has reconciled every side effect.
#[tauri::command]
pub(crate) async fn owner_operation_remove(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
) -> Result<ScopedOperationResult<()>, String> {
    run_at_path(
        app.clone(),
        journal_path(&app)?,
        expected,
        move |store, scope| {
            store
                .remove_reconciled(scope, &id, revision)
                .map_err(|error| error.to_string())
        },
    )
    .await
}

#[cfg(all(test, unix))]
#[path = "owner_operations_tests.rs"]
mod tests;
