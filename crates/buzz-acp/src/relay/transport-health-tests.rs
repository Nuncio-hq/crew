use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test(start_paused = true)]
async fn initial_burst_caps_total_elapsed_time_including_connect() {
    let attempts = AtomicUsize::new(0);
    let started = tokio::time::Instant::now();
    let result: Result<(), RelayError> = retry_initial_connect(|| {
        attempts.fetch_add(1, Ordering::SeqCst);
        async {
            tokio::time::sleep(Duration::from_secs(65)).await;
            Err(RelayError::ConnectionClosed)
        }
    })
    .await;

    assert!(result.is_err());
    assert!(
        started.elapsed() <= Duration::from_secs(300),
        "the burst deadline must include in-flight connection time; elapsed {:?}",
        started.elapsed()
    );
    assert!(attempts.load(Ordering::SeqCst) <= 6);
}

#[tokio::test(start_paused = true)]
async fn initial_burst_deadline_cancels_a_pending_connect() {
    let result = tokio::time::timeout(
        Duration::from_secs(301),
        retry_initial_connect(std::future::pending::<Result<(), RelayError>>),
    )
    .await;

    assert!(
        matches!(result, Ok(Err(_))),
        "production retry must end its pending attempt at the 300-second deadline"
    );
}

/// Drive the production autonomous-to-wait transition with a connector that
/// fails deterministically; the original WebSocket and command queues are real.
async fn failed_reconnect_attempt_times(
    error: fn() -> RelayError,
    window: Duration,
) -> Vec<Duration> {
    failed_reconnect_attempt_times_mode(error, window, true).await
}

