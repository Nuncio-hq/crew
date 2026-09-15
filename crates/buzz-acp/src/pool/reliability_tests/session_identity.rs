use super::*;

/// The production prompt worker resolves the ACP session inside the task, so
/// the exact-steer fence must learn that resolved ID from the worker itself.
/// This uses a protocol-speaking local ACP peer and the real worker entrypoint;
/// the pool metadata below is the same task metadata that dispatch installs.
#[tokio::test]
async fn prompt_worker_updates_exact_steer_identity_to_resolved_session() {
    let script_path =
        std::env::temp_dir().join(format!("crew-acp-session-identity-{}.py", Uuid::new_v4()));
    std::fs::write(
        &script_path,
        r#"import json
import sys

for line in sys.stdin:
    request = json.loads(line)
    request_id = request.get("id")
    if request_id is None:
        continue
    method = request.get("method")
    if method == "initialize":
        result = {"protocolVersion": 1, "agentCapabilities": {}}
    elif method == "session/new":
        result = {"sessionId": "resolved-session"}
    elif method == "session/prompt":
        result = {"stopReason": "end_turn"}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}), flush=True)
"#,
    )
    .expect("write protocol peer");

    let mut agent = inert_owned_agent(0).await;
    agent.acp.shutdown().await;
    agent.acp = AcpClient::spawn("python3", &[script_path.display().to_string()], &[], false)
        .await
        .expect("spawn protocol peer");
    agent
        .acp
        .initialize()
        .await
        .expect("initialize protocol peer");

    let mut ctx = make_prompt_context_no_owner();
    ctx.max_turn_duration = Duration::from_secs(5);
    let ctx = Arc::new(ctx);
    let identity = TaskSessionIdentity::new(Some("old-session".into()));
    let worker_identity = identity.clone();
    let (result_tx, mut result_rx) = mpsc::unbounded_channel();
    let turn_id = "resolved-session-turn".to_owned();
    let worker = tokio::spawn(run_prompt_task_with_session_identity(
        agent,
        None,
        Some("resolve the replacement session".into()),
        ctx,
        result_tx,
        None,
        None,
        turn_id.clone(),
        worker_identity,
    ));

    let channel = Uuid::new_v4();
    let (steer_tx, mut steer_rx) = mpsc::channel(1);
    let mut pool = AgentPool::from_slots(vec![]);
    pool.task_map_mut().insert(
        worker.id(),
        TaskMeta {
            agent_index: 0,
            channel_id: Some(channel),
            routing_channel_id: Some(channel),
            session_id: identity,
            turn_id: turn_id.clone(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: Some(steer_tx),
            successful_steer_deliveries: HashSet::new(),
        },
    );

    let mut result = tokio::time::timeout(Duration::from_secs(5), result_rx.recv())
        .await
        .expect("prompt worker must return a result")
        .expect("prompt worker result");
    assert!(matches!(
        result.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));

    let (stale_ack_tx, _stale_ack_rx) = tokio::sync::oneshot::channel();
    let stale = SteerRequest {
        prompt_blocks: vec!["stale target".into()],
        strict_target: Some(StrictSteerTarget {
            session_id: "old-session".into(),
            turn_id: turn_id.clone(),
            request_id: "stale-request".into(),
        }),
        ack_tx: stale_ack_tx,
    };
    assert!(matches!(
        pool.send_exact_steer(channel, channel, &turn_id, stale),
        Err(SteerError::StrictTargetMismatch)
    ));

    let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel();
    let current = SteerRequest {
        prompt_blocks: vec!["current target".into()],
        strict_target: Some(StrictSteerTarget {
            session_id: "resolved-session".into(),
            turn_id,
            request_id: "current-request".into(),
        }),
        ack_tx,
    };
    assert!(pool
        .send_exact_steer(channel, channel, "resolved-session-turn", current)
        .is_ok());
    assert_eq!(
        steer_rx
            .recv()
            .await
            .expect("current session target must reach the pool sender")
            .prompt_blocks,
        vec!["current target".to_owned()]
    );

    worker.await.expect("prompt worker task");
    result.agent.acp.shutdown().await;
    std::fs::remove_file(script_path).expect("remove protocol peer");
}
