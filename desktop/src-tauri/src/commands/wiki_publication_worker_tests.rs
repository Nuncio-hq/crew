//! Foreground/worker arbitration for the native Wiki recovery loop.

use super::*;
use crate::app_state::build_app_state;
use crate::app_state::owner_scope::OwnerScopeToken;
use crate::commands::wiki_publication_record::WikiPublicationLease;
use crate::commands::wiki_publication_test_fixture as fixture;
use crate::owner_operations::{OperationScope, StoreError};
use std::sync::atomic::AtomicUsize;

fn app() -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app")
}

fn token(owner: char, community: &str, workspace: u64, identity: u64) -> OwnerScopeToken {
    OwnerScopeToken {
        scope: OperationScope {
            owner: owner.to_string().repeat(64),
            community: community.into(),
        },
        workspace_generation: workspace,
        identity_generation: identity,
    }
}

#[test]
fn worker_reservation_binds_owner_community_workspace_and_identity_generation() {
    let app = app();
    let handle = app.handle().clone();
    let base = token('a', "https://one.example", 1, 1);
    reserve(&handle, &base, "30617:a:repo");

    assert!(is_reserved(&handle, &base, "30617:a:repo"));
    for other in [
        token('b', "https://one.example", 1, 1),
        token('a', "https://two.example", 1, 1),
        token('a', "https://one.example", 2, 1),
        token('a', "https://one.example", 1, 2),
    ] {
        assert!(
            !is_reserved(&handle, &other, "30617:a:repo"),
            "a reservation must not cross an owner, community, or generation change"
        );
    }
    assert!(!is_reserved(&handle, &base, "30617:a:other"));

    release(&handle, &base, "30617:a:repo");
    assert!(!is_reserved(&handle, &base, "30617:a:repo"));
}

#[test]
fn worker_reservation_map_is_bounded_by_count_not_only_expiry() {
    let app = app();
    let handle = app.handle().clone();
    let total = MAX_RESERVATIONS * 3;
    // Every reservation here is well inside its expiry window, so only the
    // count bound can keep the map from growing without limit.
    for index in 0..total {
        reserve(
            &handle,
            &token('a', "https://one.example", index as u64, 1),
            "30617:a:repo",
        );
    }
    let held = ensure_state(&handle)
        .foreground
        .lock()
        .expect("reservation lock")
        .len();
    assert!(
        held <= MAX_RESERVATIONS,
        "reservation map grew to {held} entries"
    );
    // The newest reservation is the one its caller still depends on.
    assert!(is_reserved(
        &handle,
        &token('a', "https://one.example", (total - 1) as u64, 1),
        "30617:a:repo"
    ));
}

/// After five unrecorded failures the loop parks on `wake` or a 60 s timer.
/// An explicit foreground operation is the user's way back, and it must work
/// when the storage failure clears without any owner/workspace generation
/// change — i.e. while nothing is waiting on the signal yet.
#[tokio::test]
async fn worker_explicit_reservation_wakes_the_bounded_failure_window() {
    let app = app();
    let handle = app.handle().clone();
    let claim = token('a', "https://one.example", 1, 1);
    let worker = ensure_state(&handle);

    // Nothing is awaiting the signal at this point: this is exactly the
    // terminal-branch case, where the loop has not yet polled `notified()`.
    reserve(&handle, &claim, "30617:a:repo");
    tokio::time::timeout(Duration::from_secs(5), worker.wake.notified())
        .await
        .expect("an explicit retry must release a sleeping recovery loop");

    // The permit is consumed once. An automatic scan reads reservations but
    // never creates them, so it cannot wake itself into a refresh loop.
    assert!(
        tokio::time::timeout(Duration::from_millis(50), worker.wake.notified())
            .await
            .is_err(),
        "waking must not repeat without a new foreground operation"
    );

    // Waking changes nothing about the claim it announces.
    assert!(is_reserved(&handle, &claim, "30617:a:repo"));
    assert!(!is_reserved(
        &handle,
        &token('a', "https://one.example", 1, 2),
        "30617:a:repo"
    ));
    assert!(!is_reserved(&handle, &claim, "30617:a:other"));

    // The signal is independent of dispatch serialization: reserving neither
    // takes nor holds the mutex that orders foreground and recovery work.
    drop(
        worker
            .dispatch
            .try_lock()
            .expect("reserving must not hold the dispatch lock"),
    );

    // A capped burst still bounds the map and still leaves one usable signal.
    for index in 0..(MAX_RESERVATIONS * 2) {
        reserve(
            &handle,
            &token('a', "https://one.example", index as u64, 1),
            "30617:a:repo",
        );
    }
    assert!(
        worker.foreground.lock().expect("reservation lock").len() <= MAX_RESERVATIONS,
        "waking must not defeat the reservation count bound"
    );
    tokio::time::timeout(Duration::from_secs(5), worker.wake.notified())
        .await
        .expect("a later foreground operation wakes the loop again");
}

