use super::*;

fn subscribe(state: &mut BgState, channel_id: Uuid) {
    apply_command_to_state(
        state,
        RelayCommand::Subscribe {
            channel_id,
            filter: ChannelFilter {
                kinds: Some(vec![9]),
                require_mention: true,
            },
            replay_since: Some(1000),
        },
    );
}

#[tokio::test]
async fn access_denied_updates_latest_snapshot_before_membership_add() {
    let channel = Uuid::new_v4();
    let unrelated = Uuid::new_v4();
    let mut state = BgState::new();
    let mut snapshots = state.subscription_snapshot.subscribe();
    subscribe(&mut state, channel);
    subscribe(&mut state, unrelated);
    assert_eq!(snapshots.borrow_and_update().channels.len(), 2);
    assert!(drop_channel_on_access_denied(
        &mut state,
        &channel_sub_id(channel),
        "restricted: channel access revoked"
    ));
    // Read current authority even before changed() is polled by the main loop.
    assert!(
        !snapshots.borrow().channels.contains(&channel),
        "a later membership add must not skip the dropped channel"
    );
    assert!(snapshots.borrow().channels.contains(&unrelated));
    assert!(snapshots.has_changed().unwrap());
    subscribe(&mut state, channel);
    assert_eq!(snapshots.borrow().channels.len(), 2);
}

#[test]
fn dropped_updates_coalesce_without_losing_latest_subscription_authority() {
    let mut state = BgState::new();
    let mut snapshots = state.subscription_snapshot.subscribe();
    let channel = Uuid::new_v4();
    for _ in 0..2000 {
        subscribe(&mut state, channel);
        drop_channel_on_access_denied(
            &mut state,
            &channel_sub_id(channel),
            "restricted: not a channel member",
        );
    }
    assert!(snapshots.has_changed().unwrap());
    assert!(
        snapshots.borrow_and_update().channels.is_empty(),
        "latest denied state must survive a slow consumer"
    );
    assert!(!snapshots.has_changed().unwrap());
}

