use super::*;
use axum::routing::post;

fn credential(endpoint: String) -> HermesProviderCredential {
    HermesProviderCredential {
        version: 1,
        endpoint,
        bearer_token: "real-provider-token".into(),
        account_id: "account-fixture".into(),
    }
}

async fn upstream(headers: HeaderMap, body: Bytes) -> Response<Body> {
    assert_eq!(
        headers.get(header::AUTHORIZATION).unwrap(),
        "Bearer real-provider-token"
    );
    assert_eq!(
        headers.get("chatgpt-account-id").unwrap(),
        "account-fixture"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["model"],
        "hermes-low"
    );
    let event = serde_json::json!({
        "type": "response.completed",
        "response": {"model": "hermes-low", "output": [{"type": "message"}]}
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from(format!("data: {event}\n\ndata: [DONE]\n\n")))
        .unwrap()
}

async fn test_upstream() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/responses", post(upstream)))
            .await
            .unwrap();
    });
    format!("http://{address}/responses")
}

#[tokio::test(flavor = "multi_thread")]
async fn forwards_exactly_one_authenticated_no_tools_request_and_observes_model() {
    let endpoint = test_upstream().await;
    let gateway =
        HermesOneShotGateway::start_with_credential(credential(endpoint), "hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/responses", connection.base_url))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"model":"hermes-low", "stream":true, "input":"recap"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert!(response
        .text()
        .await
        .unwrap()
        .contains("response.completed"));
    let replay = client
        .post(format!("{}/responses", connection.base_url))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"model":"hermes-low", "stream":true, "input":"again"}))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CONFLICT);
    let evidence = tokio::task::spawn_blocking(move || gateway.finish())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(evidence.effective_model, "hermes-low");
    assert_eq!(evidence.request_count, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_wrong_model_tools_bad_token_and_replay_before_upstream() {
    for (token, body, expected) in [
        (
            "wrong".to_string(),
            serde_json::json!({"model":"hermes-low","stream":true}),
            StatusCode::FORBIDDEN,
        ),
        (
            "local".to_string(),
            serde_json::json!({"model":"other","stream":true}),
            StatusCode::BAD_GATEWAY,
        ),
        (
            "local".to_string(),
            serde_json::json!({"model":"hermes-low","stream":true,"tools":[{"type":"function"}]}),
            StatusCode::BAD_GATEWAY,
        ),
    ] {
        let endpoint = test_upstream().await;
        let gateway =
            HermesOneShotGateway::start_with_credential(credential(endpoint), "hermes-low")
                .unwrap();
        let connection = gateway.connection().clone();
        let actual_token = if token == "local" {
            connection.token.clone()
        } else {
            token
        };
        let response = reqwest::Client::new()
            .post(format!("{}/responses", connection.base_url))
            .bearer_auth(actual_token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        drop(gateway);
    }
}

#[test]
fn production_credentials_require_https_exact_responses_and_safe_headers() {
    for value in [
        serde_json::json!({"version":1,"endpoint":"http://provider.test/responses","bearer_token":"x","account_id":"account"}),
        serde_json::json!({"version":1,"endpoint":"https://provider.test/chat/completions","bearer_token":"x","account_id":"account"}),
        serde_json::json!({"version":1,"endpoint":"https://provider.test/responses?next=other","bearer_token":"x","account_id":"account"}),
        serde_json::json!({"version":1,"endpoint":CODEX_RESPONSES_ENDPOINT,"bearer_token":"x","account_id":""}),
        serde_json::json!({"version":1,"endpoint":CODEX_RESPONSES_ENDPOINT,"bearer_token":"x","account_id":"account","extra":"value"}),
    ] {
        assert!(matches!(
            parse_credential(&value.to_string()),
            Err(HermesGatewayFailure::InvalidCredential)
        ));
    }
    assert!(parse_credential(
        &serde_json::json!({
            "version": 1,
            "endpoint": CODEX_RESPONSES_ENDPOINT,
            "bearer_token": "token",
            "account_id": "account"
        })
        .to_string()
    )
    .is_ok());
}

#[test]
fn response_evidence_rejects_fallback_model_and_tool_calls() {
    let mismatch = br#"data: {"type":"response.completed","response":{"model":"fallback"}}"#;
    assert_eq!(
        inspect_response(mismatch, "hermes-low"),
        Err(HermesGatewayFailure::EffectiveModelMismatch)
    );
    let tool = br#"data: {"type":"response.completed","response":{"model":"hermes-low","output":[{"type":"function_call"}]}}"#;
    assert_eq!(
        inspect_response(tool, "hermes-low"),
        Err(HermesGatewayFailure::ToolResponse)
    );
    let incomplete = br#"data: {"type":"response.created","response":{"model":"hermes-low"}}"#;
    assert_eq!(
        inspect_response(incomplete, "hermes-low"),
        Err(HermesGatewayFailure::EffectiveModelMismatch)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dropping_gateway_cancels_an_inflight_upstream_request() {
    let upstream = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/responses", upstream.local_addr().unwrap());
    let (accepted_tx, accepted_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let upstream_thread = std::thread::spawn(move || {
        let (_connection, _) = upstream.accept().unwrap();
        accepted_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(2));
    });
    let gateway =
        HermesOneShotGateway::start_with_credential(credential(endpoint), "hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let request = tokio::spawn(async move {
        reqwest::Client::new()
            .post(format!("{}/responses", connection.base_url))
            .bearer_auth(connection.token)
            .json(&serde_json::json!({"model":"hermes-low", "stream":true, "input":"recap"}))
            .send()
            .await
    });
    tokio::task::spawn_blocking(move || accepted_rx.recv_timeout(Duration::from_secs(1)))
        .await
        .unwrap()
        .expect("request must reach the deliberately stalled upstream");
    tokio::time::timeout(
        Duration::from_secs(1),
        tokio::task::spawn_blocking(move || drop(gateway)),
    )
    .await
    .expect("gateway drop must cancel its upstream request")
    .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(1), request)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    release_tx.send(()).unwrap();
    upstream_thread.join().unwrap();
}

#[cfg(unix)]
#[test]
fn locked_profile_discards_source_behavior_and_contains_only_gateway_auth() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("profiles/recap");
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(destination.join("hook.py"), "steal ambient state").unwrap();
    let connection = HermesGatewayConnection {
        base_url: "http://127.0.0.1:43123".into(),
        token: "a".repeat(64),
    };
    write_locked_profile(&destination, &connection, "hermes-low").unwrap();

    assert!(!destination.join("hook.py").exists());
    assert_eq!(
        std::fs::metadata(&destination)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for file in ["config.yaml", "auth.json"] {
        assert_eq!(
            std::fs::metadata(destination.join(file))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(destination.join("config.yaml")).unwrap()).unwrap();
    assert_eq!(config["platform_toolsets"]["cli"], serde_json::json!([]));
    assert_eq!(config["auxiliary"]["title_generation"]["enabled"], false);
    assert_eq!(config["fallback_providers"], serde_json::json!([]));
    assert_eq!(config["model"]["default"], "hermes-low");
    assert_eq!(config["model"]["base_url"], connection.base_url);
}
