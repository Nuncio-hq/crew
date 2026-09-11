//! App-lifetime worker for durable Wiki publication recovery.
//!
//! The owner-operation journal is written before relay effects.  This worker
//! is consequently the restart path: it scans only the active native scope,
//! resumes due rows, and leaves read-only or terminal rows available for an
//! explicit renderer recovery action.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use super::owner_operations::owner_operation_load;
use super::wiki_publication_commands::wiki_operation_summaries;
use super::wiki_publication_driver::drive;
use super::wiki_publication_record::WikiPublicationRecord;
use super::wiki_publication_runtime::now;
use super::wiki_publication_runtime::NativeWikiPublication;
use crate::app_state::owner_scope::capture;
use crate::owner_operations::{Operation, OperationStatus};
use nostr::PublicKey;
use tauri::{AppHandle, Manager};

const MAX_FAILURES: u8 = 5;
const MAX_PER_TICK: usize = 5;
/// Foreground reservations are advisory and short-lived. Bounding the map by
/// count as well as by expiry keeps a burst of prepares under a changing
/// identity/workspace generation from growing it without limit.
const MAX_RESERVATIONS: usize = 64;
const RESERVATION_TTL: Duration = Duration::from_secs(15);

#[derive(Default)]
struct Worker {
    running: AtomicBool,
    wake: tokio::sync::Notify,
    /// Serialize foreground dispatch and startup recovery. A prepared job is
    /// reserved briefly so the renderer's immediate dispatch owns it before
    /// the restart worker can claim the same revision.
    dispatch: tokio::sync::Mutex<()>,
    foreground: std::sync::Mutex<std::collections::HashMap<String, Instant>>,
}

/// Install the shared reservation/serialization state exactly once.
///
/// Reservation and serialization are meaningful before the recovery loop is
/// spawned, so the state is installed independently of `start`.
fn ensure_state<R: tauri::Runtime>(app: &AppHandle<R>) -> Arc<Worker> {
    if app.try_state::<Arc<Worker>>().is_none() {
        app.manage(Arc::new(Worker::default()));
    }
    app.state::<Arc<Worker>>().inner().clone()
}

/// Start the app-lifetime Wiki recovery loop exactly once.
pub(crate) fn start(app: AppHandle) {
    let worker = ensure_state(&app);
    if worker.running.swap(true, Ordering::AcqRel) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut failures = 0_u8;
        let mut last_scope = None;
        loop {
            let scope = capture(app.clone()).await.map(|captured| captured.token);
            if let Ok(scope) = &scope {
                if last_scope.as_ref() != Some(scope) {
                    failures = 0;
                    last_scope = Some(scope.clone());
                }
            }
            if failures >= MAX_FAILURES {
                // A broken storage or relay must not create an unbounded
                // request loop. A user mutation or scope change wakes it.
                tokio::select! {
                    _ = worker.wake.notified() => failures = 0,
                    _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                }
                continue;
            }
            let result = match scope {
                Ok(scope) => run_due(app.clone(), scope).await,
                Err(error) => Err(error),
            };
            if let Err(error) = &result {
                eprintln!("buzz-desktop: wiki recovery worker: {error}");
            }
            failures = next_failure_window(failures, &result);
            let delay = backoff_secs(failures);
            tokio::select! {
                _ = worker.wake.notified() => failures = 0,
                _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
            }
        }
    });
}

/// Wake the running worker after a new or changed journal row is committed.
pub(crate) fn wake<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(worker) = app.try_state::<Arc<Worker>>() {
        worker.wake.notify_one();
    }
}

fn reservation_key(
    scope: &crate::app_state::owner_scope::OwnerScopeToken,
    resource_key: &str,
) -> String {
    format!(
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
        scope.scope.owner,
        scope.scope.community,
        scope.workspace_generation,
        scope.identity_generation,
        resource_key,
    )
}

