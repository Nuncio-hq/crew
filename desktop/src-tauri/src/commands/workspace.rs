use nostr::Keys;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::managed_agents::{
    effective_repos_dir, ensure_repos_symlink, nest_dir, restore_managed_agents_on_launch,
    try_regenerate_nest, write_persisted_repos_dir,
};
use crate::relay;

const WORKSPACE_APPLY_SUPERSEDED: &str = "workspace apply superseded by a newer request";

fn next_apply_generation(generation: &std::sync::atomic::AtomicU64) -> u64 {
    generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
}

fn assert_current_apply_generation(
    generation: &std::sync::atomic::AtomicU64,
    ticket: u64,
) -> Result<(), String> {
    if generation.load(Ordering::Acquire) == ticket {
        Ok(())
    } else {
        Err(WORKSPACE_APPLY_SUPERSEDED.to_string())
    }
}

async fn begin_workspace_apply(
    transaction_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    workspace_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    generation: &std::sync::atomic::AtomicU64,
) -> (
    tokio::sync::OwnedMutexGuard<()>,
    tokio::sync::OwnedMutexGuard<()>,
    u64,
) {
    // Keep the complete apply/restore transaction serialized while preserving
    // the existing inner lock as the scope-capture/mutation fence.
    let transaction_guard = transaction_lock.lock_owned().await;
    let workspace_guard = workspace_lock.lock_owned().await;
    let ticket = next_apply_generation(generation);
    (transaction_guard, workspace_guard, ticket)
}

/// Release the short workspace fence before restore calls its normal scoped
/// capture APIs, while retaining the outer transaction until restore ends.
async fn hold_restore_transaction<F, T>(
    transaction_guard: tokio::sync::OwnedMutexGuard<()>,
    workspace_guard: tokio::sync::OwnedMutexGuard<()>,
    restore: F,
) -> T
where
    F: std::future::Future<Output = T> + Send,
{
    drop(workspace_guard);
    let _transaction_guard = transaction_guard;
    restore.await
}

/// Run the deferred restore phase after the apply fence has been released.
/// The outer transaction remains held for the whole phase, and the pending
/// latch is consumed only after a successful, non-shutdown restore.
async fn complete_restore<R, F, Fut>(
    transaction_guard: tokio::sync::OwnedMutexGuard<()>,
    workspace_guard: tokio::sync::OwnedMutexGuard<()>,
    app: AppHandle<R>,
    restore_pending: bool,
    restore: F,
) where
    R: tauri::Runtime,
    F: FnOnce(AppHandle<R>, bool) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
{
    hold_restore_transaction(transaction_guard, workspace_guard, async move {
        let restore_result = restore(app.clone(), restore_pending).await;
        if !restore_pending {
            return;
        }
        match restore_result {
            Ok(())
                if !app
                    .state::<AppState>()
                    .shutdown_started
                    .load(Ordering::SeqCst) =>
            {
                app.state::<AppState>()
                    .managed_agent_restore_pending
                    .store(false, Ordering::Release);
            }
            Ok(()) => {}
            Err(error) => eprintln!("buzz-desktop: failed to restore managed agents: {error}"),
        }
    })
    .await;
}

// The real workspace apply and its scope regression share this mutation seam.
fn apply_workspace_identity(
    state: &AppState,
    relay_url: String,
    keys: Option<Keys>,
) -> Result<(), String> {
    let identity_guard = state.identity_mutation.lock().map_err(|e| e.to_string())?;
    state.replace_workspace_identity(&identity_guard, keys, relay_url)
}

/// Adopt the pre-scoping global retention database's pending rows into `scope`.
///
/// Best-effort: a failure is logged and the boot proceeds. The migration's own
/// crash-safety guards make the next launch retry safely, and blocking the
/// workspace apply on it would be worse than a delayed publish.
fn migrate_legacy_retention_into(
    app: &AppHandle,
    scope: &crate::managed_agents::retention::RetentionScope,
) {
    let Ok(base_dir) = crate::managed_agents::managed_agents_base_dir(app) else {
        return;
    };
    match crate::managed_agents::retention::migrate_legacy_retention_db(
        &base_dir,
        &scope.db_path,
        &scope.owner_keys.public_key().to_hex(),
    ) {
        Ok(0) => {}
        Ok(copied) => {
            eprintln!("buzz-desktop: adopted {copied} legacy retained event(s) into this community")
        }
        Err(error) => eprintln!("buzz-desktop: legacy retention migration failed: {error}"),
    }
}

