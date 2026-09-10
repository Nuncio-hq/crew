use super::*;
use crate::app_state::{AppState, IdentityStorage};
use base64::Engine;
use nostr::{Event, JsonUtil, Keys};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct ResetAdmission;
impl Drop for ResetAdmission {
    fn drop(&mut self) {
        crate::relay_admission::reset_rate_limit_gate();
    }
}
struct TestRelay {
    url: String,
    connections: Arc<AtomicUsize>,
    worker: tokio::task::JoinHandle<()>,
}
impl Drop for TestRelay {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
async fn relay(
    reply: impl FnOnce(Event) -> Option<serde_json::Value> + Send + 'static,
) -> TestRelay {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let connections = Arc::new(AtomicUsize::new(0));
    let count = connections.clone();
    let worker = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(4), async {
            let (mut socket, _) = listener.accept().await.unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            let (body, auth_owner) = loop {
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() <= 1024 * 1024 + 8192);
                if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    assert!(headers.starts_with("POST /events "));
                    assert!(headers.to_ascii_lowercase().contains("authorization: nostr "));
                    let len: usize = headers.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().unwrap())
                    }).unwrap();
                    if bytes.len() >= end + 4 + len { let auth = headers.lines().find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("authorization").then_some(value.trim())
                        }).unwrap().strip_prefix("Nostr ").unwrap();
                        let decoded = base64::engine::general_purpose::STANDARD.decode(auth).unwrap();
                        let auth = Event::from_json(decoded).unwrap();
                        auth.verify().unwrap();
                        break (bytes[end + 4..end + 4 + len].to_vec(), auth.pubkey); }
                }
            };
            let event = Event::from_json(body).unwrap();
            event.verify().unwrap();
            assert_eq!(event.pubkey, auth_owner);
            if let Some(body) = reply(event) {
                let body = serde_json::to_vec(&body).unwrap();
                let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                socket.write_all(headers.as_bytes()).await.unwrap();
                socket.write_all(&body).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        }).await.expect("bounded relay worker");
    });
    TestRelay {
        url,
        connections,
        worker,
    }
}
fn app(url: &str) -> tauri::App<tauri::test::MockRuntime> {
    let state = crate::app_state::build_app_state();
    *state.relay_url_override.lock().unwrap() = Some(url.into());
    tauri::test::mock_builder()
        .manage(state)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap()
}
fn payload() -> ScopedObserverControl {
    ScopedObserverControl::CancelTurn {
        channel_id: uuid::Uuid::new_v4(),
        conversation_id: uuid::Uuid::new_v4(),
        turn_id: uuid::Uuid::new_v4().to_string(),
        request_id: uuid::Uuid::new_v4(),
    }
}
fn accepted(event: Event) -> Option<serde_json::Value> {
    Some(serde_json::json!({"event_id":event.id.to_hex(),"accepted":true,"message":"ok"}))
}

