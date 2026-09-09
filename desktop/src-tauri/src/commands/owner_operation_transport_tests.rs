use super::*;
use nostr::{Event, EventBuilder, Kind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct ResetAdmission;
impl Drop for ResetAdmission {
    fn drop(&mut self) {
        crate::relay_admission::reset_rate_limit_gate();
    }
}

fn transport(origin: String, keys: Keys) -> OwnerOperationTransport {
    OwnerOperationTransport::captured(&crate::app_state::build_app_state(), origin, keys, None)
        .unwrap()
}

fn event(keys: &Keys, content: &str) -> Event {
    EventBuilder::new(Kind::Custom(9), content)
        .sign_with_keys(keys)
        .unwrap()
}

fn response(status: u16, body: &[u8]) -> Vec<u8> {
    let mut result = format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ).into_bytes();
    result.extend_from_slice(body);
    result
}

/// Owned loopback server captures the complete request before responding.
async fn relay(reply: Vec<u8>) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let worker = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(3), async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let count = socket.read(&mut chunk).await.unwrap();
                assert!(count > 0, "client closed before a complete request");
                request.extend_from_slice(&chunk[..count]);
                assert!(request.len() <= REQUEST_LIMIT + 8192);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            // Bounded-response tests intentionally close while this writes.
            let _ = socket.write_all(&reply).await;
            request
        })
        .await
        .expect("owned loopback deadline")
    });
    (origin, worker)
}

#[tokio::test]
async fn owner_transport_preserves_typed_http_reasons_without_claiming_no_effects() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    for (status, reason, ambiguous) in [
        (400, "conflict: revision changed", false),
        (403, "restricted: not owner", false),
        (500, "internal server error", true),
        (408, "request timed out", true),
        (400, "error: side-effect-pending", true),
    ] {
        let body = serde_json::to_vec(&serde_json::json!({"error": reason})).unwrap();
        let (origin, worker) = relay(response(status, &body)).await;
        let error = transport(origin, Keys::generate())
            .query(serde_json::json!({"kinds":[30621],"limit":1}), async {
                Ok(())
            })
            .await
            .unwrap_err();
        match error {
            OperationTransportError::OutcomeUnknown {
                status: Some(actual),
                reason: actual_reason,
            } if ambiguous => {
                assert_eq!(actual, status);
                assert_eq!(actual_reason, reason);
            }
            OperationTransportError::RelayResponse {
                status: actual,
                reason: actual_reason,
            } if !ambiguous => {
                assert_eq!(actual, status);
                assert_eq!(actual_reason, reason);
            }
            other => panic!("wrong HTTP classification: {other:?}"),
        }
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn owner_transport_429_arms_shared_admission() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let (origin, worker) = relay(response(429, br#"{"error":"retry in 20s"}"#)).await;
    let error = transport(origin, Keys::generate())
        .query(serde_json::json!({"kinds":[30621],"limit":1}), async {
            Ok(())
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        OperationTransportError::RelayResponse { status: 429, .. }
    ));
    worker.await.unwrap();
    assert!(tokio::time::timeout(
        Duration::from_millis(20),
        crate::relay_admission::wait_for_rate_limit()
    )
    .await
    .is_err());
}

#[tokio::test]
async fn owner_transport_ack_uses_exact_event_and_captured_nip98_identity() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let keys = Keys::generate();
    let signed = event(&keys, "exact envelope");
    for matching in [false, true] {
        let id = if matching {
            signed.id.to_hex()
        } else {
            "0".repeat(64)
        };
        let body = serde_json::to_vec(&serde_json::json!({
            "event_id":id, "accepted":false, "message":"conflict: current head changed"
        }))
        .unwrap();
        let (origin, worker) = relay(response(200, &body)).await;
        let result = transport(origin.clone(), keys.clone())
            .publish(&signed, async { Ok(()) })
            .await;
        if matching {
            let ack = result.unwrap();
            assert!(!ack.accepted);
            assert_eq!(ack.message, "conflict: current head changed");
        } else {
            assert!(matches!(
                result,
                Err(OperationTransportError::OutcomeUnknown { .. })
            ));
        }
        let request = worker.await.unwrap();
        let end = request
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .unwrap();
        assert_eq!(&request[end + 4..], signed.as_json().as_bytes());
        let headers = String::from_utf8_lossy(&request[..end]);
        let auth = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("authorization")
                    .then_some(value.trim())
            })
            .unwrap();
        let auth_event: Event = serde_json::from_slice(
            &STANDARD
                .decode(auth.strip_prefix("Nostr ").unwrap())
                .unwrap(),
        )
        .unwrap();
        auth_event.verify().unwrap();
        assert_eq!(auth_event.pubkey, keys.public_key());
        assert!(auth_event.tags.iter().any(|tag| {
            let values = tag.as_slice();
            values.first().is_some_and(|value| value == "u")
                && values
                    .get(1)
                    .is_some_and(|url| url.starts_with(&format!("{origin}/")))
        }));
    }
}

