//! Managed transport health is independent of the socket's backoff ladder.
use super::{is_terminal_connect_error, RelayError};
use buzz_core::transport_status::{TransportCode, TransportState, TransportStatus};
use std::{future::Future, time::Duration};
use tokio::time::Instant;

const BURST_LIMIT: u32 = 6;
const BURST_DURATION: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Healthy,
    Burst,
    SlowProbe,
}

/// One health episode survives transitions between reconnect entry points.
#[derive(Debug)]
pub(super) struct TransportHealth {
    enabled: bool,
    ever_connected: bool,
    last_code: TransportCode,
    reporter: Option<super::transport_status::StatusReporter>,
    pub phase: Phase,
    pub attempts: u32,
    pub auth_rejected: bool,
    started: Instant,
    next_at: Instant,
    probe_deadline: Option<Instant>,
    probe_delay: fn() -> Duration,
}

impl Default for TransportHealth {
    fn default() -> Self {
        let mut health = Self::managed();
        health.enabled = false;
        health
    }
}

impl TransportHealth {
    pub fn with_reporter(reporter: super::transport_status::StatusReporter) -> Self {
        let mut health = Self::managed();
        health.reporter = Some(reporter);
        health
    }

    pub async fn from_environment(pubkey: &str, relay_url: &str) -> Result<Self, RelayError> {
        let config = super::transport_status::StatusConfig::parse(
            std::env::var_os("CREW_ACP_TRANSPORT_STATUS_PATH"),
            std::env::var_os("CREW_ACP_TRANSPORT_START_NONCE"),
        )
        .map_err(RelayError::TransportStatus)?;
        let Some(config) = config else {
            return Ok(Self::default());
        };
        let writer = super::transport_status::StatusWriter::new(config, pubkey, relay_url)
            .await
            .map_err(RelayError::TransportStatus)?;
        let health = Self::with_reporter(super::transport_status::StatusReporter::new(writer));
        health.report(false).await?;
        Ok(health)
    }

    pub async fn report(&self, terminal: bool) -> Result<(), RelayError> {
        let Some(reporter) = &self.reporter else {
            return Ok(());
        };
        let state = match self.phase {
            Phase::Healthy if self.ever_connected => TransportState::Connected,
            Phase::Healthy => TransportState::Connecting,
            Phase::Burst if terminal => TransportState::Exhausted,
            Phase::Burst if self.ever_connected => TransportState::Degraded,
            Phase::Burst => TransportState::Connecting,
            Phase::SlowProbe if self.auth_rejected => TransportState::AuthRejected,
            Phase::SlowProbe => TransportState::Exhausted,
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| RelayError::TransportStatus("managed transport clock unavailable".into()))?
            .as_millis();
        let next_retry_at_ms = if self.next_at > Instant::now() && self.phase != Phase::Healthy {
            Some(
                u64::try_from(
                    now_ms.saturating_add(
                        self.next_at
                            .saturating_duration_since(Instant::now())
                            .as_millis(),
                    ),
                )
                .unwrap_or(u64::MAX),
            )
        } else {
            None
        };
        let status = TransportStatus {
            state,
            code: self.last_code,
            attempts: self.attempts,
            elapsed_ms: if self.phase == Phase::Healthy {
                0
            } else {
                u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
            },
            next_retry_at_ms,
            last_error: self.last_code.message().map(str::to_owned),
        };
        reporter
            .publish(status, terminal)
            .await
            .map_err(RelayError::TransportStatus)
    }

    pub async fn report_or_warn(&self) {
        if let Err(error) = self.report(false).await {
            // The owned writer retains this update and retries at its bounded
            // renewal cadence; readers expire the previous record's lease.
            tracing::warn!("{error}");
        }
    }