/// Claim one resource for a foreground command and wake the recovery loop.
///
/// Every explicit user entry point reserves before it acts: prepare, cadence,
/// regenerate, dispatch, retry and reconcile. That makes this the one seam
/// where "the user is doing something about this Wiki" is known, so it is also
/// where the loop's bounded failure window is released. Without it, a storage
/// failure that clears without an owner/workspace generation change leaves the
/// worker asleep in its terminal branch: the user's Retry runs once directly
/// and its renewed automatic-retry window never resumes.
///
/// The recovery loop only ever *reads* reservations, so it cannot wake itself
/// and this cannot become a self-amplifying refresh loop. The signal is a
/// `Notify` permit, not a poll: it is durable when no task is waiting yet, and
/// is consumed by exactly one `notified()`.
pub(crate) fn reserve<R: tauri::Runtime>(
    app: &AppHandle<R>,
    scope: &crate::app_state::owner_scope::OwnerScopeToken,
    resource_key: &str,
) {
    let worker = ensure_state(app);
    if let Ok(mut reserved) = worker.foreground.lock() {
        let now = Instant::now();
        reserved.retain(|_, expires| *expires > now);
        let key = reservation_key(scope, resource_key);
        // Evicting the soonest-to-expire reservation is safe: a lost
        // reservation only lets the recovery worker try the same resource,
        // and both paths still serialize on `dispatch` and re-read the
        // durable row before acting.
        while !reserved.contains_key(&key) && reserved.len() >= MAX_RESERVATIONS {
            let Some(oldest) = reserved
                .iter()
                .min_by_key(|(_, expires)| **expires)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            reserved.remove(&oldest);
        }
        reserved.insert(key, now + RESERVATION_TTL);
    }
    // Signal after the reservation is visible and its guard is dropped, so a
    // woken scan sees the claim and never contends for that lock. This takes
    // no part in `dispatch` serialization; the foreground caller still owns
    // the row until it releases.
    worker.wake.notify_one();
}

pub(crate) fn release<R: tauri::Runtime>(
    app: &AppHandle<R>,
    scope: &crate::app_state::owner_scope::OwnerScopeToken,
    resource_key: &str,
) {
    if let Some(worker) = app.try_state::<Arc<Worker>>() {
        if let Ok(mut reserved) = worker.foreground.lock() {
            reserved.remove(&reservation_key(scope, resource_key));
        }
    }
}

fn is_reserved<R: tauri::Runtime>(
    app: &AppHandle<R>,
    scope: &crate::app_state::owner_scope::OwnerScopeToken,
    resource_key: &str,
) -> bool {
    let Some(worker) = app.try_state::<Arc<Worker>>() else {
        return false;
    };
    let Ok(mut reserved) = worker.foreground.lock() else {
        return true;
    };
    let now = Instant::now();
    reserved.retain(|_, expires| *expires > now);
    reserved.contains_key(&reservation_key(scope, resource_key))
}

/// Serialize a foreground dispatch with the app-lifetime recovery worker.
pub(crate) async fn serialized<R: tauri::Runtime, T, F>(
    app: &AppHandle<R>,
    action: F,
) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    let worker = ensure_state(app);
    let _guard = worker.dispatch.lock().await;
    action.await
}

#[cfg(test)]
#[path = "wiki_publication_worker_tests.rs"]
mod tests;

/// Fold one row's dispatch outcome into this tick.
///
/// `drive` persists an ordinary relay failure itself and returns the saved
/// row, so reaching `Err` here means **no durable retry record was written**:
/// a busy or refused journal write, a lost CAS, a stale owner/workspace scope.
/// Such a failure ends the tick and reaches the loop's bounded failure window.
/// Logging it and counting the row as processed would report a successful
/// tick, reset that window, and let a broken journal retry without bound.
///
/// The per-row five-attempt bound is unaffected: it lives in the durable
/// record that [`is_due`] reads, not in this counter.
fn record_row_outcome(
    outcome: Result<Operation, String>,
    processed: &mut usize,
) -> Result<(), String> {
    outcome?;
    *processed = processed.saturating_add(1);
    Ok(())
}

