//! What the attempt registry must get right for a cancel to mean anything.
//!
//! Deleting the duplicate check, the bound or the `Drop` removal from
//! `cancel_registry.rs` fails one of these.

use super::{PrivateAskAttempts, RegisterFailure, MAX_IN_FLIGHT_ASKS};

#[test]
fn a_registered_attempt_is_cancelled_through_its_own_flag() {
    let registry = PrivateAskAttempts::default();
    let registration = registry.register("attempt-a").expect("register");
    assert!(!registration.is_cancelled());
    assert!(registry.cancel("attempt-a"), "a live attempt is signalled");
    assert!(registration.is_cancelled());
    // The flag the attempt itself polls is the very same one.
    assert!(registration
        .cancel_flag()
        .load(std::sync::atomic::Ordering::Acquire));
}

#[test]
fn cancelling_an_unknown_or_finished_attempt_is_a_no_op() {
    let registry = PrivateAskAttempts::default();
    assert!(!registry.cancel("never-registered"));
    let registration = registry.register("attempt-a").expect("register");
    drop(registration);
    assert_eq!(registry.in_flight(), 0, "a finished attempt is released");
    assert!(
        !registry.cancel("attempt-a"),
        "an id whose run already returned cannot be cancelled again"
    );
}

#[test]
fn the_same_attempt_id_cannot_be_registered_twice() {
    let registry = PrivateAskAttempts::default();
    let _first = registry.register("attempt-a").expect("register");
    assert_eq!(
        registry.register("attempt-a").err(),
        Some(RegisterFailure::AlreadyRunning),
        "two runs sharing one flag would let either cancel the other"
    );
}

#[test]
fn the_number_of_in_flight_attempts_is_bounded() {
    let registry = PrivateAskAttempts::default();
    let mut held = Vec::new();
    for index in 0..MAX_IN_FLIGHT_ASKS {
        held.push(
            registry
                .register(&format!("attempt-{index}"))
                .expect("register within the bound"),
        );
    }
    assert_eq!(
        registry.register("attempt-one-past").err(),
        Some(RegisterFailure::TooManyRunning)
    );
    // The bound is a live-attempt bound, not a lifetime quota: releasing one
    // makes room for the next.
    held.pop();
    let _next = registry
        .register("attempt-one-past")
        .expect("a released slot is reusable");
    assert_eq!(registry.in_flight(), MAX_IN_FLIGHT_ASKS);
}

#[test]
fn a_registration_is_released_even_when_its_run_panicked() {
    let registry = PrivateAskAttempts::default();
    let clone = registry.clone();
    let panicked = std::thread::spawn(move || {
        let _registration = clone.register("attempt-a").expect("register");
        panic!("the run failed");
    })
    .join();
    assert!(panicked.is_err(), "the fixture must really have panicked");
    assert_eq!(
        registry.in_flight(),
        0,
        "a panicking run must not strand its id in flight"
    );
    registry
        .register("attempt-a")
        .expect("the id is claimable again");
}
