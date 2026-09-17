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
use super::private_ask::history::{self, PrivateAskHistoryEntry};
use super::recap_ownership::VerifiedStagingOwnership;
use serde::Serialize;

/// Seconds since the epoch, or zero when this machine's clock cannot be read.
///
/// Zero is never retained by the history's own bounds, so an unreadable clock
/// loses the record rather than writing an entry that would sit at the head of
/// the list forever.
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

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

/// One finished attempt as the renderer receives it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskRunResult {
    pub attempt_id: String,
    pub markdown: String,
    pub citations: Vec<PrivateAskCitation>,
    /// False when the answer was produced but could not be written to the
    /// owner-local history. It is reported rather than swallowed: the answer is
    /// still an answer, and a history quietly missing it would be a worse lie
    /// than a visible note.
    pub history_recorded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskCitation {
    pub path: String,
    pub start_line: u64,
    pub end_line: u64,
}

/// Report whether the private Ask surface exists in this build.
///
/// This must never launch anything. It is polled whenever the composer mounts,
/// and the run path now reaches a contained process, so a status that asked the
/// production entry point for a reason would start a capability probe on every
/// poll. It stops at the fences that cost nothing: the dev gate, and whether
/// this install has an owned staging tree to run inside at all.
#[tauri::command]
pub async fn private_ask_dev_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<PrivateAskDevStatus, String> {
    let enabled = private_ask_dev_enabled();
    let blocked_reason = if !enabled {
        None
    } else {
        VerifiedStagingOwnership::load(&app)
            .err()
            .map(|_| "this install has no owned staging tree for a private Ask".to_string())
            .or_else(|| {
                // The one remaining reason a developer needs stated up front.
                // It is the binding, not a fence a probe could satisfy.
                super::private_ask::dev_run("status probe")
                    .err()
                    .map(|failure| failure.to_string())
            })
    };
    Ok(PrivateAskDevStatus {
        enabled,
        blocked_reason,
    })
}

/// The viewer's own record of what they asked on this machine.
///
/// It never left this machine: a private Ask publishes nothing, so there is no
/// relay copy to reconcile with and no other reader.
#[tauri::command]
pub async fn private_ask_history<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<PrivateAskHistoryEntry>, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|_| "private Ask history is unavailable".to_string())?;
    Ok(history::load(&ownership, now_seconds()))
}

/// Run one private Ask and return its answer.
///
/// The answer and every refusal come from `private_ask::dev_run`, which reaches
/// `admit_private_ask` and `PrivateAskAttempt::run`. This command adds only the
/// dev gate and the empty-question check; it never invents a result, and it
/// never decides a reason of its own.
#[tauri::command]
pub async fn private_ask_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    question: String,
) -> Result<PrivateAskRunResult, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if question.trim().is_empty() {
        return Err("a private Ask needs a question".to_string());
    }
    let asked_at = now_seconds();
    // The attempt is synchronous and bounded by `PRIVATE_ASK_TIMEOUT`, so it is
    // moved off the async runtime rather than blocking every other command for
    // the length of a model call. It still runs on ONE thread, which is what the
    // thread-scoped no-publish attribution relies on.
    let question_for_run = question.clone();
    let outcome =
        tokio::task::spawn_blocking(move || super::private_ask::dev_run(&question_for_run))
            .await
            .map_err(|_| "private Ask did not complete safely".to_string())?;

    // History is written on both outcomes, before anything is returned: a
    // refusal is the attempt a developer most needs to look back at.
    let ownership = VerifiedStagingOwnership::load(&app).ok();
    match outcome {
        Ok(response) => {
            let entry = PrivateAskHistoryEntry::answered(&question, &response, asked_at);
            let history_recorded = ownership
                .is_some_and(|ownership| history::record(&ownership, entry, asked_at).is_ok());
            Ok(PrivateAskRunResult {
                attempt_id: response.attempt_id,
                markdown: response.markdown,
                citations: response
                    .citations
                    .iter()
                    .map(|citation| PrivateAskCitation {
                        path: citation.path().to_owned(),
                        start_line: citation.start_line(),
                        end_line: citation.end_line(),
                    })
                    .collect(),
                history_recorded,
            })
        }
        Err(failure) => {
            if let Some(ownership) = ownership {
                let entry = PrivateAskHistoryEntry::refused(
                    &question,
                    &uuid::Uuid::new_v4().to_string(),
                    &failure,
                    asked_at,
                );
                // A history that could not be written must not change the
                // reason the viewer is given for the refusal itself.
                let _ = history::record(&ownership, entry, asked_at);
            }
            Err(failure.to_string())
        }
    }
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
