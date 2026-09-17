//! Two-identity privacy, desktop half: a private Ask publishes nothing.
//!
//! The assertion is taken from the production egress funnel, not from the
//! adapter's own report. Every relay-bound egress site in the desktop tree
//! calls [`crate::egress_guard::assert_no_key_backup`] — the `EVENTS_INVENTORY`
//! scan in `egress_guard_tests.rs` fails the build if a new site does not — so
//! a zero delta across a whole attempt is a complete census, covering EVENT
//! frames over HTTP `POST /events`, the native WebSocket send loop, and the
//! captured owner-operation transport alike.
//!
//! What would fail these tests: routing any part of an Ask through
//! `relay::submit_event*`, `relay::submit_signed_event_with_keys`, or the
//! native WebSocket send loop — each of those calls the guard, so the delta
//! stops being zero. Deleting the `fetch_add` in `egress_guard.rs` also fails
//! them, because the self-test below proves the counter is live.

#![cfg(unix)]

use super::tests::{canonical_tempdir, executable, fake_runtime, owned_receipt, request, state};
use super::*;
use crate::egress_guard::{assert_no_key_backup, relay_egress_attempts_on_this_thread};

/// A benign runtime that answers successfully, so the zero-publish assertion is
/// made across a *complete* attempt rather than an early refusal.
const ANSWERING_RUNTIME: &str = "#!/usr/bin/perl\nlocal $/; my $in = <STDIN>; die \"no prompt\" unless defined $in; print '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"Scoped answer\",\"modelUsage\":{\"claude-fable-5-1\":{}}}';\n";

/// Attribution is thread-scoped, not process-global: a parallel test doing a
/// real relay egress would otherwise inflate this delta and flake the suite.
/// `PrivateAskAttempt::run` is synchronous and spawns no task, so all of its
/// work — including any future polled with `block_on` — runs on this thread.
///
/// Limit: a detached `spawn` from inside `run()` would not be attributed here.
/// No `spawn` exists under `private_ask/`; adding one is a privacy-boundary
/// change that needs its own proof.
fn egress_during<T>(operation: impl FnOnce() -> T) -> (T, u64) {
    let before = relay_egress_attempts_on_this_thread();
    let value = operation();
    (
        value,
        relay_egress_attempts_on_this_thread().saturating_sub(before),
    )
}

/// Guards the guard: if this fails, every zero-delta assertion below is
/// vacuous.
#[test]
fn the_egress_counter_observes_a_guarded_relay_boundary() {
    let (_, observed) = egress_during(|| {
        assert_no_key_backup("an ordinary event body", "private ask counter self-test")
    });
    assert_eq!(
        observed, 1,
        "the relay egress counter must observe a guarded boundary; \
         without it a zero-publish assertion proves nothing"
    );
}

#[test]
fn a_complete_private_ask_performs_no_relay_egress() {
    let fixture = canonical_tempdir();
    let path = fake_runtime(fixture.path(), "claude", ANSWERING_RUNTIME);
    let mut selected = state(&path, "claude", "claude-fable-5-1", None);
    selected.executable = executable(&path);
    let capability = PrivateAskCapability::verified_for_fixture(&selected);
    let admission = admit_private_ask(request(), selected, capability).expect("admission");
    let ownership = owned_receipt(&fixture);
    let attempt = PrivateAskAttempt::create(admission, ownership, 1).expect("attempt");

    let (outcome, observed) = egress_during(|| attempt.run());

    // On a host with the containment boundary the run completes; on a host
    // without it the attempt is refused before spawn. Either way it publishes
    // nothing, so the privacy claim does not depend on the platform.
    match &outcome {
        Ok(response) => assert!(!response.markdown.is_empty()),
        Err(failure) => assert!(
            matches!(failure, PrivateAskFailure::ProcessContainmentUnverified),
            "unexpected refusal: {failure:?}"
        ),
    }
    assert_eq!(
        observed, 0,
        "a private Ask must publish no Nostr event of any kind and perform \
         no relay write from the desktop's own identity"
    );
}

#[test]
fn a_refused_private_ask_performs_no_relay_egress() {
    let fixture = canonical_tempdir();
    // Never created: admission succeeds, the spawn cannot.
    let path = fixture.path().join("absent-runtime");
    let mut selected = state(&path, "claude", "claude-fable-5-1", None);
    selected.executable = executable(&path);
    let capability = PrivateAskCapability::verified_for_fixture(&selected);
    let admission = admit_private_ask(request(), selected, capability).expect("admission");
    let ownership = owned_receipt(&fixture);
    let attempt = PrivateAskAttempt::create(admission, ownership, 1).expect("attempt");

    let (outcome, observed) = egress_during(|| attempt.run());

    assert!(outcome.is_err(), "an absent runtime must not answer");
    assert_eq!(
        observed, 0,
        "a failing private Ask must not report, log or mirror its failure to the relay"
    );
}
