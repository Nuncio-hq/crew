//! The selection binding, end to end on the production path.
//!
//! Each test names the production line whose removal makes it fail:
//!
//! * the happy path — `binding::answer`'s `probe_receipt::load` / `capture` /
//!   `store` sequence followed by `PrivateAskCapability::from_probe`;
//! * the re-probe — `probe_receipt::load`'s `PROBE_MAX_AGE` check;
//! * the busy fence — the `refuse_busy_selection` call at the top of
//!   `binding::answer`, which must precede every capture and spawn;
//! * the receipt reuse — the `Some(capability) => capability` arm of
//!   `binding::answer`, which no longer re-probes for a live session;
//! * the independence bracket — the `session.capture()` pair around
//!   `attempt.run()` in `binding::answer`;
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

/// A stand-in employee session: a readable ledger directory and one real live child
/// whose handle this fixture owns, so the PID in the observation comes from a
/// handle rather than from a PID probe — the same rule production follows.
struct LiveSession {
    child: std::process::Child,
    observation: SessionObservation,
}

impl LiveSession {
    fn start(directory: &Path) -> Self {
        let ledger_dir = directory.join("employee-session-ledger");
        std::fs::create_dir_all(&ledger_dir).expect("ledger directory");
        std::fs::write(
            ledger_dir.join("employee-ledger.json"),
            b"{\"turn\":\"idle\"}",
        )
        .expect("ledger");
        let child = std::process::Command::new("/usr/bin/perl")
            .arg("-e")
            .arg("sleep 120;")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("employee session");
        let observation = SessionObservation {
            ledger_dir,
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
        attempt: super::super::AttemptIdentity::fresh(),
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

/// A second Ask on the same selection answers from the retained receipt,
/// beside the same live session, without spawning a probe child.
///
/// This is the point of separating the two: containment is a property of the
/// machine and is cached; independence is a property of THIS run beside THAT
/// session and is observed live every time. Restore the old
/// `needs_own_probe` rule and the receipt stamp moves on the second Ask.
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

    // The second Ask, with the same live session: it answers, and it answers
    // from the retained trace.
    ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Idle),
        Some(session.observation.clone()),
        1_500,
    )
    .expect("a second Ask answers from the retained receipt");
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
    ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Idle),
        Some(session.observation.clone()),
        stale,
    )
    .expect("a stale receipt re-probes and still answers");
    assert_eq!(
        stored_receipt(&owned_receipt(&fixture))["captured_at"],
        stale
    );
}

/// A live session that cannot be read either side of the run is a refusal, not
/// an answer: a session nobody could observe has not been shown to be
/// untouched.
///
/// Production line: the `session.capture()` pair around `attempt.run()` in
/// `binding::answer`. Drop the bracket and this answers.
#[test]
fn an_unobservable_session_refuses_rather_than_answering_beside_it() {
    let fixture = canonical_tempdir();
    let path = answering_runtime(fixture.path());
    let session = LiveSession::start(fixture.path());
    let mut unobservable = session.observation.clone();
    unobservable.ledger_dir = fixture.path().join("no-such-ledger");

    let refusal = ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Idle),
        Some(unobservable),
        2_000,
    )
    .unwrap_err();

    assert_eq!(refusal, PrivateAskFailure::SessionObservationUnavailable);
}

/// An agent whose own session is mid-turn is refused BEFORE anything is
/// captured or spawned. The bracket around an answer can only speak after the
/// model call has been paid for, so the cheap signal has to come first.
///
/// Production line: the `refuse_busy_selection(&binding.state)` call at the top
/// of `binding::answer`. Move it below the receipt load and this test still
/// sees `AgentBusy`, but a probe receipt appears on disk — which is what the
/// second half asserts, and what makes this bind to the fence's POSITION
/// rather than merely to its existence.
#[test]
fn a_busy_agent_is_refused_before_any_child_is_started() {
    let fixture = canonical_tempdir();
    // A runtime that records having been run. Nothing may execute it.
    let spawn_marker = fixture.path().join("runtime-was-executed");
    let path = fake_runtime(
        fixture.path(),
        "claude",
        &format!(
            "#!/usr/bin/perl\nopen(my $out, '>', '{}'); print $out \"spawned\"; close $out;\nprint '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"It returns 42.\\n\\n[^cite]: src/lib.rs\",\"modelUsage\":{{\"claude-fable-5-1\":{{}}}}}}';\n",
            spawn_marker.display()
        ),
    );
    let session = LiveSession::start(fixture.path());
    let ownership = owned_receipt(&fixture);
    let probe_base = ownership
        .private_ask_probe_base()
        .expect("probe base")
        .to_path_buf();

    let refusal = ask(
        ownership,
        selection(&path, AgentLifecycle::Busy),
        Some(session.observation.clone()),
        2_000,
    )
    .unwrap_err();

    assert_eq!(refusal, PrivateAskFailure::AgentBusy);
    assert!(
        !spawn_marker.exists(),
        "no runtime child may be started for a busy agent"
    );
    let receipts = std::fs::read_dir(&probe_base)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0);
    assert_eq!(
        receipts, 0,
        "no capability probe may be captured for a busy agent either"
    );
}

/// An idle agent answers, and its session bracket is what certifies the run.
#[test]
fn an_idle_agent_is_answered_and_its_session_bracket_certifies_the_run() {
    let fixture = canonical_tempdir();
    let path = answering_runtime(fixture.path());
    let session = LiveSession::start(fixture.path());

    let response = ask(
        owned_receipt(&fixture),
        selection(&path, AgentLifecycle::Idle),
        Some(session.observation.clone()),
        2_000,
    )
    .expect("an idle agent answers when its session is untouched");

    assert!(response.markdown.starts_with("It returns 42."));
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
