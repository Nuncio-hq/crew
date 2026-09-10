//! Project dispatch proofs bind the production persistence function.
use super::*;
use buzz_core::CommunityId;
use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

fn signed(keys: &Keys, kind: u16, d: &str, expected: Option<&str>, channel: Option<Uuid>) -> Event {
    let mut tags = vec![Tag::parse(["d", d]).unwrap()];
    if let Some(expected) = expected {
        tags.push(Tag::parse(["expected-revision", expected]).unwrap());
    }
    if let Some(channel) = channel {
        tags.push(Tag::parse(["buzz-related-channel", &channel.to_string()]).unwrap());
    }
    EventBuilder::new(Kind::Custom(kind), "")
        .tags(tags)
        .custom_created_at(Timestamp::now())
        .sign_with_keys(keys)
        .unwrap()
}

#[tokio::test]
async fn conditional_project_flag_off_rejects_before_database_access() {
    // This pool has no live database. Correct dispatch rejects before acquiring it.
    let pool = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(50))
        .connect_lazy_with(
            PgConnectOptions::new()
                .host("/nonexistent/crew-project-dispatch-test")
                .database("crew_project_dispatch_test"),
        );
    let db = buzz_db::Db::from_pool(pool);
    let tenant = TenantContext::resolved(CommunityId::from_uuid(Uuid::new_v4()), "fixture.example");
    let keys = Keys::generate();
    let event = signed(&keys, 30621, "project", Some("absent"), None);
    for (conditional, project, extension) in [
        (
            false,
            false,
            super::super::conditional_publication::EXTENSION,
        ),
        (
            false,
            true,
            super::super::conditional_publication::EXTENSION,
        ),
        (true, false, PROJECT_EXTENSION),
    ] {
        let result = persist_with_db(&db, &tenant, &event, None, conditional, project).await;
        assert!(
            matches!(result, Err(IngestError::Rejected(ref reason)) if reason == &format!("unsupported: {extension}"))
        );
    }
}

struct Fixture {
    db: buzz_db::Db,
    pool: sqlx::PgPool,
    tenant: TenantContext,
    keys: Keys,
    channel: Uuid,
}
impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("CREW_PROJECT_DISPATCH_TEST_DATABASE_URL")
            .expect("explicit allocated Project-dispatch fixture required");
        let options: PgConnectOptions = url.parse().unwrap();
        assert!(options
            .get_database()
            .is_some_and(|name| name.starts_with("crew_project_dispatch_")));
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect_with(options.options([("statement_timeout", "10s"), ("lock_timeout", "5s")]))
            .await
            .unwrap();
        let name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(name.starts_with("crew_project_dispatch_"));
        let id = Uuid::new_v4();
        let host = format!("dispatch-{}.example", id.simple());
        sqlx::query("INSERT INTO communities(id,host) VALUES($1,$2)")
            .bind(id)
            .bind(&host)
            .execute(&pool)
            .await
            .unwrap();
        let db = buzz_db::Db::from_pool(pool.clone());
        let keys = Keys::generate();
        let tenant = TenantContext::resolved(CommunityId::from_uuid(id), host);
        let channel = db
            .create_channel(
                tenant.community(),
                "target",
                buzz_db::channel::ChannelType::Stream,
                buzz_db::channel::ChannelVisibility::Open,
                None,
                keys.public_key().as_bytes(),
                None,
            )
            .await
            .unwrap()
            .id;
        Self {
            db,
            pool,
            tenant,
            keys,
            channel,
        }
    }
    async fn live(&self, d: &str) -> Vec<Vec<u8>> {
        sqlx::query_scalar("SELECT id FROM events WHERE community_id=$1 AND pubkey=$2 AND kind=30621 AND d_tag=$3 AND deleted_at IS NULL")
            .bind(self.tenant.community().as_uuid()).bind(self.keys.public_key().as_bytes().as_slice()).bind(d).fetch_all(&self.pool).await.unwrap()
    }
}

