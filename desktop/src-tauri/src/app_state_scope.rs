//! Native scope capture for owner-local recovery; never accepts renderer identity.
use std::sync::{atomic::Ordering, MutexGuard};

use nostr::Keys;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::app_state::{AppState, RecoveryState, ResolvedIdentity};
use crate::owner_operations::OperationScope;

pub(crate) const OWNER_SCOPE_STALE: &str =
    "active owner or workspace changed; return to the originating scope to recover";

/// Check immediately before and after a domain-owned external step.
/// The domain must still use the operation's immutable captured transport/event.
pub(crate) async fn assert_current<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    expected: &OwnerScopeToken,
) -> Result<(), String> {
    if capture(app).await?.token != *expected {
        return Err(OWNER_SCOPE_STALE.into());
    }
    Ok(())
}

/// Check a captured scope without awaiting the workspace lock.
///
/// Callers use this only while holding the workspace-apply guard and the
/// identity-mutation guard that protect the final launch seam. The synchronous
/// check lets that seam run on a blocking worker immediately before it hands a
/// command to the child-process owner; an async capture there would yield and
/// reopen the very race this fence closes.
pub(crate) fn assert_current_blocking<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    expected: &OwnerScopeToken,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.workspace_apply_generation.load(Ordering::Acquire) != expected.workspace_generation
        || state.identity_generation.load(Ordering::Acquire) != expected.identity_generation
    {
        return Err(OWNER_SCOPE_STALE.into());
    }

    let owner = state
        .keys
        .lock()
        .map_err(|_| OWNER_SCOPE_STALE.to_string())?
        .public_key()
        .to_hex();
    if owner != expected.scope.owner {
        return Err(OWNER_SCOPE_STALE.into());
    }

    let relay_url = crate::relay::relay_ws_url_with_override(&state);
    if canonical_origin(&relay_url)? != expected.scope.community {
        return Err(OWNER_SCOPE_STALE.into());
    }
    Ok(())
}

#[cfg(test)]
thread_local! {
    static KEY_LOCK_SIGNAL: std::cell::RefCell<Option<std::sync::mpsc::Sender<()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn signal_key_lock_to(sender: std::sync::mpsc::Sender<()>) {
    KEY_LOCK_SIGNAL.with(|slot| *slot.borrow_mut() = Some(sender));
}

#[cfg(test)]
fn key_lock_checkpoint() {
    KEY_LOCK_SIGNAL.with(|slot| {
        if let Some(sender) = slot.borrow().as_ref() {
            let _ = sender.send(());
        }
    });
}

/// Public fencing metadata; contains neither keys nor private payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OwnerScopeToken {
    /// Native signing owner and canonical community origin.
    pub scope: OperationScope,
    /// Serialized workspace-apply generation.
    pub workspace_generation: u64,
    /// Key-replacement generation, including A-B-A transitions.
    pub identity_generation: u64,
}

/// Keys and transport captured together while native mutation locks are held.
pub(crate) struct CapturedOwnerScope {
    /// Scope fence for every later completion or external step.
    pub token: OwnerScopeToken,
    /// Captured native keys; never serialized to the renderer.
    pub keys: Keys,
    /// Capture-time transport URL; attempted operations use their stored URL.
    pub relay_url: String,
}

/// Capture on a blocking worker, with workspace-before-identity lock order.
/// Both guards are released before this future returns or external IO begins.
#[deny(clippy::await_holding_lock)]
pub(crate) async fn capture<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<CapturedOwnerScope, String> {
    let workspace = app
        .state::<AppState>()
        .workspace_apply_lock
        .clone()
        .lock_owned()
        .await;
    tokio::task::spawn_blocking(move || {
        let _workspace = workspace;
        let state = app.state::<AppState>();
        let identity = state
            .identity_mutation
            .lock()
            .map_err(|_| "identity lock unavailable")?;
        state.capture_owner_scope(&identity)
    })
    .await
    .map_err(|_| "scope capture failed".to_string())?
}

fn canonical_origin(relay: &str) -> Result<String, String> {
    let mut url = url::Url::parse(relay).map_err(|_| "invalid recovery relay origin")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err("invalid recovery relay origin".into());
    }
    let scheme = match url.scheme() {
        "ws" | "http" => "http",
        "wss" | "https" => "https",
        _ => return Err("invalid recovery relay origin".into()),
    };
    url.set_scheme(scheme)
        .map_err(|_| "invalid recovery relay origin")?;
    Ok(url.origin().ascii_serialization())
}

impl AppState {
    /// Install startup resolution under the same identity mutation boundary.
    pub(crate) fn install_resolved_identity(
        &self,
        resolved: ResolvedIdentity,
    ) -> Result<(), String> {
        let guard = self
            .identity_mutation
            .lock()
            .map_err(|_| "identity lock unavailable")?;
        self.replace_identity_keys(&guard, resolved.keys)?;
        self.set_identity_storage(resolved.storage);
        self.identity_lost
            .store(resolved.recovery == RecoveryState::Lost, Ordering::Release);
        self.keyring_locked.store(
            resolved.recovery == RecoveryState::KeyringLocked,
            Ordering::Release,
        );
        Ok(())
    }

    /// Replace keys only under the caller's held identity-mutation guard.
    pub(crate) fn replace_identity_keys(
        &self,
        identity_guard: &MutexGuard<'_, ()>,
        keys: Keys,
    ) -> Result<(), String> {
        self.replace_identity_fields(identity_guard, Some(keys), None)
    }

