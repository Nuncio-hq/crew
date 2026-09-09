//! Loopback fixtures exercise the actual ACP handshake, with no relay data store.
use super::*;

async fn handshake_fixture(
    before_challenge: Vec<Value>,
    after_auth: Vec<Value>,
    auth_reply: Option<(bool, &'static str)>,
) -> Result<(WsStream, VecDeque<RelayMessage>), RelayError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        for frame in before_challenge {
            ws.send(Message::Text(frame.to_string().into()))
                .await
                .unwrap();
        }
        ws.send(Message::Text(
            json!(["AUTH", "fixture-challenge"]).to_string().into(),
        ))
        .await
        .unwrap();
        let auth = tests::next_test_frame(&mut ws).await;
        assert_eq!(auth[0], "AUTH");
        for frame in after_auth {
            ws.send(Message::Text(frame.to_string().into()))
                .await
                .unwrap();
        }
        if let Some((accepted, message)) = auth_reply {
            ws.send(Message::Text(
                json!(["OK", auth[1]["id"], accepted, message])
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        }
        ws.close(None).await.unwrap();
    });
    let result = do_connect(&url, &Keys::generate(), None).await;
    server.await.unwrap();
    result
}

#[tokio::test]
async fn unrelated_negative_ok_before_challenge_is_buffered() {
    let (_, buffer) = handshake_fixture(
        vec![json!([
            "OK",
            "unrelated",
            false,
            "restricted: not a channel member"
        ])],
        vec![],
        Some((true, "")),
    )
    .await
    .unwrap();
    assert!(
        matches!(buffer.front(), Some(RelayMessage::Ok { event_id, accepted: false, .. }) if event_id == "unrelated")
    );
}

#[tokio::test]
async fn unrelated_negative_ok_after_auth_is_buffered() {
    let (_, buffer) = handshake_fixture(
        vec![],
        vec![json!([
            "OK",
            "unrelated",
            false,
            "invalid: unrelated event"
        ])],
        Some((true, "")),
    )
    .await
    .unwrap();
    assert!(
        matches!(buffer.front(), Some(RelayMessage::Ok { event_id, accepted: false, .. }) if event_id == "unrelated")
    );
}

#[tokio::test]
async fn unrelated_positive_ok_cannot_hide_exact_auth_denial() {
    let result = handshake_fixture(
        vec![],
        vec![json!(["OK", "unrelated", true, ""])],
        Some((false, "blocked: fixture identity")),
    )
    .await;
    assert!(
        matches!(result, Err(RelayError::AuthFailed(ref message)) if message == "blocked: fixture identity")
    );
}

#[tokio::test]
async fn buffered_positive_ok_cannot_authenticate_closed_attempt() {
    let result = handshake_fixture(vec![json!(["OK", "unrelated", true, ""])], vec![], None).await;
    assert!(matches!(result, Err(RelayError::ConnectionClosed)));
}

#[tokio::test]
async fn auth_then_close_is_retryable_transport_failure() {
    let error = handshake_fixture(vec![], vec![], None).await.err().unwrap();
    assert!(matches!(error, RelayError::ConnectionClosed));
    assert!(!is_terminal_connect_error(&error));
}

#[tokio::test]
async fn exact_auth_denial_is_terminal_but_dependency_error_is_retryable() {
    for (message, terminal) in [
        ("blocked: fixture identity", true),
        ("error: database unavailable", false),
    ] {
        let error = handshake_fixture(vec![], vec![], Some((false, message)))
            .await
            .err()
            .unwrap();
        assert!(matches!(error, RelayError::AuthFailed(_)));
        assert_eq!(is_terminal_connect_error(&error), terminal);
    }
}

#[tokio::test]
async fn channel_closed_is_buffered_without_credential_diagnosis() {
    for message in ["closed", "restricted: not a channel member"] {
        let (_, buffer) = handshake_fixture(
            vec![],
            vec![json!(["CLOSED", "channel-fixture", message])],
            Some((true, "")),
        )
        .await
        .unwrap();
        assert!(matches!(buffer.front(), Some(RelayMessage::Closed { .. })));
        let error = handshake_fixture(
            vec![],
            vec![json!(["CLOSED", "channel-fixture", message])],
            None,
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(error, RelayError::ConnectionClosed));
    }
}
