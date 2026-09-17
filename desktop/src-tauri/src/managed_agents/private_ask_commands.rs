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
//! today it refuses every request (see `docs/crew/DECISIONS.md` D-083), so
//! these commands exist to carry that refusal to a developer's screen —  not to
//! soften it. Nothing here mints a capability.

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
        blocked_reason: enabled.then(|| {
            // Stated up front rather than after a spinner: no producer can
            // currently bound the run's egress to the model provider, so every
            // request is refused at admission.
            super::private_ask::PrivateAskFailure::EgressBoundUnverified.to_string()
        }),
    })
}

/// Run one private Ask.
///
/// Every safety decision belongs to `admit_private_ask`, which today refuses
/// unconditionally. This command therefore returns that refusal rather than a
/// placeholder answer: a developer surface that fakes a result would hide the
/// exact thing the feature is gated on.
#[tauri::command]
pub async fn private_ask_run(question: String) -> Result<String, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if question.trim().is_empty() {
        return Err("a private Ask needs a question".to_string());
    }
    Err(super::private_ask::PrivateAskFailure::EgressBoundUnverified.to_string())
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
