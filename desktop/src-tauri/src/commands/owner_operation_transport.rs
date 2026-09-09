//! Bounded transport for a natively captured Owner operation destination.
//!
//! The domain dispatcher owns journal leases, scope checks, event validation,
//! and persistence before attempts. This adapter never reads ambient identity.

use std::future::Future;
use std::time::{Duration, Instant};

use nostr::{JsonUtil, Keys};
use reqwest::{Client, Method, Url};
use serde::de::DeserializeOwned;

use crate::relay::{build_nip98_auth_header_for_keys, SubmitEventResponse};

// The relay HTTP bridge caps each complete serialized request at 1 MiB.
const REQUEST_LIMIT: usize = 1024 * 1024;
const RESPONSE_LIMIT: usize = 1024 * 1024;
const REQUEST_BUDGET: Duration = Duration::from_secs(10);

/// Advertised relay extensions; missing metadata never enables a capability.
#[derive(serde::Deserialize)]
pub(super) struct OwnerRelayInformation {
    pub(super) supported_extensions: Option<Vec<String>>,
}

/// Errors never authorize deletion of an operation with unresolved prior effects.
#[derive(Debug)]
pub(super) enum OperationTransportError {
    /// Invalid input refused before any request in this call.
    InvalidInput(String),
    /// Admission exhausted the budget before signing or sending this request.
    NotAttempted(String),
    /// Parsed HTTP refusal. Domain code interprets its reason and reconciles;
    /// even a 400 response does not prove earlier side effects are absent.
    RelayResponse { status: u16, reason: String },
    /// Delivery may have committed. Preserve the journal and reconcile/replay.
    OutcomeUnknown { status: Option<u16>, reason: String },
}

impl std::fmt::Display for OperationTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(reason) | Self::NotAttempted(reason) => f.write_str(reason),
            Self::RelayResponse { status, reason } => write!(f, "Relay HTTP {status}: {reason}"),
            Self::OutcomeUnknown { status, reason } => {
                if let Some(status) = status {
                    write!(f, "Relay HTTP {status}: ")?;
                }
                f.write_str(reason)
            }
        }
    }
}
impl std::error::Error for OperationTransportError {}

fn unknown(reason: impl Into<String>) -> OperationTransportError {
    OperationTransportError::OutcomeUnknown {
        status: None,
        reason: reason.into(),
    }
}

/// Captured identity and immutable canonical HTTP origin for one operation.
pub(super) struct OwnerOperationTransport {
    client: Client,
    origin: String,
    keys: Keys,
    auth_tag: Option<String>,
}

impl OwnerOperationTransport {
    /// Read NIP-11 only from the captured origin with the same dispatch fence.
    pub(super) async fn relay_information<F: Future<Output = Result<(), String>>>(
        &self,
        before_send: F,
    ) -> Result<OwnerRelayInformation, OperationTransportError> {
        self.json(Method::GET, "/", Vec::new(), before_send).await
    }

    /// Use the app's no-redirect client; never its general redirecting client.
    pub(super) fn captured(
        state: &crate::AppState,
        origin: String,
        keys: Keys,
        auth_tag: Option<String>,
    ) -> Result<Self, OperationTransportError> {
        validate_origin(&origin).map_err(OperationTransportError::InvalidInput)?;
        Ok(Self {
            client: state.media_fetch_client.clone(),
            origin,
            keys,
            auth_tag,
        })
    }

    /// Submit the exact persisted envelope without re-signing or retargeting.
    pub(super) async fn publish<F: Future<Output = Result<(), String>>>(
        &self,
        event: &nostr::Event,
        before_send: F,
    ) -> Result<SubmitEventResponse, OperationTransportError> {
        if event.pubkey != self.keys.public_key() || event.verify().is_err() {
            return Err(OperationTransportError::InvalidInput(
                "Owner operation has an invalid signed envelope.".into(),
            ));
        }
        let result: SubmitEventResponse = self
            .json(
                Method::POST,
                "/events",
                event.as_json().into_bytes(),
                before_send,
            )
            .await?;
        if result.event_id != event.id.to_hex() {
            return Err(unknown(
                "Relay acknowledged a different owner operation event.",
            ));
        }
        // Rejections retain their named relay outcome for domain reconciliation.
        Ok(result)
    }

    /// Query explicit bounded kinds; domain code verifies signatures and heads.
    pub(super) async fn query<F: Future<Output = Result<(), String>>>(
        &self,
        filter: serde_json::Value,
        before_send: F,
    ) -> Result<Vec<nostr::Event>, OperationTransportError> {
        if !filter
            .get("kinds")
            .and_then(|v| v.as_array())
            .is_some_and(|k| {
                !k.is_empty() && k.len() <= 8 && k.iter().all(|v| v.as_u64().is_some())
            })
            || !filter
                .get("limit")
                .and_then(|v| v.as_u64())
                .is_some_and(|limit| (1..=100).contains(&limit))
        {
            return Err(OperationTransportError::InvalidInput(
                "Owner operation requires a bounded explicit-kind query.".into(),
            ));
        }
        let body = serde_json::to_vec(&[filter]).map_err(|_| {
            OperationTransportError::InvalidInput("Could not encode owner operation query.".into())
        })?;
        self.json(Method::POST, "/query", body, before_send).await
    }

