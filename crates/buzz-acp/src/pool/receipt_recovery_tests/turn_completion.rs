use super::super::tests::make_prompt_context_no_owner;
use super::*;

#[tokio::test]
async fn rejected_receipt_cannot_panic_or_requeue_completed_agent_work() {
    for (status, body) in [
        (400, format!("{}é", "a".repeat(511))),
        (400, "restricted: unknown event kind".into()),
        (
            403,
            "restricted: agent receipt must be authored by a registered agent".into(),
        ),
    ] {
        let mut fixture = Fixture::new(Response::Reject(status, body)).await;
        let channel = Uuid::new_v4();
        let mut ctx = make_prompt_context_no_owner();
        let (metadata, _, _metadata_server) =
            super::super::tests::counting_resolver(serde_json::json!([])).await;
        ctx.channel_info = metadata;
        ctx.agent_keys = fixture.rest.keys.clone();
        ctx.rest_client = fixture.rest.clone();
        ctx.relay_url = fixture.rest.base_url.clone();
        ctx.agent_receipts_enabled = true;
        ctx.dedup_mode = DedupMode::Queue;
        ctx.channel_info
            .cache
            .write()
            .expect("channel cache")
            .insert(
                channel,
                PromptChannelInfo {
                    name: "receipt-boundary".into(),
                    channel_type: "stream".into(),
                    description: None,
                    project: None,
                },
            );
        let outbox = receipt_outbox_dir(&ctx).expect("outbox");
        fixture.outbox = outbox.clone();
        let event = EventBuilder::text_note("complete once")
            .tag(nostr::Tag::public_key(ctx.agent_keys.public_key()))
            .sign_with_keys(&Keys::generate())
            .expect("trigger");
        let trigger = event.id.to_hex();
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
        let acp = AcpClient::spawn("bash", &["-c".into(),
            "while IFS= read -r line; do printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"stopReason\":\"end_turn\"}}'; done".into()], &[], false)
            .await.expect("protocol adapter");
        let mut agent = OwnedAgent {
            index: 0,
            acp,
            state: SessionState::default(),
            model_capabilities: None,
            desired_model: None,
            model_overridden: false,
            desired_model_request_id: None,
            desired_model_pending_ack: false,
            startup_effort: None,
            agent_name: "test".into(),
            goose_system_prompt_supported: None,
            protocol_version: 1,
            load_session_supported: false,
        };
        agent.state.sessions.insert(channel, "same-session".into());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run_prompt_task(
            agent,
            Some(batch),
            None,
            Arc::new(ctx),
            tx,
            None,
            "completed-before-receipt".into(),
        ));
        tokio::time::timeout(Duration::from_secs(8), task)
            .await
            .expect("bounded receipt rejection")
            .expect("receipt rejection must not panic a completed agent task");
        let result = rx.recv().await.expect("original agent returned");
        assert!(matches!(
            result.outcome,
            PromptOutcome::Ok(StopReason::EndTurn)
        ));
        assert!(
            result.batch.is_none(),
            "never replay completed work to repair a receipt"
        );
        assert_eq!(result.agent.index, 0);
        assert_eq!(
            result
                .agent
                .state
                .sessions
                .get(&channel)
                .map(String::as_str),
            Some("same-session")
        );
        assert!(result
            .agent
            .state
            .deliveries
            .get(&channel)
            .expect("delivery state")
            .delivered_event_ids
            .contains(&trigger));
        let receipts = fixture.submitted();
        assert_eq!(receipts.len(), 1, "permanent rejection gets one attempt");
        let rejected = outbox.join(format!("{}.rejected", receipts[0].id));
        let retained: nostr::Event = serde_json::from_slice(
            &tokio::fs::read(rejected)
                .await
                .expect("retained signed receipt"),
        )
        .expect("event");
        assert_eq!(retained, receipts[0]);
        tokio::fs::remove_dir_all(outbox)
            .await
            .expect("cleanup outbox");
    }
}