async fn failed_reconnect_attempt_times_mode(
    error: fn() -> RelayError,
    window: Duration,
    managed: bool,
) -> Vec<Duration> {
    use std::cell::RefCell;
    let (mut ws, _server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    state.health = if managed {
        TransportHealth::managed()
    } else {
        TransportHealth::default()
    };
    let keys = Keys::generate();
    let (_cmd_tx, mut cmd_rx) = mpsc::channel(8);
    let (event_tx, _event_rx) = mpsc::channel(8);
    let (observer_tx, _observer_rx) = mpsc::channel(8);
    let started = tokio::time::Instant::now();
    let times = RefCell::new(Vec::new());
    let connect = || {
        times.borrow_mut().push(started.elapsed());
        std::future::ready(Err(error()))
    };
    let run = async {
        let outcome = transport_reconnect::try_autonomous_reconnect_with(
            &mut ws,
            &mut cmd_rx,
            &mut state,
            &keys,
            "ws://fixture",
            "agent",
            &event_tx,
            &observer_tx,
            None,
            connect,
        )
        .await;
        assert!(matches!(outcome, ReconnectOutcome::Failed));
        transport_reconnect::wait_for_reconnect_with(
            &mut ws,
            &mut cmd_rx,
            &mut state,
            &keys,
            "ws://fixture",
            "agent",
            &event_tx,
            &observer_tx,
            true,
            None,
            connect,
        )
        .await
    };
    assert!(tokio::time::timeout(window, run).await.is_err());
    times.into_inner()
}

#[tokio::test(start_paused = true)]
async fn autonomous_and_wait_share_six_attempt_burst() {
    let times =
        failed_reconnect_attempt_times(|| RelayError::ConnectionClosed, Duration::from_secs(200))
            .await;
    assert_eq!(
        times.len(),
        6,
        "handoff must not replenish the burst: {times:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn dns_failures_consume_the_same_six_attempt_burst() {
    let times = failed_reconnect_attempt_times(
        || RelayError::Http("failed to lookup address".into()),
        Duration::from_secs(200),
    )
    .await;
    assert_eq!(
        times.len(),
        6,
        "DNS must not evade the health budget: {times:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn terminal_auth_denial_enters_slow_probe_immediately() {
    let times = failed_reconnect_attempt_times(
        || RelayError::AuthDenied("restricted: denied".into()),
        Duration::from_secs(200),
    )
    .await;
    assert_eq!(times.len(), 1, "terminal denial ends the burst: {times:?}");
}

#[tokio::test(start_paused = true)]
async fn slow_probe_failures_never_reopen_a_burst() {
    let times =
        failed_reconnect_attempt_times(|| RelayError::ConnectionClosed, Duration::from_secs(1100))
            .await;
    assert!(
        times.len() >= 9,
        "expired burst must not prevent probes: {times:?}"
    );
    for pair in times[5..].windows(2) {
        let delay = pair[1] - pair[0];
        assert!(
            (Duration::from_secs(270)..=Duration::from_secs(330)).contains(&delay),
            "each failed probe schedules one probe after 270–330s: {times:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn last_allowed_recovery_resets_health_but_not_stability_ladder() {
    let (mut ws, _server) = super::tests::test_ws_pair().await;
    let (recovered_ws, _recovered_server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    state.backoff_step = 4;
    let keys = Keys::generate();
    let (_cmd_tx, mut cmd_rx) = mpsc::channel(8);
    let (event_tx, _event_rx) = mpsc::channel(8);
    let (observer_tx, _observer_rx) = mpsc::channel(8);
    let failed = transport_reconnect::try_autonomous_reconnect_with(
        &mut ws,
        &mut cmd_rx,
        &mut state,
        &keys,
        "ws://fixture",
        "agent",
        &event_tx,
        &observer_tx,
        None,
        || std::future::ready(Err(RelayError::ConnectionClosed)),
    )
    .await;
    assert!(matches!(failed, ReconnectOutcome::Failed));
    assert_eq!(state.health.attempts, 5);
    let mut recovery = Some(recovered_ws);
    let recovered = transport_reconnect::wait_for_reconnect_with(
        &mut ws,
        &mut cmd_rx,
        &mut state,
        &keys,
        "ws://fixture",
        "agent",
        &event_tx,
        &observer_tx,
        true,
        None,
        || {
            std::future::ready(Ok((
                recovery.take().expect("only one recovery attempt"),
                VecDeque::new(),
            )))
        },
    )
    .await;
    assert!(matches!(recovered, ReconnectOutcome::Ok));
    assert_eq!(state.health.phase, transport_health::Phase::Healthy);
    assert_eq!(state.health.attempts, 0);
    assert_eq!(
        state.backoff_step, 4,
        "health reset must not erase stability policy"
    );
}

#[tokio::test(start_paused = true)]
async fn authenticated_socket_with_buffered_drop_does_not_reset_health() {
    let (mut ws, _server) = super::tests::test_ws_pair().await;
    let mut candidates = VecDeque::new();
    let mut servers = Vec::new();
    for _ in 0..6 {
        let (client, server) = super::tests::test_ws_pair().await;
        candidates.push_back(client);
        servers.push(server);
    }
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    let keys = Keys::generate();
    let (_cmd_tx, mut cmd_rx) = mpsc::channel(8);
    let (event_tx, _event_rx) = mpsc::channel(8);
    let (observer_tx, _observer_rx) = mpsc::channel(8);
    let mut connect = || {
        std::future::ready(Ok((
            candidates
                .pop_front()
                .expect("burst must not exceed six attempts"),
            VecDeque::from([RelayMessage::Ok {
                event_id: "unrelated".into(),
                accepted: false,
                message: "auth-required: buffered connection drop".into(),
            }]),
        )))
    };
    let failed = transport_reconnect::try_autonomous_reconnect_with(
        &mut ws,
        &mut cmd_rx,
        &mut state,
        &keys,
        "ws://fixture",
        "agent",
        &event_tx,
        &observer_tx,
        None,
        &mut connect,
    )
    .await;
    assert!(matches!(failed, ReconnectOutcome::Failed));
    assert_eq!(state.health.attempts, 5);
    assert!(tokio::time::timeout(
        Duration::from_secs(100),
        transport_reconnect::wait_for_reconnect_with(
            &mut ws,
            &mut cmd_rx,
            &mut state,
            &keys,
            "ws://fixture",
            "agent",
            &event_tx,
            &observer_tx,
            true,
            None,
            &mut connect,
        ),
    )
    .await
    .is_err());
    assert!(state.health.slow());
    assert_eq!(state.health.attempts, 6);
    assert!(
        !state.health.auth_rejected,
        "buffered unrelated OK is not credential evidence"
    );
}

#[tokio::test(start_paused = true)]
async fn slow_probe_sleep_processes_intent_and_honors_shutdown() {
    let (mut ws, _server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    let keys = Keys::generate();
    let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
    let (event_tx, _event_rx) = mpsc::channel(8);
    let (observer_tx, _observer_rx) = mpsc::channel(8);
    let failed = transport_reconnect::try_autonomous_reconnect_with(
        &mut ws,
        &mut cmd_rx,
        &mut state,
        &keys,
        "ws://fixture",
        "agent",
        &event_tx,
        &observer_tx,
        None,
        || std::future::ready(Err(RelayError::AuthDenied("restricted: denied".into()))),
    )
    .await;
    assert!(matches!(failed, ReconnectOutcome::Failed));
    cmd_tx
        .send(RelayCommand::SubscribeMembership)
        .await
        .unwrap();
    cmd_tx.send(RelayCommand::Shutdown).await.unwrap();
    let result = transport_reconnect::wait_for_reconnect_with(
        &mut ws,
        &mut cmd_rx,
        &mut state,
        &keys,
        "ws://fixture",
        "agent",
        &event_tx,
        &observer_tx,
        true,
        None,
        || async { panic!("shutdown must not wait for or start a probe") },
    )
    .await;
    assert!(matches!(result, ReconnectOutcome::Shutdown));
    assert!(state.membership_sub_active);
}

// Drive the same startup engine with the managed mode explicitly selected.
async fn retry_initial_connect<F, Fut, T>(op: F) -> Result<T, RelayError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, RelayError>>,
{
    transport_reconnect::retry_initial_connect_with_health(&mut TransportHealth::managed(), op)
        .await
}

#[tokio::test(start_paused = true)]
async fn standalone_startup_preserves_legacy_unbounded_attempt_duration() {
    let started = tokio::time::Instant::now();
    let attempts = AtomicUsize::new(0);
    let result: Result<(), RelayError> = transport_reconnect::retry_initial_connect(|| {
        attempts.fetch_add(1, Ordering::SeqCst);
        async {
            tokio::time::sleep(Duration::from_secs(65)).await;
            Err(RelayError::ConnectionClosed)
        }
    })
    .await;
    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 6);
    assert!(started.elapsed() > Duration::from_secs(390));
}

#[tokio::test(start_paused = true)]
async fn dependency_auth_error_consumes_burst_without_credential_rejection() {
    let times = failed_reconnect_attempt_times(
        || RelayError::AuthDenied("error: dependency unavailable".into()),
        Duration::from_secs(200),
    )
    .await;
    assert_eq!(
        times.len(),
        6,
        "dependency denial remains retryable: {times:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn standalone_reconnect_keeps_retrying_after_managed_burst_limit() {
    let times = failed_reconnect_attempt_times_mode(
        || RelayError::ConnectionClosed,
        Duration::from_secs(250),
        false,
    )
    .await;
    assert!(
        times.len() > 10,
        "standalone must retain legacy wait retries: {times:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn standalone_dns_preserves_flat_retries_beyond_managed_limit() {
    let times = failed_reconnect_attempt_times_mode(
        || RelayError::Http("failed to lookup address".into()),
        Duration::from_secs(100),
        false,
    )
    .await;
    assert!(
        times.len() > 20,
        "legacy DNS retry behavior must remain: {times:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn local_signing_failure_is_not_verified_server_auth_rejection() {
    let mut health = TransportHealth::managed();
    let result: Result<(), RelayError> = health
        .connect(|| async { Err(RelayError::from(nostr::event::builder::Error::EmptyTags)) })
        .await;
    assert!(result.is_err());
    assert!(
        !health.auth_rejected,
        "only a correlated server AUTH denial is credential evidence"
    );
}

#[tokio::test(start_paused = true)]
async fn burst_deadline_bounds_resubscription_and_preserves_deferred_intent() {
    let (mut ws, _server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    state
        .health
        .connect(|| async { Ok::<_, RelayError>(()) })
        .await
        .unwrap();
    let channel = Uuid::new_v4();
    apply_command_to_state(
        &mut state,
        RelayCommand::Subscribe {
            channel_id: channel,
            filter: ChannelFilter {
                kinds: Some(vec![9]),
                require_mention: true,
            },
            replay_since: Some(1000),
        },
    );
    let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
    cmd_tx
        .send(RelayCommand::SubscribeMembership)
        .await
        .unwrap();
    tokio::time::advance(Duration::from_millis(299_950)).await;
    let started = tokio::time::Instant::now();
    let result = resubscribe_after_reconnect(&mut ws, &mut cmd_rx, &mut state, "agent", true).await;
    assert!(
        matches!(result, ResubscribeResult::RetryConnection),
        "replay must not declare success beyond the episode deadline"
    );
    assert!(started.elapsed() <= Duration::from_millis(50));
    assert!(
        state.membership_sub_active,
        "cancelled pacing must retain the queued membership command"
    );
    assert!(state.active_subscriptions.contains_key(&channel));
}

#[tokio::test(start_paused = true)]
async fn deadline_during_observer_send_retains_the_exact_event_for_ack_recovery() {
    let (mut ws, _unread_server) = super::tests::test_ws_pair().await;
    let keys = Keys::generate();
    let event = nostr::EventBuilder::new(nostr::Kind::Custom(24200), "x".repeat(8 * 1024 * 1024))
        .sign_with_keys(&keys)
        .unwrap();
    let event_id = event.id;
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    state
        .health
        .connect(|| async { Ok::<_, RelayError>(()) })
        .await
        .unwrap();
    let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
    cmd_tx
        .send(RelayCommand::PublishEvent {
            event: Box::new(event),
        })
        .await
        .unwrap();
    tokio::time::advance(Duration::from_millis(299_950)).await;
    assert!(matches!(
        resubscribe_after_reconnect(&mut ws, &mut cmd_rx, &mut state, "agent", true).await,
        ResubscribeResult::RetryConnection
    ));
    assert!(
        state
            .gated_observer_pending
            .iter()
            .any(|event| event.id == event_id),
        "a timed-out partial send must retain the same signed event"
    );
    drop(ws);
    tokio::time::resume();
    let (mut restored, mut server) = super::tests::test_ws_pair().await;
    let read = tokio::spawn(async move { super::tests::next_test_frame(&mut server).await });
    assert_eq!(
        drain_gated_observer_pending(&mut restored, &mut state, 1).await,
        1
    );
    assert_eq!(read.await.unwrap()[1]["id"], event_id.to_hex());
    assert!(state
        .observer_in_flight
        .iter()
        .any(|event| event.id == event_id));
    state.acknowledge_observer_frame(&event_id.to_hex());
    assert!(!state
        .observer_in_flight
        .iter()
        .any(|event| event.id == event_id));
}

#[tokio::test(start_paused = true)]
async fn final_reconnect_drain_cannot_outlive_deadline_or_lose_inflight_observer() {
    let (mut ws, _unread_server) = super::tests::test_ws_pair().await;
    let keys = Keys::generate();
    let event = nostr::EventBuilder::new(nostr::Kind::Custom(24200), "x".repeat(8 * 1024 * 1024))
        .sign_with_keys(&keys)
        .unwrap();
    let event_id = event.id;
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    state
        .health
        .connect(|| async { Ok::<_, RelayError>(()) })
        .await
        .unwrap();
    let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
    assert!(matches!(
        resubscribe_after_reconnect(&mut ws, &mut cmd_rx, &mut state, "agent", true).await,
        ResubscribeResult::Ok
    ));
    tokio::time::advance(Duration::from_millis(299_950)).await;
    cmd_tx
        .send(RelayCommand::PublishEvent {
            event: Box::new(event),
        })
        .await
        .unwrap();
    let started = tokio::time::Instant::now();
    let outcome =
        transport_reconnect::finish_reconnect(&mut ws, &mut cmd_rx, &mut state, "agent").await;
    assert!(
        matches!(outcome, ReconnectOutcome::Ok),
        "a deadline-interrupted live drain must recover and defer its intent"
    );
    assert!(started.elapsed() <= Duration::from_millis(50));
    assert!(
        !state.health.slow(),
        "an expired final drain on a live socket must not poison health"
    );
    assert!(state
        .gated_observer_pending
        .iter()
        .any(|event| event.id == event_id));
}

#[tokio::test]
async fn interrupted_subscription_command_is_parked_for_live_retry() {
    let (mut client, mut server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    let channel_id = Uuid::new_v4();
    retain_failed_command_intent(
        &mut state,
        RelayCommand::Subscribe {
            channel_id,
            filter: ChannelFilter {
                kinds: Some(vec![9]),
                require_mention: false,
            },
            replay_since: Some(1_000),
        },
    );

    assert!(state.active_subscriptions.contains_key(&channel_id));
    assert!(
        state.resubscribe_retry.contains(&channel_id),
        "an interrupted subscribe must be delivered by the bounded live retry drain"
    );
    assert_eq!(
        drain_resubscribe_retry(&mut client, &mut state, "agent", 1).await,
        1
    );
    let frame = super::tests::next_test_frame(&mut server).await;
    assert_eq!(frame[0], "REQ");
    assert_eq!(frame[1], channel_sub_id(channel_id));
}

#[tokio::test(start_paused = true)]
async fn slow_probe_replay_has_a_fresh_bounded_recovery_window() {
    let (mut ws, _server) = super::tests::test_ws_pair().await;
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    let _: Result<(), _> = state
        .health
        .connect(|| async { Err::<(), _>(RelayError::AuthDenied("restricted: fixture".into())) })
        .await;
    assert!(state.health.slow());
    tokio::time::advance(Duration::from_secs(330)).await;
    state
        .health
        .connect(|| async { Ok::<_, RelayError>(()) })
        .await
        .unwrap();
    let channel = Uuid::new_v4();
    apply_command_to_state(
        &mut state,
        RelayCommand::Subscribe {
            channel_id: channel,
            filter: ChannelFilter {
                kinds: Some(vec![9]),
                require_mention: true,
            },
            replay_since: Some(1000),
        },
    );
    let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
    cmd_tx
        .send(RelayCommand::SubscribeMembership)
        .await
        .unwrap();
    tokio::time::advance(Duration::from_millis(299_950)).await;
    let started = tokio::time::Instant::now();
    assert!(matches!(
        resubscribe_after_reconnect(&mut ws, &mut cmd_rx, &mut state, "agent", true).await,
        ResubscribeResult::RetryConnection
    ));
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(50),
        "must use the fresh probe deadline, never the expired burst deadline"
    );
    assert!(state.membership_sub_active);
}

#[tokio::test(start_paused = true)]
async fn slow_probe_connect_is_bounded_by_its_own_attempt_window() {
    let mut health = TransportHealth::managed();
    let _: Result<(), _> = health
        .connect(|| async { Err::<(), _>(RelayError::AuthDenied("restricted: fixture".into())) })
        .await;
    tokio::time::advance(Duration::from_secs(330)).await;
    let result = tokio::time::timeout(
        Duration::from_secs(301),
        health.connect(std::future::pending::<Result<(), RelayError>>),
    )
    .await;
    assert!(
        matches!(result, Ok(Err(RelayError::Timeout))),
        "the slow probe itself cannot remain pending forever"
    );
}

#[tokio::test(start_paused = true)]
async fn shutdown_remains_terminal_when_recovery_deadline_interrupts_close() {
    let (mut ws, _unread_server) = super::tests::test_ws_pair().await;
    let event = nostr::EventBuilder::new(nostr::Kind::Custom(24200), "x".repeat(8 * 1024 * 1024))
        .sign_with_keys(&Keys::generate())
        .unwrap();
    let mut state = BgState::new();
    state.health = TransportHealth::managed();
    state
        .health
        .connect(|| async { Ok::<_, RelayError>(()) })
        .await
        .unwrap();
    let (cmd_tx, mut cmd_rx) = mpsc::channel(8);
    cmd_tx
        .send(RelayCommand::PublishEvent {
            event: Box::new(event),
        })
        .await
        .unwrap();
    tokio::time::advance(Duration::from_millis(299_950)).await;
    assert!(matches!(
        resubscribe_after_reconnect(&mut ws, &mut cmd_rx, &mut state, "agent", true).await,
        ResubscribeResult::RetryConnection
    ));
    cmd_tx.send(RelayCommand::Shutdown).await.unwrap();
    let started = tokio::time::Instant::now();
    assert!(matches!(
        transport_reconnect::finish_reconnect(&mut ws, &mut cmd_rx, &mut state, "agent").await,
        ReconnectOutcome::Shutdown
    ));
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert_ne!(state.health.phase, super::transport_health::Phase::Healthy);
}
