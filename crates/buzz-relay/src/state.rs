//! Shared application state — Arc-wrapped, shared across all connections.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::body::Bytes;
use axum::extract::ws::{Message as WsMessage, Utf8Bytes as WsUtf8Bytes};
use dashmap::DashMap;
use futures_util::{
    future::join_all,
    stream::{FuturesUnordered, StreamExt},
};
use thiserror::Error;
use tokio::sync::{mpsc, watch, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use buzz_audit::AuditService;
use buzz_auth::{AuthService, Nip98ReplayGuard};
use buzz_core::tenant::TenantContext;
use buzz_core::CommunityId;
use buzz_db::Db;
use buzz_media::MediaStorage;
use buzz_pubsub::cache_invalidation::CacheInvalidation;
use buzz_pubsub::conn_control::ConnControl;
use buzz_pubsub::rate_limiter::RedisRateLimiter;
use buzz_pubsub::{PubSubManager, RedisNip98ReplayGuard};
use buzz_search::SearchService;
use buzz_workflow::WorkflowEngine;
use deadpool_redis;

use crate::audio::AudioRoomManager;
use crate::config::Config;
use crate::connection::{ConnectionSubscriptions, RestartClose};
use crate::subscription::SubscriptionRegistry;

pub(crate) type ScopedPubkeyKey = (CommunityId, [u8; 32]);

/// Stable wire reason used when the relay-membership admission boundary is
/// revoked for an already-authenticated connection.
pub(crate) const RELAY_MEMBERSHIP_REVOKED_REASON: &str = "restricted: not a relay member";

/// Upper bound for simultaneous fan-out relay-membership batches. The actual
/// state semaphore uses the lower of this cap and the configured handler
/// concurrency so Redis bursts cannot queue an unbounded number of writer
/// lookups, while the batch's identity bound remains `max_connections`.
pub(crate) const RELAY_MEMBERSHIP_FANOUT_MAX_CONCURRENT_BATCHES: usize = 32;

/// Why a community-bound socket is being asked to stop.
///
/// Ordinary lifecycle exits keep using cancellation alone and therefore retain
/// the existing bare-close behavior. Policy actions carry their stable reason
/// in the WebSocket close frame as a fallback when the control queue is full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommunityDisconnectReason {
    CommunityDeleted,
    Policy { reason: String },
}

impl CommunityDisconnectReason {
    pub(crate) fn close_message(&self) -> WsMessage {
        match self {
            Self::CommunityDeleted => WsMessage::Close(Some(axum::extract::ws::CloseFrame {
                code: axum::extract::ws::close_code::POLICY,
                reason: WsUtf8Bytes::from_static("community deleted"),
            })),
            Self::Policy { reason } => WsMessage::Close(Some(axum::extract::ws::CloseFrame {
                code: axum::extract::ws::close_code::POLICY,
                reason: WsUtf8Bytes::from(reason.clone()),
            })),
        }
    }
}

/// Per-socket lifecycle controls shared by the registry and the writer.
#[derive(Clone)]
pub(crate) struct CommunityConnectionControl {
    cancel: CancellationToken,
    reason_tx: watch::Sender<Option<CommunityDisconnectReason>>,
}

impl CommunityConnectionControl {
    pub(crate) fn new(cancel: CancellationToken) -> Self {
        let (reason_tx, _reason_rx) = watch::channel(None);
        Self { cancel, reason_tx }
    }

    pub(crate) fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub(crate) fn disconnect_reason(&self) -> watch::Receiver<Option<CommunityDisconnectReason>> {
        self.reason_tx.subscribe()
    }

    pub(crate) fn disconnect_reason_sender(
        &self,
    ) -> watch::Sender<Option<CommunityDisconnectReason>> {
        self.reason_tx.clone()
    }

    fn disconnect_community(&self) {
        self.reason_tx
            .send_replace(Some(CommunityDisconnectReason::CommunityDeleted));
        self.cancel.cancel();
    }
}

/// Leaves headroom under the process-wide drain deadline for a stalled writer.
const RESTART_CLOSE_ACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
type SlidingWindowCounter = (u32, Instant);
type ScopedRateLimiter = DashMap<ScopedPubkeyKey, SlidingWindowCounter>;

/// Per-connection entry in the connection manager.
struct ConnEntry {
    tx: mpsc::Sender<WsMessage>,
    /// Control-frame sender, drained ahead of data and before cancel wins in
    /// the send loop. Used to deliver a ban-disconnect frame that must reach
    /// the client before the socket is closed (see [`ConnectionManager::disconnect_pubkey`]).
    ctrl_tx: mpsc::Sender<WsMessage>,
    restart_tx: Option<mpsc::Sender<RestartClose>>,
    cancel: CancellationToken,
    /// Community resolved from the connection host at handshake. This is the
    /// receiver-side tenant label fan-out must compare against the event label.
    community_id: CommunityId,
    /// Shared with `ConnectionState` — both direct sends and fan-out
    /// broadcasts track the same consecutive-full counter.
    backpressure_count: Arc<AtomicU8>,
    subscriptions: ConnectionSubscriptions,
    authenticated_pubkey: Arc<std::sync::RwLock<Option<Vec<u8>>>>,
    /// Owner pubkey recorded as admission provenance for NIP-OA delegated
    /// sessions, if any. This is populated from the verified admission result,
    /// never from the durable `users.agent_owner_pubkey` metadata backfill.
    authenticated_agent_owner: Arc<std::sync::RwLock<Option<Vec<u8>>>>,
    /// Serializes durable membership revocation with live-session cleanup for
    /// this socket. A self-leave acquires the fence before deleting its roster
    /// row; sweeps and control consumers skip sockets already in flight.
    revocation_lock: Arc<tokio::sync::Mutex<()>>,
    /// Sender paired with the writer's close-reason receiver. Set immediately
    /// after registration, before any receive task starts, so a policy action
    /// can still produce a typed close when `ctrl_tx` is full.
    disconnect_reason_tx: Option<watch::Sender<Option<CommunityDisconnectReason>>>,
    grace_limit: u8,
}

/// Community-scoped lifecycle registry shared by every long-lived socket type.
///
/// A handler registers before durable active-state revalidation. Archival after
/// registration cancels the token; archival before registration is observed by
/// the revalidation. The returned guard removes the entry on every exit path.
pub struct CommunityConnectionRegistry {
    connections: Arc<DashMap<Uuid, (CommunityId, CommunityConnectionControl)>>,
}

impl Default for CommunityConnectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommunityConnectionRegistry {
    /// Creates an empty lifecycle registry.
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
        }
    }

    /// Registers one socket and returns a guard that deregisters it on drop.
    pub(crate) fn register(
        &self,
        connection_id: Uuid,
        community_id: CommunityId,
        control: CommunityConnectionControl,
    ) -> CommunityConnectionGuard {
        self.connections
            .insert(connection_id, (community_id, control));
        CommunityConnectionGuard {
            connection_id,
            connections: Arc::clone(&self.connections),
        }
    }

    /// Disconnects every socket type currently bound to `community_id` and
    /// attributes the close to community deletion.
    pub fn disconnect_community(&self, community_id: CommunityId) -> usize {
        let mut closed = 0;
        for entry in self.connections.iter() {
            if entry.value().0 == community_id {
                entry.value().1.disconnect_community();
                closed += 1;
            }
        }
        closed
    }

    /// Returns the distinct communities with live sockets on this pod.
    pub fn bound_communities(&self) -> HashSet<CommunityId> {
        self.connections
            .iter()
            .map(|entry| entry.value().0)
            .collect()
    }
}

/// Removes a socket lifecycle registration on every handler exit path.
pub struct CommunityConnectionGuard {
    connection_id: Uuid,
    connections: Arc<DashMap<Uuid, (CommunityId, CommunityConnectionControl)>>,
}

impl Drop for CommunityConnectionGuard {
    fn drop(&mut self) {
        self.connections.remove(&self.connection_id);
    }
}

/// Registers a socket, durably revalidates its community, then runs it.
///
/// The ordering is the archival admission invariant: archive-before-query is
/// observed by the query, while archive-after-registration sees the token.
pub(crate) async fn run_registered_community_connection<Check, CheckFuture, Run, RunFuture>(
    registry: &CommunityConnectionRegistry,
    connection_id: Uuid,
    community_id: CommunityId,
    control: CommunityConnectionControl,
    check_active: Check,
    run: Run,
) where
    Check: FnOnce() -> CheckFuture,
    CheckFuture: Future<Output = Result<bool, buzz_db::DbError>>,
    Run: FnOnce(CommunityConnectionControl) -> RunFuture,
    RunFuture: Future<Output = ()>,
{
    let cancel = control.cancel.clone();
    let _guard = registry.register(connection_id, community_id, control.clone());
    if !matches!(check_active().await, Ok(true)) {
        cancel.cancel();
        return;
    }
    if cancel.is_cancelled() {
        return;
    }
    run(control).await;
    cancel.cancel();
}

async fn revalidate_registered_communities<Check, CheckFuture>(
    registry: &CommunityConnectionRegistry,
    mut check_active: Check,
) -> (usize, Vec<(CommunityId, buzz_db::DbError)>)
where
    Check: FnMut(CommunityId) -> CheckFuture,
    CheckFuture: Future<Output = Result<bool, buzz_db::DbError>>,
{
    let communities = registry.bound_communities();
    let mut closed = 0;
    let mut failures = Vec::new();
    for community_id in communities {
        match check_active(community_id).await {
            Ok(false) => closed += registry.disconnect_community(community_id),
            Ok(true) => {}
            Err(error) => failures.push((community_id, error)),
        }
    }
    (closed, failures)
}

/// Tracks active Nostr WebSocket connections and provides message routing by connection ID.
pub struct ConnectionManager {
    connections: DashMap<Uuid, ConnEntry>,
    /// Sticky drain flag set by [`Self::drain_all`]. Registrations that land
    /// after the drain snapshot self-signal, so no upgrade-vs-shutdown
    /// interleaving can produce a connection that misses the restart close.
    draining: AtomicBool,
}

