use super::*;

/// Attempt autonomous reconnect on socket loss.
///
/// Returns [`ReconnectOutcome::Ok`] on success, [`ReconnectOutcome::Failed`]
/// if all attempts are exhausted, or [`ReconnectOutcome::Shutdown`] if a
/// Shutdown command was received during backoff sleep. Callers MUST check
/// for `Shutdown` and return immediately — do NOT fall through to
/// `wait_for_reconnect`, which would loop forever since the Shutdown command
/// was already consumed.
#[allow(clippy::too_many_arguments)]
pub(super) async fn try_autonomous_reconnect(
    ws: &mut WsStream,
    cmd_rx: &mut mpsc::Receiver<RelayCommand>,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    agent_pubkey_hex: &str,
    event_tx: &mpsc::Sender<Option<BuzzEvent>>,
    observer_control_tx: &mpsc::Sender<Event>,
    auth_tag: Option<&nostr::Tag>,
) -> ReconnectOutcome {
    try_autonomous_reconnect_with(
        ws,
        cmd_rx,
        state,
        keys,
        relay_url,
        agent_pubkey_hex,
        event_tx,
        observer_control_tx,
        auth_tag,
        || do_connect(relay_url, keys, auth_tag),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn try_autonomous_reconnect_with<F, Fut>(
    ws: &mut WsStream,
    cmd_rx: &mut mpsc::Receiver<RelayCommand>,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    agent_pubkey_hex: &str,
    event_tx: &mpsc::Sender<Option<BuzzEvent>>,
    observer_control_tx: &mpsc::Sender<Event>,
    auth_tag: Option<&nostr::Tag>,
    mut connect: F,
) -> ReconnectOutcome
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(WsStream, VecDeque<RelayMessage>), RelayError>>,
{
    state.requeue_observer_in_flight();
    let backoffs = STARTUP_CONNECT_BACKOFFS;
    let mut legacy_dns_retries = 0;
    let mut attempt = 0usize;
    while attempt < backoffs.len() {
        if state.health.slow() {
            return ReconnectOutcome::Failed;
        }
        if !wait_health(cmd_rx, state).await {
            return ReconnectOutcome::Shutdown;
        }
        info!(
            "autonomous reconnect attempt {}/{} to {relay_url}…",
            attempt + 1,
            backoffs.len()
        );
        match state.health.connect(&mut connect).await {
            Ok((new_ws, handshake_buffer)) => {
                *ws = new_ws;
                state.connection_generation = state.connection_generation.saturating_add(1);
                info!("autonomous reconnect succeeded (attempt {})", attempt + 1);
                let handshake_ok = process_handshake_buffer(
                    ws,
                    handshake_buffer,
                    event_tx,
                    observer_control_tx,
                    state,
                    keys,
                    relay_url,
                    agent_pubkey_hex,
                    auth_tag,
                )
                .await;
                if !handshake_ok {
                    warn!(
                        "handshake buffer drop signal after autonomous reconnect (attempt {})",
                        attempt + 1
                    );
                    state.health.failed(&RelayError::ConnectionClosed);
                    // Fall through to backoff sleep instead of returning immediately.
                    // Returning false here would skip remaining attempts; continuing
                    // without sleep would drive a tight reconnect storm.
                } else {
                    match resubscribe_after_reconnect(ws, cmd_rx, state, agent_pubkey_hex, true)
                        .await
                    {
                        ResubscribeResult::Ok => {
                            state.health.recovered().await;
                            return ReconnectOutcome::Ok;
                        }
                        ResubscribeResult::Shutdown => return ReconnectOutcome::Shutdown,
                        ResubscribeResult::RetryConnection => {
                            state.health.failed(&RelayError::ConnectionClosed);
                            warn!("resubscribe failed after autonomous reconnect — treating as failed attempt");
                            // Fall through to backoff sleep and retry.
                        }
                    }
                }
            }
            Err(e) if is_dns_error(&e) && (state.health.enabled() || legacy_dns_retries < 10) => {
                legacy_dns_retries += 1;
                state.health.defer(jittered_duration(DNS_RETRY_INTERVAL));
                continue;
            }
            Err(e) => {
                warn!("autonomous reconnect attempt {} failed: {e}", attempt + 1);
            }
        }

        // The handoff retains the next delay as well as the attempt count.
        // Waiting happens at the next entry, including autonomous → wait.
        if state.health.enabled() || attempt + 1 < backoffs.len() {
            state.health.defer(jittered_duration(backoffs[attempt]));
        } else {
            state.health.ready_now();
        }
        attempt = attempt.saturating_add(1);
    }

    ReconnectOutcome::Failed
}

/// Attempt reconnection with exponential backoff. Resubscribes all active
/// channels with `since` filters on success.
///
/// If `skip_drain` is `false`, drains the command channel until a `Reconnect`
/// command arrives (used when called from the WS-error path where the caller
/// hasn't sent Reconnect yet). If `true`, skips the drain and reconnects
/// immediately (used when called from the `RelayCommand::Reconnect` arm where
/// the command was already consumed).
#[allow(clippy::too_many_arguments)]
pub(super) async fn wait_for_reconnect(
    ws: &mut WsStream,
    cmd_rx: &mut mpsc::Receiver<RelayCommand>,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    agent_pubkey_hex: &str,
    event_tx: &mpsc::Sender<Option<BuzzEvent>>,
    observer_control_tx: &mpsc::Sender<Event>,
    skip_drain: bool,
    auth_tag: Option<&nostr::Tag>,
) -> ReconnectOutcome {
    wait_for_reconnect_with(
        ws,
        cmd_rx,
        state,
        keys,
        relay_url,
        agent_pubkey_hex,
        event_tx,
        observer_control_tx,
        skip_drain,
        auth_tag,
        || do_connect(relay_url, keys, auth_tag),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn wait_for_reconnect_with<F, Fut>(
    ws: &mut WsStream,
    cmd_rx: &mut mpsc::Receiver<RelayCommand>,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    agent_pubkey_hex: &str,
    event_tx: &mpsc::Sender<Option<BuzzEvent>>,
    observer_control_tx: &mpsc::Sender<Event>,
    skip_drain: bool,
    auth_tag: Option<&nostr::Tag>,
    mut connect: F,
) -> ReconnectOutcome
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(WsStream, VecDeque<RelayMessage>), RelayError>>,
{
    state.requeue_observer_in_flight();
    if !skip_drain {
        // Drain commands until we get Reconnect (or Shutdown).
        // Other commands update state so reconnect reflects latest intent.
        loop {
            match cmd_rx.recv().await {
                Some(RelayCommand::Reconnect) => break,
                Some(RelayCommand::Shutdown) | None => return ReconnectOutcome::Shutdown,
                Some(cmd) => apply_command_to_state(state, cmd),
            }
        }
    }

    // Keep Buzz's stability ladder separate from the shared health episode.
    let backoffs = [
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(4),
        Duration::from_secs(8),
        Duration::from_secs(16),
        Duration::from_secs(32),
    ];
    let mut attempt = state.backoff_step;
    loop {
        if !wait_health(cmd_rx, state).await {
            return ReconnectOutcome::Shutdown;
        }
        info!("attempting relay reconnect to {relay_url}…");
        match state.health.connect(&mut connect).await {
            Ok((new_ws, handshake_buffer)) => {
                *ws = new_ws;
                state.connection_generation = state.connection_generation.saturating_add(1);
                info!("relay reconnected to {relay_url}");
                let handshake_ok = process_handshake_buffer(
                    ws,
                    handshake_buffer,
                    event_tx,
                    observer_control_tx,
                    state,
                    keys,
                    relay_url,
                    agent_pubkey_hex,
                    auth_tag,
                )
                .await;
                if !handshake_ok {
                    state.health.failed(&RelayError::ConnectionClosed);
                    warn!("handshake buffer contained a drop signal after reconnect — will retry with backoff");
                    // Fall through to the backoff sleep below instead of
                    // tight-looping. A relay that consistently fails the
                    // handshake would otherwise drive a reconnect storm.
                } else {
                    match resubscribe_after_reconnect(ws, cmd_rx, state, agent_pubkey_hex, true)
                        .await
                    {
                        ResubscribeResult::Ok => {
                            return finish_reconnect(ws, cmd_rx, state, agent_pubkey_hex).await;
                        }
                        ResubscribeResult::Shutdown => return ReconnectOutcome::Shutdown,
                        ResubscribeResult::RetryConnection => {
                            state.health.failed(&RelayError::ConnectionClosed);
                            warn!("resubscribe failed after reconnect — will retry with backoff");
                            // Fall through to backoff sleep.
                        }
                    }
                }
            }
            Err(e) if is_dns_error(&e) => {
                state.health.defer(jittered_duration(DNS_RETRY_INTERVAL));
                continue;
            }
            Err(e) => {
                warn!("relay reconnect failed: {e}");
            }
        }

        // Persist ladder position before sleeping — if shutdown arrives mid-sleep,
        // the next session resumes from here rather than restarting at 0.
        state.backoff_step = attempt;

        let delay = if attempt < backoffs.len() {
            backoffs[attempt]
        } else {
            Duration::from_secs(60)
        };
        state.health.defer(jittered_duration(delay));
        attempt = attempt.saturating_add(1);
    }
}

/// Startup uses the same episode as post-start reconnect, but returns the
/// final failure instead of inventing a running process before startup succeeds.
pub(super) async fn retry_initial_connect_with_health<F, Fut, T>(
    health: &mut TransportHealth,
    mut op: F,
) -> Result<T, RelayError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, RelayError>>,
{
    let mut last_error = RelayError::ConnectionClosed;
    for delay in std::iter::once(None).chain(STARTUP_CONNECT_BACKOFFS.iter().copied().map(Some)) {
        if health.slow() {
            break;
        }
        if let Some(delay) = delay {
            health.defer(jittered_duration(delay));
            tokio::time::sleep_until(health.ready_at()).await;
        }
        match health.connect(&mut op).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                let terminal = is_terminal_connect_error(&error);
                last_error = error;
                if terminal {
                    break;
                }
            }
        }
    }
    health.report(true).await?;
    Err(last_error)
}

#[cfg(test)]
pub(super) async fn retry_initial_connect<F, Fut, T>(op: F) -> Result<T, RelayError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, RelayError>>,
{
    retry_initial_connect_with_health(&mut TransportHealth::default(), op).await
}

/// Commands update retained intent without collapsing the retry deadline.
async fn wait_health(cmd_rx: &mut mpsc::Receiver<RelayCommand>, state: &mut BgState) -> bool {
    state.health.report_or_warn().await;
    let sleep = tokio::time::sleep_until(state.health.ready_at());
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut sleep => return true,
            cmd = cmd_rx.recv() => match cmd {
                Some(RelayCommand::Shutdown) | None => return false,
                Some(cmd) => apply_command_to_state(state, cmd),
            }
        }
    }
}

/// Final live-command drain is still part of connection recovery.
pub(super) async fn finish_reconnect(
    ws: &mut WsStream,
    cmd_rx: &mut mpsc::Receiver<RelayCommand>,
    state: &mut BgState,
    agent_pubkey_hex: &str,
) -> ReconnectOutcome {
    let outcome = drain_post_reconnect(ws, cmd_rx, state, agent_pubkey_hex).await;
    if matches!(outcome, ReconnectOutcome::Ok) {
        state.health.recovered().await;
    } else if matches!(outcome, ReconnectOutcome::Failed) {
        state.health.failed(&RelayError::ConnectionClosed);
    }
    outcome
}
