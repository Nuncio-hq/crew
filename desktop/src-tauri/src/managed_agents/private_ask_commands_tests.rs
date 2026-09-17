//! The command layer refuses before it reads anything, and never invents an
//! answer the admission fences did not authorise.

use super::super::private_ask::dev_gate::dev_gate_open;
use super::*;

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
    let error = private_ask_run("What does answer do?".to_string())
        .await
        .expect_err("a private Ask must not answer without a bound selection");
    assert_eq!(
        error,
        crate::managed_agents::private_ask::dev_run("What does answer do?")
            .expect_err("no selection is bound")
            .to_string()
    );
}

#[tokio::test]
async fn an_empty_question_is_refused_without_reaching_admission() {
    for blank in ["", "   ", "\n", "\t "] {
        let error = private_ask_run(blank.to_string())
            .await
            .expect_err("a blank question is not a question");
        assert_eq!(error, "a private Ask needs a question");
    }
}

#[tokio::test]
async fn the_status_command_reports_the_blocking_reason_when_it_is_open() {
    let status = private_ask_dev_status().await.expect("status");
    assert!(status.enabled, "debug build opens the surface");
    let expected = crate::managed_agents::private_ask::dev_run("status probe")
        .expect_err("no selection is bound")
        .to_string();
    assert_eq!(status.blocked_reason.as_deref(), Some(expected.as_str()));
}

/// Cancelling twice, or cancelling something that already finished, is not an
/// error — a renderer that does so has not misbehaved.
#[tokio::test]
async fn cancelling_an_unknown_or_finished_attempt_is_a_no_op() {
    private_ask_cancel(None).await.expect("cancel with no id");
    private_ask_cancel(Some("attempt-that-never-existed".to_string()))
        .await
        .expect("cancel unknown id");
    private_ask_cancel(Some("attempt-that-never-existed".to_string()))
        .await
        .expect("cancelling twice is a no-op");
}
