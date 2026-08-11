export const RECONNECT_BASE_DELAY_MS = 1_000;
export const RECONNECT_MAX_DELAY_MS = 30_000;
export const EVENT_BATCH_MS = 16;

/**
 * Op-level timeouts tolerate degraded networks where TLS handshakes and DNS
 * resolution can take several seconds.
 */
export const AUTH_TIMEOUT_MS = 25_000;
export const HISTORY_TIMEOUT_MS = 25_000;
export const LIVE_SUBSCRIPTION_READY_TIMEOUT_MS = HISTORY_TIMEOUT_MS;
export const PUBLISH_TIMEOUT_MS = 25_000;

/**
 * A stability-gated reset prevents reconnect flapping from erasing backoff.
 */
export const BACKOFF_RESET_STABLE_MS = 60_000;

/**
 * How far before "now" a channel's live subscription starts (seconds).
 *
 * Sized to cover the two gaps a `since: now` bound leaks — the window fetch to
 * subscription-open handoff, and a publishing peer whose clock lags ours — while
 * staying short enough that the replay is a handful of recent events. Consumers
 * merge by event id, so the overlap is idempotent.
 */
export const CHANNEL_LIVE_BACKLOG_GRACE_SECONDS = 120;

/** Passive liveness thresholds for the relay heartbeat stream. */
export const STALL_CHECK_INTERVAL_MS = 10_000;
export const STALL_IDLE_TIMEOUT_MS = 60_000;
