//! Live relay-membership revocation coverage.
//!
//! The test runs against a membership-gated relay backed by the same
//! PostgreSQL writer that the relay uses.  It deliberately removes the
//! member row outside the relay process, then checks the current-membership
//! decision at every live seam: an existing subscription is policy-closed,
//! REQ and COUNT receive correlated `CLOSED` responses, EVENT receives a
//! correlated `OK false`, a rejected event is not readable by the owner, and
//! a fresh NIP-42 connection is denied.
//!
//! The test is ignored by default because it needs PostgreSQL, Redis, and a
//! running relay with `BUZZ_REQUIRE_RELAY_MEMBERSHIP=true`.

use std::time::Duration;

use buzz_test_client::{BuzzTestClient, RelayMessage, TestClientError};
use nostr::{EventBuilder, Filter, Keys, Kind};
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
    // cannot turn a lost command into a false readiness signal.
    for _ in 0..100 {
        let subscribers: i64 = tokio::time::timeout(
            DEPENDENCY_TIMEOUT,
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
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("Redis conn-control subscriber did not become ready on {channel}");
}

#[derive(Clone, Copy)]
enum ControlAck<'a> {
    None,
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
                    ControlAck::None => panic!(
                        "unexpected connection-level OK while waiting for CLOSED {sub_id}: {ok:?}"
                    ),
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

/// A writer-backed membership removal must be observed independently by the
/// REQ, COUNT, EVENT, and fresh NIP-42 admission seams. The relay used for
/// this phase has its periodic sweep interval held open by CI, so these
/// request-specific responses cannot be produced by the idle-socket sweep.
#[tokio::test]
#[ignore = "requires PostgreSQL, Redis, and a membership-gated relay"]
async fn writer_membership_removal_denies_request_seams() {
    let url = relay_url();
    let pool = db_pool().await;
    let community_id = deployment_community(&pool).await;
    let member_keys = Keys::generate();
    add_member(&pool, community_id, &member_keys).await;

    let mut req = BuzzTestClient::connect(&url, &member_keys)
        .await
        .expect("REQ probe must authenticate before removal");
    let req_sub = format!("membership-req-{}", Uuid::new_v4());

    let mut count = BuzzTestClient::connect(&url, &member_keys)
        .await
        .expect("COUNT probe must authenticate before removal");
    let count_sub = format!("membership-count-{}", Uuid::new_v4());

    let mut event = BuzzTestClient::connect(&url, &member_keys)
        .await
        .expect("EVENT probe must authenticate before removal");
    let denied_event = EventBuilder::new(Kind::TextNote, "membership-revocation-probe")
        .sign_with_keys(&member_keys)
        .expect("sign membership probe event");
    let denied_event_id = denied_event.id.to_hex();

    remove_member(&pool, community_id, &member_keys).await;

    // Probe all three request handlers concurrently. The CI phase holds the
    // periodic sweep open, so each response below comes from the handler's
    // own writer-backed current-membership check.
    let req_probe = async {
        req.subscribe(&req_sub, vec![Filter::new().kinds([Kind::TextNote])])
            .await
            .expect("send REQ after membership removal");
        recv_closed(&mut req, &req_sub, ControlAck::None).await
    };
    let count_probe = async {
        count
            .send_raw(&json!([
                "COUNT",
                count_sub.clone(),
                serde_json::to_value(Filter::new().kinds([Kind::TextNote]))
                    .expect("serialize COUNT filter"),
            ]))
            .await
            .expect("send COUNT after membership removal");
        recv_closed(&mut count, &count_sub, ControlAck::None).await
    };
    let event_probe = async {
        event
            .send_event(denied_event)
            .await
            .expect("receive EVENT denial after membership removal")
    };
    let (req_reason, count_reason, event_response) =
        tokio::join!(req_probe, count_probe, event_probe);
    assert_eq!(req_reason, MEMBERSHIP_REVOKED);
    assert_eq!(count_reason, MEMBERSHIP_REVOKED);
    assert!(!event_response.accepted);
    assert_eq!(event_response.message, MEMBERSHIP_REVOKED);
    assert_eq!(event_response.event_id, denied_event_id);

    let fresh_error = match BuzzTestClient::connect(&url, &member_keys).await {
        Ok(_) => panic!("fresh NIP-42 auth must be denied after row removal"),
        Err(error) => error,
    };
    assert!(
        matches!(&fresh_error, TestClientError::AuthFailed(message) if message.as_str() == MEMBERSHIP_REVOKED),
        "unexpected fresh-auth denial: {fresh_error}"
    );

    // The relay owner can still read the event coordinate. A rejected member
    // EVENT must not have been persisted as a side effect of the denial.
    let owner_keys =
        Keys::parse("0000000000000000000000000000000000000000000000000000000000000001")
            .expect("test relay owner key");
    let mut owner = BuzzTestClient::connect(&url, &owner_keys)
        .await
        .expect("bootstrapped relay owner must authenticate");
    let owner_sub = format!("membership-owner-{}", Uuid::new_v4());
    owner
        .subscribe(
            &owner_sub,
            vec![Filter::new()
                .kinds([Kind::TextNote])
                .ids([denied_event_id.parse::<nostr::EventId>().expect("event id")])],
        )
        .await
        .expect("owner query for denied event");
    let owner_events = owner
        .collect_until_eose(&owner_sub, Duration::from_secs(10))
        .await
        .expect("owner query must reach EOSE");
    assert!(owner_events.is_empty(), "denied event must not be stored");

    let _ = req.disconnect().await;
    let _ = count.disconnect().await;
    let _ = event.disconnect().await;
    let _ = owner.disconnect().await;
}

/// The periodic writer-backed sweep must close an already-authenticated idle
/// subscription after its member row is removed. The connection-level
/// acknowledgement and subscription CLOSED frame are both correlated before
/// the test accepts the policy reason.
#[tokio::test]
#[ignore = "requires PostgreSQL, Redis, and a membership-gated relay"]
async fn writer_membership_sweep_closes_idle_subscription() {
    let url = relay_url();
    let pool = db_pool().await;
    let community_id = deployment_community(&pool).await;
    let member_keys = Keys::generate();
    add_member(&pool, community_id, &member_keys).await;

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
    let _ = live.disconnect().await;
}

/// A production-shaped Redis connection-control command must close a live
/// socket even while its writer membership row remains present. The retained
/// row and a successful fresh NIP-42 login are the causal oracle: neither the
/// periodic writer sweep nor a request-side membership denial can explain the
/// observed policy close.
#[tokio::test]
#[ignore = "requires PostgreSQL, Redis, and a membership-gated relay"]
async fn redis_connection_control_revokes_live_socket() {
    let url = relay_url();
    let pool = db_pool().await;
    let community_id = deployment_community(&pool).await;
    let member_keys = Keys::generate();
    add_member(&pool, community_id, &member_keys).await;

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

    // The row was deliberately retained. A new NIP-42 session must therefore
    // still authenticate, proving the close came from Redis control delivery.
    let fresh = BuzzTestClient::connect(&url, &member_keys)
        .await
        .expect("Redis disconnect must not revoke the durable membership row");
    fresh
        .disconnect()
        .await
        .expect("disconnect fresh member session");
    let result = tokio::time::timeout(
        DEPENDENCY_TIMEOUT,
        sqlx::query(
            "DELETE FROM relay_members WHERE community_id = $1 AND pubkey = $2 AND role = 'member'",
        )
        .bind(community_id)
        .bind(member_keys.public_key().to_hex())
        .execute(&pool),
    )
    .await
    .expect("timed out cleaning up Redis-control membership row")
    .expect("clean up Redis-control membership row");
    assert_eq!(result.rows_affected(), 1);
    let _ = live.disconnect().await;
}
