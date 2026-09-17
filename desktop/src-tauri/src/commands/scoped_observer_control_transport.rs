//! Captured WebSocket transport for one selected-run observer control.
//!
//! Observer frames are relay-ephemeral kind:24200 events. They must use the
//! native WebSocket event path because the HTTP bridge intentionally rejects
//! this kind. The transport owns the exact relay URL and signing keys captured
//! by the command; it never resolves either value from ambient app state.

use std::future::Future;
use std::time::Duration;

use nostr::{Event, JsonUtil, Keys};
use url::Url;

use super::OperationTransportError;

const PUBLISH_BUDGET: Duration = Duration::from_secs(10);
const EVENT_LIMIT: usize = 1024 * 1024;

/// Captured destination for one observer-control publication.
pub(super) struct ScopedObserverControlTransport {
    relay_url: String,
    keys: Keys,
}

impl ScopedObserverControlTransport {
    /// Stores the exact captured WebSocket URL and native signing keys.
    pub(super) fn captured(relay_url: String, keys: Keys) -> Result<Self, OperationTransportError> {
        validate_relay_url(&relay_url).map_err(OperationTransportError::InvalidInput)?;
        Ok(Self { relay_url, keys })
    }

    /// Publishes one already-signed observer event over an authenticated
    /// WebSocket, fencing the owner after asynchronous connection/auth setup.
    pub(super) async fn publish<F, Fut>(
        &self,
        event: &Event,
        before_send: F,
    ) -> Result<crate::relay::SubmitEventResponse, OperationTransportError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        if event.pubkey != self.keys.public_key() || event.verify().is_err() {
            return Err(OperationTransportError::InvalidInput(
                "Owner operation has an invalid signed envelope.".into(),
            ));
        }
        let serialized = event.as_json().into_bytes();
        if serialized.len() > EVENT_LIMIT {
            return Err(OperationTransportError::InvalidInput(
                "Owner operation request exceeds 1 MiB.".into(),
            ));
        }
        crate::egress_guard::assert_no_key_backup_bytes(&serialized, "Owner operation")
            .map_err(OperationTransportError::InvalidInput)?;

        let deadline = tokio::time::Instant::now() + PUBLISH_BUDGET;
        tokio::time::timeout_at(deadline, crate::relay_admission::wait_for_rate_limit())
            .await
            .map_err(|_| {
                OperationTransportError::NotAttempted(
                    "Relay admission deferred the owner operation; retry later.".into(),
                )
            })?;

        // Check before opening a socket as well as after authentication. The
        // first check preserves the no-connect property when the captured
        // scope is already stale; the second closes the async setup window.
        tokio::time::timeout_at(deadline, before_send())
            .await
            .map_err(|_| {
                OperationTransportError::NotAttempted(
                    "Owner operation budget expired during its native dispatch guard.".into(),
                )
            })?
            .map_err(OperationTransportError::NotAttempted)?;

        // Connection and NIP-42 authentication do not publish the control
        // event. A failure here is therefore safe to report as not attempted.
        let mut connection = tokio::time::timeout_at(
            deadline,
            buzz_ws_client_pkg::NostrWsConnection::connect_authenticated(
                &self.relay_url,
                &self.keys,
                None,
            ),
        )
        .await
        .map_err(|_| {
            OperationTransportError::NotAttempted(
                "Owner operation WebSocket setup exceeded its request budget.".into(),
            )
        })?
        .map_err(|error| {
            OperationTransportError::NotAttempted(format!(
                "Owner operation WebSocket setup failed: {error}"
            ))
        })?;

        // This is deliberately after the async connection/authentication. A
        // scope change during setup must prevent the EVENT frame from leaving.
        tokio::time::timeout_at(deadline, before_send())
            .await
            .map_err(|_| {
                OperationTransportError::NotAttempted(
                    "Owner operation budget expired during its native dispatch guard.".into(),
                )
            })?
            .map_err(OperationTransportError::NotAttempted)?;

        if tokio::time::Instant::now() >= deadline {
            return Err(OperationTransportError::NotAttempted(
                "Owner operation budget expired before send.".into(),
            ));
        }

        // NostrWsConnection::send_event sends EVENT and then waits for OK. Any
        // failure after entering it is outcome-unknown: the relay may have
        // received the control even when its acknowledgement was lost.
        let result = tokio::time::timeout_at(deadline, connection.send_event(event.clone())).await;
        let response = match result {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                return Err(OperationTransportError::OutcomeUnknown {
                    status: None,
                    reason: format!("Owner operation WebSocket publish failed: {error}"),
                });
            }
            Err(_) => {
                return Err(OperationTransportError::OutcomeUnknown {
                    status: None,
                    reason: "Owner operation WebSocket publish exceeded its request budget.".into(),
                });
            }
        };

        Ok(crate::relay::SubmitEventResponse {
            event_id: response.event_id,
            accepted: response.accepted,
            message: response.message,
        })
    }
}

fn validate_relay_url(relay_url: &str) -> Result<(), String> {
    let url = Url::parse(relay_url).map_err(|_| "Invalid Owner operation relay URL.")?;
    if !matches!(url.scheme(), "ws" | "wss")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Owner operation relay must be a canonical WebSocket URL.".into());
    }
    Ok(())
}
