//! A receipt must survive a restart without becoming a way to invent evidence.

#![cfg(all(unix, target_os = "macos"))]

use super::super::capability::PROBE_MAX_AGE;
use super::super::probe_run::{capture_probe, ProbeContext};
use super::super::tests::{canonical_tempdir, captured_now, state};
use super::super::{PrivateAskCapability, ProofStatus, SelectedAgentState};
use super::*;
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

struct Fixture {
    _home: tempfile::TempDir,
    _installation: tempfile::TempDir,
    ownership: VerifiedStagingOwnership,
    selected: SelectedAgentState,
}

fn fixture() -> Fixture {
    let home = canonical_tempdir();
    let installation = canonical_tempdir();
    let ownership = VerifiedStagingOwnership::for_test(home.path()).expect("staging ownership");
    // The runtime lives outside the staging base, as a real installation does:
    // a runtime above the app-data tree would have the policy's runtime read
    // allowance cover the probe's own sentinel.
    let runtime = installation.path().join("claude");
    std::fs::write(&runtime, b"#!/usr/bin/perl\nexit 0;\n").expect("runtime");
    let selected = state(&runtime, "claude", "claude-fable-5-1", None);
    Fixture {
        _home: home,
        _installation: installation,
        ownership,
        selected,
    }
}

fn capture(fixture: &Fixture, captured_at: u64) -> PrivateAskProbe {
    capture_probe(ProbeContext {
        state: &fixture.selected,
        ownership: &fixture.ownership,
        session: None,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        now: captured_at,
    })
    .expect("probe")
}

/// The one receipt file this fixture wrote.
fn receipt_file(fixture: &Fixture) -> std::path::PathBuf {
    let base = fixture
        .ownership
        .private_ask_probe_base()
        .expect("receipt base");
    std::fs::read_dir(&base)
        .expect("receipt directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .expect("receipt file")
}

/// The point of persisting anything: a later process reads back the same
/// evidence and projects the same capability, without probing again.
#[test]
fn a_stored_receipt_is_read_back_as_the_same_evidence() {
    let fixture = fixture();
    let probe = capture(&fixture, now());
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    let loaded = load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .expect("receipt");

    let from_fresh =
        PrivateAskCapability::from_probe(&fixture.selected, probe, captured_now()).expect("fresh");
    let from_receipt = PrivateAskCapability::from_probe(&fixture.selected, loaded, captured_now())
        .expect("loaded");
    // Every dimension a receipt CAN carry is identical. Independence is the one
    // it deliberately cannot, so it is not compared here.
    assert_eq!(
        from_receipt.process_containment,
        from_fresh.process_containment
    );
    assert_eq!(from_receipt.tool_isolation, from_fresh.tool_isolation);
    assert_eq!(from_receipt.read_bounded, from_fresh.read_bounded);
    assert_eq!(from_receipt.egress_bounded, from_fresh.egress_bounded);
    assert_eq!(from_receipt.side_effect_free, from_fresh.side_effect_free);
    assert_eq!(from_receipt.authentication, from_fresh.authentication);
    assert_eq!(
        from_receipt.process_containment,
        ProofStatus::Verified,
        "the round trip must carry real evidence, or this proves nothing"
    );
}

/// A record that hit its own recorded-target cap proved nothing, and must not
/// come back from a receipt looking complete.
///
/// Production line: the `truncated` field of the `EgressDocument` built in
/// `ReceiptDocument::from_probe`. Writing a constant there — which it did —
/// turns an incomplete observation into a certifying one on the next restart,
/// and the round-trip test above cannot see it because its own record is not
/// truncated.
#[test]
fn a_truncated_egress_record_stays_truncated_through_a_receipt() {
    let fixture = fixture();
    let mut probe = capture(&fixture, now());
    let digest = probe.probe_program_digest.clone();
    let complete = probe.egress.clone();
    assert!(
        complete.bounds_egress(),
        "the captured record must certify, or this test cannot show the loss"
    );
    probe.egress = super::super::egress_proxy::EgressObservation::from_parts(
        complete.provider_host().to_owned(),
        complete.proxy_port(),
        complete.accepted().to_vec(),
        complete
            .refused()
            .iter()
            .map(|refused| (refused.target().to_owned(), refused.reason()))
            .collect(),
        complete.dial_failures(),
        true,
        complete.direct_connections(),
    );
    store(&fixture.ownership, &probe).expect("store");

    let loaded = load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .expect("receipt");

    assert!(
        loaded.egress.truncated(),
        "truncation must survive the round trip"
    );
    assert!(
        !loaded.egress.bounds_egress(),
        "a truncated record must not certify a bounded egress"
    );
    let capability = PrivateAskCapability::from_probe(&fixture.selected, loaded, captured_now())
        .expect("loaded");
    assert_eq!(capability.egress_bounded, ProofStatus::Unverified);
}

/// Independence is per-attempt evidence and is never restored from a receipt,
/// so a loaded trace leaves it unverified however recent it is.
#[test]
fn a_receipt_never_restores_session_isolation() {
    let fixture = fixture();
    let probe = capture(&fixture, now());
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    let loaded = load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .expect("receipt");
    let capability = PrivateAskCapability::from_probe(&fixture.selected, loaded, captured_now())
        .expect("loaded");
    assert_eq!(capability.independent_invocation, ProofStatus::Unverified);
}

/// Expiry is what stops one trace certifying forever. Removing the
/// `captured_at` window in `load` fails this.
#[test]
fn a_stale_receipt_is_not_returned_so_the_caller_must_probe_again() {
    let fixture = fixture();
    let probe = capture(&fixture, now() - PROBE_MAX_AGE - 1);
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    assert!(load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .is_none());
}

/// A receipt stamped in the future is not a reading of this machine's clock.
#[test]
fn a_receipt_from_the_future_is_refused() {
    let fixture = fixture();
    let probe = capture(&fixture, now() + 600);
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    assert!(load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .is_none());
}

/// An upgraded runtime has no receipt. The old binary's trace must not be read
/// back for it — the key carries the executable fingerprint precisely so a
/// changed binary misses rather than inherits. Removing the fingerprint from
/// `receipt_key` fails this.
#[test]
fn a_receipt_for_a_different_executable_is_ignored() {
    let fixture = fixture();
    let probe = capture(&fixture, now());
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    let upgraded = canonical_tempdir();
    let other = upgraded.path().join("claude");
    std::fs::write(&other, b"#!/usr/bin/perl\n# a later release\nexit 0;\n").expect("runtime");
    let upgraded_state = state(&other, "claude", "claude-fable-5-1", None);
    assert_ne!(
        upgraded_state.executable.fingerprint, fixture.selected.executable.fingerprint,
        "the fixture must actually differ, or this test cannot fail"
    );

    assert!(load(
        &fixture.ownership,
        &upgraded_state.executable,
        "claude",
        &digest,
        now(),
    )
    .is_none());
}

/// A trace produced by a probe program this release does not ship describes a
/// different experiment, so it is not loaded at all.
#[test]
fn a_receipt_from_a_foreign_probe_program_is_ignored() {
    let fixture = fixture();
    let probe = capture(&fixture, now());
    store(&fixture.ownership, &probe).expect("store");

    assert!(load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &"0".repeat(64),
        now(),
    )
    .is_none());
}

