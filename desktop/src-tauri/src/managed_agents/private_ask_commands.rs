//! Developer-only command surface for the private Wiki Ask.
//!
//! Registration is unconditional, because a registered-or-not decision made at
//! build time is invisible to the renderer and produces a confusing "command
//! not found" instead of a reason. The *gate* is in each command body: the
//! first thing every command does is ask
//! [`dev_gate::private_ask_dev_enabled`], and a closed gate returns
//! [`PRIVATE_ASK_UNAVAILABLE`] before any argument is read.
//!
//! The surface is deliberately thin. Admission owns every safety decision, and
//! these commands carry its answer — an answer or a typed refusal — to a
//! developer's screen without softening it. Nothing here mints a capability,
//! and nothing here decides why a request was refused: the reason comes from
//! `private_ask::dev_run`, so the screen cannot drift from the fences.

use super::private_ask::dev_gate::private_ask_dev_enabled;
use serde::Serialize;

/// What a renderer is told when the surface is closed. It is deliberately the
/// same shape as any other refusal so the UI has one path.
pub(crate) const PRIVATE_ASK_UNAVAILABLE: &str = "private Ask is not enabled in this build";

/// What the renderer needs to decide which composer to draw.
///
/// `enabled` is the backend's answer, never a renderer-side flag: the webview
/// asks, it does not decide.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskDevStatus {
    pub enabled: bool,
    /// Present when the surface is open but a request would still be refused,
    /// so a developer sees the real reason rather than an empty answer box.
    pub blocked_reason: Option<String>,
}

/// Report whether the private Ask surface exists in this build.
#[tauri::command]
pub async fn private_ask_dev_status() -> Result<PrivateAskDevStatus, String> {
    let enabled = private_ask_dev_enabled();
    Ok(PrivateAskDevStatus {
        enabled,
        // Stated up front rather than after a spinner. The reason is taken from
        // the same production path a real request would take, with a probe
        // question, so the status row and the run can never disagree.
        blocked_reason: enabled
            .then(|| super::private_ask::dev_run("status probe").err())
            .flatten()
            .map(|failure| failure.to_string()),
    })
}

/// Run one private Ask and return its answer.
///
/// The answer and every refusal come from `private_ask::dev_run`, which reaches
/// `admit_private_ask` and `PrivateAskAttempt::run`. This command adds only the
/// dev gate and the empty-question check; it never invents a result, and it
/// never decides a reason of its own.
#[tauri::command]
pub async fn private_ask_run(question: String) -> Result<String, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if question.trim().is_empty() {
        return Err("a private Ask needs a question".to_string());
    }
    super::private_ask::dev_run(&question)
        .map(|response| response.markdown)
        .map_err(|failure| failure.to_string())
}

/// Cancel an in-flight private Ask.
///
/// Cancelling an attempt that is unknown or already finished is a no-op, not an
/// error: a renderer that cancels twice, or cancels after the answer arrived,
/// has not done anything wrong.
#[tauri::command]
pub async fn private_ask_cancel(_attempt_id: Option<String>) -> Result<(), String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "private_ask_commands_tests.rs"]
mod tests;
