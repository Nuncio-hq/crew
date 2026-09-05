use super::*;

async fn spawn_ready_script(script: &str) -> AcpClient {
    let mut client = AcpClient::spawn(
        "bash",
        &["-c".into(), format!("echo ready; {script}")],
        &[],
        false,
    )
    .await
    .unwrap();
    assert_eq!(client.reader.next().await.unwrap().unwrap(), "ready");
    client
}

#[tokio::test]
async fn tracked_tool_survives_ordinary_idle() {
    let mut client = spawn_ready_script(r#"echo '{"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":"tool_call","toolCallId":"one","status":"in_progress"}}}'; sleep 0.25; echo '{"id":999,"result":{"stopReason":"end_turn"}}'; sleep 1"#).await;
    let result = client
        .read_until_response_with_idle_timeout(
            "test",
            999,
            std::time::Duration::from_millis(100),
            tokio::time::Instant::now() + std::time::Duration::from_secs(2),
            std::time::Duration::from_secs(2),
        )
        .await;
    client.shutdown().await;
    assert!(
        result.is_ok(),
        "active tool must survive ordinary idle: {result:?}"
    );
}

async fn permission_outcome(options: serde_json::Value) -> serde_json::Value {
    let mut client = AcpClient::spawn("cat", &[], &[], false).await.unwrap();
    client.set_permission_auto_approve(true);
    let result = client
        .handle_permission_request(
            &serde_json::json!({"id":"request", "params":{"options":options}}),
        )
        .await;
    assert!(
        result.is_ok(),
        "options must resolve without aborting turn: {result:?}"
    );
    let line = client.reader.next().await.unwrap().unwrap();
    client.shutdown().await;
    serde_json::from_str::<serde_json::Value>(&line).unwrap()["result"]["outcome"].clone()
}

#[tokio::test]
async fn standard_permission_kinds_are_selected_exactly() {
    for kind in ["allow_once", "reject_once", "reject_always"] {
        assert_eq!(
            permission_outcome(serde_json::json!([{"kind":kind,"optionId":"actual-id"}])).await,
            serde_json::json!({"outcome":"selected","optionId":"actual-id"})
        );
    }
}

#[tokio::test]
async fn malformed_or_unknown_permission_options_cancel() {
    for options in [
        serde_json::Value::Null,
        serde_json::json!([]),
        serde_json::json!([{"kind":"disallow","optionId":"allow","name":"Allow"}]),
        serde_json::json!([{"kind":"reject_once"}]),
        serde_json::json!([{"kind":"allow_once","optionId":""}]),
    ] {
        assert_eq!(
            permission_outcome(options).await,
            serde_json::json!({"outcome":"cancelled"})
        );
    }
}

#[test]
fn tool_budget_flag_accepts_existing_long_idle_and_short_hard_caps() {
    use clap::Parser;
    for extra in [
        vec!["--idle-timeout", "3000"],
        vec![
            "--idle-timeout",
            "1",
            "--max-turn-duration",
            "2",
            "--tool-idle-timeout",
            "2400",
        ],
    ] {
        let mut args = vec![
            "buzz-acp",
            "--private-key",
            "0000000000000000000000000000000000000000000000000000000000000001",
        ];
        args.extend(extra);
        let args =
            crate::config::CliArgs::try_parse_from(args).expect("tool budget flag must parse");
        let config =
            crate::config::Config::from_args(args).expect("existing deadline config stays valid");
        assert_eq!(
            config.tool_idle_timeout_secs,
            2400.max(config.idle_timeout_secs)
        );
    }
}

#[tokio::test]
async fn permission_priority_skips_invalid_ids_without_guessing() {
    assert_eq!(
        permission_outcome(serde_json::json!([
            {"kind":"reject_once","optionId":"reject"},
            {"kind":"allow_always","optionId":"always"},
            {"kind":"allow_once","optionId":null},
            {"kind":"allow_once","optionId":"once"}
        ]))
        .await,
        serde_json::json!({"outcome":"selected","optionId":"once"})
    );
}

#[tokio::test]
async fn tool_lifetime_remains_bounded_without_terminal_frame() {
    let mut client = spawn_ready_script(r#"echo '{"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":"tool_call","toolCallId":"one","status":"in_progress"}}}'; sleep 0.20; echo '{"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":"keepalive"}}}'; sleep 10"#).await;
    client.set_tool_idle_timeout(std::time::Duration::from_millis(300));
    let started = tokio::time::Instant::now();
    let result = client
        .read_until_response_with_idle_timeout(
            "test",
            999,
            std::time::Duration::from_millis(50),
            started + std::time::Duration::from_secs(3),
            std::time::Duration::from_secs(3),
        )
        .await;
    let elapsed = started.elapsed();
    client.shutdown().await;
    assert!(
        matches!(result, Err(AcpError::IdleTimeout(_))),
        "{result:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(450),
        "unrelated keepalive must not renew tool lifetime: {elapsed:?}"
    );
}

#[tokio::test]
async fn tracked_tool_never_extends_hard_deadline() {
    let mut client = spawn_ready_script(r#"echo '{"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":"tool_call","toolCallId":"one","status":"in_progress"}}}'; sleep 10"#).await;
    let result = client
        .read_until_response_with_idle_timeout(
            "test",
            999,
            std::time::Duration::from_millis(50),
            tokio::time::Instant::now() + std::time::Duration::from_millis(150),
            std::time::Duration::from_millis(150),
        )
        .await;
    client.shutdown().await;
    assert!(
        matches!(result, Err(AcpError::HardTimeout { .. })),
        "{result:?}"
    );
}

#[tokio::test]
async fn completed_prompt_freezes_remaining_effective_budget_for_human_decision() {
    let mut client = spawn_ready_script(r#"read -r request; sleep 0.05; echo '{"id":0,"result":{"stopReason":"end_turn"}}'; sleep 10"#).await;
    let maximum = std::time::Duration::from_secs(2);
    let result = client
        .session_prompt_with_idle_timeout("test", "prompt", maximum, maximum)
        .await;
    assert!(result.is_ok(), "{result:?}");
    let remaining = client
        .remaining_prompt_budget()
        .expect("successful prompt saves budget");
    assert!(remaining < maximum && remaining > std::time::Duration::ZERO);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        client.remaining_prompt_budget(),
        Some(remaining),
        "human decision waiting must not consume the remaining execution budget"
    );
    client.shutdown().await;
}