#[tokio::test]
async fn owner_transport_does_not_follow_redirects() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let reply = format!(
        "HTTP/1.1 302 Found\r\nLocation: http://{}/stolen\r\nContent-Length: 0\r\n\r\n",
        target.local_addr().unwrap()
    )
    .into_bytes();
    let (origin, worker) = relay(reply).await;
    let result = transport(origin, Keys::generate())
        .query(serde_json::json!({"kinds":[30621],"limit":1}), async {
            Ok(())
        })
        .await;
    assert!(matches!(
        result,
        Err(OperationTransportError::RelayResponse { status: 302, .. })
    ));
    worker.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), target.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn owner_transport_caps_declared_and_chunked_response_bodies() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let declared = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
        RESPONSE_LIMIT + 1
    )
    .into_bytes();
    let chunked = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
        RESPONSE_LIMIT + 1,
        "x".repeat(RESPONSE_LIMIT + 1)
    )
    .into_bytes();
    for reply in [declared, chunked] {
        let (origin, worker) = relay(reply).await;
        let error = transport(origin, Keys::generate())
            .query(serde_json::json!({"kinds":[30621],"limit":1}), async {
                Ok(())
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            OperationTransportError::OutcomeUnknown { .. }
        ));
        assert!(error.to_string().contains("exceeds 1 MiB"), "{error}");
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn owner_transport_preflight_blocks_wrong_signer_bad_signature_and_key_backup() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let keys = Keys::generate();
    let transport = transport(
        format!("http://{}", listener.local_addr().unwrap()),
        keys.clone(),
    );
    let mut bad_signature = event(&keys, "before mutation");
    bad_signature.content = "after mutation".into();
    for signed in [event(&Keys::generate(), "wrong owner"), bad_signature] {
        assert!(matches!(
            transport.publish(&signed, async { Ok(()) }).await,
            Err(OperationTransportError::InvalidInput(_))
        ));
    }
    let backup = event(&keys, "ncryptsec1qgg9947rlpvqu76pj5ecreduf9jxhselq2nae2kghhvd5g7dgjtcxfqtd67p9m0w57lspw8gsq6yphnm8623nsl8xn9j4jdzz84zm3frztj3z7s35vpzmqf6ksu8r89qk5z2zxfmu5gv8th8wclt0h4p");
    let error = transport
        .publish(&backup, async { Ok(()) })
        .await
        .unwrap_err();
    assert!(matches!(error, OperationTransportError::InvalidInput(_)));
    assert!(error.to_string().contains("key-backup material"));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn owner_transport_admission_expiry_and_failed_native_guard_never_send() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = transport(
        format!("http://{}", listener.local_addr().unwrap()),
        Keys::generate(),
    );
    crate::relay_admission::activate_rate_limit(Some(1));
    let called = std::sync::atomic::AtomicBool::new(false);
    let result = transport
        .request(
            Method::POST,
            "/query",
            b"[]".to_vec(),
            Duration::from_millis(25),
            async {
                called.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
        )
        .await;
    assert!(matches!(
        result,
        Err(OperationTransportError::NotAttempted(_))
    ));
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    crate::relay_admission::reset_rate_limit_gate();
    let result = transport
        .query(serde_json::json!({"kinds":[30621],"limit":1}), async {
            Err("native scope changed".into())
        })
        .await;
    assert!(matches!(
        result,
        Err(OperationTransportError::NotAttempted(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn owner_transport_polls_native_guard_after_admission_not_before_it() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = transport(
        format!("http://{}", listener.local_addr().unwrap()),
        Keys::generate(),
    );
    crate::relay_admission::activate_rate_limit(Some(1));
    let stale = AtomicBool::new(false);
    // This binds adapter guard ordering, not the shared native identity epoch.
    // Actual workspace/import ABA wiring is a separate domain integration test.
    let (result, ()) = tokio::join!(
        transport.request(
            Method::POST,
            "/query",
            b"[]".to_vec(),
            Duration::from_secs(2),
            async {
                if stale.load(Ordering::SeqCst) {
                    Err("native scope changed".into())
                } else {
                    Ok(())
                }
            }
        ),
        async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            stale.store(true, Ordering::SeqCst);
        }
    );
    assert!(matches!(
        result,
        Err(OperationTransportError::NotAttempted(_))
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[test]
fn owner_transport_rejects_noncanonical_origins_and_bounds_unicode_reasons() {
    for origin in [
        "https://host/path",
        "https://user:pass@host",
        "https://host:443",
        "https://host/",
        "https://host?q=1",
        "ws://host",
    ] {
        assert!(OwnerOperationTransport::captured(
            &crate::app_state::build_app_state(),
            origin.into(),
            Keys::generate(),
            None
        )
        .is_err());
    }
    let reason = "ữ".repeat(200);
    let bounded =
        response_reason(&serde_json::to_vec(&serde_json::json!({"error":reason})).unwrap());
    assert!(bounded.len() <= 256);
    assert!(reason.starts_with(&bounded));
}

#[tokio::test]
async fn owner_transport_real_import_aba_during_admission_is_not_attempted() {
    use crate::app_state::{owner_scope, AppState, IdentityStorage};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tauri::Manager;
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app = tauri::test::mock_builder()
        .manage(crate::app_state::build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    // Initial fixture configuration is installed before native scope capture.
    *app.state::<AppState>().relay_url_override.lock().unwrap() =
        Some(format!("ws://{}", listener.local_addr().unwrap()));
    let captured = owner_scope::capture(app.handle().clone()).await.unwrap();
    let signed = event(&captured.keys, "must never be sent after import ABA");
    let body_length = signed.as_json().len();
    let ack = serde_json::to_vec(&serde_json::json!({
        "event_id":signed.id.to_hex(), "accepted":true, "message":"accepted"
    }))
    .unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let server_accepted = accepted.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        tokio::select! {
            result = listener.accept() => {
                let (mut socket, _) = result.unwrap();
                server_accepted.fetch_add(1, Ordering::SeqCst);
                tokio::time::timeout(Duration::from_secs(3), async {
                    let mut request = Vec::new();
                    let mut chunk = [0;4096];
                    loop {
                        let count = socket.read(&mut chunk).await.unwrap();
                        assert!(count > 0, "client must finish the mutation-oracle request");
                        request.extend_from_slice(&chunk[..count]);
                        assert!(request.len() < REQUEST_LIMIT + 8192);
                        if request.windows(4).position(|part| part == b"\r\n\r\n")
                            .is_some_and(|end| request.len() >= end + 4 + body_length) { break; }
                    }
                    socket.write_all(&response(200, &ack)).await.unwrap();
                }).await.expect("owned ABA oracle response deadline");
            }
            _ = stopped => {}
        }
    });
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .unwrap();
    crate::relay_admission::activate_rate_limit(Some(1));
    let guard = owner_scope::assert_current(app.handle().clone(), &captured.token);
    let (result, ()) = tokio::join!(
        tokio::time::timeout(Duration::from_secs(3), transport.publish(&signed, guard)),
        async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let state = app.state::<AppState>();
            let mutation = state.identity_mutation.lock().unwrap();
            for keys in [Keys::generate(), captured.keys.clone()] {
                crate::commands::commit_imported_identity(
                    &state,
                    &mutation,
                    dir.path(),
                    keys,
                    |_| Ok(IdentityStorage::LocalFile),
                )
                .unwrap();
            }
        }
    );
    let _ = stop.send(());
    server.await.unwrap();
    let result = result.expect("native ABA oracle must produce a typed result");
    let accepted = accepted.load(Ordering::SeqCst);
    assert!(matches!(result, Err(OperationTransportError::NotAttempted(ref reason))
        if reason == owner_scope::OWNER_SCOPE_STALE),
        "post-admission native scope guard must prevent send: accepted_connections={accepted}, result={result:?}");
    assert_eq!(accepted, 0, "stale native scope must never connect");
    let after = owner_scope::capture(app.handle().clone()).await.unwrap();
    assert_eq!(after.token.scope, captured.token.scope);
    assert_ne!(
        after.token.identity_generation,
        captured.token.identity_generation
    );
}
