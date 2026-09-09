//! #355 source-seam counterexamples, not an implementation of contact routing.
//! The fixture runner owns the disposable database and its cleanup.

#[path = "contact_r4_postgres_tests.rs"]
mod r4_postgres_tests;

use super::{insert_event_with_thread_metadata, query_events, soft_delete_event, EventQuery};
use buzz_core::crew_role::{read_canvas_crew_metadata, resolve_routing};
use buzz_core::{CommunityId, StoredEvent};
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
struct Fixture {
    pool: PgPool,
    community: CommunityId,
    channel: Uuid,
    owner: Keys,
}

impl Fixture {
    async fn new() -> Self {
        // Refuse the shared development fallback even if test_support has one.
        let explicit = std::env::var("BUZZ_TEST_DATABASE_URL")
            .expect("#355 requires an explicitly assigned disposable BUZZ_TEST_DATABASE_URL");
        assert!(!explicit.is_empty(), "fixture URL must not be empty");
        let pool = PgPoolOptions::new()
            .max_connections(2)
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
        eprintln!("contact proof isolated database: {database}");
        let community_id = Uuid::new_v4();
        let channel = Uuid::new_v4();
        let owner = Keys::generate();
        sqlx::query("INSERT INTO communities (id, host) VALUES ($1, $2)")
            .bind(community_id)
            .bind(format!("contact-proof-{}.example", community_id.simple()))
            .execute(&pool)
            .await
            .expect("create isolated community");
        sqlx::query(
            "INSERT INTO channels (community_id, id, name, created_by) VALUES ($1, $2, $3, $4)",
        )
        .bind(community_id)
        .bind(channel)
        .bind("contact-proof")
        .bind(owner.public_key().to_bytes().to_vec())
        .execute(&pool)
        .await
        .expect("create fixture channel");
        // The desired-state template now carries the inert production
        // retention foundation. This source-seam fixture installs its own
        // guard catalog later, so remove only the production trigger copies
        // before tests exercise an old writer that predates that catalog.
        sqlx::raw_sql(
            r#"
            DO $$
            DECLARE
                relation_name regclass;
            BEGIN
                FOR relation_name IN
                    SELECT c.oid::regclass
                    FROM pg_class c
                    WHERE c.oid = 'events'::regclass
                       OR c.oid IN (
                           SELECT inhrelid FROM pg_inherits
                           WHERE inhparent = 'events'::regclass
                       )
                LOOP
                    EXECUTE format(
                        'DROP TRIGGER IF EXISTS contact_classify_original_v1 ON %s',
                        relation_name
                    );
                    EXECUTE format(
                        'DROP TRIGGER IF EXISTS contact_guard_original_v1 ON %s',
                        relation_name
                    );
                END LOOP;
            END
            $$;
            DROP TRIGGER IF EXISTS contact_stamp_decision_v1 ON contact_routes;
            DROP TRIGGER IF EXISTS contact_route_immutable_v1 ON contact_routes;
            "#,
        )
        .execute(&pool)
        .await
        .expect("remove production contact triggers for isolated fixture");
        Self {
            pool,
            community: CommunityId::from_uuid(community_id),
            channel,
            owner,
        }
    }

    fn canvas(&self, author: &Keys, contact: Option<&str>, timestamp: u64) -> Event {
        let content = contact.map_or_else(
            || "```crew\ndefinitions: {}\n```".to_string(),
            |key| format!("```crew\ncontact: {key}\n```"),
        );
        EventBuilder::new(Kind::Custom(40100), content)
            .tags([Tag::parse(["h", &self.channel.to_string()]).expect("channel tag")])
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(author)
            .expect("sign canvas")
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

    async fn latest(&self) -> StoredEvent {
        let mut events = query_events(
            &self.pool,
            &EventQuery {
                kinds: Some(vec![40100]),
                channel_id: Some(self.channel),
                limit: Some(1),
                ..EventQuery::for_community(self.community)
            },
        )
        .await
        .expect("production latest-canvas query");
        assert_eq!(events.len(), 1);
        events.pop().expect("one winner")
    }
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_latest_foreign_canvas_is_visible_but_not_owner_authority() {
    let fixture = Fixture::new().await;
    let contact_a = Keys::generate().public_key().to_hex();
    let contact_b = Keys::generate().public_key().to_hex();
    let older = fixture.canvas(&fixture.owner, Some(&contact_a), 1_800_000_000);
    let foreign = fixture.canvas(&Keys::generate(), Some(&contact_b), 1_800_000_001);
    fixture.insert(&older).await;
    fixture.insert(&foreign).await;
    let winner = fixture.latest().await;
    assert_eq!(
        winner.event.id, foreign.id,
        "must not filter backward to owner canvas"
    );
    let author = winner.event.pubkey.to_hex();
    let owner = fixture.owner.public_key().to_hex();
    let metadata = read_canvas_crew_metadata(Some(&winner.event.content), Some(&author), &owner);
    assert_eq!(metadata.contact_pubkey.as_deref(), Some(contact_b.as_str()));
    assert_eq!(metadata.crew_authority, "foreign");
    assert_eq!(
        resolve_routing(&winner.event.content, &author, &owner).expect("resolve roles"),
        None
    );
    // Display contact is intentionally retained. No authoritative contact
    // resolver/decision exists yet; do not turn these assertions into one.
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_no_contact_winner_and_deleted_winner_use_current_query() {
    let fixture = Fixture::new().await;
    let contact = Keys::generate().public_key().to_hex();
    let older = fixture.canvas(&fixture.owner, Some(&contact), 1_800_000_000);
    let newest = fixture.canvas(&fixture.owner, None, 1_800_000_001);
    fixture.insert(&older).await;
    fixture.insert(&newest).await;
    let winner = fixture.latest().await;
    assert_eq!(winner.event.id, newest.id);
    let owner = fixture.owner.public_key().to_hex();
    assert!(
        read_canvas_crew_metadata(Some(&winner.event.content), Some(&owner), &owner)
            .contact_pubkey
            .is_none()
    );
    assert!(
        soft_delete_event(&fixture.pool, fixture.community, newest.id.as_bytes())
            .await
            .expect("production canvas soft delete")
    );
    assert_eq!(fixture.latest().await.event.id, older.id);
    // This proves only current query behavior, never retroactive routing.
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_equal_timestamp_winner_is_lowest_id_not_insertion_order() {
    let fixture = Fixture::new().await;
    let a = fixture.canvas(
        &fixture.owner,
        Some(&Keys::generate().public_key().to_hex()),
        1_800_000_000,
    );
    let b = fixture.canvas(
        &fixture.owner,
        Some(&Keys::generate().public_key().to_hex()),
        1_800_000_000,
    );
    let (lower, higher) = if a.id < b.id { (&a, &b) } else { (&b, &a) };
    fixture.insert(lower).await;
    fixture.insert(higher).await;
    assert_eq!(fixture.latest().await.event.id, lower.id);
}
