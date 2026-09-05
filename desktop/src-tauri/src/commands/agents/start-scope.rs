//! Captured send scope and per-invocation replay input for detached agent wakes.

use crate::{app_state::AppState, relay};

#[derive(Default, Clone)]
pub(super) struct AgentStartScope {
    pub expected_relay_url: Option<String>,
    pub expected_signer_pubkey: Option<String>,
    pub replay_floor_unix: Option<u64>,
}

impl AgentStartScope {
    /// Detached wakes require both scope assertions; ordinary starts remain unscoped.
    pub fn validate(&self) -> Result<(), String> {
        if self.expected_relay_url.is_some()
            || self.expected_signer_pubkey.is_some()
            || self.replay_floor_unix.is_some()
        {
            let present =
                |value: &Option<String>| value.as_deref().is_some_and(|v| !v.trim().is_empty());
            if !present(&self.expected_relay_url) || !present(&self.expected_signer_pubkey) {
                return Err(
                    "agent wake requires a captured community and identity; not started".into(),
                );
            }
        }
        Ok(())
    }

    /// Bind the exact live values after preflight, before the launch consumes them.
    pub fn bind(
        &self,
        state: &AppState,
    ) -> Result<(relay::ScopedWorkspaceRelay, relay::ScopedWorkspaceSigner), String> {
        self.validate()?;
        let relay = relay::bind_expected_relay_scope(
            self.expected_relay_url.as_deref(),
            relay::relay_ws_url_with_override(state),
        )?;
        let signer = relay::bind_expected_signer(
            self.expected_signer_pubkey.as_deref(),
            super::workspace_owner_hex(state)?,
        )?;
        Ok((relay, signer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_starts_remain_unscoped_but_detached_wakes_require_complete_scope() {
        assert!(AgentStartScope::default().validate().is_ok());
        for scope in [
            AgentStartScope {
                replay_floor_unix: Some(42),
                ..Default::default()
            },
            AgentStartScope {
                expected_relay_url: Some("wss://a".into()),
                ..Default::default()
            },
            AgentStartScope {
                expected_relay_url: Some(" ".into()),
                expected_signer_pubkey: Some("owner".into()),
                ..Default::default()
            },
        ] {
            assert!(scope.validate().is_err());
        }
        assert!(AgentStartScope {
            expected_relay_url: Some("wss://a".into()),
            expected_signer_pubkey: Some("owner".into()),
            replay_floor_unix: Some(42)
        }
        .validate()
        .is_ok());
    }
    #[tokio::test]
    async fn local_preflight_rebind_rejects_switches_and_preserves_checked_pair_afterward() {
        let state = crate::app_state::build_app_state();
        *state.relay_url_override.lock().unwrap() = Some("wss://tenant-a.example".into());
        let owner = super::super::workspace_owner_hex(&state).unwrap();
        let scope = AgentStartScope {
            expected_relay_url: Some("wss://tenant-a.example".into()),
            expected_signer_pubkey: Some(owner),
            replay_floor_unix: Some(1000),
        };
        let (checked_relay, checked_owner) = scope.bind(&state).unwrap();
        // Simulate a switch during the awaited preflight: the final bind refuses it.
        tokio::task::yield_now().await;
        *state.relay_url_override.lock().unwrap() = Some("wss://tenant-b.example".into());
        assert!(scope
            .bind(&state)
            .unwrap_err()
            .contains("active community changed"));
        // A switch AFTER binding cannot retarget the exact spawn inputs.
        let key = crate::managed_agents::ManagedAgentRuntimeKey::new(
            "a".repeat(64),
            checked_relay.as_str(),
        )
        .unwrap();
        assert_eq!(key.relay_url, "wss://tenant-a.example");
        assert_eq!(
            checked_owner.as_str(),
            scope.expected_signer_pubkey.as_deref().unwrap()
        );
        *state.relay_url_override.lock().unwrap() = Some("wss://tenant-a.example".into());
        *state.keys.lock().unwrap() = nostr::Keys::generate();
        assert!(scope
            .bind(&state)
            .unwrap_err()
            .contains("active identity changed"));
    }
}
