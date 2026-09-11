//! Live relay-membership revocation coverage.
//!
//! The test runs against a membership-gated relay backed by the same
//! PostgreSQL writer that the relay uses. It deliberately removes the member
//! row outside the relay process, then checks the live NIP-42 lifecycle: an
//! existing idle subscription is policy-closed and a fresh connection is
//! denied. A separate Redis-control scenario closes a live socket while its
//! durable membership row remains present.
//!
//! The test is ignored by default because it needs PostgreSQL, Redis, and a
//! running relay with `BUZZ_REQUIRE_RELAY_MEMBERSHIP=true`.

use std::{
    future::Future,
    panic::{resume_unwind, AssertUnwindSafe},
    time::Duration,
};

use buzz_test_client::{BuzzTestClient, RelayMessage, TestClientError};
use futures_util::FutureExt;
use nostr::{Filter, Keys, Kind};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

const MEMBERSHIP_REVOKED: &str = "restricted: not a relay member";
const REDIS_CONTROL_EVENT_ID: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";
const DEPENDENCY_TIMEOUT: Duration = Duration::from_secs(10);

fn relay_url() -> String {
    std::env::var("RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_owned())
}

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_owned())
}

fn relay_host() -> String {
    let parsed = url::Url::parse(&relay_url()).expect("RELAY_URL must be a valid URL");
    let host = parsed.host_str().expect("RELAY_URL must include a host");
    match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

async fn db_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://buzz:buzz_dev@localhost:5432/buzz".to_owned());
    tokio::time::timeout(
        DEPENDENCY_TIMEOUT,
        PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url),
    )
    .await
    .expect("timed out connecting to integration PostgreSQL")
    .expect("connect to integration PostgreSQL")
}

async fn deployment_community(pool: &PgPool) -> Uuid {
    tokio::time::timeout(
        DEPENDENCY_TIMEOUT,
        sqlx::query_scalar("SELECT id FROM communities WHERE lower(host) = lower($1)")
            .bind(relay_host())
            .fetch_one(pool),
    )
    .await
    .expect("timed out looking up deployment community")
    .expect("deployment community must be seeded before the relay starts")
}

async fn add_member(pool: &PgPool, community_id: Uuid, keys: &Keys) {
    tokio::time::timeout(
        DEPENDENCY_TIMEOUT,
        sqlx::query(
            "INSERT INTO relay_members (community_id, pubkey, role, added_by)\
             VALUES ($1, $2, 'member', NULL)",
        )
        .bind(community_id)
        .bind(keys.public_key().to_hex())
        .execute(pool),
    )
    .await
    .expect("timed out inserting relay membership row")
    .expect("insert relay membership row");
}

async fn remove_member(pool: &PgPool, community_id: Uuid, keys: &Keys) {
    let result = tokio::time::timeout(
        DEPENDENCY_TIMEOUT,
        sqlx::query(
            "DELETE FROM relay_members WHERE community_id = $1 AND pubkey = $2 AND role = 'member'",
        )
        .bind(community_id)
        .bind(keys.public_key().to_hex())
        .execute(pool),
    )
    .await
    .expect("timed out removing relay membership row")
    .expect("remove relay membership row");
    assert_eq!(
        result.rows_affected(),
        1,
        "exactly one member row must be removed"
    );
}

async fn cleanup_member(pool: &PgPool, community_id: Uuid, keys: &Keys) -> Result<(), String> {
    tokio::time::timeout(
        DEPENDENCY_TIMEOUT,
        sqlx::query(
            "DELETE FROM relay_members WHERE community_id = $1 AND pubkey = $2 AND role = 'member'",
        )
        .bind(community_id)
        .bind(keys.public_key().to_hex())
        .execute(pool),
    )
    .await
    .map_err(|_| "timed out cleaning up relay membership row".to_owned())?
    .map_err(|error| format!("clean up relay membership row: {error}"))?;
    Ok(())
}

/// Run a membership scenario with a failure-safe database cleanup path.
///
/// The body owns its live clients, so unwinding drops their transports. On a
/// normal path each client uses [`disconnect_client`] for a bounded graceful
/// close; the database row is deleted in either case before a body panic is
/// rethrown.
async fn with_member_cleanup<Fut>(pool: &PgPool, community_id: Uuid, keys: &Keys, body: Fut)
where
    Fut: Future<Output = ()>,
{
    let body_result = AssertUnwindSafe(body).catch_unwind().await;
    if let Err(error) = cleanup_member(pool, community_id, keys).await {
        panic!("membership scenario cleanup failed: {error}");
    }
    if let Err(panic_payload) = body_result {
        resume_unwind(panic_payload);
    }
}

