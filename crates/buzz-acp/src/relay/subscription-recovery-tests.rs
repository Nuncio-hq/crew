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
    assert_eq!(snapshots.borrow_and_update().len(), 2);
    assert!(drop_channel_on_access_denied(
        &mut state,
        &channel_sub_id(channel),
        "restricted: channel access revoked"
    ));
    // Read current authority even before changed() is polled by the main loop.
    assert!(
        !snapshots.borrow().contains(&channel),
        "a later membership add must not skip the dropped channel"
    );
    assert!(snapshots.borrow().contains(&unrelated));
    assert!(snapshots.has_changed().unwrap());
    subscribe(&mut state, channel);
    assert_eq!(snapshots.borrow().len(), 2);
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
        snapshots.borrow_and_update().is_empty(),
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
    assert_eq!(snapshots.borrow_and_update().len(), 2);
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
    assert_eq!(snapshots.borrow_and_update().len(), 1);
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
    assert!(!snapshots.borrow().contains(&channel));
    assert!(
        execute_connected_command(&mut client, &mut state, "agent", subscribe_command(channel))
            .await
    );
    let restored = super::tests::next_test_frame(&mut server).await;
    assert_eq!(restored[1], channel_sub_id(channel));
    assert_eq!(restored[2]["#h"], json!([channel.to_string()]));
    assert_eq!(restored[2]["#p"], json!(["agent"]));
    assert_eq!(snapshots.borrow().len(), 2);
}
