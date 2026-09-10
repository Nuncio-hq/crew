//! Relay NIP-11 identity discovery shared by archive and scoped Wiki reads.
//!
//! The archive path intentionally retains its existing per-relay cache. Wiki
//! snapshot reads use the uncached scoped path below so a relay-authoritative
//! kind:30618 event is tied to the same current origin as the snapshot query.

use serde::{de, de::MapAccess, de::Visitor, Deserialize};
use std::fmt;
use tauri::{AppHandle, Manager, Runtime};

use crate::app_state::owner_scope::{self as app_state_scope, OwnerScopeToken};
use crate::app_state::AppState;
use crate::relay::{classify_request_error, relay_http_base_url, relay_ws_url_with_override};

#[derive(Debug)]
pub(crate) struct RelayInformationDocument {
    pub(crate) self_: Option<String>,
}

impl<'de> Deserialize<'de> for RelayInformationDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RelayInformationVisitor;

        impl<'de> Visitor<'de> for RelayInformationVisitor {
            type Value = RelayInformationDocument;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a NIP-11 relay information object")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut self_value: Option<Option<String>> = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "self" {
                        if self_value.is_some() {
                            return Err(de::Error::duplicate_field("self"));
                        }
                        self_value = Some(map.next_value()?);
                    } else {
                        let _: de::IgnoredAny = map.next_value()?;
                    }
                }
                Ok(RelayInformationDocument {
                    self_: self_value.flatten(),
                })
            }
        }

        deserializer.deserialize_map(RelayInformationVisitor)
    }
}

pub(crate) async fn fetch_relay_self(state: &AppState) -> Result<Option<String>, String> {
    fetch_relay_self_at(state, &relay_ws_url_with_override(state)).await
}

/// How long a fetched NIP-11 `self` pubkey stays valid in
/// [`AppState::relay_self_cache`]. The relay's signing identity changes only
/// on an operator-driven key rotation, so minutes of staleness are safe; the
/// TTL exists so even that rare rotation converges without an app restart.
pub(crate) const RELAY_SELF_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Read a still-fresh cached `self` pubkey for `relay_url`, if any. Fails open
/// (cache miss) on a poisoned lock — the fetch path never depends on the cache.
fn cached_relay_self(state: &AppState, relay_url: &str) -> Option<String> {
    let cache = state.relay_self_cache.lock().ok()?;
    let (fetched_at, relay_self) = cache.get(relay_url)?;
    (fetched_at.elapsed() < RELAY_SELF_CACHE_TTL).then(|| relay_self.clone())
}