#[tokio::test]
#[ignore = "requires allocated migrated crew_project_dispatch_ database and separate execution approval"]
async fn conditional_project_dispatch_validates_target_rolls_back_and_replays() {
    let f = Fixture::new().await;
    let a = signed(&f.keys, 30621, "project", Some("absent"), None);
    assert!(
        persist_with_db(&f.db, &f.tenant, &a, None, true, true)
            .await
            .unwrap()
            .1
    );
    sqlx::query("UPDATE channels SET archived_at=now() WHERE community_id=$1 AND id=$2")
        .bind(f.tenant.community().as_uuid())
        .bind(f.channel)
        .execute(&f.pool)
        .await
        .unwrap();
    let b = signed(
        &f.keys,
        30621,
        "project",
        Some(&a.id.to_hex()),
        Some(f.channel),
    );
    let rejected = persist_with_db(&f.db, &f.tenant, &b, None, true, true).await;
    assert!(
        matches!(rejected,Err(IngestError::Rejected(ref reason)) if reason=="restricted: project-channel-association")
    );
    assert_eq!(f.live("project").await, vec![a.id.as_bytes().to_vec()]);
    sqlx::query("UPDATE channels SET archived_at=NULL WHERE community_id=$1 AND id=$2")
        .bind(f.tenant.community().as_uuid())
        .bind(f.channel)
        .execute(&f.pool)
        .await
        .unwrap();
    assert!(
        persist_with_db(&f.db, &f.tenant, &b, None, true, true)
            .await
            .unwrap()
            .1
    );
    sqlx::query(
        "UPDATE channel_members SET removed_at=now() WHERE community_id=$1 AND channel_id=$2",
    )
    .bind(f.tenant.community().as_uuid())
    .bind(f.channel)
    .execute(&f.pool)
    .await
    .unwrap();
    assert!(
        !persist_with_db(&f.db, &f.tenant, &b, None, true, true)
            .await
            .unwrap()
            .1
    );
    assert_eq!(f.live("project").await, vec![b.id.as_bytes().to_vec()]);
    f.pool.close().await;
}

#[tokio::test]
#[ignore = "requires allocated migrated crew_project_dispatch_ database and separate execution approval"]
async fn conditional_project_capability_rollback_cannot_fall_through_to_cas() {
    let f = Fixture::new().await;
    let a = signed(&f.keys, 30621, "project", Some("absent"), None);
    assert!(
        persist_with_db(&f.db, &f.tenant, &a, None, true, true)
            .await
            .unwrap()
            .1
    );
    let b = signed(
        &f.keys,
        30621,
        "project",
        Some(&a.id.to_hex()),
        Some(f.channel),
    );
    let result = persist_with_db(&f.db, &f.tenant, &b, None, true, false).await;
    assert!(
        matches!(result,Err(IngestError::Rejected(ref reason)) if reason==&format!("unsupported: {PROJECT_EXTENSION}"))
    );
    assert_eq!(f.live("project").await, vec![a.id.as_bytes().to_vec()]);
    f.pool.close().await;
}

#[tokio::test]
#[ignore = "requires allocated migrated crew_project_dispatch_ database and separate execution approval"]
async fn conditional_project_flag_preserves_legacy_and_repository_dispatch() {
    let f = Fixture::new().await;
    let legacy = signed(&f.keys, 30621, "legacy", None, Some(f.channel));
    assert!(
        persist_with_db(&f.db, &f.tenant, &legacy, None, false, false)
            .await
            .unwrap()
            .1
    );
    let repo = signed(&f.keys, 30617, "repo", Some("absent"), None);
    assert!(
        persist_with_db(&f.db, &f.tenant, &repo, None, true, false)
            .await
            .unwrap()
            .1
    );
    let wiki = EventBuilder::new(Kind::Custom(30623), "fixture body")
        .tags([
            Tag::parse(["d", &format!("repo/p1-{}", "a".repeat(64))]).unwrap(),
            Tag::parse(["a", &format!("30617:{}:repo", f.keys.public_key().to_hex())]).unwrap(),
            Tag::parse(["expected-revision", "absent"]).unwrap(),
            Tag::parse(["wiki-version", "1"]).unwrap(),
            Tag::parse(["source-kind", "git"]).unwrap(),
            Tag::parse(["wiki-snapshot", &Uuid::new_v4().to_string()]).unwrap(),
            Tag::parse(["commit", &"a".repeat(40)]).unwrap(),
        ])
        .sign_with_keys(&f.keys)
        .unwrap();
    assert!(
        persist_with_db(&f.db, &f.tenant, &wiki, None, true, false)
            .await
            .unwrap()
            .1
    );
    assert!(
        !persist_with_db(&f.db, &f.tenant, &wiki, None, true, false)
            .await
            .unwrap()
            .1
    );
    f.pool.close().await;
}
