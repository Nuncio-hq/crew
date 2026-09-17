//! Negative certification: evidence that does not prove a property must not
//! certify it, and the refusal the viewer sees comes from admission.

#![cfg(unix)]

use super::capability::{
    PrivateAskAuthEvidence, PrivateAskProbe, PrivateAskProbeRejection, PrivateAskToolProbe,
    SessionIsolationEvidence, PRIVATE_ASK_TOOL_PROBE_ID,
};
use super::tests::{canonical_tempdir, executable, request, state, FIXTURE_PERSONA};
use super::*;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Mirror of what `from_probe` resolves, so the rebuilt policy compares equal.
fn runtime_directory_of(executable: &Path) -> PathBuf {
    executable
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn digest_of(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// A probe describing a fully contained, fully isolated run of the selected
/// agent. Individual tests spoil exactly one field.
fn healthy_probe(state: &SelectedAgentState, run_root: PathBuf) -> PrivateAskProbe {
    PrivateAskProbe {
        runtime_id: state.runtime_id.clone(),
        executable: state.executable.clone(),
        effective_model: state.effective_model.clone(),
        profile: state.profile.clone(),
        persona: FIXTURE_PERSONA.into(),
        acl_fingerprint: state.acl_fingerprint.clone(),
        session_generation: state.session_generation.clone(),
        auth: PrivateAskAuthEvidence {
            service: "buzz-desktop-demo.staging-test".into(),
            reference: "keychain-reference".into(),
            auth_available: true,
        },
        tool_probe: PrivateAskToolProbe {
            probe_id: PRIVATE_ASK_TOOL_PROBE_ID.into(),
            tool_name: "write_file".into(),
            request_observed: true,
            denied_before_effect: true,
            read_outside_requested: true,
            read_outside_denied: true,
            sentinel_before: digest_of("sentinel"),
            sentinel_after: digest_of("sentinel"),
            network_connections_observed: 0,
            surviving_descendants: 0,
        },
        containment_profile: containment::private_ask_containment_profile(
            &run_root,
            &runtime_directory_of(&state.executable.resolved_path),
        )
        .unwrap_or_default(),
        probe_run_root: run_root,
        external_state_before: digest_of("checkout"),
        external_state_after: digest_of("checkout"),
        session_isolation: Some(SessionIsolationEvidence::from_parts(
            digest_of("ledger"),
            digest_of("ledger"),
            42,
            42,
            Some(4321),
            Some(4321),
            99,
            99,
        )),
    }
}

fn fixture() -> (tempfile::TempDir, SelectedAgentState, PathBuf) {
    let directory = canonical_tempdir();
    let path = directory.path().join("runtime");
    let mut selected = state(&path, "claude", "claude-fable-5-1", None);
    selected.executable = executable(&path);
    let run_root = directory.path().join("probe-root");
    std::fs::create_dir(&run_root).unwrap();
    (directory, selected, run_root)
}

/// A complete probe is the only thing that certifies every dimension. If this
/// stops passing, the negative cases below no longer isolate one field each.
#[cfg(target_os = "macos")]
#[test]
fn a_complete_probe_certifies_every_dimension_and_admits_the_request() {
    let (_directory, selected, run_root) = fixture();
    let capability =
        PrivateAskCapability::from_probe(&selected, healthy_probe(&selected, run_root)).unwrap();
    assert_eq!(capability.authentication, ProofStatus::Verified);
    assert_eq!(capability.tool_isolation, ProofStatus::Verified);
    assert_eq!(capability.process_containment, ProofStatus::Verified);
    assert_eq!(capability.side_effect_free, ProofStatus::Verified);
    assert_eq!(capability.independent_invocation, ProofStatus::Verified);
    admit_private_ask(request(), selected, capability).unwrap();
}

/// A hostile attempt that was never made proves nothing about denial.
/// Removing `request_observed` from `valid_tool_probe_evidence` fails this.
#[cfg(target_os = "macos")]
#[test]
fn a_probe_that_never_attempted_a_tool_cannot_certify_tool_isolation() {
    let (_directory, selected, run_root) = fixture();
    let mut probe = healthy_probe(&selected, run_root);
    probe.tool_probe.request_observed = false;
    let capability = PrivateAskCapability::from_probe(&selected, probe).unwrap();
    assert_eq!(capability.tool_isolation, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::ToolIsolationUnverified
    );
}

/// A sentinel that changed means the effect landed, whatever the runtime said.
/// Removing the sentinel equality check fails this.
#[cfg(target_os = "macos")]
#[test]
fn a_probe_whose_sentinel_changed_cannot_certify_tool_isolation() {
    let (_directory, selected, run_root) = fixture();
    let mut probe = healthy_probe(&selected, run_root);
    probe.tool_probe.sentinel_after = digest_of("tampered");
    let capability = PrivateAskCapability::from_probe(&selected, probe).unwrap();
    assert_eq!(capability.tool_isolation, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::ToolIsolationUnverified
    );
}

/// An accepted connection is egress, and a surviving descendant is an escaped
/// process tree. Each must defeat its own dimension.
#[cfg(target_os = "macos")]
#[test]
fn observed_egress_or_a_surviving_descendant_cannot_certify_containment() {
    let (_directory, selected, run_root) = fixture();

    let mut egress = healthy_probe(&selected, run_root.clone());
    egress.tool_probe.network_connections_observed = 1;
    let capability = PrivateAskCapability::from_probe(&selected, egress).unwrap();
    assert_eq!(capability.tool_isolation, ProofStatus::Unverified);

    let mut escaped = healthy_probe(&selected, run_root);
    escaped.tool_probe.surviving_descendants = 1;
    let capability = PrivateAskCapability::from_probe(&selected, escaped).unwrap();
    assert_eq!(capability.process_containment, ProofStatus::Unverified);
    assert_eq!(capability.tool_isolation, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::ToolIsolationUnverified
    );
}

/// The recap no-fork policy permits both writes and network egress, so a trace
/// captured under it must not certify this recipe. Removing the profile
/// comparison in `from_probe` fails this.
#[cfg(target_os = "macos")]
#[test]
fn a_trace_captured_under_the_weaker_recap_policy_cannot_certify_containment() {
    let (_directory, selected, run_root) = fixture();
    let mut probe = healthy_probe(&selected, run_root);
    probe.containment_profile = "(version 1)(allow default)(deny process-fork)".into();
    let capability = PrivateAskCapability::from_probe(&selected, probe).unwrap();
    assert_eq!(capability.process_containment, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::ToolIsolationUnverified
    );
}

/// Changed external state after the run is a side effect even when every tool
/// attempt was refused.
#[cfg(target_os = "macos")]
#[test]
fn changed_external_state_cannot_certify_a_side_effect_free_run() {
    let (_directory, selected, run_root) = fixture();
    let mut probe = healthy_probe(&selected, run_root);
    probe.external_state_after = digest_of("checkout-modified");
    let capability = PrivateAskCapability::from_probe(&selected, probe).unwrap();
    assert_eq!(capability.side_effect_free, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::SideEffectProofUnverified
    );
}

/// The recap probe identifier must not be replayable here, and an absent
/// session snapshot cannot imply independence.
#[cfg(target_os = "macos")]
#[test]
fn recap_evidence_and_a_missing_session_snapshot_do_not_carry_over() {
    let (_directory, selected, run_root) = fixture();

    let mut replayed = healthy_probe(&selected, run_root.clone());
    replayed.tool_probe.probe_id = super::super::recap_capability::RECAP_TOOL_PROBE_ID.to_string();
    let capability = PrivateAskCapability::from_probe(&selected, replayed).unwrap();
    assert_eq!(capability.tool_isolation, ProofStatus::Unverified);

    let mut unsnapshotted = healthy_probe(&selected, run_root);
    unsnapshotted.session_isolation = None;
    let capability = PrivateAskCapability::from_probe(&selected, unsnapshotted).unwrap();
    assert_eq!(capability.independent_invocation, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::IndependentInvocationUnverified
    );
}

/// A trace about another executable, runtime or persona is not evidence about
/// this selection in either direction, so it is rejected outright.
#[test]
fn a_probe_for_another_selection_is_rejected_rather_than_projected() {
    let (directory, selected, run_root) = fixture();

    let mut other_runtime = healthy_probe(&selected, run_root.clone());
    other_runtime.runtime_id = "hermes".into();
    assert_eq!(
        PrivateAskCapability::from_probe(&selected, other_runtime).unwrap_err(),
        PrivateAskProbeRejection::RuntimeMismatch
    );

    let mut other_executable = healthy_probe(&selected, run_root.clone());
    other_executable.executable = executable(&directory.path().join("other-runtime"));
    other_executable.executable.platform = selected.executable.platform.clone();
    assert_eq!(
        PrivateAskCapability::from_probe(&selected, other_executable).unwrap_err(),
        PrivateAskProbeRejection::ExecutableMismatch
    );

    let mut other_persona = healthy_probe(&selected, run_root);
    other_persona.persona = "You are a different employee.".into();
    assert_eq!(
        PrivateAskCapability::from_probe(&selected, other_persona).unwrap_err(),
        PrivateAskProbeRejection::SelectionMismatch
    );
}

/// A policy can deny every write and still hand the run the employee's
/// worktree. Read isolation is therefore its own dimension, and the refusal the
/// viewer sees names it. Removing the `read_bounded` fence from
/// `admit_private_ask`, or the read allow-list from
/// `private_ask_containment_profile`, fails this.
#[test]
fn a_probe_that_read_outside_its_run_root_cannot_certify_read_isolation() {
    let fixture = canonical_tempdir();
    let path = fixture.path().join("runtime");
    let selected = state(&path, "claude", "claude-fable-5-1", None);

    // A read that was never attempted proves nothing.
    let run_root = fixture.path().join("probe-root-read-unattempted");
    std::fs::create_dir(&run_root).unwrap();
    let mut probe = healthy_probe(&selected, run_root);
    probe.tool_probe.read_outside_requested = false;
    let capability = PrivateAskCapability::from_probe(&selected, probe).expect("projection");
    assert_eq!(capability.read_bounded, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected.clone(), capability).unwrap_err(),
        PrivateAskFailure::ReadIsolationUnverified
    );

    // A read that returned the employee's bytes is an escape.
    let run_root = fixture.path().join("probe-root-read-escaped");
    std::fs::create_dir(&run_root).unwrap();
    let mut probe = healthy_probe(&selected, run_root);
    probe.tool_probe.read_outside_denied = false;
    let capability = PrivateAskCapability::from_probe(&selected, probe).expect("projection");
    assert_eq!(capability.read_bounded, ProofStatus::Unverified);
    assert_eq!(
        admit_private_ask(request(), selected, capability).unwrap_err(),
        PrivateAskFailure::ReadIsolationUnverified
    );
}