/// Fold one tick result into the loop's bounded failure window. Only a tick
/// with no unrecorded failure clears it.
fn next_failure_window(failures: u8, tick: &Result<(), String>) -> u8 {
    match tick {
        Ok(()) => 0,
        Err(_) => failures.saturating_add(1),
    }
}

/// Bounded exponential backoff before the next tick.
fn backoff_secs(failures: u8) -> u64 {
    (5_u64 << failures.min(5)).min(300)
}

/// Decide, from bounded journal metadata only, whether this row is due.
///
/// The signed graph is deliberately *not* verified here. A recovery tick runs
/// every few seconds; re-verifying every page signature of every retained
/// publication on each tick makes idle cost scale with Wiki size. `drive`
/// still validates the complete graph before it sends anything.
fn is_due(current: &Operation, record: &WikiPublicationRecord, now: i64) -> bool {
    !(record.cancel_requested
        || record.reconcile_only
        || record.attempts >= MAX_FAILURES
        || record.retry_at > now
        || record
            .lease
            .as_ref()
            .is_some_and(|lease| lease.expires_at > now)
        || current.reconciled
        || matches!(
            current.status,
            OperationStatus::Canceled | OperationStatus::Superseded | OperationStatus::Complete
        ))
}

async fn run_due(
    app: AppHandle,
    scope: crate::app_state::owner_scope::OwnerScopeToken,
) -> Result<(), String> {
    // Only bounded metadata is read outside the serialized lock: one summary
    // per repository coordinate. Signed payloads are loaded for the rows this
    // tick actually considers, after the lock is held.
    let summaries = wiki_operation_summaries(app.clone(), scope.clone(), false).await?;
    let mut processed = 0;
    for summary in summaries {
        if processed >= MAX_PER_TICK {
            break;
        }
        if summary.reconciled {
            continue;
        }
        if is_reserved(&app, &scope, &summary.resource_key) {
            continue;
        }
        let result = {
            let Some(worker) = app.try_state::<Arc<Worker>>() else {
                return Err("Wiki recovery worker state is unavailable.".into());
            };
            let _guard = worker.dispatch.lock().await;
            if is_reserved(&app, &scope, &summary.resource_key) {
                continue;
            }
            // The summary was read before waiting for the foreground mutex.
            // Reload the row after acquiring it so a foreground CAS cannot be
            // followed by a stale worker dispatch for the same resource.
            let current =
                owner_operation_load(app.clone(), scope.clone(), summary.id.clone(), None)
                    .await?
                    .value;
            if current.resource_key != summary.resource_key {
                return Err("Wiki recovery row changed coordinate; reload.".into());
            }
            let owner = match PublicKey::from_hex(&current.scope.owner) {
                Ok(owner) => owner,
                Err(_) => return Err("Wiki recovery row has an invalid owner.".into()),
            };
            let record: WikiPublicationRecord =
                match serde_json::from_value(current.payload.clone()) {
                    Ok(record) => record,
                    Err(_) => return Err("Invalid Wiki recovery row payload.".into()),
                };
            if let Err(error) = record.validate_projection(owner) {
                return Err(format!("Invalid Wiki recovery row: {error}"));
            }
            if current.resource_key != record.coordinate {
                return Err("Wiki recovery row does not match its signed coordinate.".into());
            }
            if !is_due(&current, &record, now()?) {
                continue;
            }
            let runtime =
                NativeWikiPublication::new(app.clone(), scope.clone(), &current.resource_key)
                    .await?;
            drive(&runtime, current, owner, false, false).await
        };
        // Every unrecorded failure ends the tick and reaches the outer bounded
        // failure/backoff window; the dispatcher's own durable failures come
        // back as `Ok(row)` and keep the scan going.
        record_row_outcome(result, &mut processed)?;
    }
    Ok(())
}