/// Like [`fetch_relay_self`] but reads NIP-11 from an explicit relay WS URL
/// instead of re-resolving the workspace override. Used by
/// [`fetch_archived_pubkeys_at`](super::super::fetch_archived_pubkeys_at) so
/// the advertised signer and snapshot query belong to the same relay target.
///
/// Successful lookups are cached per relay URL for [`RELAY_SELF_CACHE_TTL`].
/// Only a verified `Some` is cached — `Ok(None)` covers transient states that
/// must be retried, not pinned.
pub(crate) async fn fetch_relay_self_at(
    state: &AppState,
    relay_url: &str,
) -> Result<Option<String>, String> {
    if let Some(cached) = cached_relay_self(state, relay_url) {
        return Ok(Some(cached));
    }

    let http_url = relay_http_base_url(relay_url);
    let response = state
        .http_client
        .get(&http_url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
        .map_err(|e| classify_request_error(&e))?;

    if !response.status().is_success() {
        return Ok(None);
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|_| "relay returned malformed NIP-11 document".to_string())?;
    let relay_self = parse_relay_self_document(&bytes)?;
    if let Some(relay_self) = relay_self {
        if let Ok(mut cache) = state.relay_self_cache.lock() {
            cache.insert(
                relay_url.to_string(),
                (std::time::Instant::now(), relay_self.clone()),
            );
        }
        Ok(Some(relay_self))
    } else {
        Ok(None)
    }
}

/// Maximum NIP-11 body accepted by a scoped read. The document is metadata,
/// not an arbitrary relay payload; keeping this well below the generic relay
/// response bound prevents a relay from consuming a large allocation here.
pub(crate) const SCOPED_RELAY_SELF_BODY_LIMIT: usize = 64 * 1024;
/// A scoped NIP-11 lookup must finish inside the same short budget as the
/// owner-operation transport. This is deliberately uncached: a snapshot must
/// bind relay-authoritative state to the origin captured for this read.
pub(crate) const SCOPED_RELAY_SELF_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(10);

/// Read the NIP-11 `self` identity for one captured owner/community scope.
///
/// Archive and moderation callers retain their per-relay cache, while a Wiki
/// snapshot must use the current captured origin and cannot fall back to a
/// cached or operator-provided identity. The no-redirect media client and
/// streaming body limit ensure that the metadata request cannot follow an
/// off-origin redirect or allocate an unbounded response. `self` is the relay's
/// advertised signing identity; NIP-11 `pubkey` identifies an operator and is
/// not accepted here.
pub(crate) async fn fetch_relay_self_scoped<R: Runtime>(
    app: AppHandle<R>,
    expected: &OwnerScopeToken,
) -> Result<Option<String>, String> {
    let captured = app_state_scope::capture(app.clone()).await?;
    if captured.token != *expected {
        return Err(app_state_scope::OWNER_SCOPE_STALE.into());
    }
    // `OwnerScopeToken::scope.community` is the canonical configured origin;
    // never rebuild this URL from ambient env vars or a caller-supplied URL.
    let origin = captured.token.scope.community.clone();
    drop(captured);

    let state = app.state::<AppState>();
    let url = format!("{origin}/");
    let deadline = tokio::time::Instant::now() + SCOPED_RELAY_SELF_TIMEOUT;
    app_state_scope::assert_current(app.clone(), expected).await?;
    let lookup = async {
        let response = state
            .media_fetch_client
            .get(&url)
            .header("Accept", "application/nostr+json")
            .send()
            .await
            .map_err(|error| classify_request_error(&error))?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let bytes = read_bounded_response(response, deadline).await?;
        parse_relay_self_document(&bytes)
    };
    let result = match tokio::time::timeout_at(deadline, lookup).await {
        Ok(result) => result,
        Err(_) => Err("scoped relay identity lookup timed out".to_string()),
    };
    // A stale scope must win over a network or parsing failure. Otherwise an
    // old lookup can be downgraded to an ordinary unavailable result exactly
    // while the active workspace/identity is changing.
    app_state_scope::assert_current(app, expected).await?;
    result
}

/// Read a response body with both a preflight and streaming bound. A relay may
/// omit `Content-Length` or lie about it, so the cap is checked on every chunk.
async fn read_bounded_response(
    mut response: reqwest::Response,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > SCOPED_RELAY_SELF_BODY_LIMIT as u64)
    {
        return Err("scoped relay identity document exceeds the size limit".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = tokio::time::timeout_at(deadline, response.chunk())
        .await
        .map_err(|_| "scoped relay identity lookup timed out".to_string())?
        .map_err(|_| "scoped relay identity response could not be read".to_string())?
    {
        if bytes.len().saturating_add(chunk.len()) > SCOPED_RELAY_SELF_BODY_LIMIT {
            return Err("scoped relay identity document exceeds the size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Parse only the NIP-11 `self` field. Other metadata, including the operator
/// `pubkey` field, has no bearing on relay event authorship.
fn parse_relay_self_document(bytes: &[u8]) -> Result<Option<String>, String> {
    let document: RelayInformationDocument = serde_json::from_slice(bytes)
        .map_err(|_| "relay returned malformed NIP-11 document".to_string())?;
    let Some(value) = document.self_ else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(None);
    }
    Ok(Some(normalized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::owner_scope::{capture, OWNER_SCOPE_STALE};
    use crate::app_state::{build_app_state, AppState, IdentityStorage};
    use std::time::Duration;
    use tauri::Manager;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn response(status: u16, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status} Test\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    fn chunked_response(status: u16, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status} Test\r\n{headers}Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:X}\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response.extend_from_slice(b"\r\n0\r\n\r\n");
        response
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let count = socket.read(&mut chunk).await.expect("read request");
            assert!(count > 0, "client closed before request headers");
            request.extend_from_slice(&chunk[..count]);
            if request.windows(4).any(|part| part == b"\r\n\r\n") {
                return;
            }
            assert!(request.len() < 16 * 1024, "request headers are bounded");
        }
    }

    fn app() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .manage(build_app_state())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app")
    }

    fn set_origin(app: &tauri::App<tauri::test::MockRuntime>, address: std::net::SocketAddr) {
        *app.state::<AppState>()
            .relay_url_override
            .lock()
            .expect("relay override lock") = Some(format!("ws://{address}"));
    }

    #[test]
    fn scoped_parser_accepts_only_a_valid_lowercase_self_key() {
        let valid = format!(r#"{{"self":"ABCDEF{}"}}"#, "a".repeat(58));
        assert_eq!(
            parse_relay_self_document(valid.as_bytes()).expect("valid document"),
            Some(format!("{}{}", "abcdef", "a".repeat(58)))
        );
        assert_eq!(
            parse_relay_self_document(
                br#"{"pubkey":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
            )
            .expect("operator-only document"),
            None
        );
        assert!(parse_relay_self_document(br#"{"self":null,"self":"not-a-key"}"#).is_err());
    }

    #[tokio::test]
    async fn scoped_lookup_does_not_follow_an_origin_redirect() {
        let app = app();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let target = TcpListener::bind("127.0.0.1:0").await.expect("target");
        let address = listener.local_addr().expect("address");
        set_origin(&app, address);
        let reply = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            target.local_addr().expect("target address")
        )
        .into_bytes();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            read_request(&mut socket).await;
            socket.write_all(&reply).await.expect("redirect response");
        });
        let expected = capture(app.handle().clone()).await.expect("scope").token;
        let result = fetch_relay_self_scoped(app.handle().clone(), &expected).await;
        assert_eq!(result.expect("redirect is unavailable"), None);
        server.await.expect("server");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), target.accept())
                .await
                .is_err(),
            "scoped lookup must not follow the redirect"
        );
    }

    #[tokio::test]
    async fn scoped_lookup_rejects_a_body_over_the_streaming_bound() {
        let app = app();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        set_origin(&app, address);
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            SCOPED_RELAY_SELF_BODY_LIMIT + 1
        )
        .into_bytes();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            read_request(&mut socket).await;
            socket.write_all(&reply).await.expect("oversize response");
        });
        let expected = capture(app.handle().clone()).await.expect("scope").token;
        let result = fetch_relay_self_scoped(app.handle().clone(), &expected).await;
        let error = result.expect_err("oversize body must fail closed");
        assert!(error.contains("exceeds the size limit"), "{error}");
        server.await.expect("server");
    }

    #[tokio::test]
    async fn scoped_lookup_rejects_a_chunked_body_over_the_streaming_bound() {
        let app = app();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        set_origin(&app, address);
        let body = vec![b'x'; SCOPED_RELAY_SELF_BODY_LIMIT + 1];
        let reply = chunked_response(200, "Content-Type: application/nostr+json\r\n", &body);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            read_request(&mut socket).await;
            socket.write_all(&reply).await.expect("chunked response");
        });
        let expected = capture(app.handle().clone()).await.expect("scope").token;
        let result = fetch_relay_self_scoped(app.handle().clone(), &expected).await;
        let error = result.expect_err("chunked oversize body must fail closed");
        assert!(error.contains("exceeds the size limit"), "{error}");
        server.await.expect("server");
    }

    #[tokio::test]
    async fn scoped_lookup_fences_identity_aba_after_the_http_response() {
        let app = app();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        set_origin(&app, address);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let reply = response(
            200,
            "Content-Type: application/nostr+json\r\n",
            br#"{"self":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"}"#,
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            read_request(&mut socket).await;
            ready_tx.send(()).expect("ready");
            release_rx.await.expect("release");
            socket.write_all(&reply).await.expect("identity response");
        });
        let expected = capture(app.handle().clone()).await.expect("scope");
        let lookup_app = app.handle().clone();
        let lookup_token = expected.token.clone();
        let lookup =
            tokio::spawn(async move { fetch_relay_self_scoped(lookup_app, &lookup_token).await });
        ready_rx.await.expect("request reached server");
        let directory = tempfile::tempdir().expect("identity directory");
        {
            let state = app.state::<AppState>();
            let mutation = state.identity_mutation.lock().expect("identity lock");
            for keys in [nostr::Keys::generate(), expected.keys.clone()] {
                crate::commands::commit_imported_identity(
                    &state,
                    &mutation,
                    directory.path(),
                    keys,
                    |_| Ok(IdentityStorage::LocalFile),
                )
                .expect("identity replacement");
            }
        }
        release_tx.send(()).expect("release response");
        let result = lookup.await.expect("lookup task");
        assert!(
            matches!(result, Err(ref error) if error == OWNER_SCOPE_STALE),
            "A-B-A identity replacement must fence the old response: {result:?}"
        );
        server.await.expect("server");
    }
}