#[tokio::test]
async fn worker_serialized_dispatch_and_recovery_never_overlap() {
    let app = app();
    let handle = app.handle().clone();
    let concurrent = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let handle = handle.clone();
        let concurrent = concurrent.clone();
        let peak = peak.clone();
        tasks.push(tokio::spawn(async move {
            serialized(&handle, async {
                let inside = concurrent.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(inside, Ordering::SeqCst);
                tokio::task::yield_now().await;
                tokio::time::sleep(Duration::from_millis(2)).await;
                concurrent.fetch_sub(1, Ordering::SeqCst);
                Ok::<(), String>(())
            })
            .await
        }));
    }
    for task in tasks {
        task.await.expect("serialized task").expect("dispatch");
    }
    assert_eq!(
        peak.load(Ordering::SeqCst),
        1,
        "a foreground dispatch and the recovery worker must never run together"
    );
}

/// A relay failure the dispatcher already persisted is progress; a failure it
/// could not persist is not, and must not be reported as a successful tick.
#[test]
fn worker_unrecorded_row_failure_ends_the_tick_and_does_not_reset_the_failure_window() {
    let keys = nostr::Keys::generate();
    let coordinate = fixture::coordinate(&keys, "crew");
    let record = fixture::record(
        fixture::publication(&keys, "crew", None),
        &coordinate,
        &keys,
    );
    let saved = fixture::operation(
        OperationScope {
            owner: keys.public_key().to_hex(),
            community: "https://one.example".into(),
        },
        uuid::Uuid::new_v4().to_string(),
        &coordinate,
        &record,
        100,
    );

    // `drive` returns the durably saved row for an ordinary relay failure, so
    // the scan counts it and keeps going.
    let mut processed = 0;
    assert!(record_row_outcome(Ok(saved), &mut processed).is_ok());
    assert_eq!(processed, 1);

    // A refused journal write leaves no durable retry record. It ends the
    // tick, is never counted as progress, and keeps its exact reason.
    let busy = StoreError::Busy.to_string();
    let mut processed = 0;
    let tick = record_row_outcome(Err(busy.clone()), &mut processed);
    assert_eq!(processed, 0, "an unrecorded failure is not progress");
    assert_eq!(tick.as_ref().err(), Some(&busy));

    // The loop's bounded window only clears on a tick with no unrecorded
    // failure; an unrecorded one raises it up to the terminal state that waits
    // for an explicit wake.
    let failed: Result<(), String> = Err(busy);
    let mut failures = 0_u8;
    for expected in 1..=MAX_FAILURES {
        failures = next_failure_window(failures, &failed);
        assert_eq!(failures, expected);
    }
    assert_eq!(
        next_failure_window(u8::MAX, &failed),
        u8::MAX,
        "the failure window saturates instead of wrapping"
    );
    assert_eq!(
        next_failure_window(MAX_FAILURES, &Ok(())),
        0,
        "only a tick with no unrecorded failure clears the window"
    );

    assert!(
        backoff_secs(0) < backoff_secs(3),
        "consecutive unrecorded failures back off"
    );
    assert_eq!(
        backoff_secs(MAX_FAILURES),
        backoff_secs(u8::MAX),
        "the backoff is bounded"
    );
    assert!(backoff_secs(u8::MAX) <= 300);
}

#[test]
fn worker_due_decision_skips_claimed_cancelled_and_terminal_rows() {
    let keys = nostr::Keys::generate();
    let coordinate = fixture::coordinate(&keys, "crew");
    let scope = OperationScope {
        owner: keys.public_key().to_hex(),
        community: "https://one.example".into(),
    };
    let record = fixture::record(
        fixture::publication(&keys, "crew", None),
        &coordinate,
        &keys,
    );
    let operation = fixture::operation(
        scope,
        uuid::Uuid::new_v4().to_string(),
        &coordinate,
        &record,
        100,
    );
    assert!(is_due(&operation, &record, 100));

    let mut leased = record.clone();
    leased.lease = Some(WikiPublicationLease {
        worker_id: uuid::Uuid::new_v4().to_string(),
        expires_at: 160,
    });
    assert!(
        !is_due(&operation, &leased, 100),
        "a live lease belongs to another worker"
    );
    assert!(
        is_due(&operation, &leased, 200),
        "an expired lease must not strand the row"
    );

    let mut cancelled = record.clone();
    cancelled.cancel_requested = true;
    assert!(!is_due(&operation, &cancelled, 100));

    let mut read_only = record.clone();
    read_only.reconcile_only = true;
    assert!(!is_due(&operation, &read_only, 100));

    let mut exhausted = record.clone();
    exhausted.attempts = MAX_FAILURES;
    assert!(!is_due(&operation, &exhausted, 100));

    let mut waiting = record.clone();
    waiting.retry_at = 400;
    assert!(!is_due(&operation, &waiting, 100));
    assert!(is_due(&operation, &waiting, 400));

    let mut reconciled = operation.clone();
    reconciled.reconciled = true;
    reconciled.status = OperationStatus::Complete;
    assert!(!is_due(&reconciled, &record, 100));

    for status in [
        OperationStatus::Complete,
        OperationStatus::Canceled,
        OperationStatus::Superseded,
    ] {
        let mut terminal = operation.clone();
        terminal.status = status;
        assert!(!is_due(&terminal, &record, 100));
    }
}
