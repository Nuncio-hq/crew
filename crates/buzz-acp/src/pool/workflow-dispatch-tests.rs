use super::*;
use crate::{authorize_normal_listener_event, AuthorizedNormalListenerEvent, InboundAuthorGate};
use nostr::{EventBuilder, Keys, Kind, Tag};

include!("workflow-dispatch-fixture.rs");

#[tokio::test]
async fn authenticated_workflows_keep_parallel_thread_identity_through_actual_acp_prompts() {
    let relay_keys = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let (fixture, mut rest) = DispatchFixture::new(&relay_keys.public_key().to_hex()).await;
    let mut ctx = super::tests::make_prompt_context_no_owner();
    rest.keys = ctx.agent_keys.clone();
    ctx.rest_client = rest.clone();
    ctx.session_ledger_dir = fixture.root.join("ledger");
    let agent_hex = ctx.agent_keys.public_key().to_hex();
    let owner_cache = crate::OwnerCache::new(Some(owner.clone()));
    let channel = Uuid::new_v4();
    let channels = ChannelInfoResolver::new(
        HashMap::from([(
            channel,
            crate::relay::ChannelInfo {
                name: "workflows".into(),
                channel_type: "stream".into(),
                description: None,
            },
        )]),
        rest.clone(),
    );
    ctx.channel_info = channels.clone();
    let mut gate = InboundAuthorGate::connect(&rest, &agent_hex, "workflow dispatch test").await;
    let rules = [crate::filter::SubscriptionRule {
        require_mention: true,
        ..Default::default()
    }];
    let mut queue = crate::queue::EventQueue::new(DedupMode::Queue);
    let mut conversations = Vec::new();
    for content in ["workflow first", "workflow second"] {
        let event = EventBuilder::new(Kind::Custom(9), content)
            .tags([
                Tag::parse(["h", channel.to_string().as_str()]).expect("channel"),
                Tag::parse(["p", agent_hex.as_str()]).expect("recipient"),
                Tag::parse(["buzz:workflow", "true"]).expect("workflow marker"),
                Tag::parse(["buzz:workflow-owner", owner.as_str()]).expect("workflow owner"),
                Tag::parse(["buzz:workflow-mention", agent_hex.as_str()])
                    .expect("explicit mention"),
            ])
            .sign_with_keys(&relay_keys)
            .expect("signed workflow");
        let conversation = crate::conversation::id_for_event(channel, &event, false);
        conversations.push(conversation);
        let authorized = authorize_normal_listener_event(
            &mut gate,
            crate::relay::BuzzEvent {
                connection_generation: 0,
                channel_id: channel,
                event,
            },
            &crate::config::RespondTo::OwnerOnly,
            &HashSet::new(),
            &owner_cache,
            &channels,
            &rest,
        )
        .await
        .expect("authenticated workflow must cross actual listener boundary");
        let queued = AuthorizedNormalListenerEvent(authorized)
            .match_subscription(&rules, &agent_hex)
            .await
            .expect("explicit mention matches")
            .push(&mut queue, false);
        assert!(queued.accepted);
        assert_eq!(queued.channel_id, conversation);
        assert_eq!(queued.routing_channel_id, channel);
        assert_eq!(queued.effective_author, owner);
    }
    assert_ne!(conversations[0], conversations[1]);
    let first = queue.flush_next().expect("first thread");
    let second = queue
        .flush_next()
        .expect("second thread while first is in flight");
    assert_ne!(first.channel_id, second.channel_id);
    assert_eq!(first.routing_channel_id(), channel);
    assert_eq!(second.routing_channel_id(), channel);
    let first_agent = fixture.agent(0, first.channel_id).await;
    let second_agent = fixture.agent(1, second.channel_id).await;
    let ctx = Arc::new(ctx);
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(
            run_prompt_task(
                first_agent,
                Some(first),
                None,
                Arc::clone(&ctx),
                tx.clone(),
                None,
                "workflow-first".into()
            ),
            run_prompt_task(
                second_agent,
                Some(second),
                None,
                Arc::clone(&ctx),
                tx,
                None,
                "workflow-second".into()
            ),
        );
    })
    .await
    .expect("both actual ACP prompts complete concurrently");
    for _ in 0..2 {
        let result = rx.recv().await.expect("returned worker");
        assert!(matches!(
            result.outcome,
            PromptOutcome::Ok(StopReason::EndTurn)
        ));
        assert!(result.batch.is_none(), "completed work must not replay");
        let wire: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                fixture
                    .root
                    .join(format!("prompt-{}.json", result.agent.index)),
            )
            .expect("captured ACP prompt"),
        )
        .expect("wire request");
        assert_eq!(
            wire["params"]["sessionId"],
            format!("session-{}", result.agent.index)
        );
        let rendered = wire["params"]["prompt"].to_string();
        assert!(
            rendered.contains("workflow first") ^ rendered.contains("workflow second"),
            "each provider session receives exactly its own workflow thread"
        );
    }
}
