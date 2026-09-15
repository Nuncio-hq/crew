use super::*;
use crate::app_state::{AppState, IdentityStorage};
use futures_util::{SinkExt, StreamExt};
use nostr::{Event, Keys};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::Manager;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::{accept_async, tungstenite::Message};

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
async fn relay(reply: impl FnOnce(Event) -> Option<Value> + Send + 'static) -> TestRelay {
    relay_with_auth_gate(reply, None, true).await
}

async fn relay_with_auth_gate(
    reply: impl FnOnce(Event) -> Option<Value> + Send + 'static,
    auth_gate: Option<(oneshot::Sender<()>, oneshot::Receiver<()>)>,
    expect_event: bool,
) -> TestRelay {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let expected_relay_url = url.clone();
    let connections = Arc::new(AtomicUsize::new(0));
    let count = connections.clone();
    let worker = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(4), async {
            let (stream, _) = listener.accept().await.unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            let mut socket = accept_async(stream).await.unwrap();
            socket
                .send(Message::Text(
                    serde_json::json!(["AUTH", "scoped-control-test-challenge"])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();

            let auth = next_json(&mut socket).await;
            assert_eq!(auth.get(0).and_then(Value::as_str), Some("AUTH"));
            let auth_event: Event =
                serde_json::from_value(auth.get(1).cloned().expect("AUTH event payload")).unwrap();
            auth_event.verify().unwrap();
            assert_eq!(auth_event.kind.as_u16(), 22242);
            assert!(auth_event.tags.iter().any(|tag| {
                let values = tag.as_slice();
                values.first().map(String::as_str) == Some("challenge")
                    && values.get(1).map(String::as_str) == Some("scoped-control-test-challenge")
            }));
            assert!(auth_event.tags.iter().any(|tag| {
                let values = tag.as_slice();
                values.first().map(String::as_str) == Some("relay")
                    && values.get(1).map(String::as_str) == Some(expected_relay_url.as_str())
            }));

            if let Some((ready, release)) = auth_gate {
                ready.send(()).unwrap();
                release.await.unwrap();
            }

            socket
                .send(Message::Text(
                    serde_json::json!(["OK", auth_event.id.to_hex(), true, "authenticated"])
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();

            if !expect_event {
                match tokio::time::timeout(Duration::from_secs(1), socket.next()).await {
                    Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) => {}
                    Ok(Some(Ok(message))) => {
                        panic!("unexpected relay client frame after stale fence: {message:?}")
                    }
                    Err(_) => panic!("stale fenced client kept the WebSocket open"),
                }
                return;
            }

            let frame = next_json(&mut socket).await;
            assert_eq!(frame.get(0).and_then(Value::as_str), Some("EVENT"));
            let event: Event =
                serde_json::from_value(frame.get(1).cloned().expect("EVENT payload")).unwrap();
            event.verify().unwrap();
            assert_eq!(event.pubkey, auth_event.pubkey);
            let body = reply(event.clone());
            if let Some(body) = body {
                let event_id = body
                    .get("event_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| event.id.to_hex());
                let accepted = body
                    .get("accepted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let message = body.get("message").and_then(Value::as_str).unwrap_or("");
                socket
                    .send(Message::Text(
                        serde_json::json!(["OK", event_id, accepted, message])
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
        })
        .await
        .expect("bounded relay worker");
    });
    TestRelay {
        url,
        connections,
        worker,
    }
}

async fn next_json<S>(socket: &mut tokio_tungstenite::WebSocketStream<S>) -> Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        match socket.next().await.expect("relay client frame").unwrap() {
            Message::Text(text) => return serde_json::from_str(text.as_ref()).unwrap(),
            Message::Ping(data) => {
                socket.send(Message::Pong(data)).await.unwrap();
            }
            other => panic!("unexpected relay client frame: {other:?}"),
        }
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

fn steer_payload() -> ScopedObserverControl {
    ScopedObserverControl::SteerTurn {
        channel_id: uuid::Uuid::new_v4(),
        conversation_id: uuid::Uuid::new_v4(),
        session_id: "selected-session".into(),
        turn_id: "selected-turn".into(),
        request_id: uuid::Uuid::new_v4(),
        prompt: "keep the selected run focused".into(),
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
    } = malformed
    else {
        panic!("payload fixture must be a cancel control");
    };
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
async fn scoped_steer_exact_encrypted_target_uses_captured_owner() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let agent = Keys::generate();
    let recipient = agent.public_key().to_hex();
    let request = steer_payload();
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
async fn scoped_stop_scope_change_during_auth_never_sends_event() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let (auth_ready_tx, auth_ready_rx) = oneshot::channel();
    let (auth_release_tx, auth_release_rx) = oneshot::channel();
    let server =
        relay_with_auth_gate(accepted, Some((auth_ready_tx, auth_release_rx)), false).await;
    let app = app(&server.url);
    let token = capture(app.handle().clone()).await.unwrap().token;
    let sending = tokio::spawn(send_at_scope(
        app.handle().clone(),
        Keys::generate().public_key().to_hex(),
        payload(),
        token,
    ));

    auth_ready_rx.await.unwrap();
    let state = app.state::<AppState>();
    let dir = tempfile::tempdir().unwrap();
    {
        let mutation = state.identity_mutation.lock().unwrap();
        crate::commands::commit_imported_identity(
            &state,
            &mutation,
            dir.path(),
            Keys::generate(),
            |_| Ok(IdentityStorage::LocalFile),
        )
        .unwrap();
    }
    auth_release_tx.send(()).unwrap();

    let result = sending.await.unwrap();
    assert!(matches!(
        result,
        ScopedControlPublication::NotAttempted { .. }
    ));
    assert_eq!(server.connections.load(Ordering::SeqCst), 1);
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
            matches!(&result, ScopedControlPublication::Unknown { .. }),
            "{result:?}"
        );
        if outcome == 1 {
            assert!(matches!(
                &result,
                ScopedControlPublication::Unknown { message }
                    if message == "refused"
            ));
        }
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

#[tokio::test]
async fn scoped_observer_transport_blocks_key_backup_before_connect() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let server = relay(accepted).await;
    let keys = Keys::generate();
    let event = nostr::EventBuilder::new(
        nostr::Kind::Custom(buzz_core_pkg::kind::KIND_AGENT_OBSERVER_FRAME as u16),
        "ncryptsec1fixture-not-a-real-key",
    )
    .sign_with_keys(&keys)
    .unwrap();
    let transport =
        transport::ScopedObserverControlTransport::captured(server.url.clone(), keys).unwrap();
    let result = transport.publish(&event, || async { Ok(()) }).await;
    assert!(
        matches!(result, Err(OperationTransportError::InvalidInput(ref message)) if message.contains("key-backup material"))
    );
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
}
