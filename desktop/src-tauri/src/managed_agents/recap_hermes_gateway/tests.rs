use super::*;
use axum::http::HeaderMap;
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
        axum::serve(
            listener,
            Router::new().route(RESPONSES_PATH, post(upstream)),
        )
        .await
        .unwrap();
    });
    format!("http://{address}{RESPONSES_PATH}")
}

#[tokio::test(flavor = "multi_thread")]
async fn forwards_exactly_one_authenticated_no_tools_request_and_observes_model() {
    let endpoint = test_upstream().await;
    let gateway =
        HermesOneShotGateway::start_with_credential(credential(endpoint), "hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
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
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
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
    let HermesGatewayEvidence::Forwarded(exchange) = evidence else {
        panic!("forwarded evidence expected");
    };
    assert_eq!(exchange.effective_model, "hermes-low");
    assert_eq!(exchange.request_count, 1);
    assert_eq!(exchange.request_method, "POST");
    assert_eq!(exchange.request_path, RESPONSES_PATH);
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
        (
            "local".to_string(),
            serde_json::json!({"model":"hermes-low","stream":true,"tool_choice":"auto"}),
            StatusCode::BAD_GATEWAY,
        ),
        (
            "local".to_string(),
            serde_json::json!({"model":"hermes-low","stream":false}),
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
            .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
            .bearer_auth(actual_token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        drop(gateway);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_is_rejected_before_the_body_is_read() {
    let endpoint = test_upstream().await;
    let gateway =
        HermesOneShotGateway::start_with_credential(credential(endpoint), "hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"model":"hermes-low", "stream":true}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    // The second request is past the single-admission cap; even a body that is
    // unparseable must get a CONFLICT without the gateway reading it.
    let replay = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .body(vec![0xff; REQUEST_LIMIT + 64])
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CONFLICT);
    let evidence = tokio::task::spawn_blocking(move || gateway.finish())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(evidence, HermesGatewayEvidence::Forwarded(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_phase_serves_the_injected_tool_call_once() {
    let gateway = HermesOneShotGateway::start_hostile("hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let response = reqwest::Client::new()
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"model":"hermes-low", "stream":true, "input":"probe"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let body = response.text().await.unwrap();
    assert!(body.contains(RECAP_TOOL_PROBE_ID));
    assert!(body.contains("function_call"));
    let evidence = tokio::task::spawn_blocking(move || gateway.finish())
        .await
        .unwrap()
        .unwrap();
    let HermesGatewayEvidence::Hostile(exchange) = evidence else {
        panic!("hostile evidence expected");
    };
    assert!(exchange.served_tool_call);
    assert_eq!(exchange.request_method, "POST");
    assert_eq!(exchange.request_path, RESPONSES_PATH);
    assert_eq!(exchange.terminal_rejection, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_followup_with_matching_rejection_is_the_terminal_proof() {
    let gateway = HermesOneShotGateway::start_hostile("hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let client = reqwest::Client::new();
    let first = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"model":"hermes-low", "stream":true}))
        .send()
        .await
        .unwrap();
    assert!(first.status().is_success());
    let _ = first.bytes().await;
    let rejection = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({
            "model": "hermes-low",
            "stream": true,
            "input": [{
                "type": "function_call_output",
                "call_id": RECAP_TOOL_PROBE_ID,
                "output": "error: tool crew-recap-hostile-tool-v1 does not exist"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert!(rejection.status().is_success());
    let _ = rejection.bytes().await;
    let evidence = tokio::task::spawn_blocking(move || gateway.finish())
        .await
        .unwrap()
        .unwrap();
    let HermesGatewayEvidence::Hostile(exchange) = evidence else {
        panic!("hostile evidence expected");
    };
    assert_eq!(
        exchange.terminal_rejection.as_deref(),
        Some(RECAP_TOOL_PROBE_ID)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_followup_reporting_tool_success_fails_the_probe() {
    let gateway = HermesOneShotGateway::start_hostile("hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let client = reqwest::Client::new();
    let first = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"model":"hermes-low", "stream":true}))
        .send()
        .await
        .unwrap();
    let _ = first.bytes().await;
    let executed = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({
            "model": "hermes-low",
            "stream": true,
            "input": [{
                "type": "function_call_output",
                "call_id": RECAP_TOOL_PROBE_ID,
                "output": "ok: wrote probe-sentinel"
            }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(executed.status(), StatusCode::BAD_GATEWAY);
    let outcome = tokio::task::spawn_blocking(move || gateway.finish())
        .await
        .unwrap();
    assert_eq!(outcome, Err(HermesGatewayFailure::ToolExecuted));
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_followup_that_continues_normally_is_invalid() {
    let gateway = HermesOneShotGateway::start_hostile("hermes-low").unwrap();
    let connection = gateway.connection().clone();
    let client = reqwest::Client::new();
    let first = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({"model":"hermes-low", "stream":true}))
        .send()
        .await
        .unwrap();
    let _ = first.bytes().await;
    let unrelated = client
        .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
        .bearer_auth(&connection.token)
        .json(&serde_json::json!({
            "model": "hermes-low",
            "stream": true,
            "input": [{"type": "message", "content": [{"type": "input_text", "text": "next"}]}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(unrelated.status(), StatusCode::BAD_GATEWAY);
    let outcome = tokio::task::spawn_blocking(move || gateway.finish())
        .await
        .unwrap();
    assert_eq!(outcome, Err(HermesGatewayFailure::InvalidRequest));
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

#[test]
fn hostile_followup_classification_requires_probe_call_id_and_denial() {
    let rejection = serde_json::json!({
        "input": [{"type": "function_call_output", "call_id": RECAP_TOOL_PROBE_ID,
                   "output": "unknown tool"}]
    });
    assert!(matches!(
        classify_hostile_followup(&rejection),
        HostileFollowup::TerminalRejection(_)
    ));
    let success = serde_json::json!({
        "input": [{"type": "function_call_output", "call_id": RECAP_TOOL_PROBE_ID,
                   "output": "wrote file"}]
    });
    assert!(matches!(
        classify_hostile_followup(&success),
        HostileFollowup::ExecutedOrUnknown
    ));
    let unrelated = serde_json::json!({
        "input": [{"type": "function_call_output", "call_id": "other-call",
                   "output": "unknown tool"}]
    });
    assert!(matches!(
        classify_hostile_followup(&unrelated),
        HostileFollowup::Unrelated
    ));
    let no_output = serde_json::json!({"input": "text only"});
    assert!(matches!(
        classify_hostile_followup(&no_output),
        HostileFollowup::Unrelated
    ));
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
            .post(format!("{}{}", connection.base_url, RESPONSES_PATH))
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
    assert_eq!(config["mcp_servers"], serde_json::json!({}));
    assert_eq!(config["model"]["default"], "hermes-low");
    assert_eq!(config["model"]["base_url"], connection.base_url);
    let auth: serde_json::Value =
        serde_json::from_slice(&std::fs::read(destination.join("auth.json")).unwrap()).unwrap();
    assert_eq!(
        auth["credential_pool"]["openai-codex"][0]["access_token"],
        connection.token
    );
    assert_eq!(
        auth["credential_pool"]["openai-codex"][0]["base_url"],
        connection.base_url
    );
}

#[cfg(unix)]
#[test]
fn locked_profile_rejects_relative_destination_and_bad_connection() {
    let connection = HermesGatewayConnection {
        base_url: "https://provider.example".into(),
        token: "a".repeat(64),
    };
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        write_locked_profile(&root.path().join("p"), &connection, "m"),
        Err(HermesGatewayFailure::InvalidCredential)
    );
    let short_token = HermesGatewayConnection {
        base_url: "http://127.0.0.1:43123".into(),
        token: "short".into(),
    };
    assert_eq!(
        write_locked_profile(&root.path().join("p"), &short_token, "m"),
        Err(HermesGatewayFailure::InvalidCredential)
    );
    assert_eq!(
        write_locked_profile(
            Path::new("relative/destination"),
            &HermesGatewayConnection {
                base_url: "http://127.0.0.1:43123".into(),
                token: "a".repeat(64),
            },
            "m",
        ),
        Err(HermesGatewayFailure::InvalidCredential)
    );
}