async fn disconnect_client(client: BuzzTestClient, label: &str) {
    tokio::time::timeout(DEPENDENCY_TIMEOUT, client.disconnect())
        .await
        .unwrap_or_else(|_| panic!("timed out disconnecting {label}"))
        .unwrap_or_else(|error| panic!("failed disconnecting {label}: {error}"));
}

async fn publish_disconnect_pubkey(community_id: Uuid, keys: &Keys, reason: &str) -> i64 {
    let client = redis::Client::open(redis_url()).expect("valid integration Redis URL");
    let mut connection = tokio::time::timeout(
        DEPENDENCY_TIMEOUT,
        client.get_multiplexed_async_connection(),
    )
    .await
    .expect("timed out connecting to integration Redis")
    .expect("connect to integration Redis");
    let channel = format!("buzz:{community_id}:conn-control");
    let payload = json!({
        "op": "DisconnectPubkey",
        "pubkey": keys.public_key().to_bytes().to_vec(),
        "event_id": REDIS_CONTROL_EVENT_ID,
        "reason": reason,
        "exclude_conn_id": null,
    })
    .to_string();

    // The membership relay is the only conn-control subscriber in its CI
    // phase. Retry until Redis reports that target consumer, so a startup race
    // cannot turn a lost command into a false readiness signal. The deadline
    // covers the whole retry sequence, including each PUBLISH attempt.
    let deadline = tokio::time::Instant::now() + DEPENDENCY_TIMEOUT;
    for _ in 0..100 {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .unwrap_or(Duration::ZERO);
        if remaining.is_zero() {
            break;
        }
        let subscribers: i64 = tokio::time::timeout(
            remaining.min(Duration::from_secs(2)),
            redis::cmd("PUBLISH")
                .arg(&channel)
                .arg(&payload)
                .query_async(&mut connection),
        )
        .await
        .expect("timed out publishing connection-control command")
        .expect("publish connection-control command");
        if subscribers == 1 {
            return subscribers;
        }
        let sleep_for = remaining.min(Duration::from_millis(100));
        if sleep_for.is_zero() {
            break;
        }
        tokio::time::sleep(sleep_for).await;
    }
    panic!(
        "Redis conn-control subscriber did not become ready on {channel} within {DEPENDENCY_TIMEOUT:?}"
    );
}

#[derive(Clone, Copy)]
enum ControlAck<'a> {
    Required { event_id: &'a str, reason: &'a str },
    Optional { event_id: &'a str, reason: &'a str },
}

async fn recv_closed(
    client: &mut BuzzTestClient,
    sub_id: &str,
    expected_ack: ControlAck<'_>,
) -> String {
    let deadline = tokio::time::Instant::now() + DEPENDENCY_TIMEOUT;
    let mut control_ok_seen = !matches!(expected_ack, ControlAck::Required { .. });
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .unwrap_or(Duration::ZERO);
        assert!(
            !remaining.is_zero(),
            "timed out waiting for CLOSED {sub_id}"
        );
        match client
            .recv_event(remaining.min(Duration::from_secs(2)))
            .await
            .expect("relay response while waiting for CLOSED")
        {
            RelayMessage::Ok(ok) => {
                let (expected_id, expected_reason) = match expected_ack {
                    ControlAck::Required { event_id, reason }
                    | ControlAck::Optional { event_id, reason } => (event_id, reason),
                };
                assert!(!ok.accepted, "revocation control ACK must be OK false");
                assert_eq!(ok.event_id, expected_id);
                assert_eq!(ok.message, expected_reason);
                control_ok_seen = true;
            }
            RelayMessage::Closed {
                subscription_id,
                message,
            } if subscription_id == sub_id => {
                assert!(
                    control_ok_seen,
                    "CLOSED must follow the required correlated connection-level OK"
                );
                return message;
            }
            RelayMessage::Notice { .. } | RelayMessage::Auth { .. } => {}
            other => panic!("unexpected response while waiting for CLOSED {sub_id}: {other:?}"),
        }
    }
}

async fn recv_eose(client: &mut BuzzTestClient, sub_id: &str) {
    let deadline = tokio::time::Instant::now() + DEPENDENCY_TIMEOUT;
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .unwrap_or(Duration::ZERO);
        assert!(!remaining.is_zero(), "timed out waiting for EOSE {sub_id}");
        match client
            .recv_event(remaining.min(Duration::from_secs(2)))
            .await
            .expect("relay response while waiting for EOSE")
        {
            RelayMessage::Eose { subscription_id } if subscription_id == sub_id => return,
            RelayMessage::Notice { .. } | RelayMessage::Auth { .. } => {}
            RelayMessage::Event {
                subscription_id, ..
            } if subscription_id == sub_id => {}
            other => panic!("unexpected response while waiting for EOSE {sub_id}: {other:?}"),
        }
    }
}

