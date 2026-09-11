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

fn relay_url() -> String {
    std::env::var("RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_owned())
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
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect to integration PostgreSQL")
}

async fn deployment_community(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM communities WHERE lower(host) = lower($1)")
        .bind(relay_host())
        .fetch_one(pool)
        .await
        .expect("deployment community must be seeded before the relay starts")
}

async fn add_member(pool: &PgPool, community_id: Uuid, keys: &Keys) {
    sqlx::query(
        "INSERT INTO relay_members (community_id, pubkey, role, added_by)\
         VALUES ($1, $2, 'member', NULL)",
    )
    .bind(community_id)
    .bind(keys.public_key().to_hex())
    .execute(pool)
    .await
    .expect("insert relay membership row");
}

async fn remove_member(pool: &PgPool, community_id: Uuid, keys: &Keys) {
    let result = sqlx::query(
        "DELETE FROM relay_members WHERE community_id = $1 AND pubkey = $2 AND role = 'member'",
    )
    .bind(community_id)
    .bind(keys.public_key().to_hex())
    .execute(pool)
    .await
    .expect("remove relay membership row");
    assert_eq!(
        result.rows_affected(),
        1,
        "exactly one member row must be removed"
    );
}

async fn recv_closed(client: &mut BuzzTestClient, sub_id: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
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
            RelayMessage::Closed {
                subscription_id,
                message,
            } if subscription_id == sub_id => return message,
            RelayMessage::Notice { .. } | RelayMessage::Auth { .. } => {}
            other => panic!("unexpected response while waiting for CLOSED {sub_id}: {other:?}"),
        }
    }
}

async fn recv_eose(client: &mut BuzzTestClient, sub_id: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
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

/// A writer-backed membership removal must converge through NIP-42, queries,
/// writes, and the live connection-control/revalidation path.
#[tokio::test]
#[ignore = "requires PostgreSQL, Redis, and a membership-gated relay"]
async fn writer_membership_removal_revokes_live_and_fresh_access() {
    let url = relay_url();
    let pool = db_pool().await;
    let community_id = deployment_community(&pool).await;
    let member_keys = Keys::generate();
    add_member(&pool, community_id, &member_keys).await;

    let mut live = BuzzTestClient::connect(&url, &member_keys)
        .await
        .expect("member must authenticate while its writer row exists");
    let live_sub = format!("membership-live-{}", Uuid::new_v4());
    live.subscribe(&live_sub, vec![Filter::new()])
        .await
        .expect("open member subscription");
    recv_eose(&mut live, &live_sub).await;

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

    // Probe all three request handlers concurrently. This keeps the probes
    // ahead of the short revalidation tick, while each handler still performs
    // its own writer-backed current-membership check.
    let req_probe = async {
        req.subscribe(&req_sub, vec![Filter::new().kinds([Kind::TextNote])])
            .await
            .expect("send REQ after membership removal");
        recv_closed(&mut req, &req_sub).await
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
        recv_closed(&mut count, &count_sub).await
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

    let live_reason = recv_closed(&mut live, &live_sub).await;
    assert_eq!(live_reason, MEMBERSHIP_REVOKED);

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
            vec![Filter::new().ids([denied_event_id.parse::<nostr::EventId>().expect("event id")])],
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
    let _ = live.disconnect().await;
}