impl ConnectionManager {
    /// Creates a new, empty connection manager.
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            draining: AtomicBool::new(false),
        }
    }

    /// Registers a connection with its outbound sender, cancellation token,
    /// server-resolved community, shared backpressure counter, mutable
    /// subscription map, and grace limit.
    // Each argument is a distinct per-connection attribute stored verbatim in
    // `ConnEntry`; a params struct would only relocate the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn register(
        &self,
        conn_id: Uuid,
        tx: mpsc::Sender<WsMessage>,
        ctrl_tx: mpsc::Sender<WsMessage>,
        restart_tx: Option<mpsc::Sender<RestartClose>>,
        cancel: CancellationToken,
        community_id: CommunityId,
        backpressure_count: Arc<AtomicU8>,
        subscriptions: ConnectionSubscriptions,
        grace_limit: u8,
    ) {
        let drain_ctrl_tx = ctrl_tx.clone();
        let drain_cancel = cancel.clone();
        self.connections.insert(
            conn_id,
            ConnEntry {
                tx,
                ctrl_tx,
                restart_tx,
                cancel,
                community_id,
                backpressure_count,
                subscriptions,
                authenticated_pubkey: Arc::new(std::sync::RwLock::new(None)),
                authenticated_agent_owner: Arc::new(std::sync::RwLock::new(None)),
                revocation_lock: Arc::new(tokio::sync::Mutex::new(())),
                disconnect_reason_tx: None,
                grace_limit,
            },
        );
        // Insert-then-check pairs with drain_all's store-then-iterate: either
        // the drain iteration sees this entry, or this check sees the flag.
        // A registration that raced past the snapshot self-signals here, so
        // no connection can outlive graceful shutdown unclosed. A client that
        // arrives mid-shutdown should be closed at once, so the self-signal
        // always uses the immediate control-frame + cancel path regardless of
        // whether jittered drain is enabled — jitter smears the sockets that
        // were already established, not late arrivals.
        if self.draining.load(Ordering::SeqCst) {
            let _ = drain_ctrl_tx.try_send(Self::restart_close_frame());
            drain_cancel.cancel();
        }
    }

    /// Removes a connection from the registry.
    pub fn deregister(&self, conn_id: Uuid) {
        self.connections.remove(&conn_id);
    }

    /// Return whether a connection is still registered. Revocation request
    /// handling uses this to distinguish a live socket, whose fence must be
    /// respected, from a test or teardown sender that is no longer managed.
    pub(crate) fn has_connection(&self, conn_id: Uuid) -> bool {
        self.connections.contains_key(&conn_id)
    }

    /// Record the authenticated pubkey for a connection after NIP-42 succeeds.
    pub fn set_authenticated_pubkey(&self, conn_id: Uuid, pubkey_bytes: Vec<u8>) {
        self.set_authenticated_identity(conn_id, pubkey_bytes, None);
    }

    /// Record the authenticated principal and its optional NIP-OA owner for a
    /// connection after NIP-42 succeeds.
    pub fn set_authenticated_identity(
        &self,
        conn_id: Uuid,
        pubkey_bytes: Vec<u8>,
        agent_owner_pubkey: Option<Vec<u8>>,
    ) {
        if let Some(entry) = self.connections.get(&conn_id) {
            if let Ok(mut slot) = entry.authenticated_pubkey.write() {
                *slot = Some(pubkey_bytes);
            }
            if let Ok(mut slot) = entry.authenticated_agent_owner.write() {
                *slot = agent_owner_pubkey;
            }
        }
    }

    /// Attach the writer's close-reason channel to a registered connection.
    /// Registration is completed before receive tasks start, so policy close
    /// actions cannot race an uninitialized sender in production.
    pub(crate) fn set_disconnect_reason_sender(
        &self,
        conn_id: Uuid,
        reason_tx: watch::Sender<Option<CommunityDisconnectReason>>,
    ) {
        if let Some(mut entry) = self.connections.get_mut(&conn_id) {
            entry.disconnect_reason_tx = Some(reason_tx);
        }
    }

    /// Return live connection IDs authenticated as `pubkey_bytes` in one community.
    ///
    /// The same Nostr key may be connected to multiple communities at once.
    /// Callers use this for tenant-visible cleanup such as presence clearing and
    /// subscription eviction, so a connection in B must not keep A's derived
    /// state alive.
    pub fn connection_ids_for_pubkey_in_community(
        &self,
        community_id: CommunityId,
        pubkey_bytes: &[u8],
    ) -> Vec<Uuid> {
        self.connections
            .iter()
            .filter_map(|entry| {
                let matches = entry.community_id == community_id
                    && entry
                        .authenticated_pubkey
                        .read()
                        .ok()
                        .and_then(|value| {
                            value
                                .as_ref()
                                .map(|stored| stored.as_slice() == pubkey_bytes)
                        })
                        .unwrap_or(false);
                matches.then_some(*entry.key())
            })
            .collect()
    }

    /// Return live connection IDs whose authenticated principal is `pubkey` or
    /// whose verified NIP-OA owner is `pubkey` in one community. Removing a
    /// human relay member must also close delegated agent sessions that were
    /// admitted through that owner's membership row.
    pub fn connection_ids_for_pubkey_or_owner_in_community(
        &self,
        community_id: CommunityId,
        pubkey_bytes: &[u8],
    ) -> Vec<Uuid> {
        self.connections
            .iter()
            .filter_map(|entry| {
                if entry.community_id != community_id {
                    return None;
                }
                let direct_match = entry
                    .authenticated_pubkey
                    .read()
                    .ok()
                    .and_then(|value| {
                        value
                            .as_ref()
                            .map(|stored| stored.as_slice() == pubkey_bytes)
                    })
                    .unwrap_or(false);
                let owner_match = entry
                    .authenticated_agent_owner
                    .read()
                    .ok()
                    .and_then(|value| {
                        value
                            .as_ref()
                            .map(|stored| stored.as_slice() == pubkey_bytes)
                    })
                    .unwrap_or(false);
                (direct_match || owner_match).then_some(*entry.key())
            })
            .collect()
    }

    /// Snapshot authenticated identities and their owner-admission provenance
    /// for one community. The snapshot is intentionally local: the caller
    /// rechecks each identity against the writer before deciding whether to
    /// revoke it, so a Redis miss cannot leave an idle socket indefinitely.
    pub(crate) fn authenticated_identities_in_community(
        &self,
        community_id: CommunityId,
    ) -> Vec<(Uuid, Vec<u8>, Option<Vec<u8>>)> {
        self.connections
            .iter()
            .filter_map(|entry| {
                if entry.community_id != community_id {
                    return None;
                }
                let pubkey = entry.authenticated_pubkey.read().ok()?.clone()?;
                let owner = entry.authenticated_agent_owner.read().ok()?.clone();
                Some((*entry.key(), pubkey, owner))
            })
            .collect()
    }

    /// Return the authenticated pubkey recorded for a connection, if any.
    pub fn pubkey_for_conn(&self, conn_id: Uuid) -> Option<Vec<u8>> {
        self.connections
            .get(&conn_id)
            .and_then(|entry| entry.authenticated_pubkey.read().ok()?.clone())
    }

    /// Return the NIP-OA admission owner recorded for a connection, if any.
    pub fn admission_owner_for_conn(&self, conn_id: Uuid) -> Option<Vec<u8>> {
        self.connections
            .get(&conn_id)
            .and_then(|entry| entry.authenticated_agent_owner.read().ok()?.clone())
    }

    /// Backward-compatible alias for callers that used the old field name.
    /// The value is admission provenance, not the receipt/observer metadata
    /// stored in `users.agent_owner_pubkey`.
    pub fn agent_owner_for_conn(&self, conn_id: Uuid) -> Option<Vec<u8>> {
        self.admission_owner_for_conn(conn_id)
    }

    /// Try to claim the per-connection revocation fence without waiting.
    ///
    /// Membership sweeps and Redis control consumers use this form so one
    /// slow self-leave cannot make another bounded sweep wait indefinitely.
    /// The returned owned guard keeps the fence held across async cleanup.
    pub(crate) fn try_acquire_revocation_lock(
        &self,
        conn_id: Uuid,
    ) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        let lock = self
            .connections
            .get(&conn_id)
            .map(|entry| Arc::clone(&entry.revocation_lock))?;
        lock.try_lock_owned().ok()
    }

    /// Acquire the per-connection revocation fence with a bounded wait.
    ///
    /// The self-leave path claims this before its durable row deletion. A
    /// competing revocation therefore either finishes first (and the leave
    /// rechecks membership) or waits no longer than the caller's deadline.
    pub(crate) async fn acquire_revocation_lock(
        &self,
        conn_id: Uuid,
        timeout: std::time::Duration,
    ) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        let lock = self
            .connections
            .get(&conn_id)
            .map(|entry| Arc::clone(&entry.revocation_lock))?;
        tokio::time::timeout(timeout, lock.lock_owned()).await.ok()
    }

    /// Queue a control frame without applying data-buffer backpressure.
    pub(crate) fn send_control(&self, conn_id: Uuid, msg: WsMessage) -> bool {
        self.connections
            .get(&conn_id)
            .is_some_and(|entry| entry.ctrl_tx.try_send(msg).is_ok())
    }

    /// Queue one control frame, waiting briefly for a stalled writer to make
    /// room. Revocation ACKs use this path because a best-effort `try_send`
    /// can otherwise make a successful self-leave look like a lost request.
    /// The bounded deadline keeps a dead writer from holding the revocation
    /// task forever; callers receive `false` and still cancel the socket.
    pub(crate) async fn send_control_bounded(
        &self,
        conn_id: Uuid,
        msg: WsMessage,
        timeout: std::time::Duration,
    ) -> bool {
        let Some(tx) = self
            .connections
            .get(&conn_id)
            .map(|entry| entry.ctrl_tx.clone())
        else {
            return false;
        };

        match tx.try_send(msg) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Closed(_)) => false,
            Err(mpsc::error::TrySendError::Full(msg)) => {
                match tokio::time::timeout(timeout, tx.reserve()).await {
                    Ok(Ok(permit)) => {
                        permit.send(msg);
                        true
                    }
                    Ok(Err(_)) | Err(_) => false,
                }
            }
        }
    }

    /// Set a policy close reason without cancelling the connection yet.
    /// Callers that need to remove subscriptions first use this as the first
    /// step, then call [`Self::cancel_connection`] after all final frames are
    /// queued.
    pub(crate) fn mark_policy_close(&self, conn_id: Uuid, reason: &str) -> bool {
        let Some(entry) = self.connections.get(&conn_id) else {
            return false;
        };
        if let Some(reason_tx) = &entry.disconnect_reason_tx {
            reason_tx.send_replace(Some(CommunityDisconnectReason::Policy {
                reason: reason.to_owned(),
            }));
        }
        true
    }

    /// Mark an already-authenticated connection as revoked from the relay
    /// roster. The caller owns cancellation ordering so it can first evict all
    /// subscriptions and queue their correlated `CLOSED` frames.
    pub(crate) fn mark_relay_membership_revoked(&self, conn_id: Uuid) -> bool {
        self.mark_policy_close(conn_id, RELAY_MEMBERSHIP_REVOKED_REASON)
    }

    /// Cancel one live connection after its final control frames are queued.
    pub(crate) fn cancel_connection(&self, conn_id: Uuid) -> bool {
        self.connections.get(&conn_id).is_some_and(|entry| {
            entry.cancel.cancel();
            true
        })
    }

    /// Disconnect every live connection authenticated as `pubkey` **in
    /// `community`**, delivering a final `OK false` frame carrying `reason`
    /// before closing.
    ///
    /// Used for live ban and relay-membership enforcement: the decision must
    /// take effect immediately on existing sessions, not just at the next auth.
    /// The frame is sent on the control channel, which the send loop drains
    /// ahead of both queued data and the biased cancel branch, so the client
    /// learns *why* it was dropped. `event_id` labels the `OK` (a decision with
    /// no triggering client event uses a synthetic all-zero id).
    ///
    /// The `community` filter is the tenant fence: one pod holds sockets for
    /// many communities, and the same pubkey may be live in several. A ban in
    /// community A must close only A's sockets, never a session the member holds
    /// in community B ("authority stays inside the tenant fence").
    ///
    /// Returns the number of connections closed. This is the pod-local half of
    /// live enforcement; cross-pod fan-out publishes the same intent over Redis.
    pub fn disconnect_pubkey(
        &self,
        community: CommunityId,
        pubkey: &[u8],
        event_id: &str,
        reason: &str,
    ) -> usize {
        let frame = crate::protocol::RelayMessage::ok(event_id, false, reason);
        let mut closed = 0usize;
        for conn_id in self.connection_ids_for_pubkey_in_community(community, pubkey) {
            if let Some(entry) = self.connections.get(&conn_id) {
                if entry.community_id != community {
                    continue;
                }
                // Set the policy close reason before cancellation. A full
                // control buffer still gets a typed policy close from the
                // writer instead of degrading to Close(None).
                if let Some(reason_tx) = &entry.disconnect_reason_tx {
                    reason_tx.send_replace(Some(CommunityDisconnectReason::Policy {
                        reason: reason.to_owned(),
                    }));
                }
                let _ = entry
                    .ctrl_tx
                    .try_send(WsMessage::Text(frame.clone().into()));
                entry.cancel.cancel();
                closed += 1;
            }
        }
        closed
    }

    /// Closes every live connection with a `1012 Service Restart` close frame.
    ///
    /// This is the original, all-at-once drain, retained as the default path
    /// (`BUZZ_DRAIN_JITTER_MS` unset or `0`). It is synchronous and returns as
    /// soon as every close is queued and every connection cancelled, so the
    /// caller's hard-drain timeout backstops delivery unchanged.
    ///
    /// Called when graceful shutdown starts draining. Without this, upgraded
    /// WebSocket connections outlive the axum listener drain: clients ride the
    /// dying pod until the forced exit and then learn about the restart from a
    /// TCP reset (or, on an abrupt kill, from up to 60s of stall-watchdog
    /// silence). The explicit close frame tells them to reconnect immediately
    /// — and that the disconnect is a restart, not a policy action.
    ///
    /// Uses the "queue frame on ctrl, then cancel" idiom (see
    /// [`ConnectionManager::disconnect_pubkey`]): the send loop drains queued
    /// control frames — including this close — before its cancel branch closes
    /// the socket. Best-effort: a full control buffer still gets the close via
    /// cancel, just without the restart code.
    ///
    /// Returns the number of connections signalled.
    pub fn drain_all(&self) -> usize {
        // Store-then-iterate pairs with register's insert-then-check: a
        // registration that misses this iteration observes the flag and
        // self-signals instead. The flag is sticky — drain is one-way.
        self.draining.store(true, Ordering::SeqCst);
        let frame = Self::restart_close_frame();
        let mut closed = 0usize;
        for entry in self.connections.iter() {
            let _ = entry.ctrl_tx.try_send(frame.clone());
            entry.cancel.cancel();
            closed += 1;
        }
        closed
    }

    /// Closes every live connection with a `1012 Service Restart` frame,
    /// spreading closes across `[1, jitter_ms]`.
    ///
    /// This is the jittered drain, used only when `BUZZ_DRAIN_JITTER_MS > 0`.
    /// It is kept deliberately separate from [`Self::drain_all`] so that the
    /// default (jitter-off) shutdown path is byte-for-byte the previously
    /// shipped behavior; the new close-acknowledgement machinery only runs when
    /// jitter is explicitly enabled. Once the jittered path is proven in
    /// production for all cases, the two can be unified and the old one dropped.
    ///
    /// A pod under a rolling deploy can hold thousands of WebSocket sessions.
    /// Closing them simultaneously ([`Self::drain_all`]) makes every client
    /// reconnect at the same moment — a thundering herd that drives the DB
    /// pool-timeout bursts observed on each roll. Delaying each connection's
    /// close by an independent uniform random offset in `[1, jitter_ms]`
    /// smears the reconnects across the window while keeping the well-attributed
    /// 1012 close.
    ///
    /// Each delayed close is delivered over the connection's dedicated
    /// [`RestartClose`] channel: the writer flushes the 1012 frame and
    /// acknowledges the flush, so drain waits for confirmed delivery (up to
    /// [`RESTART_CLOSE_ACK_TIMEOUT`]) rather than assuming it. If the channel is
    /// full/closed or the ack times out, drain falls back to cancellation.
    ///
    /// The sticky drain flag is set before the first await, preserving
    /// [`Self::drain_all`]'s shutdown-boundary race guarantee: a registration
    /// that lands after the snapshot self-signals immediately (no jitter — a
    /// client arriving mid-shutdown should be closed at once). The returned
    /// future owns every delayed close, so the caller must await it before the
    /// relay runtime is allowed to stop.
    ///
    /// Returns the number of connections signalled.
    pub async fn drain_all_jittered(&self, jitter_ms: u64) -> usize {
        // Store-then-snapshot pairs with register's insert-then-check: either
        // the snapshot captures a registration, or it observes the sticky flag
        // and self-signals immediately.
        self.draining.store(true, Ordering::SeqCst);
        let jitter_ms = jitter_ms.max(1);
        let pending: Vec<_> = self
            .connections
            .iter()
            .map(|entry| {
                let ctrl_tx = entry.ctrl_tx.clone();
                let restart_tx = entry.restart_tx.clone();
                let cancel = entry.cancel.clone();
                let delay_ms = 1 + rand::random::<u64>() % jitter_ms;
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    let Some(restart_tx) = restart_tx else {
                        // Unit-only registrations do not own a writer task.
                        let _ = ctrl_tx.try_send(Self::restart_close_frame());
                        cancel.cancel();
                        return;
                    };
                    let (flushed_tx, flushed_rx) = tokio::sync::oneshot::channel();
                    if restart_tx
                        .try_send(RestartClose {
                            flushed: flushed_tx,
                        })
                        .is_err()
                    {
                        cancel.cancel();
                        return;
                    }
                    let flushed = tokio::time::timeout(RESTART_CLOSE_ACK_TIMEOUT, flushed_rx).await;
                    if !matches!(flushed, Ok(Ok(true))) {
                        cancel.cancel();
                    }
                }
            })
            .collect();
        let count = pending.len();
        join_all(pending).await;
        count
    }

    /// The WS close frame announcing a graceful restart: 1012 Service Restart.
    fn restart_close_frame() -> WsMessage {
        WsMessage::Close(Some(axum::extract::ws::CloseFrame {
            code: axum::extract::ws::close_code::RESTART,
            reason: axum::extract::ws::Utf8Bytes::from_static("relay restarting"),
        }))
    }

    /// Return the server-resolved community that the connection's host bound to.
    pub fn community_for_conn(&self, conn_id: Uuid) -> Option<CommunityId> {
        self.connections
            .get(&conn_id)
            .map(|entry| entry.community_id)
    }

    /// Return the subscription map for a connection, if it is still live.
    pub fn subscriptions_for(&self, conn_id: Uuid) -> Option<ConnectionSubscriptions> {
        self.connections
            .get(&conn_id)
            .map(|entry| Arc::clone(&entry.subscriptions))
    }

    /// Snapshot the number of live WebSocket connections per community.
    ///
    /// Returns a map from community UUID to connection count. Used by the
    /// usage poller; snapshotting avoids per-community gauge drift from
    /// mismatched inc/dec across async boundaries.
    pub fn per_community_ws_connections(&self) -> HashMap<CommunityId, u64> {
        let mut counts: HashMap<CommunityId, u64> = HashMap::new();
        for entry in self.connections.iter() {
            *counts.entry(entry.community_id).or_default() += 1;
        }
        counts
    }

    /// Snapshot the number of distinct authenticated pubkeys online per community.
    ///
    /// A pubkey connected to multiple pods will be counted once per pod — the
    /// dashboard sums across pods, so per-pod partial counts are correct.
    /// A pubkey connected twice on the same pod is counted once (distinct set).
    pub fn per_community_users_online(&self) -> HashMap<CommunityId, u64> {
        // community_id → set of pubkey bytes
        let mut seen: HashMap<CommunityId, HashSet<Vec<u8>>> = HashMap::new();
        for entry in self.connections.iter() {
            if let Ok(lock) = entry.authenticated_pubkey.read() {
                if let Some(pk) = lock.as_ref() {
                    seen.entry(entry.community_id)
                        .or_default()
                        .insert(pk.clone());
                }
            }
        }
        seen.into_iter()
            .map(|(cid, set)| (cid, set.len() as u64))
            .collect()
    }

    /// Return the authenticated pubkey for a connection, if any.
    pub fn pubkey_for(&self, conn_id: Uuid) -> Option<Vec<u8>> {
        self.connections
            .get(&conn_id)
            .and_then(|entry| entry.authenticated_pubkey.read().ok()?.clone())
    }

    /// Sends a text message to the given connection.
    ///
    /// Returns `false` if the connection is gone or the buffer is full.
    /// On sustained backpressure (>grace_limit consecutive full buffers),
    /// cancels the connection. Transient stalls get a warning only.
    pub fn send_to(&self, conn_id: Uuid, msg: String) -> bool {
        self.try_send_ws_message(conn_id, WsMessage::Text(msg.into()))
    }

    /// Sends an already-serialized UTF-8 text payload to the given connection.
    ///
    /// The shared `Bytes` payload is cloned into the outbound WS message without
    /// copying the frame body. Callers must only pass valid UTF-8 bytes.
    pub fn send_to_text_bytes(&self, conn_id: Uuid, msg: Arc<Bytes>) -> bool {
        let text = WsUtf8Bytes::try_from(Bytes::clone(msg.as_ref()))
            .expect("relay fan-out frames are serialized UTF-8 JSON");
        self.try_send_ws_message(conn_id, WsMessage::Text(text))
    }

    fn try_send_ws_message(&self, conn_id: Uuid, msg: WsMessage) -> bool {
        if let Some(entry) = self.connections.get(&conn_id) {
            let conn = entry.value();
            match conn.tx.try_send(msg) {
                Ok(_) => {
                    conn.backpressure_count.store(0, Ordering::Relaxed);
                    true
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    let count = conn.backpressure_count.fetch_add(1, Ordering::Relaxed) + 1;
                    if count >= conn.grace_limit {
                        tracing::warn!(conn_id = %conn_id, count, "fan-out: sustained backpressure — cancelling slow client");
                        metrics::counter!("buzz_ws_backpressure_disconnects_total").increment(1);
                        conn.cancel.cancel();
                    } else {
                        tracing::warn!(conn_id = %conn_id, count, grace = conn.grace_limit, "fan-out: send buffer full — grace {count}/{}", conn.grace_limit);
                    }
                    false
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    tracing::debug!(conn_id = %conn_id, "fan-out: send channel closed");
                    false
                }
            }
        } else {
            false
        }
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Work captured while a relay-membership revocation is between its durable
/// delete and its cross-pod publish. Subscriptions are removed before the
/// publish so no new fan-out match can be created while propagation is in
/// flight; final control frames and cancellation are applied afterwards.
struct PendingPubkeyRevocation {
    conn_id: Uuid,
    excluded: bool,
    removed: Vec<crate::subscription::RemovedSubscription>,
    /// Held from before the durable mutation (self-leave) or from the first
    /// live-session cleanup step (sweep/control command) through final frames
    /// and cancellation.
    _revocation_guard: tokio::sync::OwnedMutexGuard<()>,
}

/// Inputs owned by the NIP-43 self-leave operation while it is finalized.
/// The guard is acquired before the durable membership delete and is consumed
/// only after the origin ACK and policy close have been queued.
pub(crate) struct SelfLeaveRevocation {
    /// Originating connection excluded from the remote-style rejection path.
    pub(crate) conn_id: Uuid,
    /// Event-level success text returned when propagation succeeds.
    pub(crate) success_message: String,
    /// Per-connection fence held across durable delete and finalization.
    pub(crate) revocation_guard: tokio::sync::OwnedMutexGuard<()>,
}

#[derive(Default)]
struct RevocationOptions {
    excluded_conn_id: Option<Uuid>,
    leave_success_message: Option<String>,
    held_conn_id: Option<(Uuid, tokio::sync::OwnedMutexGuard<()>)>,
}

type RelayMembershipIdentity = (CommunityId, Vec<u8>, Option<Vec<u8>>);
type RelayMembershipGroup = (RelayMembershipIdentity, Vec<Uuid>);

/// Keep one membership sweep finite even when a relay has many idle sockets.
const RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES: usize = 512;
/// Bound concurrent writer lookups so a sweep cannot consume the whole pool.
const RELAY_MEMBERSHIP_SWEEP_CONCURRENCY: usize = 16;
/// A single writer lookup must not hold a sweep slot indefinitely.
const RELAY_MEMBERSHIP_LOOKUP_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);
/// Overall deadline for one bounded reconciliation pass.
const RELAY_MEMBERSHIP_SWEEP_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
/// A self-leave must never wait indefinitely for local cleanup of other
/// sessions. Work that does not fit this window is left live for the durable
/// membership sweep, which owns eventual cleanup from the writer-backed row.
const RELAY_MEMBERSHIP_LEAVE_LOCAL_FINALIZATION_TIMEOUT: std::time::Duration =
    RELAY_MEMBERSHIP_SWEEP_DEADLINE;

/// Test-only gate placed at the production self-leave bulk-cleanup seam. It
/// makes the origin ACK ordering falsifiable without requiring a large live
/// connection registry or a slow external service.
#[cfg(test)]
pub(crate) struct SelfLeaveBulkTestHook {
    pub(crate) entered: tokio::sync::Notify,
    pub(crate) release: tokio::sync::Notify,
}

#[cfg(test)]
static SELF_LEAVE_BULK_TEST_HOOK: std::sync::OnceLock<
    tokio::sync::Mutex<Option<Arc<SelfLeaveBulkTestHook>>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
/// Install or clear the test gate for the production self-leave bulk pass.
pub(crate) async fn install_self_leave_bulk_test_hook(hook: Option<Arc<SelfLeaveBulkTestHook>>) {
    let slot = SELF_LEAVE_BULK_TEST_HOOK.get_or_init(|| tokio::sync::Mutex::new(None));
    *slot.lock().await = hook;
}

#[cfg(test)]
async fn maybe_stall_self_leave_bulk_for_test() {
    let Some(slot) = SELF_LEAVE_BULK_TEST_HOOK.get() else {
        return;
    };
    let hook = slot.lock().await.clone();
    if let Some(hook) = hook {
        hook.entered.notify_one();
        hook.release.notified().await;
    }
}

/// Test-only gate placed at the production detached-topic-release seam. It
/// makes cancellation after registry detachment falsifiable without relying on
/// a live Redis connection or an arbitrarily large subscription set.
#[cfg(test)]
pub(crate) struct DetachedTopicReleaseTestHook {
    pub(crate) entered: tokio::sync::Notify,
    pub(crate) release: tokio::sync::Notify,
}

#[cfg(test)]
static DETACHED_TOPIC_RELEASE_TEST_HOOK: std::sync::OnceLock<
    tokio::sync::Mutex<Option<Arc<DetachedTopicReleaseTestHook>>>,
> = std::sync::OnceLock::new();

#[cfg(test)]
/// Install or clear the test gate for detached subscription-topic release.
pub(crate) async fn install_detached_topic_release_test_hook(
    hook: Option<Arc<DetachedTopicReleaseTestHook>>,
) {
    let slot = DETACHED_TOPIC_RELEASE_TEST_HOOK.get_or_init(|| tokio::sync::Mutex::new(None));
    *slot.lock().await = hook;
}

#[cfg(test)]
async fn maybe_stall_detached_topic_release_for_test() {
    let Some(slot) = DETACHED_TOPIC_RELEASE_TEST_HOOK.get() else {
        return;
    };
    let hook = slot.lock().await.clone();
    if let Some(hook) = hook {
        hook.entered.notify_one();
        hook.release.notified().await;
    }
}

/// Reserve half of every bounded pass for fresh identities whenever retry
/// work and live identities coexist. A permanently failing writer lookup must
/// therefore make progress through the fresh cursor ring instead of occupying
/// the whole window forever.
const RELAY_MEMBERSHIP_SWEEP_RETRY_BUDGET: usize = RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES / 2;
/// A concurrent trigger gets one coalesced follow-up pass, then waits for the
/// next periodic/reconnect trigger. This keeps work finite during a storm.
const RELAY_MEMBERSHIP_SWEEP_MAX_PASSES: usize = 2;
/// A self-leave must claim its fence before deleting the durable row, but a
/// stalled competing revocation must not hold the event handler forever. The
/// leave handler's database phase is bounded separately; keep this wait longer
/// than that phase plus the terminal control-frame budget so a request racing
/// an owned leave never reaches its cancellation fallback first.
pub(crate) const RELAY_MEMBERSHIP_REVOCATION_LOCK_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(12);
/// The complete writer-backed self-leave operation, from its final membership
/// read through durable deletion, must reach a terminal event response within
/// this deadline while the per-connection revocation fence remains held.
pub(crate) const RELAY_MEMBERSHIP_LEAVE_OPERATION_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(10);
/// A common EVENT admission read must not hold a receive task indefinitely
/// while waiting for the writer-backed relay-membership roster.
pub(crate) const RELAY_MEMBERSHIP_EVENT_ADMISSION_TIMEOUT: std::time::Duration =
    RELAY_MEMBERSHIP_LOOKUP_TIMEOUT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MembershipLookupOutcome {
    Allowed,
    Denied,
    Failed,
    TimedOut,
}

/// Results from one bounded lookup batch. Work that was never started because
/// the overall deadline expired is retained alongside in-flight work so the
/// next reconciliation pass can retry it.
struct MembershipLookupBatch {
    outcomes: Vec<(RelayMembershipIdentity, Vec<Uuid>, MembershipLookupOutcome)>,
    unresolved: Vec<RelayMembershipGroup>,
}

/// Run a selected set of membership checks with both per-lookup and overall
/// deadlines. Groups that time out or fail are returned as unresolved and are
/// therefore retained in the next snapshot rather than being revoked.
async fn run_bounded_membership_lookups<Lookup, CheckFuture, LookupError>(
    groups: Vec<RelayMembershipGroup>,
    lookup: Lookup,
    per_lookup_timeout: std::time::Duration,
    deadline: std::time::Duration,
    concurrency: usize,
) -> MembershipLookupBatch
where
    Lookup: Fn(CommunityId, Vec<u8>, Option<Vec<u8>>) -> CheckFuture,
    CheckFuture: Future<Output = Result<bool, LookupError>>,
{
    if groups.is_empty() || concurrency == 0 || deadline.is_zero() {
        return MembershipLookupBatch {
            unresolved: groups,
            outcomes: Vec::new(),
        };
    }

    let all_groups = groups.clone();
    let mut remaining = groups.into_iter();
    let mut checks = FuturesUnordered::new();
    let make_check = |(identity, conn_ids): (RelayMembershipIdentity, Vec<Uuid>)| {
        let lookup = &lookup;
        async move {
            let (community_id, pubkey, owner) = identity.clone();
            let outcome =
                match tokio::time::timeout(per_lookup_timeout, lookup(community_id, pubkey, owner))
                    .await
                {
                    Ok(Ok(true)) => MembershipLookupOutcome::Allowed,
                    Ok(Ok(false)) => MembershipLookupOutcome::Denied,
                    Ok(Err(_)) => MembershipLookupOutcome::Failed,
                    Err(_) => MembershipLookupOutcome::TimedOut,
                };
            ((identity, conn_ids), outcome)
        }
    };

    for _ in 0..concurrency {
        let Some(group) = remaining.next() else {
            break;
        };
        checks.push(make_check(group));
    }

    let deadline_at = Instant::now() + deadline;
    let mut outcomes = Vec::new();
    while !checks.is_empty() {
        let remaining_deadline = deadline_at.saturating_duration_since(Instant::now());
        if remaining_deadline.is_zero() {
            break;
        }
        let next = tokio::time::timeout(remaining_deadline, checks.next()).await;
        let Some(Some(((identity, conn_ids), outcome))) = next.ok() else {
            break;
        };
        outcomes.push((identity, conn_ids, outcome));
        if let Some(group) = remaining.next() {
            checks.push(make_check(group));
        }
    }

    let completed: HashSet<_> = outcomes
        .iter()
        .map(|(identity, _, _)| identity.clone())
        .collect();
    let unresolved = all_groups
        .into_iter()
        .filter(|(identity, _)| !completed.contains(identity))
        .collect();
    MembershipLookupBatch {
        outcomes,
        unresolved,
    }
}

