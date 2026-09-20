//! #412 production contact-fallback seam tests.
//!
//! These tests drive the real decision/claim functions and the real
//! production trigger catalog installed by migration 0047 — unlike the R4
//! fixture files, nothing here drops or replaces `_v1` guards. The fixture
//! runner owns the disposable database and its cleanup.

use super::{
    cancel_contact_claim, claim_contact_decision, decide_contact_route, insert_contact_receipt,
    parse_claim_tag, start_contact_claim, ContactClaimResult, ContactClass, ContactControlResult,
};
use crate::event::{insert_event_with_thread_metadata, soft_delete_event, ThreadMetadataParams};
use crate::{CommunityId, DbError};
use chrono::DateTime;
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

const KIND_MESSAGE: u16 = 9;
const KIND_CANVAS: u16 = 40100;
const KIND_RECEIPT: u16 = 46043;
const KIND_DECISION: u16 = 46044;
const CAP: &str = "crew-contact-fallback";

struct Fixture {
    pool: PgPool,
    community: CommunityId,
    channel: Uuid,
    /// Channel/community owner — the human that owns the contact agent.
    owner: Keys,
    /// The configured contact agent.
    contact: Keys,
    /// Relay identity that signs decision proofs.
    relay: Keys,
}

impl Fixture {
    async fn new() -> Self {
        let explicit = std::env::var("BUZZ_TEST_DATABASE_URL")
            .expect("#412 requires an explicitly assigned disposable BUZZ_TEST_DATABASE_URL");
        assert!(!explicit.is_empty(), "fixture URL must not be empty");
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("SET statement_timeout = '10s'")
                        .execute(&mut *connection)
                        .await?;
                    sqlx::query("SET lock_timeout = '5s'")
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&crate::test_support::database_url())
            .await
            .expect("connect assigned fixture");
        let database: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .expect("identify isolated test database");
        let suffix = database
            .strip_prefix("buzz_nt_")
            .expect("nextest database prefix");
        assert!(
            suffix.len() == 24 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "refuse non-nextest database before any fixture writes"
        );
        let community_id = Uuid::new_v4();
        let channel = Uuid::new_v4();
        let owner = Keys::generate();
        let contact = Keys::generate();
        let relay = Keys::generate();
        sqlx::query("INSERT INTO communities (id, host) VALUES ($1, $2)")
            .bind(community_id)
            .bind(format!(
                "contact-decision-{}.example",
                community_id.simple()
            ))
            .execute(&pool)
            .await
            .expect("create isolated community");
        sqlx::query(
            "INSERT INTO channels (community_id, id, name, created_by, visibility) \
             VALUES ($1, $2, $3, $4, 'private')",
        )
        .bind(community_id)
        .bind(channel)
        .bind("contact-decision")
        .bind(owner.public_key().to_bytes().to_vec())
        .execute(&pool)
        .await
        .expect("create fixture channel");
        // Owner is a human user; contact is a registered agent owned by owner.
        for (pk, agent_owner) in [
            (owner.public_key().to_bytes().to_vec(), None::<Vec<u8>>),
            (
                contact.public_key().to_bytes().to_vec(),
                Some(owner.public_key().to_bytes().to_vec()),
            ),
        ] {
            sqlx::query(
                "INSERT INTO users (community_id, pubkey, agent_owner_pubkey) \
                 VALUES ($1, $2, $3)",
            )
            .bind(community_id)
            .bind(pk)
            .bind(agent_owner)
            .execute(&pool)
            .await
            .expect("fixture user");
        }
        for pk in [
            owner.public_key().to_bytes().to_vec(),
            contact.public_key().to_bytes().to_vec(),
        ] {
            sqlx::query(
                "INSERT INTO channel_members (community_id, channel_id, pubkey) \
                 VALUES ($1, $2, $3)",
            )
            .bind(community_id)
            .bind(channel)
            .bind(pk)
            .execute(&pool)
            .await
            .expect("fixture member");
        }
        Self {
            pool,
            community: CommunityId::from_uuid(community_id),
            channel,
            owner,
            contact,
            relay,
        }
    }

    fn canvas(&self, author: &Keys, contact: Option<&Keys>, timestamp: u64) -> Event {
        let content = contact.map_or_else(
            || "```crew\ndefinitions: {}\n```".to_string(),
            |key| format!("```crew\ncontact: {}\n```", key.public_key().to_hex()),
        );
        EventBuilder::new(Kind::Custom(KIND_CANVAS), content)
            .tags([Tag::parse(["h", &self.channel.to_string()]).expect("canvas channel tag")])
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(author)
            .expect("sign canvas")
    }

    /// Owner-signed mention-less kind:9 with the capability tag.
    fn message(&self, text: &str, at: u64) -> Event {
        self.message_by(&self.owner, text, at)
    }

    fn message_by(&self, signer: &Keys, text: &str, at: u64) -> Event {
        EventBuilder::new(Kind::Custom(KIND_MESSAGE), text)
            .tags([
                Tag::parse(["h", &self.channel.to_string()]).expect("h tag"),
                Tag::parse([CAP, "1"]).expect("capability tag"),
            ])
            .custom_created_at(Timestamp::from(at))
            .sign_with_keys(signer)
            .expect("signed original")
    }

    async fn insert(&self, event: &Event) {
        event.verify().expect("fixture signature");
        let (_, inserted) = insert_event_with_thread_metadata(
            &self.pool,
            self.community,
            event,
            Some(self.channel),
            None,
        )
        .await
        .expect("production event insert");
        assert!(inserted, "fixture must insert a new event");
    }

    async fn decide(&self, event: &Event) -> super::ContactRouteOutcome {
        decide_contact_route(
            &self.pool,
            self.community,
            event,
            self.channel,
            None,
            &self.relay,
        )
        .await
        .expect("decision transaction")
    }

    async fn stored_class(&self, event: &Event) -> Option<i16> {
        sqlx::query_scalar::<_, Option<i16>>(
            "SELECT contact_class FROM events WHERE community_id=$1 AND id=$2",
        )
        .bind(self.community.as_uuid())
        .bind(event.id.as_bytes().as_slice())
        .fetch_one(&self.pool)
        .await
        .expect("stored classification")
    }

    /// (originals class=1, routes, claims, proofs, quota used) for the fixture.
    async fn counts(&self) -> (i64, i64, i64, i64, i64) {
        sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
            "SELECT \
             (SELECT count(*) FROM events WHERE community_id=$1 AND kind=9 AND contact_class=1), \
             (SELECT count(*) FROM contact_routes WHERE community_id=$1), \
             (SELECT count(*) FROM contact_claims WHERE community_id=$1), \
             (SELECT count(*) FROM events WHERE community_id=$1 AND kind=46044), \
             (SELECT coalesce(sum(used),0)::bigint FROM contact_quota WHERE community_id=$1)",
        )
        .bind(self.community.as_uuid())
        .fetch_one(&self.pool)
        .await
        .expect("fixture counts")
    }

    async fn claim_state(&self, decision_id: &[u8]) -> Option<(String, i64)> {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT state, generation FROM contact_claims \
             WHERE community_id=$1 AND decision_id=$2",
        )
        .bind(self.community.as_uuid())
        .bind(decision_id)
        .fetch_optional(&self.pool)
        .await
        .expect("claim row")
    }

    async fn configure_contact_canvas(&self, at: u64) -> Event {
        let canvas = self.canvas(&self.owner, Some(&self.contact), at);
        self.insert(&canvas).await;
        canvas
    }

    fn decision_id_of<'a>(&self, outcome: &'a super::ContactRouteOutcome) -> &'a [u8] {
        outcome
            .proof
            .as_ref()
            .expect("routed outcome carries its proof")
            .event
            .id
            .as_bytes()
    }
}

