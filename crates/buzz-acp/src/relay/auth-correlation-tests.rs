//! Loopback fixtures exercise the actual ACP handshake, with no relay data store.
use super::*;

const HANDSHAKE_FIXTURE_TIMEOUT: Duration = Duration::from_secs(3);

struct AbortOnDrop(Option<tokio::task::JoinHandle<()>>);

impl AbortOnDrop {
    async fn join(mut self) -> Result<(), tokio::task::JoinError> {
        let mut handle = self.0.take().expect("fixture task handle");
        (&mut handle).await
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

async fn handshake_fixture(
    before_challenge: Vec<Value>,
    after_auth: Vec<Value>,
    auth_reply: Option<(bool, &'static str)>,
) -> Result<(WsStream, VecDeque<RelayMessage>), RelayError> {
    handshake_fixture_with_attempt(
        before_challenge,
        after_auth,
        auth_reply,
        AuthAttemptContext {
            sequence: 1,
            id: Uuid::new_v4().to_string(),
            started_at_ms: 1,
        },
    )
    .await
}

async fn handshake_fixture_with_attempt(
    before_challenge: Vec<Value>,
    after_auth: Vec<Value>,
    auth_reply: Option<(bool, &'static str)>,
    attempt: AuthAttemptContext,
) -> Result<(WsStream, VecDeque<RelayMessage>), RelayError> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let server = AbortOnDrop(Some(tokio::spawn(async move {
        let (stream, _) = tokio::time::timeout(HANDSHAKE_FIXTURE_TIMEOUT, listener.accept())
            .await
            .expect("fixture listener accept timed out")
            .expect("fixture listener accept failed");
        let mut ws = tokio::time::timeout(
            HANDSHAKE_FIXTURE_TIMEOUT,
            tokio_tungstenite::accept_async(stream),
        )
        .await
        .expect("fixture websocket accept timed out")
        .expect("fixture websocket accept failed");
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
    })));
    let result = tokio::time::timeout(
        HANDSHAKE_FIXTURE_TIMEOUT,
        do_connect(&url, &Keys::generate(), None, attempt),
    )
    .await
    .unwrap_or(Err(RelayError::Timeout));
    let server_join = tokio::time::timeout(HANDSHAKE_FIXTURE_TIMEOUT, server.join()).await;
    if server_join.is_err() {
        return Err(RelayError::Timeout);
    }
    result
}

struct StatusFixtureDir(std::path::PathBuf);

impl Drop for StatusFixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct V2StatusHealthFixture {
    health: TransportHealth,
    path: std::path::PathBuf,
    _dir: StatusFixtureDir,
}

async fn v2_status_health_fixture() -> V2StatusHealthFixture {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("crew-338-auth-flow-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let path = root.join("status.json");
    let config = transport_status::StatusConfig {
        path: path.clone(),
        nonce: Uuid::new_v4().to_string(),
        version: 2,
        spawn_started_at_ms: Some(1),
    };
    let writer = transport_status::StatusWriter::new(config, &"a".repeat(64), "ws://fixture")
        .await
        .unwrap();
    V2StatusHealthFixture {
        health: TransportHealth::with_reporter(transport_status::StatusReporter::new(writer)),
        path,
        _dir: StatusFixtureDir(root),
    }
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
    assert!(matches!(
        result,
        Err(RelayError::AuthDenied(ref info))
            if info.classification == TransportAuthClassification::OtherDenial
    ));
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
        assert!(matches!(error, RelayError::AuthDenied(_)));
        assert_eq!(is_terminal_connect_error(&error), terminal);
    }
}

#[tokio::test]
async fn production_auth_denial_is_classified_and_persisted_without_relay_text() {
    for (message, classification) in [
        (
            "blocked: you are banned from this community",
            TransportAuthClassification::CommunityBanned,
        ),
        (
            "blocked: secret-token-from-relay",
            TransportAuthClassification::OtherDenial,
        ),
    ] {
        let mut fixture = v2_status_health_fixture().await;
        let result: Result<(), RelayError> = fixture
            .health
            .connect_with_context(|attempt| async move {
                handshake_fixture_with_attempt(vec![], vec![], Some((false, message)), attempt)
                    .await
                    .map(|_| ())
            })
            .await;
        assert!(matches!(&result, Err(RelayError::AuthDenied(_))));
        assert!(!format!("{result:?}").contains(message));
        fixture.health.report(true).await.unwrap();

        let bytes = std::fs::read(&fixture.path).unwrap();
        let record: buzz_core::transport_status::TransportRecordV2 =
            serde_json::from_slice(&bytes).unwrap();
        let received = record.received_auth.expect("exact ACK evidence");
        assert_eq!(received.classification, classification);
        assert_eq!(received.attempt_id, record.connection_attempt.unwrap().id);
        assert!(!String::from_utf8(bytes)
            .unwrap()
            .contains("secret-token-from-relay"));
    }
}

#[tokio::test]
async fn production_wrong_ack_no_ack_and_close_never_create_auth_evidence() {
    let cases = [
        (
            vec![json!([
                "OK",
                "wrong-auth-event",
                false,
                "blocked: secret-token"
            ])],
            vec![],
            None,
        ),
        (vec![], vec![], None),
        (
            vec![],
            vec![json!(["CLOSED", "fixture-subscription", "secret-token"])],
            None,
        ),
    ];
    for (before_challenge, after_auth, auth_reply) in cases {
        let mut health = TransportHealth::managed();
        let result: Result<(), RelayError> = health
            .connect_with_context(|attempt| async move {
                handshake_fixture_with_attempt(before_challenge, after_auth, auth_reply, attempt)
                    .await
                    .map(|_| ())
            })
            .await;
        assert!(matches!(result, Err(RelayError::ConnectionClosed)));
        assert!(health.received_auth.is_none());
        assert!(!health.auth_rejected);
    }
}

#[tokio::test]
async fn production_retryable_auth_ack_preserves_reconnect_behavior() {
    let mut fixture = v2_status_health_fixture().await;
    let first: Result<(), RelayError> = fixture
        .health
        .connect_with_context(|attempt| async move {
            handshake_fixture_with_attempt(
                vec![],
                vec![],
                Some((false, "error: dependency unavailable")),
                attempt,
            )
            .await
            .map(|_| ())
        })
        .await;
    assert!(matches!(
        first,
        Err(RelayError::AuthDenied(ref info)) if info.retryable
    ));
    assert!(!fixture.health.auth_rejected);
    assert_eq!(fixture.health.phase, transport_health::Phase::Burst);
    assert!(fixture.health.received_auth.is_some());
    fixture.health.report(false).await.unwrap();
    let first_record: buzz_core::transport_status::TransportRecordV2 =
        serde_json::from_slice(&std::fs::read(&fixture.path).unwrap()).unwrap();
    assert_eq!(
        first_record.transport.code,
        buzz_core::transport_status::TransportCode::ConnectionFailed
    );
    assert_eq!(
        first_record.transport.state,
        buzz_core::transport_status::TransportState::Connecting
    );
    assert_eq!(
        first_record
            .received_auth
            .as_ref()
            .expect("retryable exact ACK evidence")
            .classification,
        TransportAuthClassification::OtherDenial
    );

    let second: Result<(), RelayError> = fixture
        .health
        .connect_with_context(|attempt| async move {
            handshake_fixture_with_attempt(vec![], vec![], Some((true, "")), attempt)
                .await
                .map(|_| ())
        })
        .await;
    assert!(second.is_ok());
    assert_eq!(fixture.health.attempts, 2);
    assert!(!fixture.health.auth_rejected);
    assert!(fixture.health.received_auth.is_none());
    fixture.health.recovered().await;
    let recovered: buzz_core::transport_status::TransportRecordV2 =
        serde_json::from_slice(&std::fs::read(&fixture.path).unwrap()).unwrap();
    assert_eq!(
        recovered.transport.state,
        buzz_core::transport_status::TransportState::Connected
    );
    assert!(recovered.received_auth.is_none());
    assert!(recovered.connection_attempt.is_some());
}

#[test]
fn auth_denial_discards_relay_text_at_the_error_boundary() {
    let error = RelayError::AuthDenied(AuthDeniedInfo::from_ack(
        &AuthAttemptContext {
            sequence: 1,
            id: Uuid::new_v4().to_string(),
            started_at_ms: 1,
        },
        "a".repeat(64),
        "blocked: you are banned from this community",
        3,
    ));
    assert_eq!(error.to_string(), "Auth denied");
    assert!(is_terminal_connect_error(&error));
    let RelayError::AuthDenied(info) = error else {
        unreachable!("fixture must construct an auth denial");
    };
    assert_eq!(
        info.classification,
        TransportAuthClassification::CommunityBanned
    );
    assert!(!format!("{info:?}").contains("blocked: you are banned"));
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
