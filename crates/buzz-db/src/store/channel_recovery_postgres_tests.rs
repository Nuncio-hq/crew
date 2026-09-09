//! PostgreSQL proofs bound to the actual atomic create and discovery APIs.

use crate::{
    channel::{ChannelType, ChannelVisibility},
    Db,
};
use buzz_core::CommunityId;
use nostr::{Event, EventBuilder, Keys, Kind, Tag};
use sqlx::PgPool;
use uuid::Uuid;

async fn fixture() -> (Db, PgPool, CommunityId, Keys, Uuid, Event) {
    let base = crate::test_support::database_url();
    let admin = PgPool::connect(&base).await.expect("fixture admin");
    let name = format!("crew_atomic_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .expect("owned scratch database");
    let mut options: sqlx::postgres::PgConnectOptions =
        base.parse().expect("fixture connection options");
    options = options.database(&name);
    let pool = PgPool::connect_with(options).await.expect("scratch PG");
    crate::migration::run_migrations(&pool)
        .await
        .expect("current migrations");
    admin.close().await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO communities (id,host) VALUES ($1,$2)")
        .bind(id)
        .bind(format!("atomic-channel-{}.example", id.simple()))
        .execute(&pool)
        .await
        .expect("community");
    let keys = Keys::generate();
    let channel = Uuid::new_v4();
    let event = EventBuilder::new(Kind::Custom(9007), "")
        .tags([
            Tag::parse(["h", &channel.to_string()]).expect("h"),
            Tag::parse(["name", "recovery"]).expect("name"),
            Tag::parse(["crew-atomic-create", "1"]).expect("optin"),
        ])
        .sign_with_keys(&keys)
        .expect("sign");
    (
        Db::from_pool(pool.clone()),
        pool,
        CommunityId::from_uuid(id),
        keys,
        channel,
        event,
    )
}

async fn create(db: &Db, community: CommunityId, channel: Uuid, event: &Event) {
    db.create_channel_with_event(
        community,
        channel,
        "recovery",
        ChannelType::Stream,
        ChannelVisibility::Open,
        None,
        None,
        event,
        None,
    )
    .await
    .expect("atomic create");
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn atomic_channel_event_failure_rolls_back_channel_and_owner() {
    let (db, pool, community, _, channel, event) = fixture().await;
    // Force the actual event insert to reject after channel+owner preparation.
    db.insert_event(community, &event, None)
        .await
        .expect("preexisting event ID");
    assert!(db
        .create_channel_with_event(
            community,
            channel,
            "recovery",
            ChannelType::Stream,
            ChannelVisibility::Open,
            None,
            None,
            &event,
            None
        )
        .await
        .is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM channels WHERE community_id=$1 AND id=$2")
            .bind(community.as_uuid())
            .bind(channel)
            .fetch_one(&pool)
            .await
            .expect("channel count");
    let owners: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM channel_members WHERE community_id=$1 AND channel_id=$2",
    )
    .bind(community.as_uuid())
    .bind(channel)
    .fetch_one(&pool)
    .await
    .expect("owner count");
    assert_eq!((count, owners), (0, 0));
    cleanup(pool).await;
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn atomic_channel_commit_replays_same_identity_and_requires_current_owner() {
    let (db, pool, community, keys, channel, event) = fixture().await;
    create(&db, community, channel, &event).await;
    let relay = Keys::generate();
    let snapshot = db
        .lock_discovery_snapshot(
            community,
            channel,
            &relay.public_key().to_bytes(),
            Some(&event),
        )
        .await
        .expect("exact replay");
    assert_eq!(snapshot.channel.id, channel);
    assert_eq!(snapshot.members.len(), 1);
    snapshot.commit().await.expect("release");
    sqlx::query("UPDATE channel_members SET role='member' WHERE community_id=$1 AND channel_id=$2 AND pubkey=$3")
        .bind(community.as_uuid()).bind(channel).bind(keys.public_key().to_bytes().as_slice()).execute(&pool).await.expect("fixture demotion");
    let replay_denied = db
        .lock_discovery_snapshot(
            community,
            channel,
            &relay.public_key().to_bytes(),
            Some(&event),
        )
        .await
        .is_err();
    cleanup(pool).await;
    assert!(replay_denied, "demoted creator cannot replay discovery");
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn atomic_channel_legacy_uuid_without_original_event_is_not_adopted() {
    let (db, pool, community, keys, channel, event) = fixture().await;
    db.create_channel_with_id(
        community,
        channel,
        "legacy",
        ChannelType::Stream,
        ChannelVisibility::Open,
        None,
        &keys.public_key().to_bytes(),
        None,
    )
    .await
    .expect("legacy partial channel");
    assert!(db
        .create_channel_with_event(
            community,
            channel,
            "recovery",
            ChannelType::Stream,
            ChannelVisibility::Open,
            None,
            None,
            &event,
            None
        )
        .await
        .is_err());
    assert_eq!(
        db.get_channel(community, channel)
            .await
            .expect("channel remains")
            .name,
        "legacy"
    );
    assert!(db
        .lock_discovery_snapshot(
            community,
            channel,
            &Keys::generate().public_key().to_bytes(),
            Some(&event)
        )
        .await
        .is_err());
    cleanup(pool).await;
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn atomic_channel_discovery_fences_delete_until_snapshot_commit() {
    let (db, pool, community, _, channel, event) = fixture().await;
    create(&db, community, channel, &event).await;
    let relay = Keys::generate();
    let snapshot = db
        .lock_discovery_snapshot(
            community,
            channel,
            &relay.public_key().to_bytes(),
            Some(&event),
        )
        .await
        .expect("snapshot");
    let mut writer = pool.acquire().await.expect("delete connection");
    sqlx::query("SET lock_timeout='100ms'")
        .execute(&mut *writer)
        .await
        .expect("bounded lock wait");
    let error = sqlx::query("UPDATE channels SET deleted_at=NOW() WHERE community_id=$1 AND id=$2")
        .bind(community.as_uuid())
        .bind(channel)
        .execute(&mut *writer)
        .await;
    let blocked_code = error.err().and_then(|error| {
        error
            .as_database_error()
            .and_then(|error| error.code())
            .map(|code| code.into_owned())
    });
    snapshot.commit().await.expect("release snapshot");
    sqlx::query("UPDATE channels SET deleted_at=NOW() WHERE community_id=$1 AND id=$2")
        .bind(community.as_uuid())
        .bind(channel)
        .execute(&mut *writer)
        .await
        .expect("delete after release");
    assert!(db
        .lock_discovery_snapshot(
            community,
            channel,
            &relay.public_key().to_bytes(),
            Some(&event)
        )
        .await
        .is_err());
    drop(writer);
    cleanup(pool).await;
    assert_eq!(
        blocked_code.as_deref(),
        Some("55P03"),
        "delete must block on snapshot FOR SHARE"
    );
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn atomic_channel_discovery_failure_cannot_leave_partial_heads() {
    let (db, pool, community, _, channel, event) = fixture().await;
    create(&db, community, channel, &event).await;
    let relay = Keys::generate();
    let mut snapshot = db
        .lock_discovery_snapshot(
            community,
            channel,
            &relay.public_key().to_bytes(),
            Some(&event),
        )
        .await
        .expect("snapshot");
    let metadata = EventBuilder::new(Kind::Custom(39000), "")
        .tags([Tag::parse(["d", &channel.to_string()]).expect("d")])
        .sign_with_keys(&relay)
        .expect("metadata");
    snapshot
        .store(&metadata)
        .await
        .expect("first head stored in uncommitted transaction");
    let invalid_roster = EventBuilder::new(Kind::Custom(39002), "")
        .tags([Tag::parse(["d", &channel.to_string()]).expect("d")])
        .sign_with_keys(&relay)
        .expect("roster");
    assert!(snapshot.store(&invalid_roster).await.is_err());
    drop(snapshot);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE community_id=$1 AND channel_id=$2 AND kind BETWEEN 39000 AND 39002")
        .bind(community.as_uuid()).bind(channel).fetch_one(&pool).await.expect("head count");
    assert_eq!(count, 0);
    cleanup(pool).await;
}

async fn cleanup(pool: PgPool) {
    let name = pool
        .connect_options()
        .get_database()
        .expect("scratch database name")
        .to_owned();
    assert!(
        name.starts_with("crew_atomic_")
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    );
    pool.close().await;
    let admin = PgPool::connect(&crate::test_support::database_url())
        .await
        .expect("fixture admin");
    // A background autovacuum can briefly outlive the closed client pool.
    // Never terminate another role's backend to clean a test-owned database.
    let mut last_error = None;
    for attempt in 0..5 {
        match sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE {name}")))
            .execute(&admin)
            .await
        {
            Ok(_) => {
                admin.close().await;
                return;
            }
            Err(error) => {
                let busy = error
                    .as_database_error()
                    .and_then(|error| error.code())
                    .as_deref()
                    == Some("55006");
                last_error = Some(error);
                if !busy {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250 << attempt)).await;
            }
        }
    }
    admin.close().await;
    panic!("drop owned scratch database {name}: {last_error:?}");
}
