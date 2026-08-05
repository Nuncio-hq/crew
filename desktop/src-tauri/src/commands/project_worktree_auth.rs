//! Fail-closed authorization for Project worktree mutations.
//!
//! Preview/UI gating is advisory only. Every destructive IPC path must call
//! [`authorize_verified_channel_mutation`] (or the thread-panel equivalent)
//! and revalidate under an exclusive lease before mutating disk.

use std::path::{Path, PathBuf};

use buzz_worktree::{
    read_lifecycle_record, try_acquire_exclusive, ExclusiveLease, LeaseError, LifecycleRecord,
};

use super::project_worktree_cleanup::{prepare_managed_removal, PreparedRemoval};
use super::thread_workspace::{ThreadWorkspaceActionResult, ThreadWorkspaceActionStatus};

/// Successfully authorized mutation target. Holds the exclusive lease for the
/// duration of the caller's destructive work.
pub(crate) struct AuthorizedChannelMutation {
    pub(crate) prepared: PreparedRemoval,
    pub(crate) root_event_id: String,
    pub(crate) record: LifecycleRecord,
    pub(crate) lease: ExclusiveLease,
}

/// Capability projection for reclaim preview. Not authorization.
#[derive(Debug, Clone)]
pub(crate) struct ReclaimCapabilities {
    pub(crate) can_clear_cache: bool,
    pub(crate) can_evict: bool,
    pub(crate) clear_cache_refusal: Option<String>,
    pub(crate) eviction_refusal: Option<String>,
}

/// Authorize a channel-scoped destructive mutation. Missing root, missing or
/// conflicting lifecycle record, other-channel routing, or a busy lease all
/// return a typed refusal. Path/branch mismatches return `Err` (hard failure).
pub(crate) async fn authorize_verified_channel_mutation(
    repository_path: &str,
    worktree_path: &str,
    expected_routing_channel_id: &str,
) -> Result<Result<AuthorizedChannelMutation, ThreadWorkspaceActionResult>, String> {
    let expected = expected_routing_channel_id.trim();
    if expected.is_empty() {
        return Ok(Err(refused(
            "This worktree action requires the current channel identity.",
        )));
    }

    let prepared = prepare_managed_removal(repository_path, worktree_path).await?;
    match evaluate_channel_authorization(&prepared, expected) {
        AuthDecision::Refuse(message) => Ok(Err(refused(&message))),
        AuthDecision::Ok { root, record: _ } => {
            let lease = match try_acquire_exclusive(&prepared.common_git, &root) {
                Ok(lease) => lease,
                Err(LeaseError::Busy) => {
                    return Ok(Err(refused(
                        "This worktree action is unavailable while an agent is using this worktree.",
                    )));
                }
                Err(error) => {
                    return Err(format!(
                        "Could not acquire worktree eviction lease: {error}"
                    ));
                }
            };

            // Revalidate under the exclusive lease before the caller mutates.
            let revalidated = prepare_managed_removal(repository_path, worktree_path).await?;
            if revalidated.worktree != prepared.worktree
                || revalidated.common_git != prepared.common_git
                || revalidated.branch != prepared.branch
                || revalidated.root_event_id.as_deref() != Some(root.as_str())
            {
                return Ok(Err(refused(
                    "Worktree changed during eviction authorization; try again.",
                )));
            }
            match evaluate_channel_authorization(&revalidated, expected) {
                AuthDecision::Ok {
                    root: root2,
                    record: record2,
                } => Ok(Ok(AuthorizedChannelMutation {
                    prepared: revalidated,
                    root_event_id: root2,
                    record: record2,
                    lease,
                })),
                AuthDecision::Refuse(message) => Ok(Err(refused(&message))),
            }
        }
    }
}