#[tokio::test]
async fn denied_channel_rejoins_on_same_socket_and_reconnect_keeps_other_channels() {
    let (mut client, mut server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    let keys = Keys::generate();
    let (event_tx, mut event_rx) = mpsc::channel(1);
    let (observer_tx, _observer_rx) = mpsc::channel(1);
    let (_command_tx, mut command_rx) = mpsc::channel(1);
    let channel = Uuid::new_v4();
    let unrelated = Uuid::new_v4();
    let mut snapshots = state.subscription_snapshot.subscribe();
    let subscribe_command = |channel_id| RelayCommand::Subscribe {
        channel_id,
        filter: ChannelFilter {
            kinds: Some(vec![9]),
            require_mention: true,
        },
        replay_since: Some(2000),
    };
    for id in [channel, unrelated] {
        assert!(
            execute_connected_command(&mut client, &mut state, "agent", subscribe_command(id))
                .await
        );
        let frame = super::tests::next_test_frame(&mut server).await;
        assert_eq!(frame[0], "REQ");
        assert_eq!(frame[1], channel_sub_id(id));
    }
    assert_eq!(snapshots.borrow_and_update().channels.len(), 2);
    let closed = Message::Text(
        json!([
            "CLOSED",
            channel_sub_id(channel),
            "restricted: not a channel member"
        ])
        .to_string()
        .into(),
    );
    assert!(
        handle_ws_message(
            closed,
            &mut client,
            &event_tx,
            &observer_tx,
            &mut state,
            &keys,
            "ws://fixture",
            "agent",
            None
        )
        .await,
        "per-channel denial must retain the socket"
    );
    assert_eq!(snapshots.borrow_and_update().channels.len(), 1);
    assert!(
        event_rx.try_recv().is_err(),
        "denial must not fabricate membership removal or connection loss"
    );
    assert!(matches!(
        resubscribe_after_reconnect(&mut client, &mut command_rx, &mut state, "agent", true).await,
        ResubscribeResult::Ok
    ));
    assert_eq!(
        super::tests::next_test_frame(&mut server).await[1],
        channel_sub_id(unrelated)
    );
    assert!(
        !snapshots.has_changed().unwrap(),
        "reconnect must not clear retained subscription intent"
    );
    // The membership-add handler must consult this same current snapshot,
    // without relying on a previously consumed changed notification.
    assert!(!snapshots.borrow().channels.contains(&channel));
    assert!(
        execute_connected_command(&mut client, &mut state, "agent", subscribe_command(channel))
            .await
    );
    let restored = super::tests::next_test_frame(&mut server).await;
    assert_eq!(restored[1], channel_sub_id(channel));
    assert_eq!(restored[2]["#h"], json!([channel.to_string()]));
    assert_eq!(restored[2]["#p"], json!(["agent"]));
    assert_eq!(snapshots.borrow().channels.len(), 2);
}

/// A channel parked for retry after a partial reconnect keeps its subscription
/// intent but must NOT be published as a confirmed subscription — otherwise the
/// harness reports a channel count for a REQ the relay never accepted.
#[tokio::test]
async fn retry_parked_channel_publishes_intent_without_confirmation() {
    let (mut client, mut server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    let channel = Uuid::new_v4();
    let mut snapshots = state.subscription_snapshot.subscribe();

    state.set_connected(true);
    state.membership_sub_active = true;
    state.startup_subscriptions_ready = true;
    state.publish_subscription_snapshot();
    assert!(
        execute_connected_command(
            &mut client,
            &mut state,
            "agent",
            RelayCommand::Subscribe {
                channel_id: channel,
                filter: ChannelFilter {
                    kinds: Some(vec![9]),
                    require_mention: true,
                },
                replay_since: Some(3000),
            }
        )
        .await
    );
    assert_eq!(super::tests::next_test_frame(&mut server).await[0], "REQ");
    let confirmed = snapshots.borrow_and_update().clone();
    assert_eq!(confirmed.channels, HashSet::from([channel]));
    assert!(
        confirmed.confirmed,
        "an accepted REQ on a live socket is a confirmed subscription"
    );

    // An interrupted command parks the channel for the main-loop retry drain.
    retain_failed_command_intent(
        &mut state,
        RelayCommand::Subscribe {
            channel_id: channel,
            filter: ChannelFilter {
                kinds: Some(vec![9]),
                require_mention: true,
            },
            replay_since: Some(3000),
        },
    );
    assert!(state.resubscribe_retry.contains(&channel));
    let parked = snapshots.borrow_and_update().clone();
    assert_eq!(
        parked.channels,
        HashSet::from([channel]),
        "intent must survive so the membership diff does not re-add the channel"
    );
    assert!(
        !parked.confirmed,
        "a channel awaiting retry is not a confirmed subscription"
    );

    // A successful retry drain restores confirmation.
    assert_eq!(
        drain_resubscribe_retry(&mut client, &mut state, "agent", 1).await,
        1
    );
    assert_eq!(super::tests::next_test_frame(&mut server).await[0], "REQ");
    assert!(
        snapshots.borrow_and_update().confirmed,
        "an accepted retry REQ confirms the subscription again"
    );

    // Losing the socket cannot leave stale confirmation behind either.
    state.set_connected(false);
    let disconnected = snapshots.borrow_and_update().clone();
    assert_eq!(disconnected.channels, HashSet::from([channel]));
    assert!(!disconnected.confirmed);
}

/// The membership notification watch is part of the authority behind the
/// channel count. A channel subscription alone cannot confirm that the
/// current set is still complete: the watch may not have started, may be
/// parked for a retry, or may have dropped a notification that needs replay.
#[test]
fn membership_watch_state_gates_snapshot_readiness() {
    let mut state = BgState::new();
    let channel = Uuid::new_v4();
    let mut snapshots = state.subscription_snapshot.subscribe();

    state.connected = true;
    state.startup_subscriptions_ready = true;
    state
        .active_subscriptions
        .insert(channel, channel_sub_id(channel));
    state.publish_subscription_snapshot();
    assert_eq!(
        snapshots.borrow_and_update().channels,
        HashSet::from([channel])
    );
    assert!(
        !snapshots.borrow().confirmed,
        "a channel REQ cannot confirm readiness before the membership watch is live"
    );

    state.membership_sub_active = true;
    state.publish_subscription_snapshot();
    assert!(
        snapshots.borrow_and_update().confirmed,
        "a live membership watch plus a live channel REQ confirms readiness"
    );

    state.membership_resub_needed = true;
    state.publish_subscription_snapshot();
    assert!(
        !snapshots.borrow_and_update().confirmed,
        "a parked membership watch must make the observed set unknown"
    );

    state.membership_resub_needed = false;
    state.membership_dropped_since = Some(4_000);
    state.publish_subscription_snapshot();
    assert!(
        !snapshots.borrow_and_update().confirmed,
        "a dropped membership event must remain unknown until replay recovers"
    );

    state.membership_dropped_since = None;
    state.publish_subscription_snapshot();
    let recovered = snapshots.borrow_and_update().clone();
    assert_eq!(recovered.channels, HashSet::from([channel]));
    assert!(recovered.confirmed);

    // A healthy empty intent is still a confirmed zero once the membership
    // watch is live; this is the state the startup consumer must eventually
    // publish rather than bypassing readiness with an early count.
    state.active_subscriptions.clear();
    state.publish_subscription_snapshot();
    let zero = snapshots.borrow_and_update().clone();
    assert!(zero.channels.is_empty());
    assert!(zero.confirmed);
}

/// A membership REQ can complete before the discovered channel REQs are
/// applied. The startup marker must keep that intermediate empty intent
/// unknown, then release the actual nonempty count after the FIFO batch.
#[tokio::test]
async fn startup_barrier_blocks_empty_snapshot_before_channel_intent() {
    let (mut client, mut server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    state.connected = true;
    let channel = Uuid::new_v4();
    let mut snapshots = state.subscription_snapshot.subscribe();

    assert!(
        execute_connected_command(
            &mut client,
            &mut state,
            "agent",
            RelayCommand::SubscribeMembership,
        )
        .await
    );
    assert_eq!(super::tests::next_test_frame(&mut server).await[0], "REQ");
    assert!(
        !snapshots.borrow_and_update().confirmed,
        "membership readiness stays unknown until the startup batch marker"
    );

    assert!(
        execute_connected_command(
            &mut client,
            &mut state,
            "agent",
            RelayCommand::Subscribe {
                channel_id: channel,
                filter: ChannelFilter {
                    kinds: Some(vec![9]),
                    require_mention: true,
                },
                replay_since: Some(3000),
            },
        )
        .await
    );
    assert_eq!(
        super::tests::next_test_frame(&mut server).await[1],
        channel_sub_id(channel)
    );
    let before_marker = snapshots.borrow_and_update().clone();
    assert_eq!(before_marker.channels, HashSet::from([channel]));
    assert!(
        !before_marker.confirmed,
        "a channel REQ must remain unknown until the FIFO startup marker"
    );

    assert!(
        execute_connected_command(
            &mut client,
            &mut state,
            "agent",
            RelayCommand::StartupSubscriptionsReady,
        )
        .await
    );
    let after_marker = snapshots.borrow_and_update().clone();
    assert_eq!(after_marker.channels, HashSet::from([channel]));
    assert!(after_marker.confirmed);
}

/// A delayed rate-limited CLOSED for a channel already removed from intent may
/// arm the shared gate, but must not recreate per-channel retry metadata or
/// make the confirmed empty snapshot unknown.
#[tokio::test]
async fn late_rate_limited_closed_does_not_resurrect_removed_channel() {
    let (mut client, _server) = super::tests::test_ws_pair().await;
    let (event_tx, _event_rx) = mpsc::channel(4);
    let (observer_control_tx, _observer_control_rx) = mpsc::channel(4);
    let mut state = BgState::new();
    let keys = Keys::generate();
    let channel = Uuid::new_v4();
    let mut snapshots = state.subscription_snapshot.subscribe();

    state.connected = true;
    state.membership_sub_active = true;
    state.startup_subscriptions_ready = true;
    state
        .active_subscriptions
        .insert(channel, channel_sub_id(channel));
    state.publish_subscription_snapshot();
    assert!(snapshots.borrow_and_update().confirmed);

    // Use the production unsubscribe command so every channel-owned drain is
    // cleared before the stale CLOSED arrives.
    assert!(
        execute_connected_command(
            &mut client,
            &mut state,
            "agent",
            RelayCommand::Unsubscribe {
                channel_id: channel
            },
        )
        .await
    );
    assert!(state.active_subscriptions.is_empty());
    assert!(state.rate_limited_pending.is_empty());
    assert!(snapshots.borrow_and_update().confirmed);

    let closed = Message::Text(
        json!([
            "CLOSED",
            channel_sub_id(channel),
            "rate-limited: retry in 5s"
        ])
        .to_string()
        .into(),
    );
    assert!(
        handle_ws_message(
            closed,
            &mut client,
            &event_tx,
            &observer_control_tx,
            &mut state,
            &keys,
            "ws://fixture",
            "agent",
            None,
        )
        .await
    );
    assert!(
        state.rate_limit_gate.is_some(),
        "global gate remains enforced"
    );
    assert!(
        state.rate_limited_pending.is_empty(),
        "late CLOSED must not recreate removed-channel retry state"
    );
    let after = snapshots.borrow_and_update().clone();
    assert!(after.channels.is_empty());
    assert!(
        after.confirmed,
        "a stale per-channel CLOSED must not hide the confirmed empty intent"
    );
}

/// A failed command during the final reconnect drain must invalidate the
/// readiness snapshot for every reconnect entry point. The autonomous path
/// drains directly instead of calling `finish_reconnect`, so the guard belongs
/// to the shared production drain seam.
#[tokio::test]
async fn failed_reconnect_drain_write_clears_readiness() {
    let (mut client, _server) = super::tests::test_ws_pair().await;
    let (cmd_tx, mut cmd_rx) = mpsc::channel(4);
    let mut state = BgState::new();
    let channel = Uuid::new_v4();
    let mut snapshots = state.subscription_snapshot.subscribe();

    state.connected = true;
    state.membership_sub_active = true;
    state.startup_subscriptions_ready = true;
    state
        .active_subscriptions
        .insert(channel, channel_sub_id(channel));
    state.publish_subscription_snapshot();
    assert!(snapshots.borrow_and_update().confirmed);

    // Force the next control-plane write to fail before entering the shared
    // final-drain path.
    client.close(None).await.expect("close fixture websocket");
    cmd_tx
        .send(RelayCommand::SubscribeObserverControls)
        .await
        .expect("queue observer control command");

    let outcome =
        transport_reconnect::finish_reconnect(&mut client, &mut cmd_rx, &mut state, "agent").await;
    assert!(matches!(outcome, ReconnectOutcome::Failed));
    assert!(!state.connected);
    assert!(
        !snapshots.borrow_and_update().confirmed,
        "a failed final-drain write must not leave a stale confirmed snapshot"
    );
}

/// Exercise the actual membership CLOSED and retry command seams. The CLOSED
/// path parks the watch and publishes unknown; a successful control REQ clears
/// the retry state and publishes readiness again.
#[tokio::test]
async fn membership_watch_closed_and_retry_publish_readiness_transitions() {
    let (mut client, mut server) = super::tests::test_ws_pair().await;
    let (event_tx, mut event_rx) = mpsc::channel(4);
    let (observer_control_tx, _observer_control_rx) = mpsc::channel(4);
    let mut state = BgState::new();
    let keys = Keys::generate();
    let channel = Uuid::new_v4();
    let mut snapshots = state.subscription_snapshot.subscribe();

    state.connected = true;
    state.membership_sub_active = true;
    state.startup_subscriptions_ready = true;
    state
        .active_subscriptions
        .insert(channel, channel_sub_id(channel));
    state.publish_subscription_snapshot();
    assert!(snapshots.borrow_and_update().confirmed);

    let closed = Message::Text(
        json!([
            "CLOSED",
            MEMBERSHIP_NOTIF_SUB_ID,
            "rate-limited: retry in 5s"
        ])
        .to_string()
        .into(),
    );
    assert!(
        handle_ws_message(
            closed,
            &mut client,
            &event_tx,
            &observer_control_tx,
            &mut state,
            &keys,
            "ws://fixture",
            "agent",
            None,
        )
        .await
    );
    assert!(state.membership_resub_needed);
    assert!(
        !snapshots.borrow_and_update().confirmed,
        "rate-limited membership watch must publish unknown"
    );
    assert!(matches!(
        event_rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));

    // The test controls the existing retry gate to drive the production retry
    // command path without a wall-clock sleep.
    state.rate_limit_gate = None;
    assert!(
        execute_connected_command(
            &mut client,
            &mut state,
            "agent",
            RelayCommand::SubscribeMembership,
        )
        .await
    );
    let frame = super::tests::next_test_frame(&mut server).await;
    assert_eq!(frame[0], "REQ");
    assert_eq!(frame[1], MEMBERSHIP_NOTIF_SUB_ID);
    assert!(!state.membership_resub_needed);
    assert!(
        snapshots.borrow_and_update().confirmed,
        "successful membership retry must restore readiness"
    );
}