/// A receipt copied from another install carries another ownership digest.
/// Removing the `ownership_sha256` comparison in `load` fails this.
#[test]
fn a_receipt_from_another_install_is_refused() {
    let fixture = fixture();
    let probe = capture(&fixture, now());
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    let path = receipt_file(&fixture);
    let text =
        String::from_utf8(std::fs::read(&path).expect("receipt bytes")).expect("receipt is utf-8");
    let own_digest = fixture.ownership.ownership_digest().to_owned();
    let foreign = text.replace(&own_digest, &"a".repeat(own_digest.len()));
    assert_ne!(foreign, text, "the digest must appear in the receipt");
    std::fs::write(&path, foreign).expect("rewrite receipt");

    assert!(load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .is_none());
}

/// Garbage on disk is "no receipt", never a partial one.
#[test]
fn an_unparseable_receipt_yields_nothing_rather_than_partial_evidence() {
    let fixture = fixture();
    let probe = capture(&fixture, now());
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    std::fs::write(receipt_file(&fixture), b"{ not json").expect("corrupt receipt");

    assert!(load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .is_none());
}

/// A refusal reason this release does not know changes what `bounds_egress`
/// would conclude, so the trace is refused rather than mapped onto a guess.
#[test]
fn a_receipt_with_an_unknown_refusal_reason_is_refused() {
    let fixture = fixture();
    let probe = capture(&fixture, now());
    let digest = probe.probe_program_digest.clone();
    store(&fixture.ownership, &probe).expect("store");

    let path = receipt_file(&fixture);
    let text =
        String::from_utf8(std::fs::read(&path).expect("receipt bytes")).expect("receipt is utf-8");
    let mutated = text.replace("\"foreign-host\"", "\"reason-from-a-later-release\"");
    assert_ne!(mutated, text, "the probe must have recorded a host refusal");
    std::fs::write(&path, mutated).expect("rewrite receipt");

    assert!(load(
        &fixture.ownership,
        &fixture.selected.executable,
        "claude",
        &digest,
        now(),
    )
    .is_none());
}

/// Re-probing replaces what it supersedes rather than accumulating files, and
/// no temporary file survives a completed write.
#[test]
fn re_probing_overwrites_the_receipt_for_the_same_selection() {
    let fixture = fixture();
    for _ in 0..3 {
        let probe = capture(&fixture, now());
        store(&fixture.ownership, &probe).expect("store");
    }
    let base = fixture
        .ownership
        .private_ask_probe_base()
        .expect("receipt base");
    let files: Vec<_> = std::fs::read_dir(&base)
        .expect("receipt directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert_eq!(files.len(), 1, "temporary files must not survive either");
}
