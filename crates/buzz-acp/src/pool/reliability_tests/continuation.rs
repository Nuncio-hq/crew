use super::*;

async fn continuation_task_fixture(stop: bool, decision_wait: bool) {
    let channel = Uuid::new_v4();
    let marker = std::env::temp_dir().join(format!("crew-continuation-{}", Uuid::new_v4()));
    std::fs::write(&marker, "").expect("marker");
    let marker_path = marker.to_string_lossy().replace('\'', "'\\''");
    let second_response = if stop {
        "IFS= read -r cancel; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"stopReason\":\"cancelled\"}}'"
    } else {
        "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32000,\"message\":\"ResourceExhausted\"}}'"
    };
    let script = format!(
        r#"IFS= read -r prompt
printf '%s\n' '{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"live-session","update":{{"sessionUpdate":"plan","entries":[{{"content":"Reply","status":"pending","priority":"medium"}}]}}}}}}'
printf '%s\n' '{{"jsonrpc":"2.0","id":0,"result":{{"stopReason":"end_turn"}}}}'
IFS= read -r continuation
printf ready > '{marker_path}'
{second_response}
while IFS= read -r line; do :; done"#
    );
    let mut agent = inert_owned_agent(0).await;
    agent.acp = AcpClient::spawn("bash", &["-c".into(), script], &[], false)
        .await
        .expect("adapter");
    agent.state.sessions.insert(channel, "live-session".into());
    let mut ctx = make_prompt_context_no_owner();
    let (metadata, _, _metadata_server) =
        super::super::tests::counting_resolver(serde_json::json!([])).await;
    ctx.channel_info = metadata;
    let event = EventBuilder::new(Kind::Custom(9), "make a plan and execute")
        .tag(nostr::Tag::public_key(ctx.agent_keys.public_key()))
        .sign_with_keys(&Keys::generate())
        .expect("event");
    let event_id = event.id.to_hex();
    let batch = FlushBatch {
        channel_id: channel,
        events: vec![crate::queue::BatchEvent {
            event,
            prompt_tag: "test".into(),
            received_at: std::time::Instant::now(),
            edited_content: None,
        }],
        cancelled_events: vec![],
        cancel_reason: None,
    };
    ctx.dedup_mode = DedupMode::Queue;
    ctx.context_message_limit = 0;
    ctx.channel_info.cache.write().expect("cache").insert(
        channel,
        PromptChannelInfo {
            name: "test".into(),
            channel_type: "stream".into(),
            description: None,
            project: None,
        },
    );
    let (publisher, mut published) = crate::relay::RelayEventPublisher::test_pair();
    if decision_wait {
        ctx.max_turn_duration = Duration::from_millis(150);
        ctx.user_input_runtime = Some(crate::elicitation::QuestionRuntime::new(
            publisher,
            ctx.agent_keys.clone(),
            Arc::new(crate::OwnerCache::new(Some(
                ctx.agent_keys.public_key().to_hex(),
            ))),
            ctx.rest_client.clone(),
        ));
        agent.acp.set_user_input_enabled(true);
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (control_tx, control_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(run_prompt_task(
        agent,
        Some(batch),
        None,
        Arc::new(ctx),
        tx,
        Some(control_rx),
        "continuation-turn".into(),
    ));
    if decision_wait {
        tokio::time::timeout(Duration::from_secs(15), published.recv())
            .await
            .expect("decision published")
            .expect("question");
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(
            rx.try_recv().is_err(),
            "human decision wait must pause execution budget"
        );
        control_tx
            .send(ControlSignal::Cancel)
            .expect("Stop delivered during decision");
    } else if stop {
        tokio::time::timeout(Duration::from_secs(15), async {
            while std::fs::read_to_string(&marker).expect("marker").is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("continued prompt arrived");
        control_tx
            .send(ControlSignal::Cancel)
            .expect("Stop delivered");
    }
    let result = tokio::time::timeout(Duration::from_secs(15), rx.recv())
        .await
        .expect("prompt result bounded")
        .expect("result");
    if stop || decision_wait {
        assert!(matches!(result.outcome, PromptOutcome::Cancelled));
    } else {
        assert!(matches!(
            result.outcome,
            PromptOutcome::Error(AcpError::AgentError { .. })
        ));
        assert_eq!(
            result.batch.expect("retry retains original request").events[0]
                .event
                .id
                .to_hex(),
            event_id
        );
    }
    assert!(
        !result
            .agent
            .state
            .deliveries
            .get(&channel)
            .is_some_and(|d| d.delivered_event_ids.contains(&event_id)),
        "the initial EndTurn must not commit delivery before continued work succeeds"
    );
    assert_eq!(
        result
            .agent
            .state
            .turn_counts
            .get(&channel)
            .copied()
            .unwrap_or(0),
        0
    );
    task.await.expect("task");
    std::fs::remove_file(marker).expect("remove marker");
}

#[tokio::test]
async fn continuation_error_preserves_batch_and_never_commits_initial_success() {
    continuation_task_fixture(false, false).await;
}

#[tokio::test]
async fn stop_interrupts_continued_prompt_without_committing_initial_success() {
    continuation_task_fixture(true, false).await;
}

#[tokio::test]
async fn stop_interrupts_continuation_decision_without_committing_initial_success() {
    continuation_task_fixture(true, true).await;
}