/// Coalesce overlapping revalidation triggers and run at most one bounded
/// follow-up pass for a trigger that arrived while the first pass was active.
async fn run_coalesced_membership_revalidation<Work, WorkFuture>(
    lock: &tokio::sync::Mutex<()>,
    pending: &AtomicBool,
    mut work: Work,
) -> usize
where
    Work: FnMut() -> WorkFuture,
    WorkFuture: Future<Output = usize>,
{
    let Ok(_guard) = lock.try_lock() else {
        pending.store(true, Ordering::SeqCst);
        metrics::counter!("buzz_relay_membership_revalidation_coalesced_total").increment(1);
        return 0;
    };

    let mut closed = 0;
    for _ in 0..RELAY_MEMBERSHIP_SWEEP_MAX_PASSES {
        pending.store(false, Ordering::SeqCst);
        closed += work().await;
        if !pending.swap(false, Ordering::SeqCst) {
            break;
        }
    }
    closed
}

/// Sort identity groups and select one fair, bounded window for a sweep.
/// Advancing the cursor by the selected count makes a large roster rotate
/// across passes instead of repeatedly checking the first registry entries.
#[cfg(test)]
fn select_bounded_membership_groups(
    groups: Vec<RelayMembershipGroup>,
    cursor: &AtomicU64,
) -> Vec<(RelayMembershipIdentity, Vec<Uuid>)> {
    select_bounded_membership_groups_with_limit(
        groups,
        cursor,
        RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES,
    )
}

/// Select up to `limit` groups from the sorted identity ring and advance the
/// cursor by the selected window. Deferred groups are tracked separately by
/// the caller, so cursor progress cannot strand work that a deadline skipped.
fn select_bounded_membership_groups_with_limit(
    mut groups: Vec<RelayMembershipGroup>,
    cursor: &AtomicU64,
    limit: usize,
) -> Vec<(RelayMembershipIdentity, Vec<Uuid>)> {
    if groups.is_empty() {
        return Vec::new();
    }
    groups.sort_by(|left, right| left.0.cmp(&right.0));
    let selected_count = groups.len().min(limit);
    if selected_count == 0 {
        return Vec::new();
    }
    let start = (cursor.fetch_add(selected_count as u64, Ordering::SeqCst) as usize) % groups.len();
    (0..selected_count)
        .map(|offset| groups[(start + offset) % groups.len()].clone())
        .collect()
}

/// Choose retry work and fresh cursor work for one bounded sweep. Retry groups
/// retain FIFO order, but they receive only a bounded share when fresh
/// identities are available. This helper is shared by the production pass and
/// its fairness regression so the guard cannot be removed without changing a
/// falsifiable selection result.
fn select_membership_revalidation_work(
    grouped: &mut HashMap<RelayMembershipIdentity, Vec<Uuid>>,
    queued: Vec<RelayMembershipGroup>,
    cursor: &AtomicU64,
) -> (Vec<RelayMembershipGroup>, Vec<RelayMembershipGroup>) {
    let mut retry_candidates = Vec::new();
    for (identity, _) in queued {
        let Some(conn_ids) = grouped.remove(&identity) else {
            continue;
        };
        retry_candidates.push((identity, conn_ids));
    }

    let retry_budget = if grouped.is_empty() {
        RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES
    } else {
        RELAY_MEMBERSHIP_SWEEP_RETRY_BUDGET.max(1)
    };
    let retry_count = retry_candidates.len().min(retry_budget);
    let mut selected = retry_candidates.drain(..retry_count).collect::<Vec<_>>();
    let mut deferred = retry_candidates;
    let fresh_limit = RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES.saturating_sub(selected.len());
    let fresh_candidates = grouped.drain().collect::<Vec<_>>();
    if fresh_limit > 0 && !fresh_candidates.is_empty() {
        let fresh_selected = select_bounded_membership_groups_with_limit(
            fresh_candidates.clone(),
            cursor,
            fresh_limit,
        );
        let selected_fresh_ids: HashSet<_> = fresh_selected
            .iter()
            .map(|(identity, _)| identity.clone())
            .collect();
        selected.extend(fresh_selected);
        deferred.extend(
            fresh_candidates
                .into_iter()
                .filter(|(identity, _)| !selected_fresh_ids.contains(identity)),
        );
    } else {
        // The fresh ring is still live work even when retries consume the
        // whole bounded window. Retain it behind the retry queue instead of
        // dropping the identities that did not fit this pass.
        deferred.extend(fresh_candidates);
    }
    (selected, deferred)
}

/// Bounded failure returned by a cluster-wide live revocation.
#[derive(Debug, Error)]
pub(crate) enum RevocationError {
    /// Redis publication failed after the durable authorization mutation.
    #[error("connection-control publication failed: {0}")]
    Publish(#[from] buzz_pubsub::PubSubError),
    /// Redis did not complete the publication before the bounded deadline.
    #[error("connection-control publication timed out")]
    PublishTimeout,
    /// The initiating self-leave connection could not accept its event ACK
    /// before the control-frame deadline. The socket is still policy-closed.
    #[error("self-leave acknowledgement could not be queued before the deadline")]
    AckDeliveryTimeout,
}

/// Result of the local finalization phase. The ACK bit is meaningful only for
/// the self-leave variant; ordinary revocations use the policy-close fallback.
struct PubkeyRevocationFinish {
    closed: usize,
    ack_delivered: bool,
}

/// A stalled Redis command must not hold local revocation finalization forever.
const REVOCATION_REDIS_PUBLISH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// A stalled control writer must not block membership revocation forever.
const REVOCATION_CONTROL_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// Shared application state, cloned cheaply via inner `Arc` fields.
#[derive(Clone)]
pub struct AppState {
    /// Relay configuration.
    pub config: Arc<Config>,
    /// Database connection pool.
    pub db: Db,
    /// Redis pool for readiness health checks.
    pub redis_pool: deadpool_redis::Pool,
    /// Audit event service, absent when audit logging is disabled.
    pub audit: Option<Arc<AuditService>>,
    /// Pub/sub manager for broadcasting events to subscribers.
    pub pubsub: Arc<PubSubManager>,
    /// Authentication service.
    pub auth: Arc<AuthService>,
    /// Full-text search service.
    pub search: Arc<SearchService>,
    /// Registry of active client subscriptions.
    pub sub_registry: Arc<SubscriptionRegistry>,
    /// Registry of active WebSocket connections.
    pub conn_manager: Arc<ConnectionManager>,
    /// Lifecycle cancellation for every long-lived socket, including huddle audio.
    pub community_connections: Arc<CommunityConnectionRegistry>,
    /// Stops only the periodic lifecycle revalidator during graceful shutdown.
    pub community_revalidator_cancel: CancellationToken,
    /// Test/telemetry counter for archive disconnect publication attempts.
    pub community_disconnect_publish_attempts: Arc<AtomicU64>,
    /// Single-flight fence for bounded relay-membership reconciliation.
    pub relay_membership_revalidation_lock: Arc<tokio::sync::Mutex<()>>,
    /// Set by a reconnect/lag trigger that arrives while reconciliation is in
    /// progress; the active pass consumes at most one coalesced follow-up.
    pub relay_membership_revalidation_pending: Arc<AtomicBool>,
    /// Cursor over the sorted identity groups, so a capped sweep eventually
    /// examines every live principal instead of repeatedly favoring the first
    /// connections returned by the registry.
    pub relay_membership_revalidation_cursor: Arc<AtomicU64>,
    /// Groups that a bounded membership sweep could not finish. The queue is
    /// drained before a fresh cursor window and capped by the configured live
    /// connection limit, so a slow writer cannot create unbounded retry work.
    relay_membership_revalidation_pending_groups:
        Arc<tokio::sync::Mutex<VecDeque<RelayMembershipGroup>>>,
    /// Semaphore limiting total concurrent connections.
    pub conn_semaphore: Arc<Semaphore>,
    /// Semaphore limiting concurrent message handler tasks.
    pub handler_semaphore: Arc<Semaphore>,
    /// Semaphore limiting concurrent relay-membership authorization batches
    /// used by fan-out. This is separate from the handler semaphore because
    /// post-commit and Redis fan-out tasks do not hold a handler permit.
    pub relay_membership_fanout_semaphore: Arc<Semaphore>,
    /// Semaphore limiting concurrent git subprocess operations across
    /// the whole relay. Bounds resource use; **not** writer
    /// serialization — that's the CAS at the manifest pointer (spec
    /// §Push step 7, `Inv_NoFork`).
    pub git_semaphore: Arc<Semaphore>,
    /// Semaphore limiting concurrent media upload parsing/transcoding work.
    pub media_upload_semaphore: Arc<Semaphore>,

    /// Workflow engine for background processing.
    pub workflow_engine: Arc<WorkflowEngine>,
    /// Relay signing keypair — used to sign system messages (kind 40099).
    pub relay_keypair: nostr::Keys,
    /// Process-local generation advertised for non-mesh huddle liveness.
    ///
    /// A fresh value on every relay start lets desktop clients retire persisted
    /// admissions when an in-memory audio room is recreated at the same roster
    /// revision after a restart. Mesh rooms use their Redis-fenced generation.
    pub huddle_liveness_generation: Uuid,

    /// Recently-published event IDs for local-echo deduplication, keyed by
    /// `(community_id, event_id)`. Events fanned out in-process are added here;
    /// the Redis subscriber consumer skips them to avoid double delivery.
    ///
    /// The community is part of the key because the same Nostr event id can
    /// legitimately exist in two communities (channel-less events, and
    /// same-channel-UUID/same-`h` events across tenants). Keying on the bare id
    /// would let a local publish in community A suppress delivery of a distinct
    /// event with the same id arriving via Redis for community B — a
    /// cross-community non-interference violation. Entries expire after 60
    /// seconds via moka's TTL eviction — bounded regardless of subscriber health.
    pub local_event_ids: Arc<moka::sync::Cache<(CommunityId, [u8; 32]), ()>>,
    /// Membership cache: (community_id, channel_id, pubkey_bytes) → is_member.
    /// Short TTL (10s) — membership changes are rare but must propagate.
    #[allow(clippy::type_complexity)]
    pub membership_cache: Arc<moka::sync::Cache<(CommunityId, Uuid, Vec<u8>), bool>>,
    /// Accessible channel IDs cache: (community_id, pubkey_bytes) → channel UUIDs.
    /// Short TTL (10s) — invalidated on membership or channel visibility changes.
    #[allow(clippy::type_complexity)]
    pub accessible_channels_cache: Arc<moka::sync::Cache<(CommunityId, Vec<u8>), Vec<Uuid>>>,
    /// Per-community channel visibility string, used to gate the private-channel fan-out
    /// access check so open channels stay zero-cost. Invalidated on a flip.
    pub channel_visibility_cache: Arc<moka::sync::Cache<(CommunityId, Uuid), String>>,

    /// Bounded channel for audit logging, absent when audit logging is disabled.
    pub audit_tx: Option<mpsc::Sender<buzz_audit::NewAuditEntry>>,
    /// Media storage client (S3/MinIO).
    pub media_storage: Arc<MediaStorage>,
    /// Single-flight + cache state for the hourly S3 storage sweep. See
    /// `storage_sweep` module docs; shared with the usage-metrics tick via
    /// `Arc` the same way other cross-tick poller state lives on `AppState`.
    pub storage_sweep: Arc<tokio::sync::Mutex<crate::storage_sweep::StorageSweepState>>,
    /// Git object-store backend (content-addressed packs/manifests plus
    /// CAS-guarded manifest pointer). This is the durable git source of truth;
    /// see `api::git::store` and `docs/git-on-object-storage.md`.
    pub git_store: crate::api::git::store::GitStore,
    /// Process-local, byte-bounded cache of immutable Git pack/index pairs.
    /// Object storage remains authoritative; this only avoids repeated reads
    /// and index generation for content-addressed packs.
    pub git_pack_cache: Arc<crate::api::git::pack_cache::GitPackCache>,
    /// Audio relay room manager — tracks active huddle audio rooms.
    pub audio_rooms: Arc<AudioRoomManager>,
    /// Set to `true` on SIGTERM — readiness probe returns 503.
    pub shutting_down: Arc<AtomicBool>,
    /// Orders readiness gauge publication against terminal shutdown.
    pub(crate) readiness: Arc<crate::readiness::ReadinessCoordinator>,
    /// Process start time — used by `/_status` endpoint.
    pub started_at: Instant,
    /// Shared, community-scoped NIP-98 replay prevention.
    ///
    /// Correctness boundary for stateless workers: every pod must consult the
    /// same Redis `SET NX EX` seen-set, keyed by resolved community. Do not
    /// replace this with process-local caching; replay freshness must survive
    /// cross-pod routing.
    pub nip98_replay: Arc<dyn Nip98ReplayGuard>,
    /// Shared HTTP client for relay-proxied GIF provider requests. Reusing the
    /// connection pool avoids a fresh TLS handshake for every search/share.
    pub gif_http_client: reqwest::Client,
    /// Shared Redis-backed admission limits for ordinary HTTP and WebSocket work.
    pub admission_rate_limiter: Arc<RedisRateLimiter>,

    /// Per-agent sliding-window rate limiter for observer frames (kind 24200).
    /// Key: (community_id, agent pubkey bytes). Value: (count, window_start).
    /// 100 events/sec per agent — prevents relay/DB pressure from bursty telemetry.
    pub observer_rate_limiter: Arc<ScopedRateLimiter>,
    /// Per-uploader sliding-window rate limiter for media upload starts.
    /// Key: (community_id, uploader pubkey bytes). Value: (count, window_start).
    pub media_upload_rate_limiter: Arc<ScopedRateLimiter>,
    /// Per-claimer fixed-window rate limiter for invite claim attempts
    /// (`POST /api/invites/claim`). Entries expire after the claim window and
    /// the cache has a hard capacity because pre-membership callers can cheaply
    /// generate fresh Nostr keys.
    pub invite_claim_rate_limiter:
        Arc<moka::sync::Cache<ScopedPubkeyKey, Arc<std::sync::atomic::AtomicU32>>>,
    /// Current in-flight media uploads per (community, uploader pubkey).
    pub media_uploads_in_flight: Arc<DashMap<ScopedPubkeyKey, u32>>,
    /// Cache for observer agent-owner authorization (kind 24200).
    /// Key: (community_id, agent_pubkey_bytes, owner_pubkey_bytes). Value: is_owner.
    /// `agent_owner_pubkey` is immutable inside one community, so a long TTL
    /// (5 min) is safe once the community label is part of the key.
    /// Prevents repeated DB lookups from bursty observer traffic.
    #[allow(clippy::type_complexity)]
    pub observer_owner_cache: Arc<moka::sync::Cache<(CommunityId, Vec<u8>, Vec<u8>), bool>>,
    /// Cache for the `author_type` metric label on the ingest path.
    /// Key: (community_id, author pubkey bytes). Value: is_agent
    /// (`users.agent_owner_pubkey IS NOT NULL`). The mapping is
    /// first-write-wins and set during auth before an agent's first event,
    /// so a short TTL only bounds staleness for the rare backfill race.
    pub author_type_cache: Arc<moka::sync::Cache<(CommunityId, Vec<u8>), bool>>,

    /// Runtime conformance tracer. Production binds [`crate::conformance::NoopTracer`]
    /// (zero cost). Conformance tests bind [`crate::conformance::JsonlTracer`] to
    /// record traces for replay against `docs/spec/MultiTenantRelay.tla`.
    /// See `crates/buzz-conformance/` and `crate::conformance` for the
    /// schema, emitter helpers, and the independent checker.
    pub tracer: Arc<dyn buzz_conformance::Tracer>,

