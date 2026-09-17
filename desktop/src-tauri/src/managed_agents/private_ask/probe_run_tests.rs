//! The production probe, run for real.
//!
//! These are not logic tests. Each one launches the shipped probe program under
//! the same Seatbelt policy text a real answer runs under, and asserts what the
//! desktop measured — never what the probe said about itself.

#![cfg(all(unix, target_os = "macos"))]

use super::super::tests::{canonical_tempdir, captured_now, state};
use super::super::{PrivateAskCapability, ProofStatus};
use super::*;
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;

fn ownership(root: &std::path::Path) -> VerifiedStagingOwnership {
    VerifiedStagingOwnership::for_test(root).expect("staging ownership")
}

/// A fixture runtime in its own directory.
///
/// It must NOT live in a directory that contains the staging base. The
/// containment policy allows reads under the runtime's own directory, so a
/// runtime installed above the app-data tree would make every run root — and
/// the probe's sentinel — readable through that allowance. Real runtimes live
/// in `/usr/local/bin` or `~/.local/bin`, never above `<app-data>/agents`; a
/// fixture that ignores that would quietly prove read isolation against a
/// policy no production layout produces.
fn fixture_runtime(directory: &std::path::Path) -> std::path::PathBuf {
    let runtime = directory.join("claude");
    std::fs::write(&runtime, b"#!/usr/bin/perl\nexit 0;\n").expect("runtime");
    runtime
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

/// The whole design rests on this: the probe runs under the policy built for
/// the SELECTED RUNTIME, not a policy built for the probe. If the two ever
/// diverge, `from_probe` rebuilds a profile that does not match and the trace
/// silently stops certifying — so assert the equality directly rather than
/// inferring it from a passing projection.
#[test]
fn the_probe_runs_under_the_policy_text_a_real_answer_would_run_under() {
    let home = canonical_tempdir();
    let ownership = ownership(home.path());
    let installation = canonical_tempdir();
    let runtime = fixture_runtime(installation.path());
    let selected = state(&runtime, "claude", "claude-fable-5-1", None);

    let probe = capture_probe(ProbeContext {
        state: &selected,
        ownership: &ownership,
        session: None,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        now: now(),
    })
    .expect("probe");

    let expected = super::super::containment::private_ask_containment_profile(
        &probe.probe_run_root,
        &super::super::launch::runtime_directory(&selected.executable.resolved_path)
            .expect("runtime directory"),
        &super::super::launch::extra_read_roots(&selected.executable.resolved_path),
        probe.egress.proxy_port(),
    )
    .expect("expected profile");
    assert_eq!(probe.containment_profile, expected);
}

/// Every effect the envelope must deny, measured from this side.
#[test]
fn a_real_probe_observes_a_contained_envelope() {
    let home = canonical_tempdir();
    let ownership = ownership(home.path());
    let installation = canonical_tempdir();
    let runtime = fixture_runtime(installation.path());
    let selected = state(&runtime, "claude", "claude-fable-5-1", None);

    let probe = capture_probe(ProbeContext {
        state: &selected,
        ownership: &ownership,
        session: None,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        now: now(),
    })
    .expect("probe");

    // The sentinel is the desktop's own file, digested either side of the run.
    assert_eq!(
        probe.tool_probe.sentinel_before, probe.tool_probe.sentinel_after,
        "a write outside the run root changed bytes the desktop owns"
    );
    // The direct-connect control listener is the desktop's own socket.
    assert_eq!(probe.tool_probe.network_connections_observed, 0);
    assert_eq!(probe.egress.direct_connections(), 0);
    // Measured after the bounded owner returned, by taking a lock a survivor
    // would still hold.
    assert_eq!(probe.tool_probe.surviving_descendants, 0);
    assert!(probe.tool_probe.request_observed);
    // Folds in the link, IPv6-loopback and UNIX-socket legs as well as the
    // write, fork, DNS and proxy ones: a run that escaped by any of them did
    // not demonstrate a refusal.
    assert!(probe.tool_probe.denied_before_effect);
    assert!(probe.tool_probe.read_outside_requested);
    assert!(probe.tool_probe.read_outside_denied);
    // The proxy refused the foreign host and was reached for the provider, so
    // "accepted everything" and "was never tested" are distinguishable.
    assert!(probe.egress.bounds_egress());
}

/// The in-run hard-link vector, measured against the real envelope.
///
/// The pre-spawn fence can only inspect a run root as it stands before the
/// child starts; a link created DURING the run would hand the child a writable
/// name inside its own tree pointing at an inode outside it. Measured on macOS
/// 25.5: the same `link()` succeeds without the policy and is refused with it,
/// so the policy already covers it and this pins that.
///
/// Production line: the `(deny file-write*)` rule in
/// `private_ask_containment_profile`. Remove it and the link lands.
#[test]
fn a_hard_link_out_of_the_run_root_is_refused_by_the_real_policy() {
    let installation = canonical_tempdir();
    let run_root = canonical_tempdir();
    let outside = canonical_tempdir();
    let sentinel = outside.path().join("sentinel");
    std::fs::write(&sentinel, b"sentinel\n").expect("sentinel");
    let runtime = fixture_runtime(installation.path());
    let runtime_directory = runtime.parent().expect("runtime directory");

    let profile = super::super::containment::private_ask_containment_profile(
        run_root.path(),
        runtime_directory,
        &[],
        41_234,
    )
    .expect("policy");
    let linked = run_root.path().join("linked");

    let contained = std::process::Command::new("/usr/bin/sandbox-exec")
        .arg("-p")
        .arg(&profile)
        .arg("/bin/ln")
        .arg(&sentinel)
        .arg(&linked)
        .output()
        .expect("sandbox-exec");
    assert!(
        !contained.status.success(),
        "the policy must refuse a link from outside the run root"
    );
    assert!(
        !linked.exists(),
        "no name inside the run root may point at an outside inode"
    );

    // The same call without the policy succeeds, so the refusal above is the
    // policy's doing and not a broken invocation.
    let uncontained = std::process::Command::new("/bin/ln")
        .arg(&sentinel)
        .arg(&linked)
        .output()
        .expect("ln");
    assert!(uncontained.status.success());
    assert!(linked.exists());
}

/// A probe from this build certifies; the same trace attributed to a probe
/// program this build does not ship does not. Removing the digest check in
/// `from_probe` fails this.
#[test]
fn a_trace_from_a_foreign_probe_program_cannot_certify() {
    let home = canonical_tempdir();
    let ownership = ownership(home.path());
    let installation = canonical_tempdir();
    let runtime = fixture_runtime(installation.path());
    let selected = state(&runtime, "claude", "claude-fable-5-1", None);

    let mut probe = capture_probe(ProbeContext {
        state: &selected,
        ownership: &ownership,
        session: None,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        now: now(),
    })
    .expect("probe");
    probe.probe_program_digest = "0".repeat(64);
    assert_eq!(
        PrivateAskCapability::from_probe(&selected, probe, captured_now()).unwrap_err(),
        super::super::capability::PrivateAskProbeRejection::ForeignProbeProgram
    );
}

/// A probe that observed an escape must not certify the dimension it escaped.
/// The projection is what refuses, so this asserts the dimension rather than an
/// error from the capture itself: the capture succeeded, the evidence did not.
#[test]
fn a_probe_that_observed_an_escape_refuses_certification() {
    let home = canonical_tempdir();
    let ownership = ownership(home.path());
    let installation = canonical_tempdir();
    let runtime = fixture_runtime(installation.path());
    let selected = state(&runtime, "claude", "claude-fable-5-1", None);

    let mut probe = capture_probe(ProbeContext {
        state: &selected,
        ownership: &ownership,
        session: None,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        now: now(),
    })
    .expect("probe");
    // One escape: a descendant outlived the bounded owner.
    probe.tool_probe.surviving_descendants = 1;
    let capability =
        PrivateAskCapability::from_probe(&selected, probe, captured_now()).expect("projection");
    assert_eq!(capability.process_containment, ProofStatus::Unverified);
}

/// `independent_invocation` is only Verified from two real observations, so a
/// probe captured without them leaves it unverified rather than assuming it.
#[test]
fn a_probe_without_session_observations_cannot_verify_independent_invocation() {
    let home = canonical_tempdir();
    let ownership = ownership(home.path());
    let installation = canonical_tempdir();
    let runtime = fixture_runtime(installation.path());
    let selected = state(&runtime, "claude", "claude-fable-5-1", None);

    let probe = capture_probe(ProbeContext {
        state: &selected,
        ownership: &ownership,
        session: None,
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        now: now(),
    })
    .expect("probe");
    let capability =
        PrivateAskCapability::from_probe(&selected, probe, captured_now()).expect("projection");
    assert_eq!(capability.independent_invocation, ProofStatus::Unverified);
}

/// Authentication is its own dimension, and it moves independently of
/// containment in BOTH directions.
///
/// The platform secret store is replaced by a canary here — a unit test must
/// never read the developer's keychain, and the real reader shells out to
/// `/usr/bin/security`, which would make this assertion depend on whose Mac ran
/// the suite. The gate is NOT replaced: the canary is still refused unless the
/// proxy is serving.
///
/// The point of the test is the pairing. Coupling authentication into
/// `from_probe`'s containment verdict was tried in an earlier round and
/// reverted, because it reported a broken sandbox as an authentication failure.
/// So containment must read `Verified` in both directions while authentication
/// changes underneath it. Restoring that coupling fails the second half.
#[test]
fn authentication_moves_independently_of_containment_in_both_directions() {
    let home = canonical_tempdir();
    let ownership = ownership(home.path());
    let installation = canonical_tempdir();
    let runtime = fixture_runtime(installation.path());
    let selected = state(&runtime, "claude", "claude-fable-5-1", None);

    let capture = || {
        capture_probe(ProbeContext {
            state: &selected,
            ownership: &ownership,
            session: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            now: now(),
        })
        .expect("probe")
    };

    super::super::credential::set_test_secret_store_empty(false);
    let available = capture();
    assert!(available.auth.auth_available());
    let capability =
        PrivateAskCapability::from_probe(&selected, available, captured_now()).expect("projection");
    assert_eq!(capability.authentication, ProofStatus::Verified);
    assert_eq!(capability.process_containment, ProofStatus::Verified);

    // The same machine, the same envelope, no credential.
    super::super::credential::set_test_secret_store_empty(true);
    let absent = capture();
    super::super::credential::set_test_secret_store_empty(false);
    assert!(!absent.auth.auth_available());
    let capability =
        PrivateAskCapability::from_probe(&selected, absent, captured_now()).expect("projection");
    assert_eq!(capability.authentication, ProofStatus::Unverified);
    assert_eq!(
        capability.process_containment,
        ProofStatus::Verified,
        "a missing credential must not report itself as a broken sandbox"
    );
}