/// The periodic writer-backed sweep must close an already-authenticated idle
/// subscription after its member row is removed. The connection-level
/// acknowledgement and subscription CLOSED frame are both correlated before
/// the test accepts the policy reason.
async fn writer_membership_sweep_closes_idle_subscription_impl() {
    let url = relay_url();
    let pool = db_pool().await;
    let community_id = deployment_community(&pool).await;
    let member_keys = Keys::generate();
    add_member(&pool, community_id, &member_keys).await;

    with_member_cleanup(
        &pool,
        community_id,
        &member_keys,
        async {
            let mut live = BuzzTestClient::connect(&url, &member_keys)
                .await
                .expect("member must authenticate while its writer row exists");
            let live_sub = format!("membership-sweep-{}", Uuid::new_v4());
            live.subscribe(&live_sub, vec![Filter::new().kinds([Kind::TextNote])])
                .await
                .expect("open member subscription");
            recv_eose(&mut live, &live_sub).await;

            remove_member(&pool, community_id, &member_keys).await;
            let sweep_event_id = "0".repeat(64);
            let live_reason = recv_closed(
                &mut live,
                &live_sub,
                ControlAck::Optional {
                    event_id: sweep_event_id.as_str(),
                    reason: MEMBERSHIP_REVOKED,
                },
            )
            .await;
            assert_eq!(live_reason, MEMBERSHIP_REVOKED);

            let fresh_error = match BuzzTestClient::connect(&url, &member_keys).await {
                Ok(fresh) => {
                    disconnect_client(fresh, "unexpected fresh member session").await;
                    panic!("fresh NIP-42 auth must be denied after row removal");
                }
                Err(error) => error,
            };
            assert!(
                matches!(&fresh_error, TestClientError::AuthFailed(message) if message.as_str() == MEMBERSHIP_REVOKED),
                "unexpected fresh-auth denial: {fresh_error}"
            );
            disconnect_client(live, "sweep member session").await;
        },
    )
    .await;
}

/// A production-shaped Redis connection-control command must close a live
/// socket even while its writer membership row remains present. The retained
/// row and a successful fresh NIP-42 login are the causal oracle: neither the
/// periodic writer sweep nor a request-side membership denial can explain the
/// observed policy close.
async fn redis_connection_control_revokes_live_socket_impl() {
    let url = relay_url();
    let pool = db_pool().await;
    let community_id = deployment_community(&pool).await;
    let member_keys = Keys::generate();
    add_member(&pool, community_id, &member_keys).await;

    with_member_cleanup(&pool, community_id, &member_keys, async {
        let mut live = BuzzTestClient::connect(&url, &member_keys)
            .await
            .expect("member must authenticate while its writer row exists");
        let live_sub = format!("redis-control-live-{}", Uuid::new_v4());
        live.subscribe(&live_sub, vec![Filter::new().kinds([Kind::TextNote])])
            .await
            .expect("open member subscription");
        recv_eose(&mut live, &live_sub).await;

        let reason = "blocked: redis connection-control probe";
        let subscriber_count = publish_disconnect_pubkey(community_id, &member_keys, reason).await;
        assert!(
            subscriber_count == 1,
            "PUBLISH must reach exactly the membership relay conn-control subscriber"
        );
        assert_eq!(
            recv_closed(
                &mut live,
                &live_sub,
                ControlAck::Required {
                    event_id: REDIS_CONTROL_EVENT_ID,
                    reason,
                },
            )
            .await,
            reason
        );

        // The row was deliberately retained. A new NIP-42 session must
        // still authenticate, proving the close came from Redis control.
        let fresh = BuzzTestClient::connect(&url, &member_keys)
            .await
            .expect("Redis disconnect must not revoke the durable membership row");
        disconnect_client(fresh, "fresh Redis-control member session").await;
        disconnect_client(live, "Redis-control member session").await;
    })
    .await;
}

// These scenarios need a live, membership-gated relay in addition to their
// PostgreSQL and Redis dependencies. Keep them under the external-infra
// namespace so discovery does not route them through the database-only lane.
mod external_infra_membership_tests {
    #[tokio::test]
    #[ignore = "requires PostgreSQL, Redis, and network relay fixture"]
    async fn writer_membership_sweep_closes_idle_subscription() {
        super::writer_membership_sweep_closes_idle_subscription_impl().await;
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL, Redis, and network relay fixture"]
    async fn redis_connection_control_revokes_live_socket() {
        super::redis_connection_control_revokes_live_socket_impl().await;
    }
}