fn constraint_error(result: Result<(), sqlx::Error>, context: &str) {
    let error = result.expect_err(context);
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514"),
        "{context}: must reject the invariant, not fail setup: {error}"
    );
}

// ---------------------------------------------------------------------------
// Decision matrix
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_routed_writes_original_proof_route_claim_and_quota() {
    let f = Fixture::new().await;
    let canvas = f.configure_contact_canvas(1_800_000_000).await;
    let original = f.message("hello contact", 1_800_000_001);
    let outcome = f.decide(&original).await;
    assert_eq!(outcome.class, ContactClass::Routed);
    assert!(outcome.was_inserted);
    let proof = outcome.proof.expect("routed decision emits a proof");
    let proof_event = &proof.event;

    // Proof is relay-signed, durable, decision-phased, and bound to the
    // original + winning canvas + configured contact.
    assert_eq!(
        proof_event.pubkey.to_bytes().to_vec(),
        f.relay.public_key().to_bytes().to_vec()
    );
    assert_eq!(proof_event.kind.as_u16(), KIND_DECISION);
    let tags = serde_json::to_value(&proof_event.tags).expect("proof tags");
    let channel_s = f.channel.to_string();
    let contact_s = f.contact.public_key().to_hex();
    for expected in [
        vec!["h".to_string(), channel_s],
        vec!["p".to_string(), contact_s],
        vec!["original".to_string(), original.id.to_hex()],
        vec!["canvas".to_string(), canvas.id.to_hex()],
        vec!["phase".to_string(), "decision".to_string()],
        vec![
            "e".to_string(),
            original.id.to_hex(),
            String::new(),
            "reply".to_string(),
        ],
    ] {
        let expected = serde_json::json!(expected);
        assert!(
            tags.as_array().expect("tag array").contains(&expected),
            "proof missing tag {expected}"
        );
    }
    assert_eq!(
        f.stored_class(&original).await,
        Some(ContactClass::Routed as i16)
    );
    assert_eq!(f.counts().await, (1, 1, 1, 1, 1));

    // The route pins the decision-time canvas snapshot.
    let (route_canvas, route_contact): (Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT canvas_id, contact_pubkey FROM contact_routes \
         WHERE community_id=$1 AND original_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(original.id.as_bytes().as_slice())
    .fetch_one(&f.pool)
    .await
    .expect("route row");
    assert_eq!(route_canvas, canvas.id.as_bytes().to_vec());
    assert_eq!(route_contact, f.contact.public_key().to_bytes().to_vec());

    // The claim row is born pending for the configured contact.
    assert_eq!(
        f.claim_state(proof_event.id.as_bytes()).await,
        Some(("pending".to_string(), 0))
    );

    // The proof lands in thread queries as a reply to the original.
    let meta: Option<(Vec<u8>, i32)> = sqlx::query_as(
        "SELECT parent_event_id, depth FROM thread_metadata \
         WHERE community_id=$1 AND event_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(proof_event.id.as_bytes().as_slice())
    .fetch_optional(&f.pool)
    .await
    .expect("proof thread metadata");
    assert_eq!(
        meta,
        Some((original.id.as_bytes().to_vec(), 1)),
        "proof threads under the original"
    );
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_duplicate_ingest_replays_stored_route() {
    let f = Fixture::new().await;
    f.configure_contact_canvas(1_800_000_000).await;
    let original = f.message("send twice", 1_800_000_001);
    let first = f.decide(&original).await;
    assert_eq!(first.class, ContactClass::Routed);
    let second = f.decide(&original).await;
    assert!(matches!(second.class, ContactClass::Routed));
    assert!(!second.was_inserted, "duplicate must not re-insert");
    assert!(second.proof.is_none(), "replay emits no second proof");
    assert_eq!(
        f.counts().await,
        (1, 1, 1, 1, 1),
        "exactly one original/route/claim/proof/quota spend"
    );
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_replay_under_canvas_change_keeps_pinned_contact() {
    let f = Fixture::new().await;
    let canvas_a = f.configure_contact_canvas(1_800_000_000).await;
    let original = f.message("canvas changes after decision", 1_800_000_001);
    let first = f.decide(&original).await;
    assert_eq!(first.class, ContactClass::Routed);

    // The canvas moves to a different contact — the stored decision must not
    // move with it.
    let other_contact = Keys::generate();
    sqlx::query(
        "INSERT INTO users (community_id, pubkey, agent_owner_pubkey) \
         VALUES ($1, $2, $3)",
    )
    .bind(f.community.as_uuid())
    .bind(other_contact.public_key().to_bytes().to_vec())
    .bind(f.owner.public_key().to_bytes().to_vec())
    .execute(&f.pool)
    .await
    .expect("second registered contact");
    sqlx::query(
        "INSERT INTO channel_members (community_id, channel_id, pubkey) \
         VALUES ($1, $2, $3)",
    )
    .bind(f.community.as_uuid())
    .bind(f.channel)
    .bind(other_contact.public_key().to_bytes().to_vec())
    .execute(&f.pool)
    .await
    .expect("second contact member");
    let canvas_b = f.canvas(&f.owner, Some(&other_contact), 1_800_000_002);
    f.insert(&canvas_b).await;

    let replay = f.decide(&original).await;
    assert_eq!(replay.class, ContactClass::Routed);
    assert!(!replay.was_inserted);
    assert_eq!(f.counts().await, (1, 1, 1, 1, 1));
    let pinned: Vec<u8> = sqlx::query_scalar(
        "SELECT canvas_id FROM contact_routes \
         WHERE community_id=$1 AND original_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(original.id.as_bytes().as_slice())
    .fetch_one(&f.pool)
    .await
    .expect("pinned canvas");
    assert_eq!(pinned, canvas_a.id.as_bytes().to_vec());
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_explicit_mention_and_missing_capability_do_not_route() {
    let f = Fixture::new().await;
    f.configure_contact_canvas(1_800_000_000).await;

    // Explicit p-tag mention — suppressed even with the capability tag set.
    let mentioned = Keys::generate();
    let mention = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "ping @agent")
        .tags([
            Tag::parse(["h", &f.channel.to_string()]).expect("h"),
            Tag::parse(["p", &mentioned.public_key().to_hex()]).expect("p"),
            Tag::parse([CAP, "1"]).expect("cap"),
        ])
        .custom_created_at(Timestamp::from(1_800_000_001))
        .sign_with_keys(&f.owner)
        .expect("mention");
    let outcome = f.decide(&mention).await;
    assert_eq!(outcome.class, ContactClass::ExplicitMention);
    assert!(outcome.was_inserted);
    assert!(outcome.proof.is_none());
    assert_eq!(
        f.stored_class(&mention).await,
        Some(ContactClass::ExplicitMention as i16)
    );

    // No capability tag — a plain chat message stays un-routed.
    let plain = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "just chat")
        .tags([Tag::parse(["h", &f.channel.to_string()]).expect("h")])
        .custom_created_at(Timestamp::from(1_800_000_002))
        .sign_with_keys(&f.owner)
        .expect("plain");
    let outcome = f.decide(&plain).await;
    assert_eq!(outcome.class, ContactClass::Disabled);
    assert_eq!(
        f.stored_class(&plain).await,
        Some(ContactClass::Disabled as i16)
    );
    assert_eq!(f.counts().await, (0, 0, 0, 0, 0));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_agent_or_non_owner_signer_is_ineligible() {
    let f = Fixture::new().await;
    f.configure_contact_canvas(1_800_000_000).await;

    // A registered agent may not trigger routing (invariant #1 — raw signer
    // must be the registered owner).
    let agent_signed = f.message_by(&f.contact, "agent self-trigger", 1_800_000_001);
    let outcome = f.decide(&agent_signed).await;
    assert_eq!(outcome.class, ContactClass::SignerIneligible);
    assert_eq!(
        f.stored_class(&agent_signed).await,
        Some(ContactClass::SignerIneligible as i16)
    );

    // A different human (registered user, not the contact's owner) is denied.
    let other = Keys::generate();
    sqlx::query("INSERT INTO users (community_id, pubkey) VALUES ($1, $2)")
        .bind(f.community.as_uuid())
        .bind(other.public_key().to_bytes().to_vec())
        .execute(&f.pool)
        .await
        .expect("other human user");
    sqlx::query(
        "INSERT INTO channel_members (community_id, channel_id, pubkey) \
         VALUES ($1, $2, $3)",
    )
    .bind(f.community.as_uuid())
    .bind(f.channel)
    .bind(other.public_key().to_bytes().to_vec())
    .execute(&f.pool)
    .await
    .expect("other human member");
    let foreign_signer = f.message_by(&other, "not the owner", 1_800_000_002);
    let outcome = f.decide(&foreign_signer).await;
    assert_eq!(outcome.class, ContactClass::SignerIneligible);
    assert_eq!(f.counts().await, (0, 0, 0, 0, 0));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_no_usable_contact_is_no_contact() {
    let f = Fixture::new().await;

    // No canvas at all.
    let no_canvas = f.message("nobody home", 1_800_000_001);
    let outcome = f.decide(&no_canvas).await;
    assert_eq!(outcome.class, ContactClass::NoContact);

    // Canvas exists but has no crew contact field.
    let bare = f.canvas(&f.owner, None, 1_800_000_002);
    f.insert(&bare).await;
    let no_contact_field = f.message("no contact field", 1_800_000_003);
    let outcome = f.decide(&no_contact_field).await;
    assert_eq!(outcome.class, ContactClass::NoContact);

    // Winning canvas authored by a foreign signer — displayable, never
    // authoritative for routing.
    let foreign = f.canvas(&Keys::generate(), Some(&f.contact), 1_800_000_004);
    f.insert(&foreign).await;
    let foreign_canvas = f.message("foreign canvas", 1_800_000_005);
    let outcome = f.decide(&foreign_canvas).await;
    assert_eq!(outcome.class, ContactClass::NoContact);

    // Owner-authored canvas naming a contact that is not a registered agent.
    let stranger = Keys::generate();
    sqlx::query("INSERT INTO users (community_id, pubkey) VALUES ($1, $2)")
        .bind(f.community.as_uuid())
        .bind(stranger.public_key().to_bytes().to_vec())
        .execute(&f.pool)
        .await
        .expect("unregistered human");
    sqlx::query(
        "INSERT INTO channel_members (community_id, channel_id, pubkey) \
         VALUES ($1, $2, $3)",
    )
    .bind(f.community.as_uuid())
    .bind(f.channel)
    .bind(stranger.public_key().to_bytes().to_vec())
    .execute(&f.pool)
    .await
    .expect("stranger member");
    let unregistered = f.canvas(&f.owner, Some(&stranger), 1_800_000_006);
    f.insert(&unregistered).await;
    let unregistered_contact = f.message("unregistered contact", 1_800_000_007);
    let outcome = f.decide(&unregistered_contact).await;
    assert_eq!(outcome.class, ContactClass::NoContact);

    // Registered contact, owner-authored canvas — but the contact was removed
    // from the channel (private channel: member-or-bust).
    sqlx::query(
        "UPDATE channel_members SET removed_at = now() \
                 WHERE community_id=$1 AND channel_id=$2 AND pubkey=$3",
    )
    .bind(f.community.as_uuid())
    .bind(f.channel)
    .bind(f.contact.public_key().to_bytes().to_vec())
    .execute(&f.pool)
    .await
    .expect("remove contact member");
    let removed_member = f.canvas(&f.owner, Some(&f.contact), 1_800_000_008);
    f.insert(&removed_member).await;
    let removed_contact = f.message("removed contact", 1_800_000_009);
    let outcome = f.decide(&removed_contact).await;
    assert_eq!(outcome.class, ContactClass::NoContact);

    assert_eq!(f.counts().await, (0, 0, 0, 0, 0));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_quota_exhausted_stores_original_as_no_route() {
    let f = Fixture::new().await;
    f.configure_contact_canvas(1_800_000_000).await;
    // Preload all 16 stripes at the ceiling — stripe selection keys on the
    // original id's low nibble, so the outcome is deterministic. The
    // quota-parity guard forbids fabrication inside a normal session, so the
    // preload runs under the privileged replication-role bypass on one
    // connection and resets it before returning — fixture setup only, never a
    // production path.
    let mut connection = f.pool.acquire().await.expect("preload connection");
    sqlx::query("SET session_replication_role = 'replica'")
        .execute(&mut *connection)
        .await
        .expect("privileged fixture preload");
    sqlx::query(
        "INSERT INTO contact_quota (community_id, stripe, used) \
         SELECT $1, generate_series(0,15), 8192",
    )
    .bind(f.community.as_uuid())
    .execute(&mut *connection)
    .await
    .expect("exhaust every stripe");
    sqlx::query("SET session_replication_role = 'origin'")
        .execute(&mut *connection)
        .await
        .expect("restore session triggers");
    drop(connection);
    let original = f.message("out of quota", 1_800_000_001);
    let outcome = f.decide(&original).await;
    assert_eq!(outcome.class, ContactClass::Quota);
    assert!(outcome.was_inserted, "the chat message itself still stores");
    assert!(outcome.proof.is_none());
    assert_eq!(
        f.stored_class(&original).await,
        Some(ContactClass::Quota as i16)
    );
    // Replay keeps the exhausted outcome without spending again.
    let replay = f.decide(&original).await;
    assert_eq!(replay.class, ContactClass::Quota);
    assert!(!replay.was_inserted);
    assert_eq!(f.counts().await, (0, 0, 0, 0, 8192 * 16));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_crash_during_publish_leaves_no_partial_state() {
    let f = Fixture::new().await;
    f.configure_contact_canvas(1_800_000_000).await;
    // Fault injection: the route insert blows up mid-transaction — the model
    // of a crash after the original+proof writes but before commit. The whole
    // decision must roll back atomically.
    sqlx::raw_sql(
        "CREATE FUNCTION contact_decision_fault() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
         RAISE EXCEPTION 'injected mid-publish crash' USING ERRCODE='XX000'; \
         END $$; \
         CREATE TRIGGER contact_decision_fault BEFORE INSERT ON contact_routes \
         FOR EACH ROW EXECUTE FUNCTION contact_decision_fault()",
    )
    .execute(&f.pool)
    .await
    .expect("install fault trigger");
    let original = f.message("crashed decision", 1_800_000_001);
    let result =
        decide_contact_route(&f.pool, f.community, &original, f.channel, None, &f.relay).await;
    assert!(result.is_err(), "mid-publish crash must surface an error");
    let all_events: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE community_id=$1")
        .bind(f.community.as_uuid())
        .fetch_one(&f.pool)
        .await
        .expect("no partial events");
    assert_eq!(all_events, 1, "only the canvas may be stored");
    assert_eq!(
        f.counts().await,
        (0, 0, 0, 0, 0),
        "no original, route, claim, proof, or spent quota survives a crash"
    );

    // After the crash the same original decides cleanly — no phantom state.
    sqlx::query("DROP TRIGGER contact_decision_fault ON contact_routes")
        .execute(&f.pool)
        .await
        .expect("remove fault");
    let outcome = f.decide(&original).await;
    assert_eq!(outcome.class, ContactClass::Routed);
    assert_eq!(f.counts().await, (1, 1, 1, 1, 1));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_partition_fail_closed_until_guard_installed() {
    let f = Fixture::new().await;
    // A leaf attached outside the production installer carries no deferred
    // guards — the decision path must fail closed rather than write an
    // unverifiable decision. The catch-all covers the future range, so drop
    // it inside the same fixture-owned maintenance transaction first.
    let mut tx = f.pool.begin().await.expect("attach transaction");
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::deletion::SCHEMA_DESTRUCTION_LOCK_KEY)
        .execute(&mut *tx)
        .await
        .expect("maintenance fence");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM events_p_future")
        .fetch_one(&mut *tx)
        .await
        .expect("empty fixture range");
    assert_eq!(rows, 0, "drop only an empty fixture range");
    sqlx::query("DROP TABLE events_p_future")
        .execute(&mut *tx)
        .await
        .expect("owned fixture range");
    sqlx::query("CREATE TABLE events_412_attach (LIKE events INCLUDING DEFAULTS INCLUDING CONSTRAINTS INCLUDING GENERATED)")
        .execute(&mut *tx)
        .await
        .expect("preexisting empty child");
    sqlx::query("ALTER TABLE events ATTACH PARTITION events_412_attach FOR VALUES FROM ('2099-05-01') TO ('2099-06-01')")
        .execute(&mut *tx)
        .await
        .expect("actual ATTACH");
    tx.commit().await.expect("attach commits bare");
    // The canvas lives in the past-range leaf; only the tested original lands
    // on the bare attach.
    f.configure_contact_canvas(1_700_000_000).await;
    let future = chrono::DateTime::parse_from_rfc3339("2099-05-15T00:00:00Z")
        .expect("fixed range")
        .timestamp() as u64;
    let original = f.message("uncovered leaf", future);
    let error = decide_contact_route(&f.pool, f.community, &original, f.channel, None, &f.relay)
        .await
        .expect_err("uncovered partition must fail the decision closed");
    assert!(
        error.to_string().contains("events_412_attach"),
        "error must name the uncovered leaf: {error}"
    );
    assert_eq!(f.counts().await, (0, 0, 0, 0, 0));
    // Installing the real leaf guard restores availability.
    sqlx::query("SELECT contact_install_leaf_guards_v1('events_412_attach'::regclass)")
        .execute(&f.pool)
        .await
        .expect("install leaf guards");
    let outcome = f.decide(&original).await;
    assert_eq!(outcome.class, ContactClass::Routed);
    assert_eq!(f.counts().await, (1, 1, 1, 1, 1));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_partition_manager_installs_guards_on_new_leaves() {
    let f = Fixture::new().await;
    // The production partition manager — not a test helper — must leave each
    // new events leaf carrying both deferred guards.
    crate::partition::ensure_future_partitions(&f.pool, 1)
        .await
        .expect("real partition ensure");
    let uncovered: Vec<String> = sqlx::query_scalar(
        "SELECT inhrelid::regclass::text FROM pg_inherits \
         WHERE inhparent='events'::regclass \
         EXCEPT \
         SELECT tgrelid::regclass::text FROM pg_trigger \
         WHERE tgname='contact_check_original_v1' AND NOT tgisinternal \
         EXCEPT \
         SELECT tgrelid::regclass::text FROM pg_trigger \
         WHERE tgname='contact_check_proof_delete_v1' AND NOT tgisinternal",
    )
    .fetch_all(&f.pool)
    .await
    .expect("catalog scan");
    assert!(
        uncovered.is_empty(),
        "leaves missing contact guards: {uncovered:?}"
    );
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_decision_reply_original_threads_proof_under_same_root() {
    let f = Fixture::new().await;
    f.configure_contact_canvas(1_800_000_000).await;
    let root = f.message("thread root", 1_800_000_001);
    let outcome = f.decide(&root).await;
    assert_eq!(outcome.class, ContactClass::Routed);

    // A capability-carrying reply routes too, and its proof threads under the
    // original reply's root.
    let reply = EventBuilder::new(Kind::Custom(KIND_MESSAGE), "reply needs contact")
        .tags([
            Tag::parse(["h", &f.channel.to_string()]).expect("h"),
            Tag::parse([CAP, "1"]).expect("cap"),
            Tag::parse(["e", &root.id.to_hex(), "", "reply"]).expect("e"),
        ])
        .custom_created_at(Timestamp::from(1_800_000_002))
        .sign_with_keys(&f.owner)
        .expect("reply");
    let root_created =
        DateTime::from_timestamp(root.created_at.as_secs() as i64, 0).expect("root ts");
    let meta = ThreadMetadataParams {
        event_id: reply.id.as_bytes(),
        event_created_at: DateTime::from_timestamp(reply.created_at.as_secs() as i64, 0)
            .expect("reply ts"),
        channel_id: f.channel,
        parent_event_id: Some(root.id.as_bytes()),
        parent_event_created_at: Some(root_created),
        root_event_id: Some(root.id.as_bytes()),
        root_event_created_at: Some(root_created),
        depth: 1,
        broadcast: true,
    };
    let outcome = decide_contact_route(
        &f.pool,
        f.community,
        &reply,
        f.channel,
        Some(meta),
        &f.relay,
    )
    .await
    .expect("reply decision");
    assert_eq!(outcome.class, ContactClass::Routed);
    let proof = outcome.proof.expect("proof");
    let (tm_root, tm_parent): (Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT root_event_id, parent_event_id FROM thread_metadata \
         WHERE community_id=$1 AND event_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(proof.event.id.as_bytes().as_slice())
    .fetch_one(&f.pool)
    .await
    .expect("proof thread row");
    assert_eq!(tm_root, root.id.as_bytes().to_vec());
    assert_eq!(tm_parent, reply.id.as_bytes().to_vec());
}

// ---------------------------------------------------------------------------
// Claim lifecycle — the R5 claim/start/cancel seam
// ---------------------------------------------------------------------------

/// Route one original and return its decision id.
async fn routed_decision(f: &Fixture, at: u64) -> (Event, Vec<u8>) {
    f.configure_contact_canvas(at - 1).await;
    let original = f.message(&format!("routed {at}"), at);
    let outcome = f.decide(&original).await;
    assert_eq!(outcome.class, ContactClass::Routed);
    (original, f.decision_id_of(&outcome).to_vec())
}

fn receipt_for(f: &Fixture, decision: &[u8], generation: i64, at: u64) -> Event {
    let decision_hex = hex::encode(decision);
    EventBuilder::new(Kind::Custom(KIND_RECEIPT), "{\"summary\":\"done\"}")
        .tags([
            Tag::parse(["h", &f.channel.to_string()]).expect("h"),
            Tag::parse(["e", &decision_hex, "", "reply"]).expect("e"),
            Tag::parse(["claim", &format!("{decision_hex}:{generation}")]).expect("claim"),
        ])
        .custom_created_at(Timestamp::from(at))
        .sign_with_keys(&f.contact)
        .expect("receipt")
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_claim_full_lifecycle_claim_start_complete_via_receipt() {
    let f = Fixture::new().await;
    let (_original, decision) = routed_decision(&f, 1_800_000_010).await;
    let holder = f.contact.public_key().to_bytes().to_vec();

    let claimed = claim_contact_decision(&f.pool, f.community, &decision, f.channel, &holder, 60)
        .await
        .expect("claim");
    let generation = match claimed {
        ContactClaimResult::Granted { generation } => generation,
        other => panic!("expected Granted, got {other:?}"),
    };
    assert_eq!(
        f.claim_state(&decision).await,
        Some(("claimed".into(), generation))
    );

    let started = start_contact_claim(
        &f.pool,
        f.community,
        &decision,
        f.channel,
        &holder,
        generation,
    )
    .await
    .expect("start");
    assert_eq!(started, ContactControlResult::Applied);
    assert_eq!(
        f.claim_state(&decision).await,
        Some(("started".into(), generation))
    );

    let receipt = receipt_for(&f, &decision, generation, 1_800_000_030);
    let (stored, inserted) = insert_contact_receipt(
        &f.pool,
        f.community,
        &receipt,
        Some(f.channel),
        None,
        super::ContactClaimRef {
            decision_id: decision.as_slice().try_into().expect("32 bytes"),
            generation,
        },
    )
    .await
    .expect("receipt completes claim");
    assert!(inserted);
    assert_eq!(
        f.claim_state(&decision).await,
        Some(("completed".into(), generation))
    );
    let stored_receipt_id: Vec<u8> = sqlx::query_scalar(
        "SELECT receipt_id FROM contact_claims WHERE community_id=$1 AND decision_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(&decision)
    .fetch_one(&f.pool)
    .await
    .expect("receipt id recorded");
    assert_eq!(stored_receipt_id, stored.event.id.as_bytes().to_vec());
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_claim_fencing_wrong_generation_and_holder() {
    let f = Fixture::new().await;
    let (_original, decision) = routed_decision(&f, 1_800_000_010).await;
    let holder = f.contact.public_key().to_bytes().to_vec();

    // Only the configured contact may claim.
    let stranger = Keys::generate();
    let denied = claim_contact_decision(
        &f.pool,
        f.community,
        &decision,
        f.channel,
        &stranger.public_key().to_bytes().to_vec(),
        60,
    )
    .await
    .expect("claim by stranger");
    assert_eq!(
        denied,
        ContactClaimResult::Denied("only the configured contact may claim")
    );

    // Claim of an unknown decision is missing, not granted.
    let missing = claim_contact_decision(&f.pool, f.community, &[7_u8; 32], f.channel, &holder, 60)
        .await
        .expect("claim of unknown decision");
    assert_eq!(missing, ContactClaimResult::Missing);

    let generation =
        match claim_contact_decision(&f.pool, f.community, &decision, f.channel, &holder, 60)
            .await
            .expect("claim")
        {
            ContactClaimResult::Granted { generation } => generation,
            other => panic!("expected Granted, got {other:?}"),
        };

    // Idempotent re-claim returns the same generation — no fence bump.
    let again = claim_contact_decision(&f.pool, f.community, &decision, f.channel, &holder, 60)
        .await
        .expect("re-claim");
    assert_eq!(again, ContactClaimResult::Granted { generation });

    // Start at the wrong generation is fenced out.
    let stale = start_contact_claim(
        &f.pool,
        f.community,
        &decision,
        f.channel,
        &holder,
        generation + 9,
    )
    .await
    .expect("stale start");
    assert!(matches!(stale, ContactControlResult::Denied(_)));
    assert_eq!(
        f.claim_state(&decision).await,
        Some(("claimed".into(), generation))
    );

    // Correct generation starts.
    assert_eq!(
        start_contact_claim(
            &f.pool,
            f.community,
            &decision,
            f.channel,
            &holder,
            generation
        )
        .await
        .expect("start"),
        ContactControlResult::Applied
    );

    // A receipt at the stale generation cannot complete the claim.
    let stale_receipt = receipt_for(&f, &decision, generation + 9, 1_800_000_030);
    let rejected = insert_contact_receipt(
        &f.pool,
        f.community,
        &stale_receipt,
        Some(f.channel),
        None,
        super::ContactClaimRef {
            decision_id: decision.as_slice().try_into().expect("32"),
            generation: generation + 9,
        },
    )
    .await;
    assert!(
        matches!(rejected, Err(DbError::AccessDenied(_))),
        "stale-generation receipt must be denied: {rejected:?}"
    );
    // The rejected receipt must not have persisted.
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(stale_receipt.id.as_bytes().as_slice())
            .fetch_one(&f.pool)
            .await
            .expect("receipt readback");
    assert_eq!(count, 0);
    assert_eq!(
        f.claim_state(&decision).await,
        Some(("started".into(), generation))
    );
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_claim_cancel_is_terminal() {
    let f = Fixture::new().await;
    let (_original, decision) = routed_decision(&f, 1_800_000_010).await;
    let holder = f.contact.public_key().to_bytes().to_vec();
    let generation =
        match claim_contact_decision(&f.pool, f.community, &decision, f.channel, &holder, 60)
            .await
            .expect("claim")
        {
            ContactClaimResult::Granted { generation } => generation,
            other => panic!("expected Granted, got {other:?}"),
        };
    assert_eq!(
        cancel_contact_claim(
            &f.pool,
            f.community,
            &decision,
            f.channel,
            &holder,
            generation
        )
        .await
        .expect("cancel"),
        ContactControlResult::Applied
    );
    assert_eq!(
        f.claim_state(&decision).await,
        Some(("cancelled".into(), generation))
    );
    // Terminal: never claimable again.
    let reclaim = claim_contact_decision(&f.pool, f.community, &decision, f.channel, &holder, 60)
        .await
        .expect("reclaim");
    assert_eq!(reclaim, ContactClaimResult::Denied("decision cancelled"));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_claim_lease_expiry_reclaims_under_new_generation() {
    let f = Fixture::new().await;
    let (_original, decision) = routed_decision(&f, 1_800_000_010).await;
    let holder = f.contact.public_key().to_bytes().to_vec();
    let generation =
        match claim_contact_decision(&f.pool, f.community, &decision, f.channel, &holder, 60)
            .await
            .expect("claim")
        {
            ContactClaimResult::Granted { generation } => generation,
            other => panic!("expected Granted, got {other:?}"),
        };
    assert_eq!(generation, 1);

    // Expire the lease in place (same generation, live state transition is a
    // no-op through the guard), then re-claim under a fresh generation.
    sqlx::query(
        "UPDATE contact_claims SET lease_expires_at = clock_timestamp() - interval '1 second' \
         WHERE community_id=$1 AND decision_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(&decision)
    .execute(&f.pool)
    .await
    .expect("expire lease");
    let reclaimed = claim_contact_decision(&f.pool, f.community, &decision, f.channel, &holder, 60)
        .await
        .expect("reclaim expired lease");
    assert_eq!(
        reclaimed,
        ContactClaimResult::Granted {
            generation: generation + 1
        }
    );
    // The stale generation is fenced — start at the old generation fails.
    let stale = start_contact_claim(
        &f.pool,
        f.community,
        &decision,
        f.channel,
        &holder,
        generation,
    )
    .await
    .expect("stale start");
    assert!(matches!(stale, ContactControlResult::Denied(_)));
    assert_eq!(
        f.claim_state(&decision).await,
        Some(("claimed".into(), generation + 1))
    );
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_claim_original_delete_cancels_before_and_after_start() {
    let f = Fixture::new().await;

    // Delete before start: claimed claim dies in the same statement.
    let (original_a, decision_a) = routed_decision(&f, 1_800_000_010).await;
    let holder = f.contact.public_key().to_bytes().to_vec();
    let gen_a =
        match claim_contact_decision(&f.pool, f.community, &decision_a, f.channel, &holder, 60)
            .await
            .expect("claim a")
        {
            ContactClaimResult::Granted { generation } => generation,
            other => panic!("expected Granted, got {other:?}"),
        };
    assert!(
        soft_delete_event(&f.pool, f.community, original_a.id.as_bytes())
            .await
            .expect("soft delete original")
    );
    assert_eq!(
        f.claim_state(&decision_a).await,
        Some(("cancelled".into(), gen_a)),
        "deleting the original must fence the pending claim"
    );
    let denied_start =
        start_contact_claim(&f.pool, f.community, &decision_a, f.channel, &holder, gen_a)
            .await
            .expect("start after delete");
    assert_eq!(
        denied_start,
        ContactControlResult::Denied("decision cancelled")
    );

    // Delete after start: the started claim is cancelled, and a receipt for it
    // can no longer complete.
    let (original_b, decision_b) = routed_decision(&f, 1_800_000_050).await;
    let gen_b =
        match claim_contact_decision(&f.pool, f.community, &decision_b, f.channel, &holder, 60)
            .await
            .expect("claim b")
        {
            ContactClaimResult::Granted { generation } => generation,
            other => panic!("expected Granted, got {other:?}"),
        };
    assert_eq!(
        start_contact_claim(&f.pool, f.community, &decision_b, f.channel, &holder, gen_b)
            .await
            .expect("start b"),
        ContactControlResult::Applied
    );
    assert!(
        soft_delete_event(&f.pool, f.community, original_b.id.as_bytes())
            .await
            .expect("soft delete started original")
    );
    assert_eq!(
        f.claim_state(&decision_b).await,
        Some(("cancelled".into(), gen_b))
    );
    let receipt = receipt_for(&f, &decision_b, gen_b, 1_800_000_060);
    let rejected = insert_contact_receipt(
        &f.pool,
        f.community,
        &receipt,
        Some(f.channel),
        None,
        super::ContactClaimRef {
            decision_id: decision_b.as_slice().try_into().expect("32"),
            generation: gen_b,
        },
    )
    .await;
    assert!(
        matches!(rejected, Err(DbError::AccessDenied(_))),
        "cancelled claim cannot complete: {rejected:?}"
    );
}

// ---------------------------------------------------------------------------
// Guard surface — unauthorized direct writes must fail closed
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_guard_direct_writes_without_decision_path_are_rejected() {
    let f = Fixture::new().await;
    f.configure_contact_canvas(1_800_000_000).await;
    let original = f.message("guard surface", 1_800_000_001);

    // A raw classified write outside the reviewed path — the GUC flag is the
    // gate `contact_classify_original_v1` enforces.
    constraint_error(
        sqlx::query(
            "INSERT INTO events (community_id,id,pubkey,created_at,kind,tags,content,sig,channel_id,contact_class) \
             VALUES ($1,$2,$3,$4,9,$5,$6,$7,$8,1)",
        )
        .bind(f.community.as_uuid())
        .bind(original.id.as_bytes().as_slice())
        .bind(original.pubkey.to_bytes().to_vec())
        .bind(DateTime::from_timestamp(original.created_at.as_secs() as i64, 0).expect("ts"))
        .bind(serde_json::to_value(&original.tags).expect("tags"))
        .bind(&original.content)
        .bind(original.sig.serialize().to_vec())
        .bind(f.channel)
        .execute(&f.pool)
        .await
        .map(|_| ()),
        "classified write without the decision flag",
    );

    // A bare route row with no consistent decision set fails at COMMIT.
    let mut tx = f.pool.begin().await.expect("bare route tx");
    sqlx::query(
        "INSERT INTO contact_routes \
         (community_id, original_id, original_created_at, channel_id, \
          contact_pubkey, relay_pubkey, decision_id, decision_created_at, stripe) \
         VALUES ($1, $2, now(), $3, $4, $5, $6, now(), 0)",
    )
    .bind(f.community.as_uuid())
    .bind(original.id.as_bytes().as_slice())
    .bind(f.channel)
    .bind(f.contact.public_key().to_bytes().to_vec())
    .bind(f.relay.public_key().to_bytes().to_vec())
    .bind(vec![9_u8; 32])
    .execute(&mut *tx)
    .await
    .expect("route insert itself is deferred-checked");
    constraint_error(tx.commit().await, "route without proof/original/claim");

    // Once routed, the evidence is immutable: class, route, and proof.
    let outcome = f.decide(&original).await;
    assert_eq!(outcome.class, ContactClass::Routed);
    constraint_error(
        sqlx::query("UPDATE events SET contact_class=4 WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(original.id.as_bytes().as_slice())
            .execute(&f.pool)
            .await
            .map(|_| ()),
        "recorded classification is immutable",
    );
    constraint_error(
        sqlx::query("DELETE FROM contact_routes WHERE community_id=$1")
            .bind(f.community.as_uuid())
            .execute(&f.pool)
            .await
            .map(|_| ()),
        "routes are never deleted",
    );
    let proof_id = f.decision_id_of(&outcome).to_vec();
    constraint_error(
        sqlx::query("UPDATE events SET deleted_at=now() WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(&proof_id)
            .execute(&f.pool)
            .await
            .map(|_| ()),
        "decision proof is fully immutable",
    );
    constraint_error(
        sqlx::query("DELETE FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(&proof_id)
            .execute(&f.pool)
            .await
            .map(|_| ()),
        "hard-deleting a decision proof is forbidden",
    );
    // And the original still cannot be hard-deleted outside the fenced
    // community executor.
    constraint_error(
        sqlx::query("DELETE FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(original.id.as_bytes().as_slice())
            .execute(&f.pool)
            .await
            .map(|_| ()),
        "hard-deleting the original is forbidden",
    );
}

#[test]
fn contact_claim_tag_parsing_is_strict() {
    let signer = Keys::generate();
    let decision = [3_u8; 32];
    let with_claim = EventBuilder::new(Kind::Custom(KIND_RECEIPT), "x")
        .tags([Tag::parse(["claim", &format!("{}:4", hex::encode(decision))]).expect("claim tag")])
        .sign_with_keys(&signer)
        .expect("signed");
    assert_eq!(
        parse_claim_tag(&with_claim).expect("parse"),
        Some(super::ContactClaimRef {
            decision_id: decision,
            generation: 4
        })
    );

    let without = EventBuilder::new(Kind::Custom(KIND_RECEIPT), "x")
        .sign_with_keys(&signer)
        .expect("signed");
    assert_eq!(parse_claim_tag(&without).expect("parse"), None);

    for value in [
        "not-a-pair",
        &format!("{}", hex::encode(decision)),
        "aa:bb",
        "aa:1:2",
    ] {
        let bad = EventBuilder::new(Kind::Custom(KIND_RECEIPT), "x")
            .tags([Tag::parse(["claim", value]).expect("tag")])
            .sign_with_keys(&signer)
            .expect("signed");
        assert!(
            parse_claim_tag(&bad).is_err(),
            "malformed claim tag {value:?} must be rejected"
        );
    }
}
