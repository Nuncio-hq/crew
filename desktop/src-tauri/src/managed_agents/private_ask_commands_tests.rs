//! The command layer refuses before it reads anything, and never invents an
//! answer the admission fences did not authorise.

use super::super::private_ask::dev_gate::dev_gate_open;
use super::super::private_ask::{AttemptIdentity, PrivateAskAttempts};
use super::*;
use tauri::Manager;

/// A minimal app handle. The commands need one for the owner-local history and
/// the attempt registry and nothing else; a mock app gives a real `AppHandle`
/// with a per-test identifier so two tests never share an app-data tree.
fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = format!(
        "xyz.nuncio.crew.private-ask-command-{}",
        uuid::Uuid::new_v4().simple()
    );
    let app = tauri::test::mock_builder()
        .build(context)
        .expect("build the private Ask command fixture app");
    // The same state the shipped app manages, so these tests drive the command
    // through its real registry rather than a stand-in.
    app.manage(PrivateAskAttempts::default());
    app
}

fn attempt_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The gate a closed build would apply. `cfg!(debug_assertions)` is true under
/// `cargo test`, so the closed branch is exercised through the pure decision
/// rather than through the cached reader.
#[test]
fn a_closed_gate_refuses_every_command_before_reading_its_arguments() {
    assert!(!dev_gate_open(false, None));

    // The refusal a command returns when the gate is closed is a fixed string
    // with no argument in it: nothing the renderer sent is echoed back, and no
    // work is attempted.
    assert_eq!(
        PRIVATE_ASK_UNAVAILABLE,
        "private Ask is not enabled in this build"
    );
}

#[tokio::test]
async fn an_open_gate_still_refuses_the_run_with_the_blocking_reason() {
    // This build has the gate open (debug assertions), so the command is
    // reachable — and must still refuse, with the reason the production path
    // produced rather than a placeholder answer or a string chosen here.
    assert!(super::private_ask_dev_enabled());
    let app = mock_app();
    let id = attempt_id();
    let result = private_ask_run(
        app.handle().clone(),
        app.state::<PrivateAskAttempts>(),
        id.clone(),
        "What does answer do?".to_string(),
    )
    .await
    .expect("a refusal is a completed attempt, not a failed command");
    assert_eq!(result.attempt_id, id);
    assert!(result.markdown.is_empty());
    assert_eq!(
        result.refusal.as_deref(),
        Some(
            crate::managed_agents::private_ask::dev_run(
                "What does answer do?",
                &AttemptIdentity::fresh()
            )
            .expect_err("no selection is bound")
            .to_string()
            .as_str()
        )
    );
    // A mock app has no owned staging tree, so the refusal could not be kept.
    // That travels with the refusal instead of being dropped: the composer
    // shows the notice, and the list being one short is explained.
    assert!(!result.history_recorded);
}

#[tokio::test]
async fn an_empty_question_or_a_foreign_attempt_id_is_refused_at_the_boundary() {
    let app = mock_app();
    for blank in ["", "   ", "\n", "\t "] {
        let error = private_ask_run(
            app.handle().clone(),
            app.state::<PrivateAskAttempts>(),
            attempt_id(),
            blank.to_string(),
        )
        .await
        .expect_err("a blank question is not a question");
        assert_eq!(error, "a private Ask needs a question");
    }
    for foreign in [
        "",
        "attempt-1",
        "../../etc/passwd",
        &uuid::Uuid::nil().to_string(),
    ] {
        let error = private_ask_run(
            app.handle().clone(),
            app.state::<PrivateAskAttempts>(),
            foreign.to_string(),
            "What does answer do?".to_string(),
        )
        .await
        .expect_err("an id the surface did not mint is refused");
        assert_eq!(error, "a private Ask needs its own attempt id");
    }
}

#[tokio::test]
async fn the_status_command_reports_a_reason_without_launching_anything() {
    let status = private_ask_dev_status(mock_app().handle().clone())
        .await
        .expect("status");
    assert!(status.enabled, "debug build opens the surface");
    // A mock app has no owned staging tree, so the status stops at that fence
    // and says so. The point is that it stopped there: the status is polled on
    // every mount, and reaching the run path would start a capability probe —
    // a contained child process — each time the composer appeared.
    assert_eq!(
        status.blocked_reason.as_deref(),
        Some("this install has no owned staging tree for a private Ask")
    );
}

/// Cancelling twice, or cancelling something that already finished, is not an
/// error — a renderer that does so has not misbehaved.
#[tokio::test]
async fn cancelling_an_unknown_or_finished_attempt_is_a_no_op() {
    let app = mock_app();
    private_ask_cancel(app.state::<PrivateAskAttempts>(), None)
        .await
        .expect("cancel with no id");
    for repeat in 0..2 {
        private_ask_cancel(app.state::<PrivateAskAttempts>(), Some(attempt_id()))
            .await
            .unwrap_or_else(|_| panic!("cancelling an unknown id is a no-op (round {repeat})"));
    }
    private_ask_cancel(
        app.state::<PrivateAskAttempts>(),
        Some("not-a-uuid".to_string()),
    )
    .await
    .expect("a malformed id is ignored, not an error");
}

/// The cancel reaches the run through the registry's own flag, and the outcome
/// the viewer sees is their withdrawal — not the generic process wording.
#[tokio::test]
async fn a_cancelled_attempt_is_reported_and_recorded_as_cancelled() {
    let app = mock_app();
    let registry = app.state::<PrivateAskAttempts>();
    let id = attempt_id();
    let registration = registry.register(&id).expect("register");
    assert!(
        private_ask_cancel(app.state::<PrivateAskAttempts>(), Some(id.clone()))
            .await
            .is_ok()
    );
    assert!(registration.is_cancelled(), "the live flag was raised");

    let identity = AttemptIdentity::new(&id, registration.cancel_flag());
    let failure = crate::managed_agents::private_ask::dev_run("What does answer do?", &identity)
        .expect_err("a withdrawn question is not asked");
    assert_eq!(failure.to_string(), "the private Ask was cancelled");
    // The owner-local record carries the same sentence the screen does.
    let entry = crate::managed_agents::private_ask::history::PrivateAskHistoryEntry::refused(
        "What does answer do?",
        &id,
        &failure,
        1,
    );
    assert_eq!(
        entry.refusal.as_deref(),
        Some("the private Ask was cancelled")
    );
}

/// One id, one run. Two runs sharing a flag would let either cancel the other.
#[tokio::test]
async fn an_attempt_id_that_is_already_running_is_refused() {
    let app = mock_app();
    let id = attempt_id();
    let _held = app
        .state::<PrivateAskAttempts>()
        .register(&id)
        .expect("register");
    let error = private_ask_run(
        app.handle().clone(),
        app.state::<PrivateAskAttempts>(),
        id,
        "What does answer do?".to_string(),
    )
    .await
    .expect_err("the id is in flight");
    assert_eq!(error, "that private Ask is already running");
}