    /// Apply the workspace pair atomically in existing keys-then-relay order.
    pub(crate) fn replace_workspace_identity(
        &self,
        identity_guard: &MutexGuard<'_, ()>,
        keys: Option<Keys>,
        relay_url: String,
    ) -> Result<(), String> {
        self.replace_identity_fields(identity_guard, keys, Some(relay_url))
    }

    fn replace_identity_fields(
        &self,
        _identity_guard: &MutexGuard<'_, ()>,
        keys: Option<Keys>,
        relay_url: Option<String>,
    ) -> Result<(), String> {
        let mut active = self.keys.lock().map_err(|_| "identity lock unavailable")?;
        #[cfg(test)]
        key_lock_checkpoint();
        // get_active_workspace holds keys while reading relay; never invert it.
        let relay_guard = if relay_url.is_some() {
            Some(
                self.relay_url_override
                    .lock()
                    .map_err(|_| "relay lock unavailable")?,
            )
        } else {
            None
        };
        // All needed locks are acquired before any field changes.
        if let Some(keys) = keys {
            self.identity_generation
                .fetch_update(Ordering::Release, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| "identity generation exhausted")?;
            *active = keys;
        }
        if let (Some(mut relay_guard), Some(relay_url)) = (relay_guard, relay_url) {
            *relay_guard = Some(relay_url);
        }
        Ok(())
    }

    /// Caller holds workspace lock first, then this identity guard off executor.
    /// Calling signing_keys here preserves lost/locked/reset recovery failures.
    pub(crate) fn capture_owner_scope(
        &self,
        _identity_guard: &MutexGuard<'_, ()>,
    ) -> Result<CapturedOwnerScope, String> {
        if self.reset_failed.load(Ordering::Acquire) {
            return Err("identity reset is incomplete".into());
        }
        let keys = self.signing_keys()?;
        let relay_url = crate::relay::relay_ws_url_with_override(self);
        let scope = OperationScope {
            owner: keys.public_key().to_hex(),
            community: canonical_origin(&relay_url)?,
        };
        Ok(CapturedOwnerScope {
            token: OwnerScopeToken {
                scope,
                workspace_generation: self.workspace_apply_generation.load(Ordering::Acquire),
                identity_generation: self.identity_generation.load(Ordering::Acquire),
            },
            keys,
            relay_url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{build_app_state, IdentityStorage};

    #[test]
    fn owner_scope_runtime_import_aba_invalidates_capture() {
        let state = build_app_state();
        let directory = tempfile::tempdir().unwrap();
        let guard = state.identity_mutation.lock().unwrap();
        let initial = state.capture_owner_scope(&guard).unwrap();
        for keys in [Keys::generate(), initial.keys.clone()] {
            crate::commands::commit_imported_identity(
                &state,
                &guard,
                directory.path(),
                keys,
                |_| Ok(IdentityStorage::LocalFile),
            )
            .unwrap();
        }
        let after = state.capture_owner_scope(&guard).unwrap();
        assert_eq!(initial.token.scope, after.token.scope);
        assert_ne!(
            initial.token, after.token,
            "runtime import A-B-A must fence old work"
        );
    }

    #[test]
    fn owner_scope_preserves_recovery_rejection() {
        let state = build_app_state();
        let guard = state.identity_mutation.lock().unwrap();
        for flag in [
            &state.identity_lost,
            &state.keyring_locked,
            &state.reset_failed,
        ] {
            flag.store(true, Ordering::Release);
            assert!(state.capture_owner_scope(&guard).is_err());
            flag.store(false, Ordering::Release);
        }
    }

    #[test]
    fn owner_scope_startup_resolution_advances_epoch_and_recovery_state() {
        let state = build_app_state();
        let before = state.identity_generation.load(Ordering::Acquire);
        state
            .install_resolved_identity(ResolvedIdentity {
                keys: Keys::generate(),
                storage: IdentityStorage::LocalFile,
                recovery: RecoveryState::KeyringLocked,
            })
            .unwrap();
        assert!(state.identity_generation.load(Ordering::Acquire) > before);
        let guard = state.identity_mutation.lock().unwrap();
        assert!(state.capture_owner_scope(&guard).is_err());
    }

    #[test]
    fn owner_scope_failed_import_keeps_epoch_and_current_identity() {
        let state = build_app_state();
        let dir = tempfile::tempdir().unwrap();
        let guard = state.identity_mutation.lock().unwrap();
        let before = state.capture_owner_scope(&guard).unwrap().token;
        let result = crate::commands::commit_imported_identity(
            &state,
            &guard,
            dir.path(),
            Keys::generate(),
            |_| Err("fixture persistence failed".into()),
        );
        assert!(result.is_err());
        assert_eq!(state.capture_owner_scope(&guard).unwrap().token, before);
    }

    #[test]
    fn owner_scope_origin_is_host_derived_and_rejects_credentials() {
        assert_eq!(
            canonical_origin("wss://EXAMPLE.com:443/relay").unwrap(),
            "https://example.com"
        );
        assert_eq!(
            canonical_origin("ws://localhost:3123/path").unwrap(),
            "http://localhost:3123"
        );
        for bad in [
            "wss://user@example.com",
            "wss://example.com?secret=1",
            "file:///tmp/relay",
        ] {
            assert!(canonical_origin(bad).is_err());
        }
    }
}