#[tokio::test]
async fn scoped_stop_expected_scope_and_malformed_target_never_connect() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let server = relay(accepted).await;
    let app = app(&server.url);
    let token = capture(app.handle().clone()).await.unwrap().token;
    let mut stale = token.clone();
    stale.identity_generation += 1;
    let result = send_at_scope(
        app.handle().clone(),
        Keys::generate().public_key().to_hex(),
        payload(),
        stale,
    )
    .await;
    assert!(matches!(
        result,
        ScopedControlPublication::NotAttempted { .. }
    ));
    let mut malformed = payload();
    let ScopedObserverControl::CancelTurn {
        ref mut turn_id, ..
    } = malformed;
    *turn_id = " ".into();
    let result = send_at_scope(
        app.handle().clone(),
        Keys::generate().public_key().to_hex(),
        malformed,
        token,
    )
    .await;
    assert!(matches!(
        result,
        ScopedControlPublication::NotAttempted { .. }
    ));
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn scoped_stop_exact_encrypted_target_uses_captured_owner() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let agent = Keys::generate();
    let recipient = agent.public_key().to_hex();
    let request = payload();
    let expected_payload = serde_json::to_value(&request).unwrap();
    let (seen_tx, seen_rx) = tokio::sync::oneshot::channel();
    let server = relay(move |event| {
        let decoded: serde_json::Value =
            buzz_core_pkg::observer::decrypt_observer_payload(&agent, &event).unwrap();
        assert_eq!(decoded, expected_payload);
        seen_tx.send(event.pubkey.to_hex()).unwrap();
        accepted(event)
    })
    .await;
    let app = app(&server.url);
    let token = capture(app.handle().clone()).await.unwrap().token;
    let expected_owner = token.scope.owner.clone();
    let result = send_at_scope(app.handle().clone(), recipient, request, token).await;
    assert!(matches!(result, ScopedControlPublication::Accepted { .. }));
    assert_eq!(seen_rx.await.unwrap(), expected_owner);
    assert_eq!(server.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn scoped_stop_identity_aba_during_admission_never_connects() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let server = relay(accepted).await;
    let app = app(&server.url);
    let captured = capture(app.handle().clone()).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    // This production preparation seam has already captured and checked the
    // expected token. An initial-scope rejection can no longer mask the guard.
    let prepared = prepare_control(
        app.handle().clone(),
        Keys::generate().public_key().to_hex(),
        payload(),
        captured.token,
    )
    .await
    .unwrap();
    crate::relay_admission::activate_rate_limit(Some(1));
    let (result, ()) = tokio::join!(publish_prepared(app.handle().clone(), prepared), async {
        let state = app.state::<AppState>();
        let mutation = state.identity_mutation.lock().unwrap();
        for keys in [Keys::generate(), captured.keys] {
            crate::commands::commit_imported_identity(&state, &mutation, dir.path(), keys, |_| {
                Ok(IdentityStorage::LocalFile)
            })
            .unwrap();
        }
    });
    assert!(matches!(
        result,
        ScopedControlPublication::NotAttempted { .. }
    ));
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn scoped_stop_sent_ack_mismatch_refusal_and_disconnect_are_unknown() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    for outcome in 0..3 {
        let server = relay(move |event| match outcome {
            0 => Some(serde_json::json!({"event_id":"0".repeat(64),"accepted":true,"message":"wrong event"})),
            1 => Some(serde_json::json!({"event_id":event.id.to_hex(),"accepted":false,"message":"refused"})),
            _ => None,
        }).await;
        let app = app(&server.url);
        let token = capture(app.handle().clone()).await.unwrap().token;
        let result = send_at_scope(
            app.handle().clone(),
            Keys::generate().public_key().to_hex(),
            payload(),
            token,
        )
        .await;
        assert!(
            matches!(result, ScopedControlPublication::Unknown { .. }),
            "{result:?}"
        );
        assert_eq!(server.connections.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn scoped_stop_post_send_identity_change_preserves_accepted_outcome() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let fixture = app("ws://localhost:1");
    let handle = fixture.handle().clone();
    let dir = tempfile::tempdir().unwrap();
    let server = relay(move |event| {
        let state = handle.state::<AppState>();
        let mutation = state.identity_mutation.lock().unwrap();
        crate::commands::commit_imported_identity(
            &state,
            &mutation,
            dir.path(),
            Keys::generate(),
            |_| Ok(IdentityStorage::LocalFile),
        )
        .unwrap();
        accepted(event)
    })
    .await;
    *fixture
        .state::<AppState>()
        .relay_url_override
        .lock()
        .unwrap() = Some(server.url.clone());
    let token = capture(fixture.handle().clone()).await.unwrap().token;
    let result = send_at_scope(
        fixture.handle().clone(),
        Keys::generate().public_key().to_hex(),
        payload(),
        token,
    )
    .await;
    assert!(
        matches!(result, ScopedControlPublication::Accepted { .. }),
        "{result:?}"
    );
    assert_eq!(server.connections.load(Ordering::SeqCst), 1);
}