#[derive(Deserialize)]
struct RelayInfoIcon {
    #[serde(default)]
    icon: Option<String>,
}

/// Fetch a relay's workspace icon from its NIP-11 relay information document.
///
/// Works for any workspace (active or not) with a plain unauthenticated HTTP
/// GET — no WebSocket session needed. Returns `None` when the relay has no
/// icon set, is unreachable, or serves a malformed document: the rail falls
/// back to initials in all three cases.
#[tauri::command]
pub async fn fetch_workspace_icon(
    relay_url: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let http_url = relay::relay_http_base_url(&relay_url);
    let Ok(response) = state
        .http_client
        .get(&http_url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
    else {
        return Ok(None);
    };
    if !response.status().is_success() {
        return Ok(None);
    }
    let doc = response
        .json::<RelayInfoIcon>()
        .await
        .unwrap_or(RelayInfoIcon { icon: None });
    Ok(doc.icon.filter(|icon| !icon.is_empty()))
}

#[derive(Serialize)]
pub struct ActiveWorkspaceInfo {
    relay_url: String,
    pubkey: String,
}

/// Returns the current active workspace info (relay URL + pubkey).
#[tauri::command]
pub fn get_active_workspace(state: State<'_, AppState>) -> Result<ActiveWorkspaceInfo, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let relay_url = relay::relay_ws_url_with_override(&state);
    Ok(ActiveWorkspaceInfo {
        relay_url,
        pubkey: keys.public_key().to_hex(),
    })
}

/// Validate a candidate `repos_dir` without mutating the filesystem.
///
/// The Add/Edit workspace dialogs call this on submit to block Save on a bad
/// path, so a typo never reaches `apply_workspace`. Reuses the same
/// `validate_repos_dir` the boot/apply path uses — one source of truth for
/// "what's a valid repos dir". An empty/whitespace value clears the override
/// and is valid. `Err` carries the human-readable reason for inline display.
#[tauri::command]
pub async fn validate_repos_dir(dir: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let trimmed = dir.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let nest = nest_dir().ok_or("cannot resolve home directory for nest")?;
        crate::managed_agents::validate_repos_dir(&nest, trimmed).map(|_| ())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Apply a workspace's configuration to the backend session.
///
/// Called by the frontend on app init (after reload) to configure the
/// Tauri backend with the selected workspace's relay URL, keys, and repos
/// directory.
///
/// A bad `repos_dir` is non-fatal: relay/keys always apply (the relay is the
/// active workspace's own choice — orthogonal to the filesystem repos dir),
/// the bad value is NOT persisted (so the next boot starts clean), the
/// `REPOS` symlink is skipped (REPOS stays a real dir), a `repos-dir-error`
/// event surfaces the reason, and the command returns `Ok`. The dialogs
/// already block a bad path at Save (`validate_repos_dir`); this fallback only
/// catches a value that went bad after save (deleted dir, unmounted volume).
#[tauri::command]
pub async fn apply_workspace(
    relay_url: String,
    nsec: Option<String>,
    repos_dir: Option<String>,
    agent_managed_profiles: Option<bool>,
    thread_scoped_acp_sessions: Option<bool>,
    app: AppHandle,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    // Take the generation only after entering the serialized transaction. An
    // apply that is already running remains authoritative until it releases
    // the lock; the next apply then advances the generation. This keeps every
    // awaited reconciliation/event-sync phase inside one ordered transaction.
    let (apply_transaction_guard, apply_guard, apply_generation) = begin_workspace_apply(
        state.workspace_apply_transaction_lock.clone(),
        state.workspace_apply_lock.clone(),
        &state.workspace_apply_generation,
    )
    .await;

    let restore_app = app.clone();
    let apply_app = app.clone();
    // Capture the caller's relay before the blocking apply. Reading shared
    // state afterward could pick up a newer concurrent community switch.
    let profile_reconcile_relay = relay_url.clone();
    tokio::task::spawn_blocking(move || {
        let app = apply_app;
        let state = app.state::<AppState>();

        // ── Validate before mutating ──────────────────────────────────────────
        let parsed_keys = match nsec.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(nsec_trimmed) => {
                Some(Keys::parse(nsec_trimmed).map_err(|e| format!("invalid nsec: {e}"))?)
            }
            None => None,
        };

        // Decide the effective repos_dir from the candidate. A bad path does NOT
        // reject — it is treated as if no override were set: relay/keys still
        // apply, the bad value is not persisted, and a `repos-dir-error` surfaces
        // the reason. Persisting a bad path would make every later boot read it,
        // fail to resolve the symlink, and silently skip agent restore. One
        // validate (inside `effective_repos_dir`) drives both the emit and the
        // persisted value. `nest` is resolved softly: when absent there is nothing
        // to persist or symlink, and relay/keys must still apply unconditionally.
        let nest = nest_dir();
        let effective_repos_dir = match nest.as_deref() {
            Some(nest) => match effective_repos_dir(nest, repos_dir.as_deref()) {
                Ok(value) => value,
                Err(error) => {
                    let _ = app.emit("repos-dir-error", error);
                    None
                }
            },
            None => None,
        };

        // Defense in depth: this transaction still owns the serialized apply
        // generation before making its first mutation. Normal queued applies
        // cannot advance it until this transaction releases the guard.
        assert_current_apply_generation(&state.workspace_apply_generation, apply_generation)?;

        // ── Apply all state changes (nothing below can fail) ──────────────────
        apply_workspace_identity(&state, relay_url, parsed_keys)?;
        crate::relay_admission::reset_gate_for_workspace_change();

        // Keep the backend-side reconcile guard aligned with the frontend
        // experiment before launch-time restore can spawn any agents. Missing
        // means the stable behavior: desktop remains authoritative.
        state
            .managed_agent_profile_reconcile_enabled()
            .store(!agent_managed_profiles.unwrap_or(false), Ordering::Release);
        // Persisted frontend experiment state must land before launch-time
        // restore so every restored agent starts with the selected ACP policy.
        // Missing preserves the stable channel-scoped behavior.
        state.thread_scoped_acp_sessions_enabled().store(
            thread_scoped_acp_sessions.unwrap_or(false),
            Ordering::Release,
        );

        // ── Filesystem side-effect (non-fatal) ────────────────────────────────
        // Persist the *effective* repos_dir (None when the candidate failed
        // validation) for the backend to read at boot, then re-point REPOS to
        // match. Persisting first makes the dotfile authoritative even if the
        // symlink apply fails here (e.g. a non-empty real REPOS): the next boot
        // reads the persisted value and resolves the symlink before any agent can
        // clone into REPOS. A bad candidate persists `None`, so the next boot is
        // clean and agent restore proceeds. Failure of either must NOT fail the
        // command — relay/keys are already applied. Surface symlink errors via
        // `repos-dir-error`.
        if let Some(nest) = nest.as_deref() {
            if let Err(error) = write_persisted_repos_dir(nest, effective_repos_dir.as_deref()) {
                eprintln!("buzz-desktop: persist repos dir failed: {error}");
            }
            if let Err(error) = ensure_repos_symlink(nest, effective_repos_dir.as_deref()) {
                eprintln!("buzz-desktop: repos dir setup failed: {error}");
                let _ = app.emit("repos-dir-error", error);
            }
        }

        try_regenerate_nest(&app);

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    assert_current_apply_generation(&state.workspace_apply_generation, apply_generation)?;

    let state = restore_app.state::<AppState>();
    super::agents::provider_access::reconcile_on_workspace_apply(&restore_app, &state).await?;
    // The Bumble→Pollen migration may have renamed stopped agents. Reconcile
    // their relay profiles independently of runtime restore; successful writes
    // record this relay while retaining the agent for other communities, and
    // failures retry on the next workspace apply.
    crate::managed_agents::spawn_pending_profile_reconciliations(
        &restore_app,
        &profile_reconcile_relay,
    );

    // Backfill this exact relay+owner scope only after the workspace has been
    // applied. Running at process boot would target the fallback relay and
    // collapse every community into one pending-event store.
    match crate::managed_agents::retention::active_retention_scope(&restore_app, &state) {
        Ok(scope) => {
            // Adopt whatever the pre-scoping release left queued in the global
            // retention database BEFORE the scoped reconcile and flush run, so
            // stranded tombstones and archive requests publish on this boot
            // instead of being abandoned by the storage cutover. Best-effort:
            // it is not a prerequisite for the superseding head — the team leg
            // below builds the repaired roster's head fresh from disk with a
            // monotonic `created_at` regardless of what the legacy copy left.
            migrate_legacy_retention_into(&restore_app, &scope);
            // Await the reconcile to completion — do NOT spawn it — and
            // propagate its failure. The boot migration may have repaired team
            // membership on disk; the frontend starts inbound history replay
            // the moment `useCommunityInit` observes the applied workspace, and
            // an old relay team head could otherwise win that race and overwrite
            // the repaired `persona_ids`. The team leg is fatal (see
            // `run_event_sync`): only its success durably retains the corrected
            // head with a superseding `monotonic_created_at`, so
            // `retain_inbound_event`'s equal/older guard rejects the stale head.
            // On failure we return `Err` — the command reports failure,
            // `useCommunityInit` never exposes the community, and inbound replay
            // never starts against an un-superseded disk state.
            crate::event_sync::run_event_sync_blocking(
                restore_app.clone(),
                scope.owner_keys,
                scope.db_path,
            )
            .await?;
        }
        Err(error) => {
            // Scope resolution is a prerequisite for establishing the
            // superseding head, so its failure is fatal for the same reason:
            // without a scope we cannot retain the repaired roster ahead of an
            // inbound replay. Fail the command rather than silently opening the
            // inbound lane.
            return Err(format!(
                "scoped event-sync unavailable after workspace apply: {error}"
            ));
        }
    }

    let restore_pending = state.managed_agent_restore_pending.load(Ordering::Acquire);

    // Transfer only the outer transaction guard to launch restoration. The
    // inner scope fence is released before restore's normal capture path; a
    // queued workspace still cannot mutate relay/identity until every restore
    // read and side effect has completed.
    #[cfg(feature = "mesh-llm")]
    {
        let restore_transaction_lock = apply_transaction_guard;
        let app = restore_app.clone();
        tauri::async_runtime::spawn(async move {
            complete_restore(
                restore_transaction_lock,
                apply_guard,
                app,
                restore_pending,
                |app, restore_pending| async move {
                    let state = app.state::<AppState>();
                    if restore_pending {
                        if let Err(error) =
                            crate::commands::mesh_llm::restore_mesh_sharing(&app, &state).await
                        {
                            eprintln!("buzz-desktop: failed to restore Share Compute: {error}");
                        }
                    }
                    crate::mesh_llm::publish_current_status_once(&app, "workspace apply").await;
                    if restore_pending {
                        restore_managed_agents_on_launch(&app, &state.shutdown_started).await
                    } else {
                        Ok(())
                    }
                },
            )
            .await;
        });
        return Ok(());
    }

    #[cfg(not(feature = "mesh-llm"))]
    if restore_pending {
        let restore_transaction_lock = apply_transaction_guard;
        let app = restore_app.clone();
        tauri::async_runtime::spawn(async move {
            complete_restore(
                restore_transaction_lock,
                apply_guard,
                app,
                restore_pending,
                |app, _| async move {
                    let state = app.state::<AppState>();
                    restore_managed_agents_on_launch(&app, &state.shutdown_started).await
                },
            )
            .await;
        });
        return Ok(());
    }

    assert_current_apply_generation(&state.workspace_apply_generation, apply_generation)?;

    drop(apply_guard);
    drop(apply_transaction_guard);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    use std::time::Duration;

    use super::{
        assert_current_apply_generation, begin_workspace_apply, complete_restore,
        next_apply_generation,
    };
    use tauri::Manager;

    #[test]
    fn explicit_newer_generation_supersedes_older_ticket() {
        let generation = AtomicU64::new(0);
        let older = next_apply_generation(&generation);
        let newer = next_apply_generation(&generation);

        let error = assert_current_apply_generation(&generation, older).unwrap_err();
        assert!(error.contains("superseded"), "{error}");
        assert_current_apply_generation(&generation, newer).unwrap();
    }

    #[tokio::test]
    async fn queued_apply_cannot_supersede_running_transaction_or_restore_phase() {
        let transaction_lock = Arc::new(tokio::sync::Mutex::new(()));
        let workspace_lock = Arc::new(tokio::sync::Mutex::new(()));
        let generation = Arc::new(AtomicU64::new(0));
        let (running_transaction_guard, running_workspace_guard, running_ticket) =
            begin_workspace_apply(
                Arc::clone(&transaction_lock),
                Arc::clone(&workspace_lock),
                &generation,
            )
            .await;

        let queued_transaction_lock = Arc::clone(&transaction_lock);
        let queued_workspace_lock = Arc::clone(&workspace_lock);
        let queued_generation = Arc::clone(&generation);
        let queued = tokio::spawn(async move {
            let (_transaction_guard, _workspace_guard, ticket) = begin_workspace_apply(
                queued_transaction_lock,
                queued_workspace_lock,
                &queued_generation,
            )
            .await;
            ticket
        });
        tokio::task::yield_now().await;

        // A queued workspace has not advanced the generation, so every awaited
        // phase of the running transaction, including one-shot launch restore,
        // remains authoritative while it holds the lock.
        assert_eq!(generation.load(Ordering::Acquire), running_ticket);
        assert_current_apply_generation(&generation, running_ticket).unwrap();
        assert!(!queued.is_finished());

        drop(running_workspace_guard);
        // The outer transaction guard still covers the restore phase, so a
        // queued apply cannot enter while the inner scope lock is available
        // for the normal owner-scope capture path.
        let capture_workspace_guard = workspace_lock.lock_owned().await;
        assert!(!queued.is_finished());
        drop(capture_workspace_guard);
        drop(running_transaction_guard);
        let queued_ticket = queued.await.unwrap();
        assert!(queued_ticket > running_ticket);
        assert_current_apply_generation(&generation, queued_ticket).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restore_handoff_runs_recovery_capture_and_holds_outer_transaction() {
        let identifier = format!(
            "xyz.nuncio.crew.test.workspace-{}",
            uuid::Uuid::new_v4().simple()
        );
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = identifier;
        let app = tauri::test::mock_builder()
            .manage(crate::app_state::build_app_state())
            .build(context)
            .unwrap();
        let state = app.state::<crate::app_state::AppState>();
        state
            .managed_agent_restore_pending
            .store(true, Ordering::Release);
        let (transaction_guard, workspace_guard, running_ticket) = begin_workspace_apply(
            Arc::clone(&state.workspace_apply_transaction_lock),
            Arc::clone(&state.workspace_apply_lock),
            &state.workspace_apply_generation,
        )
        .await;
        let (recovery_tx, recovery_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let recovery_app = app.handle().clone();
        let restore = tokio::spawn(async move {
            complete_restore(
                transaction_guard,
                workspace_guard,
                recovery_app,
                true,
                |app, _| async move {
                    let result = crate::app_state::owner_scope::capture(app).await;
                    let _ = recovery_tx.send(result.is_ok());
                    release_rx.await.unwrap();
                    result.map(|_| ())
                },
            )
            .await;
        });

        let queued_transaction_lock = Arc::clone(&state.workspace_apply_transaction_lock);
        let queued_workspace_lock = Arc::clone(&state.workspace_apply_lock);
        let queued_app = app.handle().clone();
        let mut queued = tokio::spawn(async move {
            let queued_state = queued_app.state::<crate::app_state::AppState>();
            let (_transaction_guard, _workspace_guard, ticket) = begin_workspace_apply(
                queued_transaction_lock,
                queued_workspace_lock,
                &queued_state.workspace_apply_generation,
            )
            .await;
            ticket
        });

        tokio::task::yield_now().await;
        assert!(
            matches!(futures_util::poll!(&mut queued), std::task::Poll::Pending),
            "queued apply must be pending on the held outer transaction"
        );
        let capture_succeeded = tokio::time::timeout(Duration::from_secs(3), recovery_rx)
            .await
            .expect("restore recovery must not wait on its released inner lock")
            .expect("restore recovery signal");
        assert!(capture_succeeded, "owner scope capture failed");
        assert_eq!(
            state.workspace_apply_generation.load(Ordering::Acquire),
            running_ticket
        );

        release_tx.send(()).unwrap();
        restore.await.unwrap();
        assert!(!state.managed_agent_restore_pending.load(Ordering::Acquire));
        let queued_ticket = tokio::time::timeout(Duration::from_secs(3), queued)
            .await
            .expect("queued apply must proceed after restore")
            .unwrap();
        assert!(queued_ticket > running_ticket);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restore_handoff_releases_outer_transaction_after_restore_failure() {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = format!(
            "xyz.nuncio.crew.test.workspace-{}",
            uuid::Uuid::new_v4().simple()
        );
        let app = tauri::test::mock_builder()
            .manage(crate::app_state::build_app_state())
            .build(context)
            .unwrap();
        let state = app.state::<crate::app_state::AppState>();
        state
            .managed_agent_restore_pending
            .store(true, Ordering::Release);
        let (transaction_guard, workspace_guard, running_ticket) = begin_workspace_apply(
            Arc::clone(&state.workspace_apply_transaction_lock),
            Arc::clone(&state.workspace_apply_lock),
            &state.workspace_apply_generation,
        )
        .await;
        let queued_transaction_lock = Arc::clone(&state.workspace_apply_transaction_lock);
        let queued_workspace_lock = Arc::clone(&state.workspace_apply_lock);
        let queued_app = app.handle().clone();
        let queued = tokio::spawn(async move {
            let queued_state = queued_app.state::<crate::app_state::AppState>();
            let (_transaction_guard, _workspace_guard, ticket) = begin_workspace_apply(
                queued_transaction_lock,
                queued_workspace_lock,
                &queued_state.workspace_apply_generation,
            )
            .await;
            ticket
        });

        complete_restore(
            transaction_guard,
            workspace_guard,
            app.handle().clone(),
            true,
            |app, _| async move {
                assert!(crate::app_state::owner_scope::capture(app).await.is_ok());
                Err("simulated restore failure".to_string())
            },
        )
        .await;

        let queued_ticket = tokio::time::timeout(Duration::from_secs(3), queued)
            .await
            .expect("queued apply must proceed after failed restore")
            .unwrap();
        assert!(queued_ticket > running_ticket);
        assert!(state.managed_agent_restore_pending.load(Ordering::Acquire));
    }
}

#[cfg(test)]
mod owner_scope_tests {
    use super::*;
    #[test]
    fn owner_scope_workspace_writer_takes_keys_before_relay() {
        let state = std::sync::Arc::new(crate::app_state::build_app_state());
        let writer = state.clone();
        let relay = state.relay_url_override.lock().unwrap();
        let (signal, received) = std::sync::mpsc::channel();
        let task = std::thread::spawn(move || {
            crate::app_state::owner_scope::signal_key_lock_to(signal);
            apply_workspace_identity(
                &writer,
                "wss://scope.example".into(),
                Some(Keys::generate()),
            )
            .unwrap();
        });
        let keys_acquired = received
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        let keys_held = state.keys.try_lock().is_err();
        drop(relay);
        task.join().unwrap();
        assert!(
            keys_acquired && keys_held,
            "writer must follow existing get_active_workspace keys-then-relay order"
        );
    }

    #[tokio::test]
    async fn owner_scope_workspace_origin_aba_invalidates_capture() {
        let state = crate::app_state::build_app_state();
        let capture = || {
            let guard = state.identity_mutation.lock().unwrap();
            state.capture_owner_scope(&guard).unwrap().token
        };
        {
            let transaction_lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
            let (_transaction, _workspace, _) = begin_workspace_apply(
                transaction_lock,
                state.workspace_apply_lock.clone(),
                &state.workspace_apply_generation,
            )
            .await;
            apply_workspace_identity(&state, "wss://scope-a.example".into(), None).unwrap();
        }
        let initial = capture();
        for origin in ["wss://scope-b.example", "wss://scope-a.example"] {
            let transaction_lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
            let (_transaction, _workspace, _) = begin_workspace_apply(
                transaction_lock,
                state.workspace_apply_lock.clone(),
                &state.workspace_apply_generation,
            )
            .await;
            apply_workspace_identity(&state, origin.into(), None).unwrap();
        }
        let after = capture();
        assert_eq!(initial.scope, after.scope);
        assert_eq!(initial.identity_generation, after.identity_generation);
        assert_ne!(initial, after);
    }

    #[tokio::test]
    async fn owner_scope_workspace_key_replacement_advances_identity_epoch() {
        let state = crate::app_state::build_app_state();
        let transaction_lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
        let (_transaction, _workspace, _) = begin_workspace_apply(
            transaction_lock,
            state.workspace_apply_lock.clone(),
            &state.workspace_apply_generation,
        )
        .await;
        let before = state.identity_generation.load(Ordering::Acquire);
        apply_workspace_identity(&state, "wss://scope.example".into(), Some(Keys::generate()))
            .unwrap();
        assert!(state.identity_generation.load(Ordering::Acquire) > before);
    }
}
