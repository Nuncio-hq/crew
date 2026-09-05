use super::*;

#[tokio::test]
async fn busy_session_owner_waits_while_unrelated_work_uses_idle_slot() {
    let channel = Uuid::new_v4();
    let mut owner = inert_owned_agent(0).await;
    owner
        .state
        .sessions
        .insert(channel, "original-session".into());
    let other = inert_owned_agent(1).await;
    let mut pool = AgentPool::from_slots(vec![Some(owner), Some(other)]);
    let owner = pool.try_claim(Some(channel)).expect("claim owner");
    let handle = pool.join_set.spawn(std::future::pending());
    pool.task_map_mut().insert(
        handle.id(),
        TaskMeta {
            agent_index: 0,
            channel_id: Some(Uuid::new_v4()),
            routing_channel_id: None,
            turn_id: "other-turn".into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            successful_steer_deliveries: HashSet::new(),
        },
    );
    assert!(pool.session_owner_busy(channel));
    assert!(pool.try_claim(Some(channel)).is_none());
    assert!(
        !pool.take_fill_demand(),
        "affinity wait must not spawn a substitute engine"
    );
    let other = pool
        .try_claim(Some(Uuid::new_v4()))
        .expect("unrelated work progresses");
    assert_eq!(other.index, 1);
    pool.return_agent(other);
    pool.task_map_mut().remove(&handle.id());
    pool.return_agent(owner);
    assert_eq!(
        pool.try_claim(Some(channel)).expect("owner returns").index,
        0
    );
    // A crashed owner has no live task and must not strand ledger recovery.
    assert!(!pool.session_owner_busy(channel));
    assert_eq!(
        pool.try_claim(Some(channel))
            .expect("dead owner can recover")
            .index,
        1
    );
    handle.abort();
}

/// A `Busy` outcome must carry the triggering batch back out (in
/// Queue-mode dedup) instead of silently dropping it — exercised through
/// `run_prompt_task` end-to-end, not just
/// `resolve_and_bind_channel_workspace` in isolation, so this also pins
/// the `PromptOutcome::WorkspaceBusy` shape the main loop switches on.
#[tokio::test]
async fn busy_shared_checkout_returns_batch_for_requeue() {
    let (fixture, repo) = init_git_fixture();
    let owner_keys = Keys::generate();
    let agent_keys = Keys::generate();
    let mut ctx = make_prompt_context_with_owner(&agent_keys, owner_keys.public_key());
    ctx.dedup_mode = DedupMode::Queue;
    let ctx = Arc::new(ctx);

    // `run_prompt_task` derives the workspace-lookup `cid` from the
    // batch's own `channel_id` (see `PromptSource::Channel(b.channel_id)`),
    // so — unlike the direct-call test above — the two must match here.
    let first_channel_id = Uuid::new_v4();
    let second_channel_id = Uuid::new_v4();
    let first =
        owner_project_workspace_batch_with(first_channel_id, &owner_keys, &repo, &[("ws", "main")]);
    let second = owner_project_workspace_batch_with(
        second_channel_id,
        &owner_keys,
        &repo,
        &[("ws", "main")],
    );
    let mut first_state = SessionState::default();

    // Hold the shared MainCheckout lease directly (no live task needed —
    // same technique as `path_lease_busy_is_a_named_refusal_not_an_error`).
    let held =
        resolve_and_bind_channel_workspace(&first_channel_id, &first, &ctx, &mut first_state)
            .await
            .expect("first turn binds");
    assert!(matches!(held, ChannelWorkspace::Bound { .. }));

    let agent = inert_owned_agent(0).await;
    let (result_tx, mut result_rx) = mpsc::unbounded_channel();

    run_prompt_task(
        agent,
        Some(second.clone()),
        None,
        Arc::clone(&ctx),
        result_tx.clone(),
        None,
        "turn-busy".into(),
    )
    .await;

    let result = result_rx.recv().await.expect("prompt result");
    let PromptOutcome::WorkspaceBusy { message } = result.outcome else {
        panic!("expected a WorkspaceBusy outcome so the main loop requeues, not respawns");
    };
    assert!(
        message.contains("Main checkout busy"),
        "busy copy carried through: {message}"
    );

    let requeued = result
        .batch
        .expect("Queue-mode dedup must return the triggering batch for requeue");
    assert_eq!(requeued.events.len(), second.events.len());
    assert_eq!(
        requeued.events[0].event.id, second.events[0].event.id,
        "the exact triggering event must survive for requeue, not a substitute"
    );
    assert!(
        !result.agent.state.sessions.contains_key(&second_channel_id),
        "no ACP session should have been created while the checkout was busy"
    );

    drop(held);
    std::fs::remove_dir_all(&fixture).expect("fixture cleanup");
}
