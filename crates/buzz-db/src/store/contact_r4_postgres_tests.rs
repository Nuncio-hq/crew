//! R4 runs against the real partitioned event store in nextest-owned databases.
//! No routing resolver, consumer, schema startup, or network service is enabled.

use super::Fixture;
use crate::event::contact_proof_insert::ContactClass;
use crate::event::insert_event_with_thread_metadata_classified_tx;
use chrono::{DateTime, Utc};
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::{Postgres, Transaction};

// Provisional local Crew allocation; no wire handler or capability is enabled.
const CONTACT_PROOF: u16 = 46044;
const CONTACT_CONTROL: u16 = 24210;

fn message(f: &Fixture, text: &str, at: u64) -> Event {
    EventBuilder::new(Kind::Custom(9), text)
        .tags([Tag::parse(["h", &f.channel.to_string()]).expect("h tag")])
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(&f.owner)
        .expect("signed original")
}

fn timestamp(event: &Event) -> DateTime<Utc> {
    DateTime::from_timestamp(event.created_at.as_secs() as i64, 0).expect("fixture timestamp")
}

async fn install(f: &Fixture) {
    sqlx::raw_sql(include_str!("contact_r4_schema.sql"))
        .execute(&f.pool)
        .await
        .expect("install isolated proof schema");
    sqlx::query(
        "INSERT INTO contact_quota (community_id, stripe) SELECT $1, generate_series(0,15)",
    )
    .bind(f.community.as_uuid())
    .execute(&f.pool)
    .await
    .expect("sixteen real quota stripes");
}

async fn insert(
    f: &Fixture,
    tx: &mut Transaction<'_, Postgres>,
    event: &Event,
    class: Option<ContactClass>,
) -> bool {
    event.verify().expect("valid fixture signature");
    insert_event_with_thread_metadata_classified_tx(
        tx,
        f.community,
        event,
        Some(f.channel),
        None,
        class,
    )
    .await
    .expect("actual original INSERT with typed class")
    .1
}

async fn classification(f: &Fixture, event: &Event) -> Option<i16> {
    sqlx::query_scalar("SELECT contact_class FROM events WHERE community_id=$1 AND id=$2")
        .bind(f.community.as_uuid())
        .bind(event.id.as_bytes().as_slice())
        .fetch_one(&f.pool)
        .await
        .expect("stored original classification")
}