/// Preview-only capability projection. Must mirror refusal classes used by
/// mutation authorization, but never grants authority.
pub(crate) fn project_reclaim_capabilities(
    prepared: &PreparedRemoval,
    expected_routing_channel_id: Option<&str>,
    dirty: bool,
    busy: bool,
    has_ignored_local_state: bool,
) -> ReclaimCapabilities {
    let expected = expected_routing_channel_id.map(str::trim).unwrap_or("");
    let auth = if expected.is_empty() {
        AuthDecision::Refuse(
            "Cache clear and free local space require the current channel identity.".into(),
        )
    } else {
        evaluate_channel_authorization(prepared, expected)
    };

    match auth {
        AuthDecision::Ok { .. } => {
            let clear_cache_refusal = if busy {
                Some("An agent is using this worktree.".to_string())
            } else {
                None
            };
            let eviction_refusal = if busy {
                Some("An agent is using this worktree.".to_string())
            } else if dirty {
                Some(
                    "Free local space is unavailable while files have uncommitted changes."
                        .to_string(),
                )
            } else if has_ignored_local_state {
                Some(IGNORED_LOCAL_EVICTION_REFUSAL.to_string())
            } else {
                None
            };
            ReclaimCapabilities {
                can_clear_cache: clear_cache_refusal.is_none(),
                can_evict: eviction_refusal.is_none(),
                clear_cache_refusal,
                eviction_refusal,
            }
        }
        AuthDecision::Refuse(message) => ReclaimCapabilities {
            can_clear_cache: false,
            can_evict: false,
            clear_cache_refusal: Some(message.clone()),
            eviction_refusal: Some(message),
        },
    }
}

/// Shared copy for eviction refusals when ignored/local checkout state remains.
pub(crate) const IGNORED_LOCAL_EVICTION_REFUSAL: &str = "Free local space is unavailable while ignored or other local files remain. Clear generated cache or review local files first.";

enum AuthDecision {
    Ok {
        root: String,
        record: LifecycleRecord,
    },
    Refuse(String),
}

fn evaluate_channel_authorization(
    prepared: &PreparedRemoval,
    expected_routing_channel_id: &str,
) -> AuthDecision {
    let Some(root) = prepared.root_event_id.as_deref() else {
        return AuthDecision::Refuse(
            "This worktree action is unavailable for legacy worktrees without a verified root claim."
                .into(),
        );
    };
    let record = match read_lifecycle_record(&prepared.common_git, root) {
        Ok(Some(record)) => record,
        Ok(None) => {
            return AuthDecision::Refuse(
                "This worktree action is unavailable until a trusted agent turn verifies this worktree."
                    .into(),
            );
        }
        Err(_) => {
            return AuthDecision::Refuse(
                "This worktree action is unavailable while lifecycle metadata conflicts.".into(),
            );
        }
    };
    if !record.root_event_id.eq_ignore_ascii_case(root) {
        return AuthDecision::Refuse(
            "This worktree action is unavailable while lifecycle metadata conflicts.".into(),
        );
    }
    if record.branch != prepared.branch {
        return AuthDecision::Refuse(
            "This worktree action is unavailable while lifecycle metadata conflicts.".into(),
        );
    }
    if !paths_match(&record.worktree_path, &prepared.worktree) {
        return AuthDecision::Refuse(
            "This worktree action is unavailable while lifecycle metadata conflicts.".into(),
        );
    }
    if record.routing_channel_id != expected_routing_channel_id {
        return AuthDecision::Refuse(
            "This worktree action is unavailable for worktrees owned by another channel.".into(),
        );
    }
    AuthDecision::Ok {
        root: root.to_string(),
        record,
    }
}

fn paths_match(recorded: &str, actual: &Path) -> bool {
    let recorded_path = PathBuf::from(recorded);
    if recorded_path == actual {
        return true;
    }
    match (
        std::fs::canonicalize(&recorded_path),
        std::fs::canonicalize(actual),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn refused(message: &str) -> ThreadWorkspaceActionResult {
    ThreadWorkspaceActionResult {
        status: ThreadWorkspaceActionStatus::Refused,
        message: message.to_string(),
    }
}
