use super::*;
use buzz_core_pkg::transport_status::{TransportCode, TransportState};

fn ticket(index: usize) -> ReadTicket {
    ReadTicket {
        key: ManagedAgentRuntimeKey::new(format!("{index:064x}"), "ws://fixture").unwrap(),
        nonce: uuid::Uuid::new_v4().simple().to_string(),
        path: PathBuf::from("/fixture/status.json"),
        owner: "a".repeat(64),
        epoch: 0,
    }
}
fn record(ticket: &ReadTicket, terminal: bool) -> TransportRecord {
    TransportRecord {
        version: 1,
        runtime_id: ticket.key.runtime_id(),
        start_nonce: ticket.nonce.clone(),
        sequence: 1,
        timestamp_ms: 100_000,
        terminal,
        transport: TransportStatus {
            state: if terminal {
                TransportState::Exhausted
            } else {
                TransportState::Connected
            },
            code: TransportCode::None,
            attempts: 0,
            elapsed_ms: 0,
            next_retry_at_ms: None,
            last_error: None,
        },
    }
}

#[test]
fn late_read_cannot_recache_after_disable_or_reenable() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = ticket(1);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    assert!(monitor.disable());
    assert!(!monitor.disable(), "repeated leave must be idempotent");
    assert!(!monitor.apply(
        &ticket,
        Ok(record(&ticket, false)),
        true,
        Instant::now(),
        100_000
    ));
    assert_eq!(monitor.status().state, TransportState::Unknown);
    assert!(monitor.enable(&ticket.owner).unwrap());
    assert!(
        !monitor.enable(&ticket.owner).unwrap(),
        "repeated rejoin must be idempotent"
    );
    assert!(!monitor.apply(
        &ticket,
        Ok(record(&ticket, false)),
        true,
        Instant::now(),
        100_000
    ));
    assert_eq!(monitor.status().state, TransportState::Unknown);
    let current = monitor.snapshot().unwrap();
    assert!(monitor.apply(
        &current,
        Ok(record(&current, false)),
        true,
        Instant::now(),
        100_000
    ));
}

#[test]
fn owner_change_cannot_reenable_previous_owners_generation() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket(1), &diagnostics);
    monitor.disable();
    assert!(monitor.enable(&"b".repeat(64)).is_err());
    assert!(monitor.snapshot().is_none());
}

#[test]
fn stopped_process_cannot_accept_live_record_even_for_matching_nonce() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = ticket(1);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    assert!(!monitor.apply(
        &ticket,
        Ok(record(&ticket, false)),
        false,
        Instant::now(),
        100_000
    ));
    assert_eq!(monitor.status().state, TransportState::Unknown);
}

#[test]
fn retirement_cache_is_bounded_and_expires_monotonically() {
    let now = Instant::now();
    let mut cache = Diagnostics::default();
    for index in 0..257 {
        cache.retire(
            ticket(index),
            true,
            now + Duration::from_secs(index as u64),
            TransportLease::default(),
        );
    }
    assert_eq!(cache.entries.len(), RETIRED_CAPACITY);
    assert!(!cache.entries.contains_key(&ticket(0).key));
    cache.prune(now + Duration::from_secs(256) + RETIRED_TTL);
    assert!(cache.entries.is_empty());
}

#[test]
fn clear_or_replacement_invalidates_a_retired_read_token() {
    let now = Instant::now();
    let old = ticket(1);
    let mut cache = Diagnostics::default();
    cache.retire(old.clone(), true, now, TransportLease::default());
    cache.clear_key(&old.key);
    assert!(!cache.apply(&old, record(&old, true), false, now, 100_000));
    let replacement = ticket(1);
    cache.retire(replacement, true, now, TransportLease::default());
    assert!(!cache.apply(&old, record(&old, true), false, now, 100_000));
}

#[test]
fn retired_read_cannot_overwrite_new_live_generation_or_outlive_window() {
    let now = Instant::now();
    let old = ticket(1);
    let mut cache = Diagnostics::default();
    cache.retire(old.clone(), true, now, TransportLease::default());
    assert!(!cache.apply(&old, record(&old, true), true, now, 100_000));
    assert!(cache.pending(now + FINAL_READ_WINDOW).is_empty());
    assert!(!cache.apply(
        &old,
        record(&old, true),
        false,
        now + FINAL_READ_WINDOW,
        100_000
    ));
}

#[test]
fn disabled_generation_cannot_create_retirement_metadata() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket(1), &diagnostics);
    monitor.disable();
    monitor.retire(true, Instant::now());
    assert!(diagnostics.lock().unwrap().entries.is_empty());
}
