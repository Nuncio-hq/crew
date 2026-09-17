//! The selection binding, end to end on the production path.
//!
//! Each test names the production line whose removal makes it fail:
//!
//! * the happy path — `binding::answer`'s `probe_receipt::load` / `capture` /
//!   `store` sequence followed by `PrivateAskCapability::from_probe`;
//! * the re-probe — `probe_receipt::load`'s `PROBE_MAX_AGE` check;
//! * the busy refusal — the `independent_invocation` clause of the busy fence in
//!   `admit_private_ask`;
//! * the citation fence — the `citations::resolve` call in
//!   `PrivateAskAttempt::run`.
//!
//! They need the real Seatbelt boundary, so they are macOS-only; off macOS the
//! containment profile refuses first and `fail_closed_tests` asserts that.

#![cfg(all(unix, target_os = "macos"))]

use super::super::capability::PROBE_MAX_AGE;
use super::super::session_evidence::SessionObservation;
use super::super::tests::{
    canonical_tempdir, executable, fake_runtime, grounding, owned_receipt, request, state,
};
use super::super::{AgentLifecycle, PrivateAskFailure, PrivateAskResponse, SelectedAgentState};
use super::{answer, PrivateAskBinding};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use std::path::Path;

/// A stand-in employee session: two readable artefacts and one real live child
/// whose handle this fixture owns, so the PID in the observation comes from a
/// handle rather than from a PID probe — the same rule production follows.
struct LiveSession {
    child: std::process::Child,
    observation: SessionObservation,
}

impl LiveSession {
    fn start(directory: &Path) -> Self {
        let ledger = directory.join("employee-ledger.json");
        let observer_sequence = directory.join("employee-observer-sequence");
        std::fs::write(&ledger, b"{\"turn\":\"idle\"}").expect("ledger");
        std::fs::write(&observer_sequence, b"42").expect("observer sequence");
        let child = std::process::Command::new("/usr/bin/perl")
            .arg("-e")
            .arg("sleep 120;")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("employee session");
        let observation = SessionObservation {
            ledger,
            observer_sequence,
            acp_pid: Some(child.id()),
        };
        Self { child, observation }
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        // Stop only what this fixture started.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A benign runtime that answers with one grounded citation.
fn answering_runtime(directory: &Path) -> std::path::PathBuf {
    fake_runtime(
        directory,
        "claude",
        "#!/usr/bin/perl\nlocal $/; my $in = <STDIN>; die \"no prompt\" unless defined $in; print '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"It returns 42.\\n\\n[^cite]: src/lib.rs\",\"modelUsage\":{\"claude-fable-5-1\":{}}}';\n",
    )
}

fn selection(path: &Path, lifecycle: AgentLifecycle) -> SelectedAgentState {
    let mut selected = state(path, "claude", "claude-fable-5-1", None);
    selected.executable = executable(path);
    selected.lifecycle = lifecycle;
    selected
}

fn ask(
    ownership: VerifiedStagingOwnership,
    state: SelectedAgentState,
    session: Option<SessionObservation>,
    now: u64,
) -> Result<PrivateAskResponse, PrivateAskFailure> {
    answer(PrivateAskBinding {
        ownership,
        state,
        request: request(),
        session,
        hermes_profile: None,
        now,
    })
}

/// The one receipt the binding retained, read back as plain JSON.
fn stored_receipt(ownership: &VerifiedStagingOwnership) -> serde_json::Value {
    let base = ownership.private_ask_probe_base().expect("probe base");
    let mut files: Vec<_> = std::fs::read_dir(&base)
        .expect("probe base readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    assert_eq!(files.len(), 1, "exactly one receipt for one selection");
    let bytes = std::fs::read(files.pop().expect("receipt")).expect("receipt readable");
    serde_json::from_slice(&bytes).expect("receipt parses")
}

#[test]
fn a_selection_with_no_retained_probe_probes_itself_and_answers() {
    let fixture = canonical_tempdir();
    let path = answering_runtime(fixture.path());
    let ownership = owned_receipt(&fixture);
    let session = LiveSession::start(fixture.path());

    let response = ask(
        ownership,
        selection(&path, AgentLifecycle::Idle),
        Some(session.observation.clone()),
        1_000,
    )
    .expect("a bound selection answers");

    assert!(response.markdown.starts_with("It returns 42."));
    // The citation is the one the answer printed, resolved against the
    // grounding — not a copy of the request.
    assert_eq!(response.citations, vec![grounding()]);

    // The trace this attempt paid for is retained for the next one.
    let ownership = owned_receipt(&fixture);
    assert_eq!(stored_receipt(&ownership)["captured_at"], 1_000);
}

#[test]
fn a_retained_probe_is_reused_until_it_goes_stale() {
    let fixture = canonical_tempdir();
    let path = answering_runtime(fixture.path());
    let session = LiveSession::start(fixture.path());

    // One real capture, so there is a receipt on disk to reuse.
    ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Idle),
        Some(session.observation.clone()),
        1_000,
    )
    .expect("first answer");
    assert_eq!(
        stored_receipt(&owned_receipt(&fixture))["captured_at"],
        1_000
    );

    // With no session to bracket, the binding has nothing to observe that the
    // retained trace lacks, so it reuses it and the stamp does not move. (The
    // Ask itself is still refused — a trace with no session evidence cannot
    // certify an independent invocation — which is the fail-closed direction.)
    assert_eq!(
        ask(
            owned_receipt(&fixture),
            selection(&path, AgentLifecycle::Idle),
            None,
            1_500,
        )
        .unwrap_err(),
        PrivateAskFailure::IndependentInvocationUnverified
    );
    assert_eq!(
        stored_receipt(&owned_receipt(&fixture))["captured_at"],
        1_000,
        "a fresh receipt is reused rather than re-captured"
    );

    // Past the window the stale trace is not loaded at all, so the selection is
    // re-probed and the new stamp is retained. Deleting the `PROBE_MAX_AGE`
    // check in `probe_receipt::load` leaves the stamp at 1_000 and lets an
    // expired trace go on certifying.
    let stale = 1_000 + PROBE_MAX_AGE + 1;
    let _ = ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Idle),
        None,
        stale,
    );
    assert_eq!(
        stored_receipt(&owned_receipt(&fixture))["captured_at"],
        stale
    );
}

#[test]
fn a_busy_agent_is_refused_rather_than_answered() {
    let fixture = canonical_tempdir();
    let path = answering_runtime(fixture.path());

    // The probe is captured with no live session to bracket, so
    // `independent_invocation` is unverified. An idle agent does not need it;
    // a busy one does, and is refused rather than answered beside a session
    // nothing observed.
    let refusal = ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Busy),
        None,
        2_000,
    )
    .unwrap_err();

    assert_eq!(refusal, PrivateAskFailure::AgentBusy);
}

#[test]
fn an_answer_citing_outside_the_snapshot_is_refused_through_the_binding() {
    let fixture = canonical_tempdir();
    let path = fake_runtime(
        fixture.path(),
        "claude",
        "#!/usr/bin/perl\nlocal $/; my $in = <STDIN>; print '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"Here.\\n\\n[^cite]: /Users/employee/.ssh/id_ed25519\",\"modelUsage\":{\"claude-fable-5-1\":{}}}';\n",
    );

    let session = LiveSession::start(fixture.path());

    let refusal = ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Idle),
        Some(session.observation.clone()),
        3_000,
    )
    .unwrap_err();

    assert_eq!(refusal, PrivateAskFailure::InvalidOutput);
}