    async fn json<T: DeserializeOwned, F: Future<Output = Result<(), String>>>(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
        before_send: F,
    ) -> Result<T, OperationTransportError> {
        if body.len() > REQUEST_LIMIT {
            return Err(OperationTransportError::InvalidInput(
                "Owner operation request exceeds 1 MiB.".into(),
            ));
        }
        crate::egress_guard::assert_no_key_backup_bytes(&body, "Owner operation")
            .map_err(OperationTransportError::InvalidInput)?;
        let bytes = self
            .request(method, path, body, REQUEST_BUDGET, before_send)
            .await?;
        serde_json::from_slice(&bytes)
            .map_err(|_| unknown("Relay returned an invalid owner operation response."))
    }

    async fn request<F: Future<Output = Result<(), String>>>(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
        budget: Duration,
        before_send: F,
    ) -> Result<Vec<u8>, OperationTransportError> {
        // Paths are native constants, never a persisted arbitrary request URL.
        if !matches!(
            (method.as_str(), path),
            ("POST", "/events" | "/query") | ("GET", "/")
        ) {
            return Err(OperationTransportError::InvalidInput(
                "Unsupported owner operation endpoint.".into(),
            ));
        }
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + budget;
        tokio::time::timeout_at(deadline, crate::relay_admission::wait_for_rate_limit())
            .await
            .map_err(|_| {
                OperationTransportError::NotAttempted(
                    "Relay admission deferred the owner operation; retry later.".into(),
                )
            })?;
        // The caller's native scope/lease check runs after the admission await.
        // Its future must not perform the external operation itself.
        tokio::time::timeout_at(deadline, before_send)
            .await
            .map_err(|_| {
                OperationTransportError::NotAttempted(
                    "Owner operation budget expired during its native dispatch guard.".into(),
                )
            })?
            .map_err(OperationTransportError::NotAttempted)?;
        let remaining = budget
            .checked_sub(started.elapsed())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                OperationTransportError::NotAttempted(
                    "Owner operation budget expired before send.".into(),
                )
            })?;
        let url = format!("{}{path}", self.origin);
        // Auth timestamps are minted only after admission has cleared.
        let auth = build_nip98_auth_header_for_keys(&self.keys, &method, &url, &body)
            .map_err(OperationTransportError::InvalidInput)?;
        let mut request = self
            .client
            .request(method, &url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .timeout(remaining)
            .body(body);
        if path == "/" {
            request = request.header("Accept", "application/nostr+json");
        }
        if let Some(tag) = &self.auth_tag {
            request = request.header("x-auth-tag", tag);
        }
        tokio::time::timeout_at(deadline, async {
            let response = request
                .send()
                .await
                .map_err(|_| unknown("Owner operation relay request failed."))?;
            let status = response.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                // Honour overload even if its response body stalls or is invalid.
                crate::relay_admission::activate_rate_limit(None);
            }
            let bytes = bounded_body(response).await.map_err(unknown)?;
            if status.is_success() {
                return Ok(bytes);
            }
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let hint = crate::relay::extract_retry_in_hint(&String::from_utf8_lossy(&bytes));
                crate::relay_admission::activate_rate_limit(hint);
            }
            let reason = response_reason(&bytes);
            if status.is_server_error()
                || status == reqwest::StatusCode::REQUEST_TIMEOUT
                || reason.starts_with("error:")
            {
                return Err(OperationTransportError::OutcomeUnknown {
                    status: Some(status.as_u16()),
                    reason,
                });
            }
            Err(OperationTransportError::RelayResponse {
                status: status.as_u16(),
                reason,
            })
        })
        .await
        .map_err(|_| unknown("Owner operation relay exceeded its request budget."))?
    }
}

fn validate_origin(origin: &str) -> Result<(), String> {
    let url = Url::parse(origin).map_err(|_| "Invalid Owner operation relay origin.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.origin().ascii_serialization() != origin
    {
        return Err("Owner operation relay must be a canonical HTTP origin.".into());
    }
    Ok(())
}

async fn bounded_body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|size| size > RESPONSE_LIMIT as u64)
    {
        return Err("Owner operation response exceeds 1 MiB.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read Owner operation response.".to_string())?
    {
        if chunk.len() > RESPONSE_LIMIT.saturating_sub(bytes.len()) {
            return Err("Owner operation response exceeds 1 MiB.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn response_reason(bytes: &[u8]) -> String {
    let value: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => return "Relay response has no structured reason.".into(),
    };
    let reason = value
        .get("error")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
        .unwrap_or("Relay refused the request.");
    let mut end = reason.len().min(256);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    reason[..end].to_owned()
}

#[cfg(test)]
#[path = "owner_operation_transport_tests.rs"]
mod tests;