    pub fn managed() -> Self {
        Self::new(probe_delay)
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn ready_now(&mut self) {
        self.next_at = Instant::now();
    }

    pub fn new(probe_delay: fn() -> Duration) -> Self {
        let now = Instant::now();
        Self {
            enabled: true,
            ever_connected: false,
            last_code: TransportCode::None,
            reporter: None,
            phase: Phase::Healthy,
            attempts: 0,
            auth_rejected: false,
            started: now,
            next_at: now,
            probe_deadline: None,
            probe_delay,
        }
    }

    /// Subscription recovery shares the burst deadline. Slow probes do not
    /// inherit the expired burst window.
    pub fn recovery_deadline(&self) -> Option<Instant> {
        if !self.enabled {
            return None;
        }
        match self.phase {
            Phase::Burst => Some(self.started + BURST_DURATION),
            Phase::SlowProbe => self.probe_deadline,
            Phase::Healthy => None,
        }
    }

    pub fn ready_at(&self) -> Instant {
        self.next_at
    }

    pub fn slow(&self) -> bool {
        self.phase == Phase::SlowProbe
    }

    /// Retain the episode until authentication AND subscription recovery finish.
    pub async fn recovered(&mut self) {
        self.ever_connected = true;
        self.last_code = TransportCode::None;
        self.phase = Phase::Healthy;
        self.probe_deadline = None;
        self.attempts = 0;
        self.auth_rejected = false;
        self.next_at = Instant::now();
        self.report_or_warn().await;
    }

    pub fn defer(&mut self, delay: Duration) {
        if !self.slow() {
            self.next_at = Instant::now() + delay;
            if self.enabled {
                self.next_at = self.next_at.min(self.started + BURST_DURATION);
            }
        }
    }

    fn slow_probe(&mut self) {
        self.phase = Phase::SlowProbe;
        self.probe_deadline = None;
        self.next_at = Instant::now() + (self.probe_delay)();
    }

    pub fn failed(&mut self, error: &RelayError) {
        if !self.enabled {
            return;
        }
        self.auth_rejected =
            matches!(error, RelayError::AuthDenied(_)) && is_terminal_connect_error(error);
        self.last_code = if self.auth_rejected {
            TransportCode::AuthDenied
        } else if matches!(error, RelayError::Timeout) {
            TransportCode::Timeout
        } else {
            TransportCode::ConnectionFailed
        };
        if self.slow()
            || self.auth_rejected
            || self.attempts >= BURST_LIMIT
            || Instant::now() >= self.started + BURST_DURATION
        {
            self.slow_probe();
        }
    }

    /// Count all connection attempts, including DNS. Slow probes have their own
    /// normal connection timeout and never reuse the expired burst deadline.
    pub async fn connect<F, Fut, T>(&mut self, op: F) -> Result<T, RelayError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, RelayError>>,
    {
        if !self.enabled {
            return op().await;
        }
        if self.phase == Phase::Healthy {
            self.phase = Phase::Burst;
            self.started = Instant::now();
        }
        if self.phase == Phase::Burst
            && (self.attempts >= BURST_LIMIT || Instant::now() >= self.started + BURST_DURATION)
        {
            self.slow_probe();
            return Err(RelayError::Timeout);
        }
        if self.slow() {
            self.probe_deadline = Some(Instant::now() + BURST_DURATION);
        }
        self.attempts = self.attempts.saturating_add(1);
        self.report_or_warn().await;
        let result = match self.recovery_deadline() {
            Some(deadline) => tokio::time::timeout_at(deadline, op())
                .await
                .unwrap_or(Err(RelayError::Timeout)),
            None => op().await,
        };
        if let Err(error) = &result {
            self.failed(error);
        }
        result
    }
}

/// Separate from Buzz's existing ±20% ladder jitter. A full random u32 gives
/// inclusive 270–330 seconds; the clock is not the source of probe entropy.
fn probe_delay() -> Duration {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    let sample = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Duration::from_millis(270_000 + u64::from(sample) * 60_000 / u64::from(u32::MAX))
}