    /// Inter-relay mesh handle, set once by `main.rs` after `mesh_boot` (never
    /// a constructor parameter, so `AppState::new` call sites are untouched).
    /// `None`/unset ⇒ mesh-off / single-instance: consumers must behave
    /// byte-identically to a relay without the mesh. Access via
    /// [`AppState::mesh`].
    pub mesh: Arc<std::sync::OnceLock<crate::mesh_boot::MeshHandle>>,
}

impl AppState {
    /// Constructs `AppState` from its component services.
    ///
    /// Returns `(state, audit_shutdown)`. The caller should call
    /// `audit_shutdown.drain().await` during graceful shutdown so queued
    /// audit entries are flushed before the process exits.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Config,
        db: Db,
        redis_pool: deadpool_redis::Pool,
        audit: impl Into<Option<AuditService>>,
        pubsub: Arc<PubSubManager>,
        auth: AuthService,
        search: SearchService,
        workflow_engine: Arc<WorkflowEngine>,
        relay_keypair: nostr::Keys,
        media_storage: MediaStorage,
    ) -> (Self, AuditShutdownHandle) {
        let max_connections = config.max_connections;
        let max_concurrent_handlers = config.max_concurrent_handlers;
        let relay_membership_fanout_batches =
            max_concurrent_handlers.clamp(1, RELAY_MEMBERSHIP_FANOUT_MAX_CONCURRENT_BATCHES);
        let search_arc = Arc::new(search);

        let audit_arc = audit.into().map(Arc::new);
        let (audit_tx, mut audit_rx) = mpsc::channel::<buzz_audit::NewAuditEntry>(1000);
        let audit_for_worker = audit_arc.clone();
        let audit_cancel = CancellationToken::new();
        let audit_cancel_worker = audit_cancel.clone();
        let audit_worker_handle = tokio::spawn(async move {
            let Some(audit_for_worker) = audit_for_worker else {
                audit_cancel_worker.cancelled().await;
                return;
            };
            // Normal operation: process entries as they arrive.
            loop {
                tokio::select! {
                    entry = audit_rx.recv() => {
                        match entry {
                            Some(entry) => log_audit_entry(&audit_for_worker, entry).await,
                            None => break, // channel closed
                        }
                    }
                    _ = audit_cancel_worker.cancelled() => {
                        // Close the receiver: rejects future sends and lets us
                        // drain everything already buffered without a race.
                        audit_rx.close();
                        break;
                    }
                }
            }
            // Drain: recv() returns buffered entries, then None once empty.
            let mut drained = 0u32;
            while let Some(entry) = audit_rx.recv().await {
                log_audit_entry(&audit_for_worker, entry).await;
                drained += 1;
            }
            if drained > 0 {
                tracing::info!(drained, "audit worker flushed remaining entries");
            }
            tracing::warn!("audit log worker exited (expected on shutdown)");
        });

        let git_max_concurrent_ops = config.git_max_concurrent_ops;
        let media_max_concurrent_uploads = config.media_max_concurrent_uploads;
        let git_store = crate::api::git::store::GitStore::new(
            &config.media.s3_endpoint,
            &config.media.s3_access_key,
            &config.media.s3_secret_key,
            &config.media.s3_bucket,
            &config.media.s3_region,
            config.media.s3_addressing_style,
        )
        .expect("media storage was already constructed with this S3 config");
        let git_pack_cache = Arc::new(
            crate::api::git::pack_cache::GitPackCache::new(
                &config.git_pack_cache_path,
                config.git_pack_cache_max_bytes,
                config.git_pack_cache_max_concurrent_populations,
            )
            .expect("git pack cache path must be available"),
        );
        let nip98_replay: Arc<dyn Nip98ReplayGuard> =
            Arc::new(RedisNip98ReplayGuard::new(redis_pool.clone()));
        let gif_http_client = crate::api::gifs::build_gif_http_client();
        let admission_rate_limiter = Arc::new(RedisRateLimiter::new(redis_pool.clone()));
        let audit_enabled = audit_arc.is_some();
        let state = Self {
            config: Arc::new(config),
            db,
            redis_pool,
            audit: audit_arc,
            pubsub,
            auth: Arc::new(auth),
            search: search_arc,
            sub_registry: Arc::new(SubscriptionRegistry::new()),
            conn_manager: Arc::new(ConnectionManager::new()),
            community_connections: Arc::new(CommunityConnectionRegistry::new()),
            community_revalidator_cancel: CancellationToken::new(),
            community_disconnect_publish_attempts: Arc::new(AtomicU64::new(0)),
            relay_membership_revalidation_lock: Arc::new(tokio::sync::Mutex::new(())),
            relay_membership_revalidation_pending: Arc::new(AtomicBool::new(false)),
            relay_membership_revalidation_cursor: Arc::new(AtomicU64::new(0)),
            relay_membership_revalidation_pending_groups: Arc::new(tokio::sync::Mutex::new(
                VecDeque::new(),
            )),
            conn_semaphore: Arc::new(Semaphore::new(max_connections)),
            handler_semaphore: Arc::new(Semaphore::new(max_concurrent_handlers)),
            relay_membership_fanout_semaphore: Arc::new(Semaphore::new(
                relay_membership_fanout_batches,
            )),
            git_semaphore: Arc::new(Semaphore::new(git_max_concurrent_ops)),
            media_upload_semaphore: Arc::new(Semaphore::new(media_max_concurrent_uploads)),
            workflow_engine,
            relay_keypair,
            huddle_liveness_generation: Uuid::new_v4(),

            local_event_ids: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(60))
                    .build(),
            ),
            membership_cache: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(10))
                    .support_invalidation_closures()
                    .build(),
            ),
            accessible_channels_cache: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(10))
                    .support_invalidation_closures()
                    .build(),
            ),
            channel_visibility_cache: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(10))
                    .support_invalidation_closures()
                    .build(),
            ),
            audit_tx: audit_enabled.then_some(audit_tx),
            media_storage: Arc::new(media_storage),
            storage_sweep: Arc::new(tokio::sync::Mutex::new(
                crate::storage_sweep::StorageSweepState::default(),
            )),
            git_store,
            git_pack_cache,
            audio_rooms: Arc::new(AudioRoomManager::new()),
            shutting_down: Arc::new(AtomicBool::new(false)),
            readiness: Arc::new(crate::readiness::ReadinessCoordinator::default()),
            started_at: Instant::now(),
            nip98_replay,
            gif_http_client,
            admission_rate_limiter,
            observer_rate_limiter: Arc::new(DashMap::new()),
            media_upload_rate_limiter: Arc::new(DashMap::new()),
            invite_claim_rate_limiter: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(crate::api::invites::CLAIM_RATE_CACHE_CAPACITY)
                    .time_to_live(crate::api::invites::CLAIM_RATE_WINDOW)
                    .build(),
            ),
            media_uploads_in_flight: Arc::new(DashMap::new()),
            observer_owner_cache: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(1_000)
                    .time_to_live(std::time::Duration::from_secs(300))
                    .build(),
            ),
            author_type_cache: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(300))
                    .build(),
            ),
            // Default to NoopTracer: production builds pay zero cost.
            // Conformance tests overwrite this with a JsonlTracer after
            // construction (see test helpers in
            // `crates/buzz-test-client` once those land).
            tracer: Arc::new(crate::conformance::NoopTracer),
            mesh: Arc::new(std::sync::OnceLock::new()),
        };
        (
            state,
            AuditShutdownHandle {
                cancel: audit_cancel,
                handle: audit_worker_handle,
            },
        )
    }

    /// Atomically closes readiness publication before exposing shutdown to
    /// the relay's other fast-path lifecycle checks.
    pub fn begin_shutdown(&self) {
        self.readiness.begin_shutdown();
        self.shutting_down.store(true, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn set_readiness_evaluator(
        &mut self,
        evaluator: Arc<dyn crate::readiness::ReadinessEvaluator>,
    ) {
        self.readiness = Arc::new(crate::readiness::ReadinessCoordinator::with_evaluator(
            evaluator,
        ));
    }

    /// Inter-relay mesh handle. `None` ⇒ mesh-off / single-instance: callers
    /// must no-op to today's behavior. Set once by `main.rs` after boot.
    pub fn mesh(&self) -> Option<&crate::mesh_boot::MeshHandle> {
        self.mesh.get()
    }

    /// Record an event ID as locally-published for dedup, scoped to the
    /// community it was fanned out in. Called before Redis publish so the
    /// multi-node consumer can skip the echo for *this* community only — a
    /// same-id event in another community is a distinct delivery and must not
    /// be suppressed.
    pub fn mark_local_event(&self, community: CommunityId, event_id: &nostr::EventId) {
        self.local_event_ids
            .insert((community, event_id.to_bytes()), ());
    }

    /// Check channel membership with a 10-second cache. Falls back to DB on miss.
    pub async fn is_member_cached(
        &self,
        community_id: CommunityId,
        channel_id: Uuid,
        pubkey: &[u8],
    ) -> Result<bool, buzz_db::DbError> {
        let key = (community_id, channel_id, pubkey.to_vec());
        if let Some(cached) = self.membership_cache.get(&key) {
            metrics::counter!("buzz_membership_cache_hits_total").increment(1);
            return Ok(cached);
        }
        metrics::counter!("buzz_membership_cache_misses_total").increment(1);
        let result = self.db.is_member(community_id, channel_id, pubkey).await?;
        self.membership_cache.insert(key, result);
        Ok(result)
    }

    /// Invalidate caches after a membership change (add/remove member).
    ///
    /// Drops the local moka entries AND fire-and-forget publishes the same drop
    /// to every other pod over Redis (see [`apply_cache_invalidation`]). The
    /// publish is spawned, not awaited: the local drop is already done, and a
    /// dropped publish is backstopped by the REQ denial-path DB confirmation.
    pub fn invalidate_membership(&self, tenant: &TenantContext, channel_id: Uuid, pubkey: &[u8]) {
        self.invalidate_membership_local(tenant.community(), channel_id, pubkey);
        self.spawn_cache_invalidation(
            tenant,
            CacheInvalidation::Membership {
                channel_id,
                pubkey: pubkey.to_vec(),
            },
        );
    }

    /// Local-only membership drop. The cross-pod consumer calls this directly so
    /// applying a received drop never re-publishes it.
    pub(crate) fn invalidate_membership_local(
        &self,
        community_id: CommunityId,
        channel_id: Uuid,
        pubkey: &[u8],
    ) {
        self.membership_cache
            .invalidate(&(community_id, channel_id, pubkey.to_vec()));
        self.accessible_channels_cache
            .invalidate(&(community_id, pubkey.to_vec()));
    }

    /// Invalidate all users' accessible-channels cache (e.g. new open channel created).
    pub fn invalidate_all_accessible_channels(&self, tenant: &TenantContext) {
        self.invalidate_all_accessible_channels_local(tenant.community());
        self.spawn_cache_invalidation(tenant, CacheInvalidation::AccessibleAll);
    }

    /// Local-only accessible-channels drop. See [`invalidate_membership_local`].
    pub(crate) fn invalidate_all_accessible_channels_local(&self, community_id: CommunityId) {
        if let Err(error) = self
            .accessible_channels_cache
            .invalidate_entries_if(move |(entry_community, _), _| *entry_community == community_id)
        {
            // AppState enables invalidation closures at construction time. If
            // that invariant ever regresses, prefer over-invalidating to
            // serving stale access state.
            tracing::error!(
                ?error,
                "community-scoped accessible-channel invalidation unavailable; falling back to full invalidation"
            );
            self.accessible_channels_cache.invalidate_all();
        }
    }

    /// Invalidate the cached visibility for a single channel (e.g. after a flip).
    pub fn invalidate_channel_visibility(&self, tenant: &TenantContext, channel_id: Uuid) {
        self.invalidate_channel_visibility_local(tenant.community(), channel_id);
        self.spawn_cache_invalidation(tenant, CacheInvalidation::Visibility { channel_id });
    }

    /// Local-only visibility drop. See [`invalidate_membership_local`].
    pub(crate) fn invalidate_channel_visibility_local(
        &self,
        community_id: CommunityId,
        channel_id: Uuid,
    ) {
        self.channel_visibility_cache
            .invalidate(&(community_id, channel_id));
    }

    /// Invalidate all caches after a channel is deleted.
    ///
    /// Channel deletion is a rare admin operation, but it is still tenant-local:
    /// a deletion in A must not flush B's cache entries. Predicate invalidation
    /// keeps the safety property that stale `is_member=true` entries for the
    /// deleted channel are removed without turning the cache drop into a
    /// cross-community signal.
    pub fn invalidate_channel_deleted(&self, tenant: &TenantContext) {
        self.invalidate_channel_deleted_local(tenant.community());
        self.spawn_cache_invalidation(tenant, CacheInvalidation::ChannelDeleted);
    }

    /// Local-only channel-deleted drop. See [`invalidate_membership_local`].
    pub(crate) fn invalidate_channel_deleted_local(&self, community_id: CommunityId) {
        if let Err(error) =
            self.membership_cache
                .invalidate_entries_if(move |(entry_community, _, _), _| {
                    *entry_community == community_id
                })
        {
            tracing::error!(
                ?error,
                "community-scoped membership invalidation unavailable; falling back to full invalidation"
            );
            self.membership_cache.invalidate_all();
        }
        if let Err(error) = self
            .accessible_channels_cache
            .invalidate_entries_if(move |(entry_community, _), _| *entry_community == community_id)
        {
            tracing::error!(
                ?error,
                "community-scoped accessible-channel invalidation unavailable; falling back to full invalidation"
            );
            self.accessible_channels_cache.invalidate_all();
        }
        if let Err(error) = self
            .channel_visibility_cache
            .invalidate_entries_if(move |(entry_community, _), _| *entry_community == community_id)
        {
            tracing::error!(
                ?error,
                "community-scoped visibility invalidation unavailable; falling back to full invalidation"
            );
            self.channel_visibility_cache.invalidate_all();
        }
    }

    /// Fire-and-forget publish of a cache-key drop to all other pods. Failures
    /// are logged and swallowed — the REQ denial-path DB confirmation is the
    /// backstop, so a missed publish degrades to a <=10s TTL wait, never a leak.
    fn spawn_cache_invalidation(&self, tenant: &TenantContext, invalidation: CacheInvalidation) {
        let pubsub = Arc::clone(&self.pubsub);
        let tenant = tenant.clone();
        tokio::spawn(async move {
            if let Err(e) = pubsub
                .publish_cache_invalidation(&tenant, &invalidation)
                .await
            {
                tracing::warn!("Failed to publish cache invalidation {invalidation:?}: {e}");
            }
        });
    }

    /// Apply a cache-key drop received from another pod. Calls the local-only
    /// drop variants so a received drop is never re-published (no fan-out loop).
    pub fn apply_cache_invalidation(
        &self,
        community_id: CommunityId,
        invalidation: CacheInvalidation,
    ) {
        match invalidation {
            CacheInvalidation::Membership { channel_id, pubkey } => {
                self.invalidate_membership_local(community_id, channel_id, &pubkey);
            }
            CacheInvalidation::AccessibleAll => {
                self.invalidate_all_accessible_channels_local(community_id);
            }
            CacheInvalidation::Visibility { channel_id } => {
                self.invalidate_channel_visibility_local(community_id, channel_id);
            }
            CacheInvalidation::ChannelDeleted => {
                self.invalidate_channel_deleted_local(community_id);
            }
        }
    }

    /// Remove every subscription held by one connection and clear the
    /// connection-local subscription map. The registry removal happens before
    /// any revocation publication so both local fan-out and a raced REQ see an
    /// empty live subscription set.
    async fn detach_connection_subscriptions(
        &self,
        conn_id: Uuid,
    ) -> Vec<crate::subscription::RemovedSubscription> {
        // Acquire the connection-local map before removing the registry
        // entry. If this future is cancelled while another handler owns the
        // map lock, the registry remains intact and a later retry still owns
        // the complete `RemovedSubscription` snapshot. Hold the guard across
        // the synchronous registry removal so no REQ can repopulate the map
        // between the two operations.
        if let Some(subscriptions) = self.conn_manager.subscriptions_for(conn_id) {
            let mut subscriptions = subscriptions.lock().await;
            let removed = self.sub_registry.remove_connection(conn_id);
            subscriptions.clear();
            removed
        } else {
            self.sub_registry.remove_connection(conn_id)
        }
    }

    /// Release the Redis topic retains belonging to a detached subscription
    /// snapshot. This is kept separate from detachment so a self-leave can
    /// queue its terminal event ACK and policy close before a large topic
    /// cleanup runs.
    async fn release_detached_subscription_topics(
        &self,
        tenant: &TenantContext,
        removed: &[crate::subscription::RemovedSubscription],
    ) {
        if removed.is_empty() {
            return;
        }

        #[cfg(test)]
        maybe_stall_detached_topic_release_for_test().await;

        for subscription in removed {
            if subscription.scope.is_global() {
                self.pubsub
                    .release_topic(tenant, buzz_pubsub::EventTopic::Global)
                    .await;
            }
            for &channel_id in subscription.scope.channel_ids() {
                self.pubsub
                    .release_topic(tenant, buzz_pubsub::EventTopic::Channel(channel_id))
                    .await;
            }
        }
    }

    /// Keep the detached topic snapshot alive in a task that is independent of
    /// the revocation caller. A bounded caller may be cancelled while Redis
    /// desired-topic mutation is waiting on its mutex; the detached registry
    /// state must still release every retain after that cancellation.
    fn spawn_detached_subscription_topic_release(
        &self,
        tenant: &TenantContext,
        removed: &[crate::subscription::RemovedSubscription],
    ) {
        if removed.is_empty() {
            return;
        }
        let state = self.clone();
        let tenant = tenant.clone();
        let removed = removed.to_vec();
        std::mem::drop(tokio::spawn(async move {
            state
                .release_detached_subscription_topics(&tenant, &removed)
                .await;
        }));
    }

    /// Remove every subscription held by one connection, clear the
    /// connection-local subscription map, and schedule release of its retained
    /// Redis topics. The registry removal happens before any revocation
    /// publication so both local fan-out and a raced REQ see an empty live
    /// subscription set.
    pub(crate) async fn evict_connection_subscriptions(
        &self,
        tenant: &TenantContext,
        conn_id: Uuid,
    ) -> Vec<crate::subscription::RemovedSubscription> {
        let removed = self.detach_connection_subscriptions(conn_id).await;
        self.spawn_detached_subscription_topic_release(tenant, &removed);
        removed
    }

    /// Snapshot and evict all local connections for a tenant-scoped pubkey.
    /// Policy close reasons are marked before the snapshot is released, while
    /// cancellation is deliberately deferred until final `CLOSED` frames have
    /// been queued.
    async fn prepare_pubkey_revocation(
        &self,
        tenant: &TenantContext,
        pubkey: &[u8],
        reason: &str,
        excluded_conn_id: Option<Uuid>,
        skip_excluded: bool,
        held_conn_id: Option<(Uuid, tokio::sync::OwnedMutexGuard<()>)>,
    ) -> Vec<PendingPubkeyRevocation> {
        let mut pending = Vec::new();
        let mut held_guard = held_conn_id;
        for conn_id in self
            .conn_manager
            .connection_ids_for_pubkey_or_owner_in_community(tenant.community(), pubkey)
        {
            if skip_excluded && excluded_conn_id == Some(conn_id) {
                continue;
            }
            let preheld_guard = (excluded_conn_id == Some(conn_id))
                .then(|| held_guard.take().map(|(_, guard)| guard))
                .flatten();
            if let Some(entry) = self
                .prepare_connection_revocation(
                    tenant,
                    conn_id,
                    reason,
                    excluded_conn_id == Some(conn_id),
                    preheld_guard,
                )
                .await
            {
                pending.push(entry);
            }
        }
        pending
    }

    /// Prepare one connection for a local policy revocation. Keeping this
    /// operation connection-id based lets the durable membership sweep close
    /// only the denied identity; selecting by pubkey would also close valid
    /// owner-attested agent sessions that happen to share that pubkey as an
    /// owner relationship.
    async fn prepare_connection_revocation_without_topic_release(
        &self,
        conn_id: Uuid,
        reason: &str,
        excluded: bool,
        preheld_guard: Option<tokio::sync::OwnedMutexGuard<()>>,
    ) -> Option<PendingPubkeyRevocation> {
        let revocation_guard = match preheld_guard {
            Some(guard) => guard,
            None => self.conn_manager.try_acquire_revocation_lock(conn_id)?,
        };
        let marked = if reason == RELAY_MEMBERSHIP_REVOKED_REASON {
            self.conn_manager.mark_relay_membership_revoked(conn_id)
        } else {
            self.conn_manager.mark_policy_close(conn_id, reason)
        };
        if !marked {
            return None;
        }
        let removed = self.detach_connection_subscriptions(conn_id).await;
        Some(PendingPubkeyRevocation {
            conn_id,
            excluded,
            removed,
            _revocation_guard: revocation_guard,
        })
    }

    /// Prepare one connection for a local policy revocation and schedule
    /// release of its retained Redis topics. Self-leave uses the paired
    /// `without_topic_release` variant for its origin so the terminal ACK and
    /// policy close do not wait behind topic cleanup.
    async fn prepare_connection_revocation(
        &self,
        tenant: &TenantContext,
        conn_id: Uuid,
        reason: &str,
        excluded: bool,
        preheld_guard: Option<tokio::sync::OwnedMutexGuard<()>>,
    ) -> Option<PendingPubkeyRevocation> {
        let pending = self
            .prepare_connection_revocation_without_topic_release(
                conn_id,
                reason,
                excluded,
                preheld_guard,
            )
            .await?;
        self.spawn_detached_subscription_topic_release(tenant, &pending.removed);
        Some(pending)
    }

    /// Queue final subscription closures and cancel the pending connections.
    /// A full control queue is safe because `mark_policy_close` has already
    /// installed the policy close fallback consumed by the writer.
    async fn finish_pubkey_revocation(
        &self,
        pending: Vec<PendingPubkeyRevocation>,
        event_id: &str,
        reason: &str,
        ack: Option<(Uuid, WsMessage)>,
        send_unsolicited_ack: bool,
    ) -> PubkeyRevocationFinish {
        let mut closed = 0;
        let mut ack_delivered = ack.is_none();
        for entry in pending {
            let ack_frame = ack.as_ref().and_then(|(ack_conn_id, frame)| {
                (entry.excluded && *ack_conn_id == entry.conn_id).then_some(frame)
            });
            if let Some(frame) = ack_frame {
                // The self-leave ACK is the one control frame that cannot be
                // replaced by a policy-close fallback. Wait briefly for the
                // writer to make room and report a terminal failure if it
                // remains unavailable.
                ack_delivered = self
                    .conn_manager
                    .send_control_bounded(
                        entry.conn_id,
                        frame.clone(),
                        REVOCATION_CONTROL_SEND_TIMEOUT,
                    )
                    .await;
            } else if send_unsolicited_ack {
                let frame = crate::protocol::RelayMessage::ok(event_id, false, reason);
                let _ = self
                    .conn_manager
                    .send_control(entry.conn_id, WsMessage::Text(frame.into()));
            }
            for subscription in entry.removed {
                // A self-leave response is an event-level ACK. Existing query
                // subscriptions still receive their own correlated CLOSED.
                let frame = crate::protocol::RelayMessage::closed(&subscription.sub_id, reason);
                let _ = self
                    .conn_manager
                    .send_control(entry.conn_id, WsMessage::Text(frame.into()));
            }
            self.conn_manager.cancel_connection(entry.conn_id);
            closed += 1;
        }
        if ack.is_some() && !ack_delivered {
            metrics::counter!("buzz_relay_revocation_ack_delivery_failures_total").increment(1);
            tracing::error!(
                event_id,
                "self-leave ACK could not be queued before the bounded control deadline"
            );
        }
        PubkeyRevocationFinish {
            closed,
            ack_delivered,
        }
    }

    /// Disconnect a tenant-scoped pubkey on this pod only. Used by the Redis
    /// consumer after a command has already been published by another pod.
    /// The local eviction is idempotent, so a command may safely be delivered
    /// more than once or after the origin pod already closed its sockets.
    pub async fn disconnect_pubkey_local(
        &self,
        tenant: &TenantContext,
        pubkey: &[u8],
        event_id: &str,
        reason: &str,
        excluded_conn_id: Option<Uuid>,
    ) -> usize {
        let pending = self
            .prepare_pubkey_revocation(tenant, pubkey, reason, excluded_conn_id, true, None)
            .await;
        self.finish_pubkey_revocation(pending, event_id, reason, None, true)
            .await
            .closed
    }

    /// Finalize a self-leave whose durable delete lost a race to another
    /// revocation. The caller holds the connection fence from before the
    /// membership delete; consuming it here closes the origin and queues the
    /// one correlated `OK false` without publishing a second cluster command.
    /// `None` means the connection deregistered while the race was resolved;
    /// `Some(false)` means finalization ran but the bounded ACK send failed.
    pub(crate) async fn finalize_fenced_connection_revocation(
        &self,
        tenant: &TenantContext,
        conn_id: Uuid,
        event_id: &str,
        reason: &str,
        ack: WsMessage,
        revocation_guard: tokio::sync::OwnedMutexGuard<()>,
    ) -> Option<bool> {
        let pending = self
            .prepare_connection_revocation(tenant, conn_id, reason, true, Some(revocation_guard))
            .await?;
        Some(
            self.finish_pubkey_revocation(
                vec![pending],
                event_id,
                reason,
                Some((conn_id, ack)),
                false,
            )
            .await
            .ack_delivered,
        )
    }

    /// Enforce a live ban or relay-membership revocation cluster-wide. The
    /// durable delete occurs before this call; local subscriptions are evicted
    /// first, then the tenant-scoped connection-control publication is awaited,
    /// and only then are affected sockets cancelled. Current writer-backed
    /// membership gates remain the backstop for offline or lagged subscribers.
    ///
    /// A zero Redis subscriber count is accepted: a single-pod deployment may
    /// have no remote subscriber, and the durable current-membership gate still
    /// denies any later request. Actual Redis errors are returned to the caller
    /// so the mutation surface cannot report propagation success.
    pub(crate) async fn disconnect_pubkey_clusterwide(
        &self,
        tenant: &TenantContext,
        pubkey: &[u8],
        event_id: &str,
        reason: &str,
    ) -> Result<usize, RevocationError> {
        self.disconnect_pubkey_clusterwide_inner(
            tenant,
            pubkey,
            event_id,
            reason,
            RevocationOptions::default(),
        )
        .await
    }

    /// Variant used by a WebSocket NIP-43 self-leave. The initiating socket is
    /// excluded from the remote-style `OK false` path and receives exactly one
    /// event-level response before its policy close: `OK true` when Redis
    /// publication succeeded, `OK false` when it failed.
    pub(crate) async fn disconnect_pubkey_clusterwide_for_leave(
        &self,
        tenant: &TenantContext,
        pubkey: &[u8],
        event_id: &str,
        reason: &str,
        leave: SelfLeaveRevocation,
    ) -> Result<usize, RevocationError> {
        self.disconnect_pubkey_clusterwide_inner(
            tenant,
            pubkey,
            event_id,
            reason,
            RevocationOptions {
                excluded_conn_id: Some(leave.conn_id),
                leave_success_message: Some(leave.success_message),
                held_conn_id: Some((leave.conn_id, leave.revocation_guard)),
            },
        )
        .await
    }

    async fn disconnect_pubkey_clusterwide_inner(
        &self,
        tenant: &TenantContext,
        pubkey: &[u8],
        event_id: &str,
        reason: &str,
        options: RevocationOptions,
    ) -> Result<usize, RevocationError> {
        // Self-leave has one request-scoped connection whose event ACK must be
        // queued promptly. Detach that origin first without awaiting Redis
        // topic cleanup, then let the other local sessions use the ordinary
        // revocation path in a bounded background pass. The durable writer
        // row remains the retry authority when that pass cannot finish.
        let is_self_leave =
            options.leave_success_message.is_some() && options.excluded_conn_id.is_some();
        let (origin_pending, origin_removed) = match (is_self_leave, options.excluded_conn_id) {
            (true, Some(origin_conn_id)) => {
                let origin_pending = self
                    .prepare_connection_revocation_without_topic_release(
                        origin_conn_id,
                        reason,
                        true,
                        options.held_conn_id.map(|(_, guard)| guard),
                    )
                    .await;
                let origin_removed = origin_pending
                    .as_ref()
                    .map(|pending| pending.removed.clone())
                    .unwrap_or_default();
                (origin_pending, origin_removed)
            }
            _ => (None, Vec::new()),
        };

        let pending = if is_self_leave {
            let bulk_state = self.clone();
            let bulk_tenant = tenant.clone();
            let bulk_pubkey = pubkey.to_vec();
            let bulk_event_id = event_id.to_owned();
            let bulk_reason = reason.to_owned();
            let excluded_conn_id = options.excluded_conn_id;
            std::mem::drop(tokio::spawn(async move {
                let cleanup_event_id = bulk_event_id.clone();
                let cleanup_reason = bulk_reason.clone();
                let cleanup = async move {
                    #[cfg(test)]
                    maybe_stall_self_leave_bulk_for_test().await;

                    let pending = bulk_state
                        .prepare_pubkey_revocation(
                            &bulk_tenant,
                            &bulk_pubkey,
                            &cleanup_reason,
                            excluded_conn_id,
                            true,
                            None,
                        )
                        .await;
                    bulk_state
                        .finish_pubkey_revocation(
                            pending,
                            &cleanup_event_id,
                            &cleanup_reason,
                            None,
                            true,
                        )
                        .await
                        .closed
                };

                match tokio::time::timeout(
                    RELAY_MEMBERSHIP_LEAVE_LOCAL_FINALIZATION_TIMEOUT,
                    cleanup,
                )
                .await
                {
                    Ok(closed) => {
                        tracing::debug!(
                            event_id = %bulk_event_id,
                            closed,
                            "completed bounded local self-leave cleanup"
                        );
                    }
                    Err(_) => {
                        metrics::counter!("buzz_relay_self_leave_local_cleanup_timeouts_total")
                            .increment(1);
                        tracing::warn!(
                            event_id = %bulk_event_id,
                            "local self-leave cleanup exceeded its bounded window; durable membership sweep will retry"
                        );
                    }
                }
            }));
            Vec::new()
        } else {
            // Ordinary revocations retain their existing synchronous ordering:
            // local subscriptions are detached before the control publish.
            self.prepare_pubkey_revocation(
                tenant,
                pubkey,
                reason,
                options.excluded_conn_id,
                false,
                None,
            )
            .await
        };

        // Topic release for the origin is intentionally fire-and-forget after
        // detachment and terminal frame queuing. The registry is already
        // empty, so no stale subscription can receive fan-out while this
        // in-memory Redis interest is being released.
        self.spawn_detached_subscription_topic_release(tenant, &origin_removed);

        let command = ConnControl::DisconnectPubkey {
            pubkey: pubkey.to_vec(),
            event_id: event_id.to_owned(),
            reason: reason.to_owned(),
            exclude_conn_id: options.excluded_conn_id,
        };
        let publish_result = match tokio::time::timeout(
            REVOCATION_REDIS_PUBLISH_TIMEOUT,
            self.pubsub.publish_conn_control(tenant, &command),
        )
        .await
        {
            Ok(Ok(subscriber_count)) => Ok(subscriber_count),
            Ok(Err(error)) => Err(RevocationError::Publish(error)),
            Err(_) => {
                metrics::counter!("buzz_relay_revocation_publish_timeouts_total").increment(1);
                tracing::error!(
                    event_id,
                    "connection-control publication exceeded the bounded deadline"
                );
                Err(RevocationError::PublishTimeout)
            }
        };

        let ack = match (options.leave_success_message, options.excluded_conn_id) {
            (Some(success), Some(conn_id)) => {
                let frame = match &publish_result {
                    Ok(_) => crate::protocol::RelayMessage::ok(event_id, true, &success),
                    Err(_) => crate::protocol::RelayMessage::ok(
                        event_id,
                        false,
                        "error: live membership revocation propagation failed",
                    ),
                };
                Some((conn_id, frame.into()))
            }
            _ => None,
        };
        let finish = if let Some(origin_pending) = origin_pending {
            self.finish_pubkey_revocation(vec![origin_pending], event_id, reason, ack, false)
                .await
        } else {
            self.finish_pubkey_revocation(pending, event_id, reason, ack, true)
                .await
        };

        let _subscriber_count = publish_result?;
        if !finish.ack_delivered {
            return Err(RevocationError::AckDeliveryTimeout);
        }
        Ok(finish.closed)
    }

    /// Disconnect a community locally and publish the command to every relay pod.
    ///
    /// Publication is awaited so the archive API can distinguish durable state
    /// from propagation completion and offer a retryable response on failure.
    pub async fn disconnect_community_clusterwide(
        &self,
        tenant: &TenantContext,
    ) -> Result<usize, buzz_pubsub::PubSubError> {
        let closed = self
            .community_connections
            .disconnect_community(tenant.community());
        self.community_disconnect_publish_attempts
            .fetch_add(1, Ordering::Relaxed);
        self.pubsub
            .publish_conn_control(tenant, &ConnControl::DisconnectCommunity)
            .await?;
        Ok(closed)
    }

    /// Revalidate all communities with live sockets and cancel inactive ones.
    ///
    /// This is the durable backstop for Redis pub/sub's lossy offline-subscriber
    /// semantics: a pod that missed a successful publish eventually observes the
    /// archived row directly.
    pub async fn revalidate_live_communities(&self) -> usize {
        let (closed, failures) =
            revalidate_registered_communities(&self.community_connections, |community_id| {
                self.db.is_community_active_for_maintenance(community_id)
            })
            .await;
        for (community_id, error) in failures {
            tracing::warn!(%community_id, %error, "community lifecycle revalidation failed; retaining its sockets until next tick");
        }
        closed
    }

    /// Reconcile every authenticated local WebSocket against the writer-backed
    /// relay-membership roster. This is the durable backstop for a Redis
    /// disconnect, a broadcast receiver with no listeners, or a lagged
    /// connection-control consumer: a removed row eventually closes the idle
    /// socket even when no imperative command reached this process.
    ///
    /// Identity checks are deduplicated per `(community, pubkey, owner)` so a
    /// client with many subscriptions does not amplify database work. A DB
    /// error leaves the socket in place for the next bounded retry; it never
    /// turns an unavailable roster read into an authoritative revoke.
    pub async fn revalidate_live_relay_memberships(&self) -> usize {
        if !self.config.require_relay_membership {
            return 0;
        }

        run_coalesced_membership_revalidation(
            &self.relay_membership_revalidation_lock,
            &self.relay_membership_revalidation_pending,
            || self.revalidate_live_relay_memberships_once(),
        )
        .await
    }

    /// Run one bounded membership reconciliation pass. The caller supplies the
    /// single-flight fence; keeping the pass separate makes the finite work
    /// and its timeout behavior independently testable.
    async fn revalidate_live_relay_memberships_once(&self) -> usize {
        self.revalidate_live_relay_memberships_once_with_lookup(
            |community_id, pubkey, owner| async move {
                crate::api::relay_members::current_relay_membership_for_auth(
                    self,
                    community_id,
                    &pubkey,
                    owner.as_deref(),
                )
                .await
            },
        )
        .await
    }

    /// Execute one bounded reconciliation pass with an injected writer lookup.
    ///
    /// Production calls this with
    /// [`crate::api::relay_members::current_relay_membership_for_auth`]. The
    /// injected seam is kept at the executor boundary so tests exercise the
    /// same identity snapshot, fair planner, retry queue, revocation fence,
    /// subscription eviction, and terminal cancellation used in production;
    /// only the external writer result is controlled by the test.
    async fn revalidate_live_relay_memberships_once_with_lookup<Lookup, CheckFuture, LookupError>(
        &self,
        lookup: Lookup,
    ) -> usize
    where
        Lookup: Fn(CommunityId, Vec<u8>, Option<Vec<u8>>) -> CheckFuture,
        CheckFuture: Future<Output = Result<bool, LookupError>>,
    {
        let mut grouped: HashMap<RelayMembershipIdentity, Vec<Uuid>> = HashMap::new();
        for community_id in self.conn_manager.per_community_ws_connections().into_keys() {
            for (conn_id, pubkey, owner) in self
                .conn_manager
                .authenticated_identities_in_community(community_id)
            {
                grouped
                    .entry((community_id, pubkey, owner))
                    .or_default()
                    .push(conn_id);
            }
        }

        // Retry deferred work before advancing the fresh cursor window. Use
        // current connection IDs for a still-live identity so a reconnect can
        // replace stale IDs while a disconnected socket is simply discarded.
        // The shared planner reserves a fresh slice whenever both classes are
        // present, so a persistent writer failure cannot starve new sockets.
        let (selected, deferred) = {
            let mut retry_queue = self
                .relay_membership_revalidation_pending_groups
                .lock()
                .await;
            let queued = retry_queue.drain(..).collect::<Vec<_>>();
            select_membership_revalidation_work(
                &mut grouped,
                queued,
                &self.relay_membership_revalidation_cursor,
            )
        };
        if selected.is_empty() {
            self.retain_relay_membership_revalidation_groups(deferred)
                .await;
            return 0;
        }

        let batch = run_bounded_membership_lookups(
            selected,
            lookup,
            RELAY_MEMBERSHIP_LOOKUP_TIMEOUT,
            RELAY_MEMBERSHIP_SWEEP_DEADLINE,
            RELAY_MEMBERSHIP_SWEEP_CONCURRENCY,
        )
        .await;

        let event_id = "0".repeat(64);
        let mut pending = Vec::new();
        let mut retry = deferred;
        retry.extend(batch.unresolved);
        for ((community_id, pubkey, owner), conn_ids, outcome) in batch.outcomes {
            match outcome {
                MembershipLookupOutcome::Allowed => {}
                MembershipLookupOutcome::Denied => {
                    let tenant = TenantContext::resolved(community_id, "membership-reconciler");
                    let mut retry_conn_ids = Vec::new();
                    for conn_id in conn_ids {
                        if let Some(entry) = self
                            .prepare_connection_revocation(
                                &tenant,
                                conn_id,
                                RELAY_MEMBERSHIP_REVOKED_REASON,
                                false,
                                None,
                            )
                            .await
                        {
                            pending.push(entry);
                        } else if self.conn_manager.has_connection(conn_id) {
                            // A competing self-leave or control revocation owns
                            // the fence. Keep this identity for a later pass so
                            // the bounded sweep cannot silently skip it.
                            retry_conn_ids.push(conn_id);
                        }
                    }
                    if !retry_conn_ids.is_empty() {
                        retry.push(((community_id, pubkey, owner), retry_conn_ids));
                    }
                }
                MembershipLookupOutcome::Failed => {
                    metrics::counter!("buzz_relay_membership_revalidation_lookup_failures_total")
                        .increment(1);
                    tracing::warn!(
                        %community_id,
                        "live relay-membership reconciliation failed; retaining identity for the next bounded sweep"
                    );
                    retry.push(((community_id, pubkey, owner), conn_ids));
                }
                MembershipLookupOutcome::TimedOut => {
                    metrics::counter!("buzz_relay_membership_revalidation_lookup_timeouts_total")
                        .increment(1);
                    tracing::warn!(
                        %community_id,
                        "live relay-membership reconciliation timed out; retaining identity for the next bounded sweep"
                    );
                    retry.push(((community_id, pubkey, owner), conn_ids));
                }
            }
        }

        self.retain_relay_membership_revalidation_groups(retry)
            .await;

        self.finish_pubkey_revocation(
            pending,
            &event_id,
            RELAY_MEMBERSHIP_REVOKED_REASON,
            None,
            false,
        )
        .await
        .closed
    }

    /// Retain unresolved membership groups in a bounded FIFO. At most one
    /// group exists per authenticated identity, so the configured connection
    /// limit bounds this queue without discarding a live retry.
    async fn retain_relay_membership_revalidation_groups(&self, groups: Vec<RelayMembershipGroup>) {
        if groups.is_empty() {
            return;
        }
        let capacity = self.config.max_connections.max(1);
        let mut queue = self
            .relay_membership_revalidation_pending_groups
            .lock()
            .await;
        for (identity, conn_ids) in groups {
            if let Some((_, queued_conn_ids)) = queue
                .iter_mut()
                .find(|(queued_identity, _)| queued_identity == &identity)
            {
                *queued_conn_ids = conn_ids;
                continue;
            }
            debug_assert!(
                queue.len() < capacity,
                "membership retry queue exceeded the live connection bound"
            );
            if queue.len() >= capacity {
                metrics::counter!("buzz_relay_membership_revalidation_retry_queue_overflow_total")
                    .increment(1);
                tracing::error!(
                    capacity,
                    "membership retry queue reached its configured live-connection bound"
                );
                continue;
            }
            queue.push_back((identity, conn_ids));
        }
    }

    /// Get accessible channel IDs with a 10-second cache. Falls back to DB on miss.
    pub async fn get_accessible_channel_ids_cached(
        &self,
        community_id: CommunityId,
        pubkey: &[u8],
    ) -> Result<Vec<Uuid>, buzz_db::DbError> {
        let key = (community_id, pubkey.to_vec());
        if let Some(cached) = self.accessible_channels_cache.get(&key) {
            metrics::counter!("buzz_accessible_channels_cache_hits_total").increment(1);
            return Ok(cached);
        }
        metrics::counter!("buzz_accessible_channels_cache_misses_total").increment(1);
        let result = self
            .db
            .get_accessible_channel_ids(community_id, pubkey)
            .await?;
        self.accessible_channels_cache.insert(key, result.clone());
        Ok(result)
    }

    /// Channel visibility string. Caches only `private` (10s); never caches a
    /// non-private value.
    ///
    /// The fan-out access gate fails open on a non-private result, so a stale
    /// cached `open` on another node would mask the filter for the whole TTL
    /// after an open->private flip (no cross-node cache invalidation). Caching
    /// only `private` keeps the cache fail-safe: the worst stale entry is an
    /// over-restrictive `private` (drops non-members on a now-open channel for
    /// <=10s), never a leak.
    ///
    /// `prefetched` lets a caller that already holds the channel row for this
    /// request (ingest's once-per-request fetch, E1 §4.8) reuse it instead of
    /// re-SELECTing. The gate is unchanged: a cached `private` still wins over
    /// the prefetched row (the cache is fail-safe by design), and a `private`
    /// read from the row still populates the cache. With `Some(row)` this
    /// method performs no DB I/O and cannot error.
    pub async fn channel_visibility_cached(
        &self,
        community_id: CommunityId,
        channel_id: Uuid,
        prefetched: Option<&buzz_db::channel::ChannelRecord>,
    ) -> Result<String, buzz_db::DbError> {
        if let Some(cached) = self
            .channel_visibility_cache
            .get(&(community_id, channel_id))
        {
            return Ok(cached);
        }
        let visibility = match prefetched {
            Some(row) => row.visibility.clone(),
            None => {
                self.db
                    .get_channel(community_id, channel_id)
                    .await?
                    .visibility
            }
        };
        if visibility == "private" {
            self.channel_visibility_cache
                .insert((community_id, channel_id), visibility.clone());
        }
        Ok(visibility)
    }
}

/// A channel-visibility read resolved at ingest and threaded through to
/// fan-out within the same request (E1 phase-2, §4.8 phase-2 addendum).
///
/// The community and channel ids the visibility was resolved under travel
/// with the value so it can never be consulted for a different channel or
/// community's fan-out (channel UUIDs collide across communities —
/// `Inv_LabelPropagation`). Consumers must treat a missing/mismatched bundle
/// as "no threaded visibility" and fall back to a fresh fail-closed lookup —
/// never as "assume open".
#[derive(Debug, Clone)]
pub struct ThreadedChannelVisibility {
    /// Community the visibility was resolved under (server-resolved tenant).
    pub community_id: CommunityId,
    /// Channel the visibility was resolved for.
    pub channel_id: Uuid,
    /// The visibility string read at ingest (`"open"` / `"private"` / ...).
    pub visibility: String,
}

/// Handle for graceful audit worker shutdown.
///
/// Signals the worker to stop accepting new entries, drain its buffer,
/// and exit. Independent of `Arc<AppState>` lifetime — works even when
/// background tasks (reaper, pubsub, health) still hold state clones.
pub struct AuditShutdownHandle {
    cancel: CancellationToken,
    handle: JoinHandle<()>,
}

impl AuditShutdownHandle {
    /// Signal the audit worker to drain and wait up to `timeout` for it to finish.
    pub async fn drain(self, timeout: std::time::Duration) {
        self.cancel.cancel();
        match tokio::time::timeout(timeout, self.handle).await {
            Ok(Ok(())) => tracing::info!("Audit worker drained cleanly"),
            Ok(Err(e)) => tracing::error!("Audit worker panicked: {e}"),
            Err(_) => tracing::error!(
                ?timeout,
                "Audit worker did not drain in time — exiting anyway"
            ),
        }
    }
}

/// Log a single audit entry with metrics. Extracted so the normal loop
/// and the post-cancel drain share the same logic.
async fn log_audit_entry(audit: &buzz_audit::AuditService, entry: buzz_audit::NewAuditEntry) {
    let t = std::time::Instant::now();
    let mut retry_delay_ms = 50u64;
    let mut retries = 0u64;
    loop {
        match audit.log(entry.clone()).await {
            Ok(_) => {
                metrics::histogram!("buzz_audit_log_seconds").record(t.elapsed().as_secs_f64());
                return;
            }
            Err(buzz_audit::AuditError::Database(sqlx::Error::Database(database_error)))
                if database_error.code().as_deref() == Some("55P03") =>
            {
                retries += 1;
                metrics::counter!("buzz_audit_log_lock_retries_total").increment(1);
                tracing::warn!(
                    retries,
                    retry_delay_ms,
                    "Audit advisory lock timed out; preserving entry for retry"
                );
                tokio::time::sleep(std::time::Duration::from_millis(retry_delay_ms)).await;
                retry_delay_ms = (retry_delay_ms * 2).min(1_000);
            }
            Err(error) => {
                metrics::counter!("buzz_audit_log_errors_total").increment(1);
                tracing::error!("Audit log failed: {error}");
                return;
            }
        }
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("relay_url", &self.config.relay_url)
            .field("max_connections", &self.config.max_connections)
            .finish()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::connection::{AuthState, ConnectionState};
    use crate::protocol::RelayMessage;
    use std::collections::HashMap;
    use tokio::sync::{Mutex, RwLock};

    struct ActiveLookupProbe(Arc<AtomicU8>);

    impl Drop for ActiveLookupProbe {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Helper: create a ConnectionManager with one registered connection.
    /// Returns (manager, conn_id, receiver, ctrl_receiver, cancel,
    /// shared_backpressure_count).
    fn setup_conn(
        buffer_size: usize,
    ) -> (
        ConnectionManager,
        Uuid,
        mpsc::Receiver<WsMessage>,
        mpsc::Receiver<WsMessage>,
        CancellationToken,
        Arc<AtomicU8>,
    ) {
        let mgr = ConnectionManager::new();
        let conn_id = Uuid::new_v4();
        let (tx, rx) = mpsc::channel(buffer_size);
        let (ctrl_tx, ctrl_rx) = mpsc::channel(buffer_size);
        let cancel = CancellationToken::new();
        let bp = Arc::new(AtomicU8::new(0));
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::clone(&bp),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        (mgr, conn_id, rx, ctrl_rx, cancel, bp)
    }

    /// A relay state whose Redis is deliberately unreachable, so admission
    /// checks resolve to `AdmissionError::Unavailable` without any live
    /// infrastructure. Shared with `crate::rejection`'s tests.
    pub(crate) async fn test_state() -> Arc<AppState> {
        let mut config = crate::config::Config::from_env().expect("default config loads");
        config.require_relay_membership = false;
        config.redis_url = "redis://127.0.0.1:1".to_string();
        let pool = sqlx::PgPool::connect_lazy(&config.database_url).expect("lazy pg pool");
        let db = buzz_db::Db::from_pool(pool.clone());
        let redis_pool = deadpool_redis::Config::from_url(&config.redis_url)
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .expect("redis pool");
        let pubsub = Arc::new(
            buzz_pubsub::PubSubManager::new(&config.redis_url, redis_pool.clone())
                .await
                .expect("pubsub manager"),
        );
        let audit = buzz_audit::AuditService::new(pool.clone());
        let auth = buzz_auth::AuthService::new(config.auth.clone());
        let search = buzz_search::SearchService::new(pool.clone());
        let workflow_engine = Arc::new(buzz_workflow::WorkflowEngine::new(
            db.clone(),
            buzz_workflow::WorkflowConfig::default(),
        ));
        let media_storage = buzz_media::MediaStorage::new(&config.media).expect("media storage");
        let (state, _audit_shutdown) = AppState::new(
            config,
            db,
            redis_pool,
            audit,
            pubsub,
            auth,
            search,
            workflow_engine,
            nostr::Keys::generate(),
            media_storage,
        );
        Arc::new(state)
    }

    async fn audit_worker_retries_lock_timeout_until_original_entry_is_appended_once() {
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
        let observer = sqlx::PgPool::connect(&database_url)
            .await
            .expect("connect observer pool");
        let application_name = format!("audit-retry-test-{}", Uuid::new_v4());
        let hook_application_name = application_name.clone();
        let audit_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .after_connect(move |conn, _meta| {
                let application_name = hook_application_name.clone();
                Box::pin(async move {
                    sqlx::query(
                        "SELECT set_config('application_name', $1, false), \
                                set_config('lock_timeout', '100', false)",
                    )
                    .bind(application_name)
                    .execute(&mut *conn)
                    .await?;
                    Ok(())
                })
            })
            .connect(&database_url)
            .await
            .expect("connect audit pool");

        let community_id = Uuid::new_v4();
        sqlx::query("INSERT INTO communities (id, host) VALUES ($1, $2)")
            .bind(community_id)
            .bind(format!("audit-retry-{community_id}.example"))
            .execute(&observer)
            .await
            .expect("insert test community");
        let object_id = format!("audit-retry-object-{}", Uuid::new_v4());
        let entry = buzz_audit::NewAuditEntry {
            community_id: CommunityId::from_uuid(community_id),
            action: buzz_audit::AuditAction::EventCreated,
            actor_pubkey: Some(vec![0xab; 32]),
            object_id: Some(object_id.clone()),
            detail: serde_json::json!({"test": "lock-timeout-retry"}),
        };

        // Mirrors buzz_audit::service::AUDIT_LOCK_NAMESPACE.
        let lock_key = format!("buzz_audit:{community_id}");
        let mut holder = observer.acquire().await.expect("acquire lock holder");
        sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 0))")
            .bind(&lock_key)
            .execute(&mut *holder)
            .await
            .expect("hold community audit lock");

        let audit = Arc::new(AuditService::new(audit_pool));
        let worker = tokio::spawn({
            let audit = Arc::clone(&audit);
            async move { log_audit_entry(&audit, entry).await }
        });

        // Observe one timed-out advisory-lock attempt and then a second wait.
        // Releasing during the first wait would not prove that the worker
        // preserved and retried the original queue entry.
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            let mut saw_first_wait = false;
            let mut saw_retry_gap = false;
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (\
                         SELECT 1 FROM pg_stat_activity \
                         WHERE application_name = $1 \
                           AND query LIKE 'SELECT pg_advisory_lock%' \
                           AND wait_event = 'advisory'\
                     )",
                )
                .bind(&application_name)
                .fetch_one(&observer)
                .await
                .expect("inspect audit lock waiter");
                if waiting {
                    if saw_retry_gap {
                        break;
                    }
                    saw_first_wait = true;
                } else if saw_first_wait {
                    saw_retry_gap = true;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("audit worker never retried after lock_timeout");

        sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1, 0))")
            .bind(&lock_key)
            .execute(&mut *holder)
            .await
            .expect("release community audit lock");
        tokio::time::timeout(std::time::Duration::from_secs(3), worker)
            .await
            .expect("audit worker did not finish after lock release")
            .expect("audit worker task panicked");

        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_log WHERE community_id = $1 AND object_id = $2",
        )
        .bind(community_id)
        .bind(&object_id)
        .fetch_one(&observer)
        .await
        .expect("count retried audit rows");
        assert_eq!(rows, 1, "the preserved entry must be appended exactly once");

        sqlx::query("DELETE FROM audit_log WHERE community_id = $1 AND object_id = $2")
            .bind(community_id)
            .bind(&object_id)
            .execute(&observer)
            .await
            .expect("remove test audit row");
        sqlx::query("DELETE FROM communities WHERE id = $1")
            .bind(community_id)
            .execute(&observer)
            .await
            .expect("remove test community");
    }

    mod postgres_tests {
        #[tokio::test]
        #[ignore = "requires Postgres"]
        async fn audit_worker_retries_lock_timeout_until_original_entry_is_appended_once() {
            super::audit_worker_retries_lock_timeout_until_original_entry_is_appended_once().await;
        }
    }

    #[test]
    fn send_to_resets_grace_counter_on_success() {
        let (mgr, id, _rx, _ctrl_rx, _cancel, bp) = setup_conn(16);
        // Simulate prior backpressure.
        bp.store(2, Ordering::Relaxed);
        assert!(mgr.send_to(id, "hello".into()));
        assert_eq!(
            bp.load(Ordering::Relaxed),
            0,
            "successful send should reset counter"
        );
    }

    #[test]
    fn send_to_increments_grace_counter_on_full() {
        // Buffer size 1 — fill it, then the next send is Full.
        let (mgr, id, _rx, _ctrl_rx, cancel, bp) = setup_conn(1);
        assert!(mgr.send_to(id, "fill".into()));
        // Buffer is now full.
        assert!(!mgr.send_to(id, "overflow-1".into()));
        assert_eq!(bp.load(Ordering::Relaxed), 1, "first overflow → count=1");
        assert!(
            !cancel.is_cancelled(),
            "should not cancel on first overflow"
        );

        assert!(!mgr.send_to(id, "overflow-2".into()));
        assert_eq!(bp.load(Ordering::Relaxed), 2);
        assert!(
            !cancel.is_cancelled(),
            "should not cancel on second overflow"
        );
    }

    #[test]
    fn send_to_cancels_after_grace_limit() {
        let (mgr, id, _rx, _ctrl_rx, cancel, _bp) = setup_conn(1);
        assert!(mgr.send_to(id, "fill".into()));
        // Exhaust grace: 3 consecutive Full events (matches grace_limit=3 from setup_conn).
        for _ in 0..3u8 {
            mgr.send_to(id, "overflow".into());
        }
        assert!(
            cancel.is_cancelled(),
            "should cancel after grace_limit overflows"
        );
    }

    #[test]
    fn shared_counter_between_direct_and_fanout() {
        // Verify that ConnectionState::send() and ConnectionManager::send_to()
        // share the same backpressure counter via Arc<AtomicU8>.
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let bp = Arc::new(AtomicU8::new(0));

        let conn = ConnectionState {
            conn_id,
            tenant: buzz_core::tenant::TenantContext::resolved(
                buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
                "test.local".to_string(),
            ),
            remote_addr: "127.0.0.1:1234".parse().unwrap(),
            auth_state: RwLock::new(AuthState::Failed),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            send_tx: tx.clone(),
            ctrl_tx,
            cancel: cancel.clone(),
            backpressure_count: Arc::clone(&bp),
            grace_limit: 3,
        };

        let mgr = ConnectionManager::new();
        mgr.register(
            conn_id,
            tx,
            conn.ctrl_tx.clone(),
            None,
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::clone(&bp),
            Arc::clone(&conn.subscriptions),
            3,
        );

        // Fill the buffer via direct send.
        assert!(conn.send("fill".into()));
        // Overflow via fan-out.
        assert!(!mgr.send_to(conn_id, "overflow-fanout".into()));
        assert_eq!(
            bp.load(Ordering::Relaxed),
            1,
            "fan-out overflow increments shared counter"
        );
        // Overflow via direct send.
        assert!(!conn.send("overflow-direct".into()));
        assert_eq!(
            bp.load(Ordering::Relaxed),
            2,
            "direct overflow increments same counter"
        );
        // One more fan-out overflow → should cancel (3 consecutive).
        mgr.send_to(conn_id, "overflow-final".into());
        assert!(
            cancel.is_cancelled(),
            "shared counter reached limit via mixed path"
        );
    }

    #[tokio::test]
    async fn tracks_connections_by_authenticated_pubkey_within_community() {
        let mgr = ConnectionManager::new();
        let community_a = buzz_core::tenant::CommunityId::from_uuid(Uuid::from_u128(0xAAAA));
        let community_b = buzz_core::tenant::CommunityId::from_uuid(Uuid::from_u128(0xBBBB));
        let conn_a = Uuid::new_v4();
        let conn_b = Uuid::new_v4();
        let (tx_a, _rx_a) = mpsc::channel(1);
        let (ctrl_tx_a, _ctrl_rx_a) = mpsc::channel(1);
        let (tx_b, _rx_b) = mpsc::channel(1);
        let (ctrl_tx_b, _ctrl_rx_b) = mpsc::channel(1);
        mgr.register(
            conn_a,
            tx_a,
            ctrl_tx_a,
            None,
            CancellationToken::new(),
            community_a,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        mgr.register(
            conn_b,
            tx_b,
            ctrl_tx_b,
            None,
            CancellationToken::new(),
            community_b,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );

        let pubkey = vec![7u8; 32];
        mgr.set_authenticated_pubkey(conn_a, pubkey.clone());
        mgr.set_authenticated_pubkey(conn_b, pubkey.clone());

        assert_eq!(
            mgr.connection_ids_for_pubkey_in_community(community_a, &pubkey),
            vec![conn_a]
        );
        assert_eq!(
            mgr.connection_ids_for_pubkey_in_community(community_b, &pubkey),
            vec![conn_b]
        );
        assert!(mgr.subscriptions_for(conn_a).is_some());
        assert!(mgr.subscriptions_for(conn_b).is_some());
    }

    #[tokio::test]
    async fn pubkey_for_conn_returns_authenticated_pubkey() {
        let mgr = ConnectionManager::new();
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let bp = Arc::new(AtomicU8::new(0));
        let subscriptions = Arc::new(Mutex::new(HashMap::new()));
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel,
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            bp,
            subscriptions,
            3,
        );

        assert_eq!(mgr.pubkey_for_conn(conn_id), None);
        let pubkey = vec![9u8; 32];
        mgr.set_authenticated_pubkey(conn_id, pubkey.clone());
        assert_eq!(mgr.pubkey_for_conn(conn_id), Some(pubkey));
        assert_eq!(mgr.pubkey_for_conn(Uuid::new_v4()), None);
    }

    #[tokio::test]
    async fn accessible_channel_invalidation_is_scoped_to_community() {
        let state = test_state().await;
        let community_a = CommunityId::from_uuid(Uuid::from_u128(0xAAAA));
        let community_b = CommunityId::from_uuid(Uuid::from_u128(0xBBBB));
        let pubkey = vec![7u8; 32];
        let channels_a = vec![Uuid::from_u128(1)];
        let channels_b = vec![Uuid::from_u128(2)];

        state
            .accessible_channels_cache
            .insert((community_a, pubkey.clone()), channels_a);
        state
            .accessible_channels_cache
            .insert((community_b, pubkey.clone()), channels_b.clone());

        state.invalidate_all_accessible_channels_local(community_a);

        assert_eq!(
            state
                .accessible_channels_cache
                .get(&(community_a, pubkey.clone())),
            None
        );
        assert_eq!(
            state
                .accessible_channels_cache
                .get(&(community_b, pubkey.clone())),
            Some(channels_b),
            "A's cache drop must not evict B's accessible-channel entry"
        );
    }

    #[tokio::test]
    async fn channel_deleted_invalidation_is_scoped_to_community() {
        let state = test_state().await;
        let community_a = CommunityId::from_uuid(Uuid::from_u128(0xAAAA));
        let community_b = CommunityId::from_uuid(Uuid::from_u128(0xBBBB));
        let channel_id = Uuid::from_u128(1);
        let pubkey = vec![7u8; 32];

        for community in [community_a, community_b] {
            state
                .membership_cache
                .insert((community, channel_id, pubkey.clone()), true);
            state
                .accessible_channels_cache
                .insert((community, pubkey.clone()), vec![channel_id]);
            state
                .channel_visibility_cache
                .insert((community, channel_id), "private".to_string());
        }

        state.invalidate_channel_deleted_local(community_a);

        assert_eq!(
            state
                .membership_cache
                .get(&(community_a, channel_id, pubkey.clone())),
            None
        );
        assert_eq!(
            state
                .accessible_channels_cache
                .get(&(community_a, pubkey.clone())),
            None
        );
        assert_eq!(
            state
                .channel_visibility_cache
                .get(&(community_a, channel_id)),
            None
        );
        assert_eq!(
            state
                .membership_cache
                .get(&(community_b, channel_id, pubkey.clone())),
            Some(true)
        );
        assert_eq!(
            state
                .accessible_channels_cache
                .get(&(community_b, pubkey.clone())),
            Some(vec![channel_id])
        );
        assert_eq!(
            state
                .channel_visibility_cache
                .get(&(community_b, channel_id)),
            Some("private".to_string()),
            "A's channel deletion must not evict B's cache entries"
        );
    }

    #[test]
    fn community_lifecycle_disconnect_covers_socket_types_and_preserves_tenant_fence() {
        let registry = CommunityConnectionRegistry::new();
        let community_a = CommunityId::from_uuid(Uuid::from_u128(0xa));
        let community_b = CommunityId::from_uuid(Uuid::from_u128(0xb));
        let ordinary_a = CancellationToken::new();
        let audio_a = CancellationToken::new();
        let ordinary_b = CancellationToken::new();
        let ordinary_a_control = CommunityConnectionControl::new(ordinary_a.clone());
        let audio_a_control = CommunityConnectionControl::new(audio_a.clone());
        let ordinary_b_control = CommunityConnectionControl::new(ordinary_b.clone());
        let ordinary_a_reason = ordinary_a_control.disconnect_reason();
        let audio_a_reason = audio_a_control.disconnect_reason();
        let ordinary_b_reason = ordinary_b_control.disconnect_reason();
        let _ordinary_a_guard = registry.register(Uuid::new_v4(), community_a, ordinary_a_control);
        let _audio_a_guard = registry.register(Uuid::new_v4(), community_a, audio_a_control);
        let _ordinary_b_guard = registry.register(Uuid::new_v4(), community_b, ordinary_b_control);

        assert_eq!(registry.disconnect_community(community_a), 2);
        assert!(ordinary_a.is_cancelled());
        assert!(audio_a.is_cancelled());
        assert!(!ordinary_b.is_cancelled());
        assert_eq!(
            (*ordinary_a_reason.borrow()).clone(),
            Some(CommunityDisconnectReason::CommunityDeleted)
        );
        assert_eq!(
            (*audio_a_reason.borrow()).clone(),
            Some(CommunityDisconnectReason::CommunityDeleted)
        );
        assert_eq!((*ordinary_b_reason.borrow()).clone(), None);
    }

    #[tokio::test]
    async fn register_then_revalidate_closes_both_archive_race_orderings() {
        let registry = CommunityConnectionRegistry::new();
        let community = CommunityId::from_uuid(Uuid::from_u128(0xa));

        // Archive wins before durable revalidation: the check observes inactive
        // and the socket body never starts.
        let cancel_before = CancellationToken::new();
        let started_before = Arc::new(AtomicBool::new(false));
        let started_before_run = Arc::clone(&started_before);
        run_registered_community_connection(
            &registry,
            Uuid::new_v4(),
            community,
            CommunityConnectionControl::new(cancel_before.clone()),
            || async { Ok(false) },
            move |_| async move { started_before_run.store(true, Ordering::SeqCst) },
        )
        .await;
        assert!(cancel_before.is_cancelled());
        assert!(!started_before.load(Ordering::SeqCst));

        // Archive wins after registration but while revalidation is paused: its
        // sweep sees the token, and even an active query result cannot start the
        // socket body afterward.
        let cancel_during = CancellationToken::new();
        let registered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let registered_check = Arc::clone(&registered);
        let resume_check = Arc::clone(&resume);
        let started_during = Arc::new(AtomicBool::new(false));
        let started_during_run = Arc::clone(&started_during);
        let future = run_registered_community_connection(
            &registry,
            Uuid::new_v4(),
            community,
            CommunityConnectionControl::new(cancel_during.clone()),
            move || async move {
                registered_check.notify_one();
                resume_check.notified().await;
                Ok(true)
            },
            move |_| async move { started_during_run.store(true, Ordering::SeqCst) },
        );
        tokio::pin!(future);
        tokio::select! {
            _ = registered.notified() => {}
            _ = &mut future => panic!("revalidation should be paused"),
        }
        assert_eq!(registry.disconnect_community(community), 1);
        resume.notify_one();
        future.await;
        assert!(cancel_during.is_cancelled());
        assert!(!started_during.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn revalidation_continues_after_one_community_lookup_failure() {
        let registry = CommunityConnectionRegistry::new();
        let archived_a = CommunityId::from_uuid(Uuid::from_u128(0xa));
        let failed = CommunityId::from_uuid(Uuid::from_u128(0xb));
        let archived_c = CommunityId::from_uuid(Uuid::from_u128(0xc));
        let cancel_a = CancellationToken::new();
        let cancel_failed = CancellationToken::new();
        let cancel_c = CancellationToken::new();
        let _guard_a = registry.register(
            Uuid::new_v4(),
            archived_a,
            CommunityConnectionControl::new(cancel_a.clone()),
        );
        let _guard_failed = registry.register(
            Uuid::new_v4(),
            failed,
            CommunityConnectionControl::new(cancel_failed.clone()),
        );
        let _guard_c = registry.register(
            Uuid::new_v4(),
            archived_c,
            CommunityConnectionControl::new(cancel_c.clone()),
        );

        let (closed, failures) =
            revalidate_registered_communities(&registry, |community| async move {
                if community == failed {
                    Err(buzz_db::DbError::InvalidData(
                        "injected lookup failure".into(),
                    ))
                } else {
                    Ok(false)
                }
            })
            .await;

        assert_eq!(closed, 2);
        assert!(cancel_a.is_cancelled());
        assert!(!cancel_failed.is_cancelled());
        assert!(cancel_c.is_cancelled());
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, failed);
        assert_eq!(
            registry.bound_communities(),
            HashSet::from([archived_a, failed, archived_c])
        );
    }

    #[test]
    fn membership_revalidation_window_is_capped_and_rotates_fairly() {
        let community = CommunityId::from_uuid(Uuid::from_u128(0xbeef));
        let groups: Vec<(RelayMembershipIdentity, Vec<Uuid>)> = (0..600u16)
            .map(|index| {
                (
                    (
                        community,
                        [index.to_be_bytes().as_slice(), &[0; 30]].concat(),
                        None,
                    ),
                    vec![Uuid::from_u128(index as u128 + 1)],
                )
            })
            .collect();
        let cursor = AtomicU64::new(0);

        let first = select_bounded_membership_groups(groups.clone(), &cursor);
        let second = select_bounded_membership_groups(groups, &cursor);

        assert_eq!(first.len(), RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES);
        assert_eq!(second.len(), RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES);
        let first_ids: HashSet<_> = first
            .iter()
            .map(|(identity, _)| identity.1[..2].to_vec())
            .collect();
        let second_ids: HashSet<_> = second
            .iter()
            .map(|(identity, _)| identity.1[..2].to_vec())
            .collect();
        assert_eq!(
            first_ids.intersection(&second_ids).count(),
            424,
            "the second window advances by one full cap and wraps only for the remaining identities"
        );
        assert!(
            second_ids.contains(512u16.to_be_bytes().as_slice()),
            "the next sweep reaches identities beyond the first cap"
        );
    }

    #[tokio::test]
    async fn bounded_membership_lookups_limit_concurrency_and_preserve_timeouts() {
        let community = CommunityId::from_uuid(Uuid::from_u128(0xfeed));
        let groups = (0..8u8)
            .map(|index| {
                (
                    (community, vec![index; 32], None),
                    vec![Uuid::from_u128(index as u128 + 1)],
                )
            })
            .collect();
        let active = Arc::new(AtomicU8::new(0));
        let maximum = Arc::new(AtomicU8::new(0));
        let outcomes = run_bounded_membership_lookups(
            groups,
            {
                let active = Arc::clone(&active);
                let maximum = Arc::clone(&maximum);
                move |_community, pubkey, _owner| {
                    let active = Arc::clone(&active);
                    let maximum = Arc::clone(&maximum);
                    async move {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum.fetch_max(current, Ordering::SeqCst);
                        let _probe = ActiveLookupProbe(Arc::clone(&active));
                        if pubkey[0] < 4 {
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        } else {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        Ok::<bool, ()>(pubkey[0] >= 4)
                    }
                }
            },
            std::time::Duration::from_millis(20),
            std::time::Duration::from_millis(150),
            2,
        )
        .await;

        assert_eq!(
            outcomes.outcomes.len(),
            8,
            "each selected identity gets one bounded result"
        );
        assert_eq!(
            outcomes
                .outcomes
                .iter()
                .filter(|(_, _, outcome)| *outcome == MembershipLookupOutcome::Denied)
                .count(),
            4
        );
        assert_eq!(
            outcomes
                .outcomes
                .iter()
                .filter(|(_, _, outcome)| *outcome == MembershipLookupOutcome::TimedOut)
                .count(),
            4,
            "stalled lookups are classified as retryable instead of being revoked"
        );
        assert!(
            maximum.load(Ordering::SeqCst) <= 2,
            "lookup fan-out is capped"
        );
    }

    #[tokio::test]
    async fn bounded_membership_lookups_retain_unstarted_groups_after_deadline() {
        let community = CommunityId::from_uuid(Uuid::from_u128(0xcafe));
        let groups = (0..8u8)
            .map(|index| {
                (
                    (community, vec![index; 32], None),
                    vec![Uuid::from_u128(index as u128 + 1)],
                )
            })
            .collect();
        let batch = run_bounded_membership_lookups(
            groups,
            |_community, _pubkey, _owner| async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                Ok::<bool, ()>(true)
            },
            std::time::Duration::from_millis(200),
            std::time::Duration::from_millis(25),
            2,
        )
        .await;

        assert!(
            batch.outcomes.is_empty(),
            "the overall deadline expires before either in-flight lookup completes"
        );
        assert_eq!(
            batch.unresolved.len(),
            8,
            "both in-flight and never-started groups must remain retryable"
        );
    }

    #[tokio::test]
    async fn persistent_membership_failures_cannot_starve_fresh_revocation() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::from_u128(0xd00d));
        let register = |pubkey: Vec<u8>| {
            let conn_id = Uuid::new_v4();
            let (tx, _rx) = mpsc::channel(1);
            let (ctrl_tx, _ctrl_rx) = mpsc::channel(1);
            let cancel = CancellationToken::new();
            state.conn_manager.register(
                conn_id,
                tx,
                ctrl_tx,
                None,
                cancel.clone(),
                community,
                Arc::new(AtomicU8::new(0)),
                Arc::new(Mutex::new(HashMap::new())),
                3,
            );
            state.conn_manager.set_authenticated_pubkey(conn_id, pubkey);
            cancel
        };

        // Fill one complete bounded pass with identities whose writer lookup
        // will fail forever. The first pass must retain every one for retry.
        let mut failed_cancels = Vec::with_capacity(RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES);
        for index in 0..RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES {
            let [high, low] = (index as u16).to_be_bytes();
            let mut pubkey = vec![0u8; 32];
            pubkey[0] = high;
            pubkey[1] = low;
            failed_cancels.push(register(pubkey));
        }
        assert_eq!(
            state
                .revalidate_live_relay_memberships_once_with_lookup(
                    |_community, _pubkey, _owner| async {
                        Err::<bool, _>("persistent writer failure")
                    },
                )
                .await,
            0,
            "failed writer probes must remain retryable"
        );
        assert_eq!(
            state
                .relay_membership_revalidation_pending_groups
                .lock()
                .await
                .len(),
            RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES,
            "the complete failed window is retained for the next pass"
        );

        // A fresh revoked window arrives while the retry queue is full. The
        // second pass uses the same production planner and revocation executor;
        // its injected writer result denies every fresh identity that fits the
        // reserved slice. One tail identity must remain queued for a later
        // pass, which makes dropping unselected fresh work falsifiable too.
        let fresh_count = RELAY_MEMBERSHIP_SWEEP_RETRY_BUDGET + 1;
        let mut fresh_cancels = Vec::with_capacity(fresh_count);
        for index in 0..fresh_count {
            let [high, low] = (index as u16).to_be_bytes();
            let mut pubkey = vec![0u8; 32];
            pubkey[0] = 0xfe;
            pubkey[1] = high;
            pubkey[2] = low;
            fresh_cancels.push(register(pubkey));
        }
        let fresh_lookups = Arc::new(AtomicU64::new(0));
        let make_lookup = || {
            let fresh_lookups = Arc::clone(&fresh_lookups);
            move |_community: CommunityId, pubkey: Vec<u8>, _owner: Option<Vec<u8>>| {
                let is_fresh = pubkey.first() == Some(&0xfe);
                if is_fresh {
                    fresh_lookups.fetch_add(1, Ordering::SeqCst);
                }
                async move {
                    if is_fresh {
                        Ok::<bool, &'static str>(false)
                    } else {
                        Err::<bool, &'static str>("persistent writer failure")
                    }
                }
            }
        };
        let closed = state
            .revalidate_live_relay_memberships_once_with_lookup(make_lookup())
            .await;

        assert_eq!(
            closed, RELAY_MEMBERSHIP_SWEEP_RETRY_BUDGET,
            "fresh denied work must be revoked in this pass"
        );
        assert_eq!(
            fresh_lookups.load(Ordering::SeqCst),
            RELAY_MEMBERSHIP_SWEEP_RETRY_BUDGET as u64
        );
        assert_eq!(
            fresh_cancels
                .iter()
                .filter(|cancel| cancel.is_cancelled())
                .count(),
            RELAY_MEMBERSHIP_SWEEP_RETRY_BUDGET,
            "the reserved fresh slice must reach the production revocation executor"
        );
        assert!(
            fresh_cancels
                .iter()
                .filter(|cancel| !cancel.is_cancelled())
                .count()
                == 1,
            "one fresh tail remains retryable after the bounded pass"
        );
        assert!(
            failed_cancels.iter().all(|cancel| !cancel.is_cancelled()),
            "failed identities remain live until a writer result is available"
        );
        let pending = state
            .relay_membership_revalidation_pending_groups
            .lock()
            .await;
        assert_eq!(
            pending.len(),
            RELAY_MEMBERSHIP_SWEEP_MAX_IDENTITIES + 1,
            "unselected fresh work must be retained with the failed retry queue"
        );
        assert_eq!(
            pending
                .iter()
                .filter(|(identity, _)| identity.1.first() == Some(&0xfe))
                .count(),
            1,
            "the fresh tail must remain represented for a later pass"
        );
    }

    #[tokio::test]
    async fn membership_revalidation_single_flight_coalesces_one_follow_up() {
        let lock = tokio::sync::Mutex::new(());
        let pending = AtomicBool::new(false);
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let calls = Arc::new(AtomicU8::new(0));
        let first = run_coalesced_membership_revalidation(&lock, &pending, {
            let started = Arc::clone(&started);
            let release = Arc::clone(&release);
            let calls = Arc::clone(&calls);
            move || {
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                let calls = Arc::clone(&calls);
                async move {
                    let call = calls.fetch_add(1, Ordering::SeqCst);
                    if call == 0 {
                        started.notify_one();
                        release.notified().await;
                    }
                    1
                }
            }
        });
        tokio::pin!(first);
        tokio::select! {
            _ = started.notified() => {}
            _ = &mut first => panic!("first reconciliation should be paused in its work pass"),
        }

        assert_eq!(
            run_coalesced_membership_revalidation(&lock, &pending, || async { 99 }).await,
            0,
            "an overlapping trigger must return without running an unbounded second sweep"
        );
        assert!(pending.load(Ordering::SeqCst));

        release.notify_one();
        assert_eq!(
            first.await,
            2,
            "the active sweep consumes one coalesced retry"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn fenced_connection_is_deferred_without_an_unsolicited_ack() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let tenant = TenantContext::resolved(community, "test.local");
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        state.conn_manager.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        let pubkey = vec![0xabu8; 32];
        state
            .conn_manager
            .set_authenticated_pubkey(conn_id, pubkey.clone());
        let origin_guard = state
            .conn_manager
            .try_acquire_revocation_lock(conn_id)
            .expect("origin revocation fence");

        assert!(state
            .prepare_connection_revocation(
                &tenant,
                conn_id,
                RELAY_MEMBERSHIP_REVOKED_REASON,
                true,
                None,
            )
            .await
            .is_none());
        assert!(!cancel.is_cancelled());
        assert!(matches!(
            ctrl_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));

        drop(origin_guard);
        let pending = state
            .prepare_connection_revocation(
                &tenant,
                conn_id,
                RELAY_MEMBERSHIP_REVOKED_REASON,
                true,
                None,
            )
            .await
            .expect("unfenced connection can be prepared");
        assert_eq!(
            state
                .finish_pubkey_revocation(
                    vec![pending],
                    &"0".repeat(64),
                    RELAY_MEMBERSHIP_REVOKED_REASON,
                    None,
                    false,
                )
                .await
                .closed,
            1
        );
        assert!(cancel.is_cancelled());
        assert!(matches!(
            ctrl_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn fenced_self_leave_race_finalizes_origin_with_one_failure_ack() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let tenant = TenantContext::resolved(community, "test.local");
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        state.conn_manager.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        let guard = state
            .conn_manager
            .try_acquire_revocation_lock(conn_id)
            .expect("origin self-leave fence");
        let event_id = "3".repeat(64);
        let ack = RelayMessage::ok(
            &event_id,
            false,
            crate::handlers::ingest::RELAY_MEMBERSHIP_NOT_FOUND_MESSAGE,
        )
        .into();

        assert_eq!(
            state
                .finalize_fenced_connection_revocation(
                    &tenant,
                    conn_id,
                    &event_id,
                    RELAY_MEMBERSHIP_REVOKED_REASON,
                    ack,
                    guard,
                )
                .await,
            Some(true),
            "a registered origin must be finalized and receive its bounded ACK"
        );
        let frame = ctrl_rx.try_recv().expect("origin failure ACK is queued");
        assert!(matches!(
            frame,
            WsMessage::Text(ref text)
                if text.contains("OK")
                    && text.contains(&event_id)
                    && text.contains("false")
        ));
        assert!(matches!(
            ctrl_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        assert!(
            cancel.is_cancelled(),
            "the raced origin must be policy-closed"
        );
        state.conn_manager.deregister(conn_id);
    }

    #[test]
    fn community_lifecycle_guard_deregisters_on_early_return() {
        let registry = CommunityConnectionRegistry::new();
        let community = CommunityId::from_uuid(Uuid::from_u128(0xa));
        let cancel = CancellationToken::new();
        let guard = registry.register(
            Uuid::new_v4(),
            community,
            CommunityConnectionControl::new(cancel.clone()),
        );
        assert_eq!(registry.bound_communities(), HashSet::from([community]));

        drop(guard);

        assert!(registry.bound_communities().is_empty());
        assert_eq!(registry.disconnect_community(community), 0);
        assert!(!cancel.is_cancelled());
    }

    #[tokio::test]
    async fn disconnect_pubkey_closes_matching_conns_with_reason() {
        let (mgr, id, _rx, mut ctrl_rx, cancel, _bp) = setup_conn(8);
        let pubkey = vec![3u8; 32];
        mgr.set_authenticated_pubkey(id, pubkey.clone());

        // setup_conn registers the connection under the nil community.
        let community = buzz_core::tenant::CommunityId::from_uuid(Uuid::nil());
        let closed = mgr.disconnect_pubkey(
            community,
            &pubkey,
            "0".repeat(64).as_str(),
            "blocked: banned",
        );

        assert_eq!(closed, 1, "the one matching connection is closed");
        assert!(
            cancel.is_cancelled(),
            "connection is cancelled (socket close)"
        );
        // The reason frame is queued on the control channel ahead of the close.
        let frame = ctrl_rx.try_recv().expect("reason frame delivered");
        match frame {
            WsMessage::Text(t) => {
                assert!(t.as_str().contains("blocked: banned"), "carries the reason");
                assert!(t.as_str().contains("false"), "is an OK false frame");
            }
            other => panic!("expected text frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn disconnect_pubkey_marks_a_policy_close_for_the_writer_fallback() {
        let mgr = ConnectionManager::new();
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let control = CommunityConnectionControl::new(cancel.clone());
        let mut reason_rx = control.disconnect_reason();
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        mgr.set_disconnect_reason_sender(conn_id, control.disconnect_reason_sender());
        let pubkey = vec![9u8; 32];
        mgr.set_authenticated_pubkey(conn_id, pubkey.clone());

        assert_eq!(
            mgr.disconnect_pubkey(
                CommunityId::from_uuid(Uuid::nil()),
                &pubkey,
                &"0".repeat(64),
                RELAY_MEMBERSHIP_REVOKED_REASON,
            ),
            1
        );
        assert!(cancel.is_cancelled());
        assert_eq!(
            (*reason_rx.borrow_and_update()).clone(),
            Some(CommunityDisconnectReason::Policy {
                reason: RELAY_MEMBERSHIP_REVOKED_REASON.to_owned(),
            })
        );
    }

    #[tokio::test]
    async fn disconnect_pubkey_local_queues_rejection_before_cancellation() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let tenant = TenantContext::resolved(community, "test.local");
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        state.conn_manager.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        let pubkey = vec![8u8; 32];
        state
            .conn_manager
            .set_authenticated_pubkey(conn_id, pubkey.clone());

        assert_eq!(
            state
                .disconnect_pubkey_local(
                    &tenant,
                    &pubkey,
                    &"0".repeat(64),
                    RELAY_MEMBERSHIP_REVOKED_REASON,
                    None,
                )
                .await,
            1
        );
        let frame = ctrl_rx.try_recv().expect("local rejection is queued");
        assert!(matches!(frame, WsMessage::Text(ref text) if text.as_str().contains("false")));
        assert!(cancel.is_cancelled(), "local revocation cancels the socket");
    }

    #[tokio::test]
    async fn disconnect_pubkey_local_skips_the_excluded_origin_connection() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let tenant = TenantContext::resolved(community, "test.local");
        let pubkey = vec![6u8; 32];

        let register = |state: &AppState| {
            let conn_id = Uuid::new_v4();
            let (tx, _rx) = mpsc::channel(1);
            let (ctrl_tx, ctrl_rx) = mpsc::channel(4);
            let cancel = CancellationToken::new();
            state.conn_manager.register(
                conn_id,
                tx,
                ctrl_tx,
                None,
                cancel.clone(),
                community,
                Arc::new(AtomicU8::new(0)),
                Arc::new(Mutex::new(HashMap::new())),
                3,
            );
            state
                .conn_manager
                .set_authenticated_pubkey(conn_id, pubkey.clone());
            (conn_id, ctrl_rx, cancel)
        };

        let (origin_conn, mut origin_ctrl, origin_cancel) = register(&state);
        let (_other_conn, mut other_ctrl, other_cancel) = register(&state);

        assert_eq!(
            state
                .disconnect_pubkey_local(
                    &tenant,
                    &pubkey,
                    &"0".repeat(64),
                    RELAY_MEMBERSHIP_REVOKED_REASON,
                    Some(origin_conn),
                )
                .await,
            1
        );
        assert!(!origin_cancel.is_cancelled(), "origin socket stays live");
        assert!(matches!(
            origin_ctrl.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        assert!(other_cancel.is_cancelled(), "other session is revoked");
        let frame = other_ctrl.try_recv().expect("other rejection is queued");
        assert!(matches!(frame, WsMessage::Text(ref text) if text.as_str().contains("false")));
    }

    #[tokio::test]
    async fn self_leave_origin_finishes_while_bulk_cleanup_is_stalled() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let tenant = TenantContext::resolved(community, "test.local");
        let pubkey = vec![5u8; 32];

        let register = |state: &AppState| {
            let conn_id = Uuid::new_v4();
            let (tx, _rx) = mpsc::channel(1);
            let (ctrl_tx, ctrl_rx) = mpsc::channel(4);
            let cancel = CancellationToken::new();
            state.conn_manager.register(
                conn_id,
                tx,
                ctrl_tx,
                None,
                cancel.clone(),
                community,
                Arc::new(AtomicU8::new(0)),
                Arc::new(Mutex::new(HashMap::new())),
                3,
            );
            state
                .conn_manager
                .set_authenticated_pubkey(conn_id, pubkey.clone());
            (conn_id, ctrl_rx, cancel)
        };

        let (origin_conn, mut origin_ctrl, origin_cancel) = register(&state);
        let (other_conn, mut other_ctrl, other_cancel) = register(&state);
        let hook = Arc::new(SelfLeaveBulkTestHook {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        install_self_leave_bulk_test_hook(Some(Arc::clone(&hook))).await;

        let event_id = "4".repeat(64);
        let revocation_guard = state
            .conn_manager
            .try_acquire_revocation_lock(origin_conn)
            .expect("origin revocation fence");
        let finalize_state = Arc::clone(&state);
        let finalize_tenant = tenant.clone();
        let finalize_pubkey = pubkey.clone();
        let finalize_event_id = event_id.clone();
        let finalize = tokio::spawn(async move {
            finalize_state
                .disconnect_pubkey_clusterwide_for_leave(
                    &finalize_tenant,
                    &finalize_pubkey,
                    &finalize_event_id,
                    RELAY_MEMBERSHIP_REVOKED_REASON,
                    SelfLeaveRevocation {
                        conn_id: origin_conn,
                        success_message: "you left".to_owned(),
                        revocation_guard,
                    },
                )
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), hook.entered.notified())
            .await
            .expect("bulk cleanup reaches its bounded background seam");
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), finalize)
            .await
            .expect("self-leave finalizer must not wait for bulk cleanup")
            .expect("self-leave finalizer task");
        install_self_leave_bulk_test_hook(None).await;

        assert!(
            matches!(
                result,
                Err(RevocationError::Publish(_)) | Err(RevocationError::PublishTimeout)
            ),
            "the unreachable test Redis should make publication fail after origin finalization"
        );
        let frame = origin_ctrl
            .try_recv()
            .expect("origin failure ACK is queued");
        assert!(matches!(
            frame,
            WsMessage::Text(ref text)
                if text.contains(&event_id) && text.contains("OK") && text.contains("false")
        ));
        assert!(
            origin_cancel.is_cancelled(),
            "origin policy close is terminal"
        );
        assert!(
            !other_cancel.is_cancelled(),
            "the stalled bulk pass must not delay or preempt origin finalization"
        );

        hook.release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if other_cancel.is_cancelled() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("deferred bulk cleanup eventually revokes the other session");
        let frame = other_ctrl.try_recv().expect("other rejection is queued");
        assert!(matches!(frame, WsMessage::Text(ref text) if text.contains("false")));
        state.conn_manager.deregister(origin_conn);
        state.conn_manager.deregister(other_conn);
    }

    #[tokio::test]
    async fn revocation_preparation_does_not_wait_for_detached_topic_release() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let tenant = TenantContext::resolved(community, "test.local");
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let subscriptions = Arc::new(Mutex::new(HashMap::new()));
        state.conn_manager.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel,
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::clone(&subscriptions),
            3,
        );
        let pubkey = vec![0xabu8; 32];
        state
            .conn_manager
            .set_authenticated_pubkey(conn_id, pubkey.clone());
        let sub_id = "detached-release".to_owned();
        state
            .sub_registry
            .register_scoped(community, conn_id, sub_id.clone(), Vec::new(), None);
        subscriptions.lock().await.insert(sub_id, Vec::new());
        state
            .pubsub
            .retain_topic(&tenant, buzz_pubsub::EventTopic::Global)
            .await;
        assert_eq!(
            state
                .pubsub
                .topic_refcount(&tenant, buzz_pubsub::EventTopic::Global)
                .await,
            1
        );

        let hook = Arc::new(DetachedTopicReleaseTestHook {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        install_detached_topic_release_test_hook(Some(Arc::clone(&hook))).await;

        // The caller deadline models the self-leave bulk window. Before the
        // fix, detachment completed and this future then waited in the topic
        // release hook until the deadline cancelled it, losing the only
        // `RemovedSubscription` snapshot. The fixed path returns the pending
        // revocation while an independent task owns that snapshot.
        let prepared = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            state.prepare_pubkey_revocation(
                &tenant,
                &pubkey,
                RELAY_MEMBERSHIP_REVOKED_REASON,
                None,
                false,
                None,
            ),
        )
        .await;
        let release_started =
            tokio::time::timeout(std::time::Duration::from_secs(1), hook.entered.notified()).await;
        hook.release.notify_one();
        install_detached_topic_release_test_hook(None).await;

        assert!(
            release_started.is_ok(),
            "detached release task must reach the production topic-release seam"
        );
        let pending = prepared.expect(
            "revocation preparation must finish after handing topic release to an independent task",
        );
        assert_eq!(pending.len(), 1);
        state
            .finish_pubkey_revocation(
                pending,
                &"0".repeat(64),
                RELAY_MEMBERSHIP_REVOKED_REASON,
                None,
                false,
            )
            .await;

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if state
                    .pubsub
                    .topic_refcount(&tenant, buzz_pubsub::EventTopic::Global)
                    .await
                    == 0
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("detached topic release must finish after caller cancellation");
        state.conn_manager.deregister(conn_id);
    }

    #[tokio::test]
    async fn detach_cancellation_keeps_registry_snapshot_for_retry() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(4);
        let subscriptions = Arc::new(Mutex::new(HashMap::new()));
        state.conn_manager.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            CancellationToken::new(),
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::clone(&subscriptions),
            3,
        );
        let sub_id = "lock-stalled".to_owned();
        state
            .sub_registry
            .register_scoped(community, conn_id, sub_id.clone(), Vec::new(), None);
        let local_lock = subscriptions.lock().await;

        let cancelled = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            state.detach_connection_subscriptions(conn_id),
        )
        .await;
        assert!(cancelled.is_err(), "the local map lock must remain stalled");
        assert!(
            state.sub_registry.contains(conn_id, &sub_id),
            "cancellation while waiting for the local map must retain the registry snapshot"
        );

        drop(local_lock);
        let removed = state.detach_connection_subscriptions(conn_id).await;
        assert_eq!(removed.len(), 1);
        assert!(!state.sub_registry.contains(conn_id, &sub_id));
        state.conn_manager.deregister(conn_id);
    }

    #[test]
    fn owner_revocation_selection_includes_nip_oa_agent_sessions() {
        let mgr = ConnectionManager::new();
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let community = CommunityId::from_uuid(Uuid::nil());
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel,
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        let owner = vec![4u8; 32];
        mgr.set_authenticated_identity(conn_id, vec![5u8; 32], Some(owner.clone()));

        assert_eq!(
            mgr.connection_ids_for_pubkey_or_owner_in_community(community, &owner),
            vec![conn_id]
        );
    }

    #[test]
    fn owner_revocation_selection_ignores_direct_member_backfill_metadata() {
        let mgr = ConnectionManager::new();
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(1);
        let community = CommunityId::from_uuid(Uuid::nil());
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            CancellationToken::new(),
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        let owner = vec![4u8; 32];
        // A direct member may have the same owner persisted for receipts, but
        // that metadata is deliberately not copied into connection admission
        // provenance.
        mgr.set_authenticated_identity(conn_id, vec![5u8; 32], None);

        assert!(mgr
            .connection_ids_for_pubkey_or_owner_in_community(community, &owner)
            .is_empty());
    }

    #[tokio::test]
    async fn finish_pubkey_revocation_gives_self_leave_one_success_and_others_failure() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let event_id = "1".repeat(64);
        let reason = RELAY_MEMBERSHIP_REVOKED_REASON;

        let register = |state: &AppState| {
            let conn_id = Uuid::new_v4();
            let (tx, _rx) = mpsc::channel(1);
            let (ctrl_tx, ctrl_rx) = mpsc::channel(8);
            let cancel = CancellationToken::new();
            state.conn_manager.register(
                conn_id,
                tx,
                ctrl_tx,
                None,
                cancel.clone(),
                community,
                Arc::new(AtomicU8::new(0)),
                Arc::new(Mutex::new(HashMap::new())),
                3,
            );
            (conn_id, ctrl_rx, cancel)
        };

        let (self_conn, mut self_ctrl, self_cancel) = register(&state);
        let (other_conn, mut other_ctrl, other_cancel) = register(&state);
        let self_guard = state
            .conn_manager
            .try_acquire_revocation_lock(self_conn)
            .expect("self revocation fence");
        let other_guard = state
            .conn_manager
            .try_acquire_revocation_lock(other_conn)
            .expect("other revocation fence");
        let pending = vec![
            PendingPubkeyRevocation {
                conn_id: self_conn,
                excluded: true,
                removed: vec![crate::subscription::RemovedSubscription {
                    sub_id: "self-sub".to_owned(),
                    community_id: community,
                    scope: crate::subscription::SubscriptionScope::Global,
                }],
                _revocation_guard: self_guard,
            },
            PendingPubkeyRevocation {
                conn_id: other_conn,
                excluded: false,
                removed: Vec::new(),
                _revocation_guard: other_guard,
            },
        ];
        let success = crate::protocol::RelayMessage::ok(event_id.as_str(), true, "you left");
        let finish = state
            .finish_pubkey_revocation(
                pending,
                event_id.as_str(),
                reason,
                Some((self_conn, WsMessage::Text(success.into()))),
                true,
            )
            .await;
        assert_eq!(finish.closed, 2);
        assert!(finish.ack_delivered);

        let self_ack = self_ctrl.try_recv().expect("self ACK is queued");
        assert!(matches!(self_ack, WsMessage::Text(ref text) if text.as_str().contains("true")));
        let self_closed = self_ctrl.try_recv().expect("self subscription is closed");
        assert!(
            matches!(self_closed, WsMessage::Text(ref text) if text.as_str().contains("CLOSED") && text.as_str().contains("self-sub"))
        );
        let other_ack = other_ctrl
            .try_recv()
            .expect("other session gets a failure ACK");
        assert!(
            matches!(other_ack, WsMessage::Text(ref text) if text.as_str().contains("false") && text.as_str().contains(reason))
        );
        assert!(self_cancel.is_cancelled());
        assert!(other_cancel.is_cancelled());
    }

    #[tokio::test]
    async fn self_leave_ack_full_control_queue_is_bounded_and_reported() {
        let state = test_state().await;
        let community = CommunityId::from_uuid(Uuid::nil());
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        state.conn_manager.register(
            conn_id,
            tx,
            ctrl_tx.clone(),
            None,
            cancel.clone(),
            community,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        // Occupy the only control slot. The ACK path must wait only for its
        // bounded deadline, then return a terminal failure while still
        // cancelling the policy-revoked socket.
        ctrl_tx
            .try_send(WsMessage::Ping(axum::body::Bytes::new()))
            .expect("fill control queue");
        let revocation_guard = state
            .conn_manager
            .try_acquire_revocation_lock(conn_id)
            .expect("revocation fence");

        let started = std::time::Instant::now();
        let finish = state
            .finish_pubkey_revocation(
                vec![PendingPubkeyRevocation {
                    conn_id,
                    excluded: true,
                    removed: Vec::new(),
                    _revocation_guard: revocation_guard,
                }],
                &"2".repeat(64),
                RELAY_MEMBERSHIP_REVOKED_REASON,
                Some((
                    conn_id,
                    WsMessage::Text(
                        crate::protocol::RelayMessage::ok(&"2".repeat(64), true, "you left").into(),
                    ),
                )),
                true,
            )
            .await;

        assert!(!finish.ack_delivered);
        assert!(cancel.is_cancelled());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(matches!(ctrl_rx.try_recv(), Ok(WsMessage::Ping(_))));
    }

    #[tokio::test]
    async fn disconnect_pubkey_ignores_non_matching_conns() {
        let (mgr, id, _rx, _ctrl_rx, cancel, _bp) = setup_conn(8);
        mgr.set_authenticated_pubkey(id, vec![1u8; 32]);

        let community = buzz_core::tenant::CommunityId::from_uuid(Uuid::nil());
        let closed = mgr.disconnect_pubkey(
            community,
            &[2u8; 32],
            "0".repeat(64).as_str(),
            "blocked: banned",
        );

        assert_eq!(closed, 0, "no connection matches a different pubkey");
        assert!(!cancel.is_cancelled(), "unrelated connection stays live");
    }

    #[tokio::test]
    async fn disconnect_pubkey_is_fenced_to_the_banning_community() {
        // Same pubkey, two live sockets in two different communities on one pod.
        // A ban in community A must close only A's socket, never B's — the
        // tenant fence on live-disconnect fan-out (B1).
        let mgr = ConnectionManager::new();
        let pubkey = vec![7u8; 32];

        let community_a = buzz_core::tenant::CommunityId::from_uuid(Uuid::from_u128(0xa));
        let community_b = buzz_core::tenant::CommunityId::from_uuid(Uuid::from_u128(0xb));

        let register = |community| {
            let conn_id = Uuid::new_v4();
            let (tx, _rx) = mpsc::channel(8);
            let (ctrl_tx, _ctrl_rx) = mpsc::channel(8);
            let cancel = CancellationToken::new();
            mgr.register(
                conn_id,
                tx,
                ctrl_tx,
                None,
                cancel.clone(),
                community,
                Arc::new(AtomicU8::new(0)),
                Arc::new(Mutex::new(HashMap::new())),
                3,
            );
            mgr.set_authenticated_pubkey(conn_id, pubkey.clone());
            cancel
        };

        let cancel_a = register(community_a);
        let cancel_b = register(community_b);

        let closed = mgr.disconnect_pubkey(
            community_a,
            &pubkey,
            "0".repeat(64).as_str(),
            "blocked: banned",
        );

        assert_eq!(closed, 1, "only the community-A socket is closed");
        assert!(cancel_a.is_cancelled(), "community-A session is closed");
        assert!(
            !cancel_b.is_cancelled(),
            "community-B session stays live — ban does not cross the tenant fence"
        );
    }

    #[tokio::test]
    async fn drain_all_jittered_waits_for_writer_acknowledgement_without_cancelling() {
        let mgr = Arc::new(ConnectionManager::new());
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(8);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(8);
        let (restart_tx, mut restart_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            Some(restart_tx),
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );

        let drain_mgr = Arc::clone(&mgr);
        let drain = tokio::spawn(async move { drain_mgr.drain_all_jittered(1).await });
        let restart = restart_rx.recv().await.expect("restart command delivered");
        assert!(!drain.is_finished(), "drain waits for the writer flush");
        restart.flushed.send(true).expect("acknowledge flush");

        assert_eq!(drain.await.expect("drain task"), 1);
        assert!(
            !cancel.is_cancelled(),
            "successful flush does not use cancellation fallback"
        );
    }

    #[tokio::test]
    async fn drain_all_jittered_cancels_when_restart_channel_is_full_or_closed() {
        for keep_receiver in [true, false] {
            let mgr = ConnectionManager::new();
            let conn_id = Uuid::new_v4();
            let (tx, _rx) = mpsc::channel(8);
            let (ctrl_tx, _ctrl_rx) = mpsc::channel(8);
            let (restart_tx, restart_rx) = mpsc::channel(1);
            let (pending_tx, _pending_rx) = tokio::sync::oneshot::channel();
            if keep_receiver {
                restart_tx
                    .try_send(RestartClose {
                        flushed: pending_tx,
                    })
                    .expect("fill restart channel");
            } else {
                drop(restart_rx);
            }
            let cancel = CancellationToken::new();
            mgr.register(
                conn_id,
                tx,
                ctrl_tx,
                Some(restart_tx),
                cancel.clone(),
                buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
                Arc::new(AtomicU8::new(0)),
                Arc::new(Mutex::new(HashMap::new())),
                3,
            );

            assert_eq!(mgr.drain_all_jittered(1).await, 1);
            assert!(
                cancel.is_cancelled(),
                "unavailable writer cancels as fallback"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn drain_all_jittered_cancels_when_flush_ack_times_out() {
        // A writer that accepts the restart command but never acknowledges the
        // flush (e.g. wedged mid-send) must not stall the drain: after
        // RESTART_CLOSE_ACK_TIMEOUT the connection falls back to cancellation.
        let mgr = Arc::new(ConnectionManager::new());
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(8);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(8);
        let (restart_tx, mut restart_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            Some(restart_tx),
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );

        let drain_mgr = Arc::clone(&mgr);
        let drain = tokio::spawn(async move { drain_mgr.drain_all_jittered(1).await });
        // Take the restart command but hold the ack sender forever.
        let restart = restart_rx.recv().await.expect("restart command delivered");
        assert!(!drain.is_finished(), "drain waits on the ack timeout");
        // Advance past the 5s ack timeout under paused time.
        tokio::time::sleep(RESTART_CLOSE_ACK_TIMEOUT + std::time::Duration::from_millis(1)).await;

        assert_eq!(drain.await.expect("drain task"), 1);
        assert!(
            cancel.is_cancelled(),
            "an un-acknowledged flush falls back to cancellation"
        );
        drop(restart);
    }

    #[tokio::test]
    async fn drain_all_sends_restart_close_and_cancels_every_conn() {
        // Graceful shutdown must tell every live client to reconnect — across
        // all communities — with a 1012 restart close frame queued ahead of
        // the cancel-driven socket close.
        let mgr = ConnectionManager::new();

        let register = |community| {
            let conn_id = Uuid::new_v4();
            let (tx, _rx) = mpsc::channel(8);
            let (ctrl_tx, ctrl_rx) = mpsc::channel(8);
            let cancel = CancellationToken::new();
            mgr.register(
                conn_id,
                tx,
                ctrl_tx,
                None,
                cancel.clone(),
                community,
                Arc::new(AtomicU8::new(0)),
                Arc::new(Mutex::new(HashMap::new())),
                3,
            );
            (ctrl_rx, cancel)
        };

        let (mut ctrl_a, cancel_a) = register(buzz_core::tenant::CommunityId::from_uuid(
            Uuid::from_u128(0xa),
        ));
        let (mut ctrl_b, cancel_b) = register(buzz_core::tenant::CommunityId::from_uuid(
            Uuid::from_u128(0xb),
        ));

        let closed = mgr.drain_all();

        assert_eq!(closed, 2, "every connection is signalled, no tenant fence");
        assert!(cancel_a.is_cancelled(), "community-A session is cancelled");
        assert!(cancel_b.is_cancelled(), "community-B session is cancelled");

        for ctrl_rx in [&mut ctrl_a, &mut ctrl_b] {
            let frame = ctrl_rx.try_recv().expect("close frame delivered");
            match frame {
                WsMessage::Close(Some(close)) => {
                    assert_eq!(
                        close.code,
                        axum::extract::ws::close_code::RESTART,
                        "close code is 1012 Service Restart"
                    );
                    assert_eq!(close.reason.as_str(), "relay restarting");
                }
                other => panic!("expected a restart close frame, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn drain_all_full_control_buffer_still_cancels() {
        // Best-effort delivery: a wedged control channel must not block the
        // drain — the cancel still closes the socket, just without the frame.
        let mgr = ConnectionManager::new();
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(8);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        mgr.register(
            conn_id,
            tx,
            ctrl_tx.clone(),
            None,
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        // Wedge the 1-slot control channel.
        ctrl_tx
            .try_send(WsMessage::Text("wedge".into()))
            .expect("fill control channel");

        let closed = mgr.drain_all();

        assert_eq!(closed, 1);
        assert!(
            cancel.is_cancelled(),
            "cancel fires even when the close frame cannot be queued"
        );
        // Only the wedge frame is present — the close was dropped, not queued.
        assert!(matches!(
            ctrl_rx.try_recv().expect("wedge frame"),
            WsMessage::Text(_)
        ));
        assert!(ctrl_rx.try_recv().is_err(), "no second frame queued");
    }

    #[tokio::test]
    async fn register_after_drain_self_signals_restart_close_and_cancel() {
        // The shutdown-boundary race: an upgrade accepted before SIGTERM can
        // finish its async admission check and register AFTER drain_all's
        // one-shot snapshot. The sticky drain flag makes that interleaving
        // deterministic — register itself queues the 1012 and cancels, so no
        // late registration can ride out graceful shutdown unclosed.
        let mgr = ConnectionManager::new();

        // Drain with zero connections — sets the sticky flag.
        assert_eq!(mgr.drain_all(), 0);

        // Late registration lands after the snapshot.
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(8);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );

        assert!(
            cancel.is_cancelled(),
            "late registration is cancelled by the sticky drain flag"
        );
        match ctrl_rx.try_recv().expect("close frame delivered") {
            WsMessage::Close(Some(close)) => {
                assert_eq!(
                    close.code,
                    axum::extract::ws::close_code::RESTART,
                    "late registration still gets the 1012 restart close"
                );
                assert_eq!(close.reason.as_str(), "relay restarting");
            }
            other => panic!("expected a restart close frame, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn drain_all_is_immediate() {
        // The default (jitter-off) drain queues the frame and cancels
        // synchronously — the frame is present the moment drain_all() returns.
        let mgr = Arc::new(ConnectionManager::new());
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(8);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );

        let closed = mgr.drain_all();

        assert_eq!(closed, 1);
        assert!(cancel.is_cancelled(), "default drain cancels synchronously");
        assert!(
            matches!(
                ctrl_rx
                    .try_recv()
                    .expect("close frame delivered synchronously"),
                WsMessage::Close(Some(_))
            ),
            "the restart close is queued before drain_all() returns"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn drain_all_jittered_defers_close_until_within_jitter_window() {
        // With jitter, the close is deferred within the owned drain future.
        // The sticky drain flag is still set immediately, so a late
        // registration self-signals with no delay.
        let mgr = Arc::new(ConnectionManager::new());
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(8);
        let (ctrl_tx, mut ctrl_rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        mgr.register(
            conn_id,
            tx,
            ctrl_tx,
            None,
            cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );

        let jitter_ms = 20_000u64;
        // Poll the owned drain through its first await. Dropping this future
        // would drop the timers too; the shutdown path must retain and await it.
        let drain = mgr.drain_all_jittered(jitter_ms);
        tokio::pin!(drain);
        assert!(
            futures_util::poll!(&mut drain).is_pending(),
            "jittered drain remains pending while its timers are owned"
        );

        // Not closed yet — the delayed drain is parked on its timer.
        assert!(
            !cancel.is_cancelled(),
            "jittered close is deferred, not synchronous"
        );
        assert!(
            ctrl_rx.try_recv().is_err(),
            "no close frame queued before the delay elapses"
        );

        // A registration racing past the snapshot still self-signals at once,
        // regardless of jitter — clients arriving mid-shutdown are closed now.
        let late_id = Uuid::new_v4();
        let (late_tx, _late_rx) = mpsc::channel(8);
        let (late_ctrl_tx, mut late_ctrl_rx) = mpsc::channel(8);
        let late_cancel = CancellationToken::new();
        mgr.register(
            late_id,
            late_tx,
            late_ctrl_tx,
            None,
            late_cancel.clone(),
            buzz_core::tenant::CommunityId::from_uuid(Uuid::nil()),
            Arc::new(AtomicU8::new(0)),
            Arc::new(Mutex::new(HashMap::new())),
            3,
        );
        assert!(
            late_cancel.is_cancelled(),
            "late registration self-signals immediately, unaffected by jitter"
        );
        assert!(
            matches!(
                late_ctrl_rx.try_recv().expect("late close frame"),
                WsMessage::Close(Some(_))
            ),
            "late registration gets the restart close with no delay"
        );

        // Advance past the whole jitter window; awaiting the owned drain must
        // complete only after the deferred close has fired.
        tokio::time::advance(std::time::Duration::from_millis(jitter_ms + 1)).await;
        assert_eq!(drain.await, 1, "one captured connection drained");

        assert!(
            cancel.is_cancelled(),
            "the jittered connection is closed within the jitter window"
        );
        match ctrl_rx.try_recv().expect("deferred close frame delivered") {
            WsMessage::Close(Some(close)) => {
                assert_eq!(
                    close.code,
                    axum::extract::ws::close_code::RESTART,
                    "jittered close is still 1012 Service Restart"
                );
                assert_eq!(close.reason.as_str(), "relay restarting");
            }
            other => panic!("expected a restart close frame, got {other:?}"),
        }
    }
}