fn constraint_error(result: Result<(), sqlx::Error>, context: &str) {
    let error = result.expect_err(context);
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514"),
        "{context}: must reject the invariant, not fail setup: {error}"
    );
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_parent_and_every_leaf_classify_old_writers() {
    let f = Fixture::new().await;
    install(&f).await;
    let parent = message(&f, "parent old writer", 1_800_000_000);
    f.insert(&parent).await;
    assert_eq!(
        classification(&f, &parent).await,
        Some(0),
        "missing INSERT guard"
    );

    // Derive a valid timestamp for each actual attached leaf, including the
    // two catchalls. Do not merely infer coverage from parent trigger presence.
    let leaves: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname, pg_get_expr(c.relpartbound,c.oid) FROM pg_inherits i \
         JOIN pg_class c ON c.oid=i.inhrelid WHERE i.inhparent='events'::regclass",
    )
    .fetch_all(&f.pool)
    .await
    .expect("actual partition catalog");
    assert!(leaves.len() > 2, "healthy partitioned fixture");
    for (leaf, bound) in leaves {
        let at = if bound.contains("MINVALUE") {
            1_735_689_600
        } else {
            let lower = bound.split('\'').nth(1).expect("range lower bound");
            sqlx::query_scalar::<_, i64>("SELECT extract(epoch FROM $1::timestamptz)::bigint")
                .bind(lower)
                .fetch_one(&f.pool)
                .await
                .expect("partition lower timestamp") as u64
        };
        let event = message(&f, &leaf, at);
        let quoted = leaf.replace('"', "\"\"");
        // Catalog-owned identifier is double-quoted and escaped; values remain bound.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO \"{quoted}\" (community_id,id,pubkey,created_at,kind,tags,content,sig,channel_id) \
             VALUES ($1,$2,$3,$4,9,$5,$6,$7,$8)"
        )))
        .bind(f.community.as_uuid()).bind(event.id.as_bytes().as_slice())
        .bind(event.pubkey.to_bytes().to_vec()).bind(timestamp(&event))
        .bind(serde_json::to_value(&event.tags).expect("tags"))
        .bind(&event.content).bind(event.sig.serialize().to_vec()).bind(f.channel)
        .execute(&f.pool).await.expect("direct leaf original INSERT");
        assert_eq!(
            classification(&f, &event).await,
            Some(0),
            "unguarded leaf {leaf}"
        );
    }
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_legacy_null_cannot_be_promoted() {
    let f = Fixture::new().await;
    let old = message(&f, "pre-migration", 1_800_000_000);
    f.insert(&old).await;
    install(&f).await;
    assert_eq!(classification(&f, &old).await, None, "no backfill");
    constraint_error(
        sqlx::query("UPDATE events SET contact_class=1 WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(old.id.as_bytes().as_slice())
            .execute(&f.pool)
            .await
            .map(|_| ()),
        "legacy original must never become routable",
    );
    assert_eq!(classification(&f, &old).await, None);
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_routed_marker_without_route_rolls_back() {
    let f = Fixture::new().await;
    install(&f).await;
    let event = message(&f, "torn decision", 1_800_000_000);
    let mut tx = f.pool.begin().await.expect("begin");
    assert!(insert(&f, &mut tx, &event, Some(ContactClass::Routed)).await);
    let within: i16 =
        sqlx::query_scalar("SELECT contact_class FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(event.id.as_bytes().as_slice())
            .fetch_one(&mut *tx)
            .await
            .expect("INSERT succeeded before deferred check");
    assert_eq!(within, 1, "typed marker is bound on original INSERT");
    constraint_error(tx.commit().await, "route/proof must exist at commit");
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(event.id.as_bytes().as_slice())
            .fetch_one(&f.pool)
            .await
            .expect("readback rollback");
    assert_eq!(
        count, 0,
        "failed decision cannot leave an accepted original"
    );
}

// A preselected fixture decision exercises storage only. It deliberately does
// not claim to prove canvas authority, readiness or relay ingress authorization.
async fn route(
    f: &Fixture,
    tx: &mut Transaction<'_, Postgres>,
    original: &Event,
    with_proof: bool,
    limit: i32,
) -> ContactClass {
    // Lock before the known-original lookup: two concurrent duplicates must
    // see the first committed classification before reserving another slot.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "crew_contact:{}:{}",
            f.community.as_uuid(),
            f.channel
        ))
        .execute(&mut **tx)
        .await
        .expect("contact lock");
    let known: Option<Option<i16>> =
        sqlx::query_scalar("SELECT contact_class FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(original.id.as_bytes().as_slice())
            .fetch_optional(&mut **tx)
            .await
            .expect("known original including deleted");
    assert!(
        known.is_none(),
        "fixture route accepts fresh originals only"
    );
    let reserved = sqlx::query(
        "UPDATE contact_quota SET used=used+1 WHERE community_id=$1 AND stripe=0 AND used<$2",
    )
    .bind(f.community.as_uuid())
    .bind(limit)
    .execute(&mut **tx)
    .await
    .expect("bounded reservation")
    .rows_affected()
        == 1;
    if !reserved {
        assert!(insert(f, tx, original, Some(ContactClass::Quota)).await);
        return ContactClass::Quota;
    }
    assert!(insert(f, tx, original, Some(ContactClass::Routed)).await);
    let relay = Keys::generate();
    let contact = Keys::generate();
    let proof = EventBuilder::new(Kind::Custom(CONTACT_PROOF), "fixture decision")
        .tags([
            Tag::parse(["h", &f.channel.to_string()]).expect("h"),
            Tag::parse(["p", &contact.public_key().to_hex()]).expect("p"),
            Tag::parse(["original", &original.id.to_hex()]).expect("original"),
            Tag::parse(["phase", "decision"]).expect("phase"),
        ])
        .custom_created_at(original.created_at)
        .sign_with_keys(&relay)
        .expect("proof signature");
    if with_proof {
        assert!(insert(f, tx, &proof, None).await);
    }
    sqlx::query(
        "INSERT INTO contact_routes (community_id,original_id,original_created_at,channel_id,\
         contact_pubkey,relay_pubkey,decision_id,decision_created_at,stripe) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,0)",
    )
    .bind(f.community.as_uuid())
    .bind(original.id.as_bytes().as_slice())
    .bind(timestamp(original))
    .bind(f.channel)
    .bind(contact.public_key().to_bytes().to_vec())
    .bind(relay.public_key().to_bytes().to_vec())
    .bind(proof.id.as_bytes().as_slice())
    .bind(timestamp(&proof))
    .execute(&mut **tx)
    .await
    .expect("route row");
    ContactClass::Routed
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_missing_proof_rolls_back_original_route_and_quota() {
    let f = Fixture::new().await;
    install(&f).await;
    let event = message(&f, "missing proof", 1_800_000_000);
    let mut tx = f.pool.begin().await.expect("begin");
    assert_eq!(
        route(&f, &mut tx, &event, false, 8192).await,
        ContactClass::Routed
    );
    constraint_error(tx.commit().await, "route must bind a stored signed proof");
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM events WHERE community_id=$1), \
         (SELECT count(*) FROM contact_routes WHERE community_id=$1), \
         (SELECT sum(used)::bigint FROM contact_quota WHERE community_id=$1)",
    )
    .bind(f.community.as_uuid())
    .fetch_one(&f.pool)
    .await
    .expect("atomic rollback counts");
    assert_eq!(counts, (0, 0, 0));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_quota_full_preserves_chat_and_replay_classification() {
    let f = Fixture::new().await;
    install(&f).await;
    // Lower limit uses the same bounded reservation branch; this is not an
    // 8192-row capacity/load measurement. The schema maximum remains 8192.
    for index in 0..2 {
        let event = message(&f, &format!("capacity {index}"), 1_800_000_000 + index);
        let mut tx = f.pool.begin().await.expect("begin");
        let class = route(&f, &mut tx, &event, true, 1).await;
        assert_eq!(
            class,
            if index == 0 {
                ContactClass::Routed
            } else {
                ContactClass::Quota
            }
        );
        tx.commit().await.expect("both originals accepted");
        assert_eq!(classification(&f, &event).await, Some(class as i16));
        let mut replay = f.pool.begin().await.expect("begin replay");
        assert!(!insert(&f, &mut replay, &event, Some(ContactClass::Disabled)).await);
        replay.commit().await.expect("duplicate unchanged");
        assert_eq!(classification(&f, &event).await, Some(class as i16));
    }
    for (index, class) in [ContactClass::ExplicitMention, ContactClass::Disabled]
        .into_iter()
        .enumerate()
    {
        let event = message(
            &f,
            "explicit/disabled at quota",
            1_800_000_010 + index as u64,
        );
        let mut tx = f.pool.begin().await.expect("begin ordinary chat");
        assert!(insert(&f, &mut tx, &event, Some(class)).await);
        tx.commit()
            .await
            .expect("ordinary chat unaffected by quota");
    }
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM contact_routes WHERE community_id=$1), \
         (SELECT sum(used)::bigint FROM contact_quota WHERE community_id=$1)",
    )
    .bind(f.community.as_uuid())
    .fetch_one(&f.pool)
    .await
    .expect("routed-only count");
    assert_eq!(counts, (1, 1));
    let event = message(&f, "capacity 1", 1_800_000_001);
    constraint_error(
        sqlx::query("UPDATE events SET contact_class=1 WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(event.id.as_bytes().as_slice())
            .execute(&f.pool)
            .await
            .map(|_| ()),
        "quota decision must not become routable on a later UPDATE",
    );
}

#[test]
fn contact_proof_r4_provisional_kind_allocation_is_unclaimed_and_control_ephemeral() {
    assert!(!buzz_core::kind::is_ephemeral(u32::from(CONTACT_PROOF)));
    assert!(buzz_core::kind::is_ephemeral(u32::from(CONTACT_CONTROL)));
    assert!(!buzz_core::kind::ALL_KINDS.contains(&u32::from(CONTACT_PROOF)));
    assert!(!buzz_core::kind::ALL_KINDS.contains(&u32::from(CONTACT_CONTROL)));
    assert_ne!(
        u32::from(CONTACT_PROOF),
        buzz_core::kind::KIND_AGENT_RECEIPT
    );
    // Names are deliberately local until ingress/search/feed gates are added.
    // Reserving constants here is not relay-only proof authorization.
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_route_proof_and_quota_are_reciprocal() {
    let f = Fixture::new().await;
    install(&f).await;
    for (index, mutation) in [
        "UPDATE contact_quota SET used=0 WHERE community_id=$1",
        "UPDATE contact_routes SET contact_pubkey=decode(repeat('00',32),'hex') WHERE community_id=$1",
        "UPDATE contact_routes SET original_id=decode(repeat('ff',32),'hex') WHERE community_id=$1",
    ].into_iter().enumerate() {
        let original = message(&f, mutation, 1_800_000_100 + index as u64);
        let mut tx = f.pool.begin().await.expect("begin malformed transaction");
        assert_eq!(route(&f, &mut tx, &original, true, 8192).await, ContactClass::Routed);
        // Immediate rejection is also correct; otherwise deferred commit must
        // reject. These are valid SQL statements against a healthy fixture.
        match sqlx::query(mutation).bind(f.community.as_uuid()).execute(&mut *tx).await {
            Err(error) => {
                constraint_error(Err(error), mutation);
                tx.rollback().await.expect("rollback rejected mutation");
            }
            Ok(_) => constraint_error(tx.commit().await, mutation),
        }
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM contact_routes WHERE community_id=$1")
            .bind(f.community.as_uuid()).fetch_one(&f.pool).await.expect("no torn routes");
        assert_eq!(count, 0);
    }
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_partition_catalog_requires_enabled_exact_guards() {
    let f = Fixture::new().await;
    install(&f).await;
    let tables: Vec<(i64, String)> = sqlx::query_as(
        "SELECT c.oid::bigint,c.relname FROM pg_class c WHERE c.oid='events'::regclass \
         OR c.oid IN (SELECT inhrelid FROM pg_inherits WHERE inhparent='events'::regclass)",
    )
    .fetch_all(&f.pool)
    .await
    .expect("parent and attached leaves");
    assert!(tables.len() > 3, "healthy real partition tree");
    for (oid, table) in tables {
        let classify: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_trigger t JOIN pg_proc p ON p.oid=t.tgfoid \
             WHERE t.tgrelid=$1::oid AND t.tgenabled='O' AND t.tgtype=23 \
             AND p.proname='contact_classify_original'",
        )
        .bind(oid)
        .fetch_one(&f.pool)
        .await
        .expect("class guard catalog");
        assert_eq!(
            classify, 1,
            "{table} needs BEFORE INSERT OR UPDATE row guard"
        );
        if table != "events" {
            let deferred: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_trigger t JOIN pg_proc p ON p.oid=t.tgfoid \
                 WHERE t.tgrelid=$1::oid AND t.tgenabled='O' AND t.tgtype=5 \
                 AND t.tgdeferrable AND t.tginitdeferred AND p.proname='contact_check_original'",
            )
            .bind(oid)
            .fetch_one(&f.pool)
            .await
            .expect("deferred guard catalog");
            assert_eq!(
                deferred, 1,
                "{table} needs deferred reciprocal route validation"
            );
        }
    }
}

#[path = "contact_r4_next_support.rs"]
mod next_support;
#[path = "contact_r4_partition_postgres_tests.rs"]
mod partition_postgres_tests;
#[path = "contact_r4_race_postgres_tests.rs"]
mod race_postgres_tests;
#[path = "contact_r4_retention_postgres_tests.rs"]
mod retention_postgres_tests;
