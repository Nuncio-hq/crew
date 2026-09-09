//! Opt-in proofs against an explicitly allocated, migrated scratch database.
use super::*;
use crate::channel::{ChannelType, ChannelVisibility};
use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::PgPool;

#[path = "project_channel_link_race_tests.rs"]
mod races_postgres_tests;

struct Fixture {
    db: Db,
    pool: PgPool,
    community: CommunityId,
    keys: Keys,
    channel: Uuid,
    timestamp: u64,
}

impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("CREW_PROJECT_LINK_TEST_DATABASE_URL")
            .expect("explicit owned Project-link database required");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(5))
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("SET statement_timeout='10s'")
                        .execute(&mut *connection)
                        .await?;
                    sqlx::query("SET lock_timeout='5s'")
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        let name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            name.starts_with("crew_project_link_"),
            "refusing non-owned database"
        );
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO communities (id,host) VALUES ($1,$2)")
            .bind(id)
            .bind(format!("project-link-{}.example", id.simple()))
            .execute(&pool)
            .await
            .unwrap();
        let db = Db::from_pool(pool.clone());
        let keys = Keys::generate();
        let community = CommunityId::from_uuid(id);
        let channel = db
            .create_channel(
                community,
                "link-target",
                ChannelType::Stream,
                ChannelVisibility::Open,
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
            community,
            keys,
            channel,
            timestamp: Timestamp::now().as_secs(),
        }
    }

    fn event(&self, timestamp: u64, linked: bool) -> Event {
        let mut tags = vec![Tag::parse(["d", "project"]).unwrap()];
        if linked {
            tags.push(Tag::parse(["buzz-related-channel", &self.channel.to_string()]).unwrap());
        }
        EventBuilder::new(Kind::Custom(30621), "")
            .tags(tags)
            .custom_created_at(Timestamp::from(self.timestamp + timestamp))
            .sign_with_keys(&self.keys)
            .unwrap()
    }

    async fn write(
        &self,
        event: &Event,
        expected: Option<&Event>,
    ) -> Result<ParameterizedReplaceStatus> {
        let mut tx = self.pool.begin().await?;
        let condition = expected.map_or(ParameterizedReplacePrecondition::Unconditional, |old| {
            ParameterizedReplacePrecondition::ExpectedRevision(old.id.as_bytes())
        });
        let result = self
            .db
            .replace_project_event_in_transaction(
                &mut tx,
                self.community,
                event,
                "project",
                condition,
            )
            .await;
        match result {
            Ok(result) => {
                tx.commit().await?;
                Ok(result.status)
            }
            Err(error) => {
                tx.rollback().await?;
                Err(error)
            }
        }
    }

    async fn live_ids(&self) -> Vec<Vec<u8>> {
        sqlx::query_scalar(
            "SELECT id FROM events WHERE community_id=$1 AND kind=30621 AND deleted_at IS NULL",
        )
        .bind(self.community.as_uuid())
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_ineligible_target_rolls_back_replacement() {
    let f = Fixture::new().await;
    let a = f.event(100, false);
    assert_eq!(
        f.write(&a, None).await.unwrap(),
        ParameterizedReplaceStatus::Inserted
    );
    sqlx::query("UPDATE channels SET archived_at=now() WHERE community_id=$1 AND id=$2")
        .bind(f.community.as_uuid())
        .bind(f.channel)
        .execute(&f.pool)
        .await
        .unwrap();
    let b = f.event(101, true);
    assert!(f.write(&b, Some(&a)).await.is_err());
    assert_eq!(f.live_ids().await, vec![a.id.as_bytes().to_vec()]);
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_exact_replay_survives_changed_eligibility() {
    let f = Fixture::new().await;
    let a = f.event(100, true);
    assert_eq!(
        f.write(&a, None).await.unwrap(),
        ParameterizedReplaceStatus::Inserted
    );
    sqlx::query(
        "UPDATE channel_members SET removed_at=now() WHERE community_id=$1 AND channel_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(f.channel)
    .execute(&f.pool)
    .await
    .unwrap();
    assert_eq!(
        f.write(&a, None).await.unwrap(),
        ParameterizedReplaceStatus::Duplicate
    );
    assert_eq!(f.live_ids().await, vec![a.id.as_bytes().to_vec()]);
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_deleted_successor_cannot_resurrect_old_head() {
    let f = Fixture::new().await;
    let a = f.event(100, false);
    let b = f.event(101, true);
    assert_eq!(
        f.write(&a, None).await.unwrap(),
        ParameterizedReplaceStatus::Inserted
    );
    assert_eq!(
        f.write(&b, Some(&a)).await.unwrap(),
        ParameterizedReplaceStatus::Inserted
    );
    sqlx::query("UPDATE events SET deleted_at=now() WHERE community_id=$1 AND id=$2")
        .bind(f.community.as_uuid())
        .bind(b.id.as_bytes().as_slice())
        .execute(&f.pool)
        .await
        .unwrap();
    assert_ne!(
        f.write(&a, None).await.unwrap(),
        ParameterizedReplaceStatus::Inserted
    );
    assert!(f.live_ids().await.is_empty());
    let c = f.event(102, false);
    assert_eq!(
        f.write(&c, Some(&a)).await.unwrap(),
        ParameterizedReplaceStatus::RevisionMissing
    );
    assert!(f.live_ids().await.is_empty());
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_membership_row_lock_lasts_until_outer_commit() {
    let f = Fixture::new().await;
    let a = f.event(100, false);
    f.write(&a, None).await.unwrap();
    let b = f.event(101, true);
    let mut link = f.pool.begin().await.unwrap();
    let result =
        f.db.replace_project_event_in_transaction(
            &mut link,
            f.community,
            &b,
            "project",
            ParameterizedReplacePrecondition::ExpectedRevision(a.id.as_bytes()),
        )
        .await
        .unwrap();
    assert_eq!(result.status, ParameterizedReplaceStatus::Inserted);
    // The direct row writer deliberately bypasses the membership advisory lock,
    // as relay-admin kick does. A bounded lock timeout makes missing row fencing
    // falsifiable without a timing-based assertion about spawned task progress.
    let mut kick = f.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL lock_timeout='100ms'")
        .execute(&mut *kick)
        .await
        .unwrap();
    let error = sqlx::query("UPDATE channel_members SET removed_at=now() WHERE community_id=$1 AND channel_id=$2 AND pubkey=$3")
        .bind(f.community.as_uuid()).bind(f.channel).bind(f.keys.public_key().as_bytes().as_slice())
        .execute(&mut *kick).await.unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
    kick.rollback().await.unwrap();
    link.commit().await.unwrap();
    sqlx::query("UPDATE channel_members SET removed_at=now() WHERE community_id=$1 AND channel_id=$2 AND pubkey=$3")
        .bind(f.community.as_uuid()).bind(f.channel).bind(f.keys.public_key().as_bytes().as_slice())
        .execute(&f.pool).await.unwrap();
    assert_eq!(f.live_ids().await, vec![b.id.as_bytes().to_vec()]);
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_channel_row_lock_lasts_until_outer_commit() {
    let f = Fixture::new().await;
    let a = f.event(100, true);
    let mut link = f.pool.begin().await.unwrap();
    f.db.replace_project_event_in_transaction(
        &mut link,
        f.community,
        &a,
        "project",
        ParameterizedReplacePrecondition::Unconditional,
    )
    .await
    .unwrap();
    let mut archive = f.pool.begin().await.unwrap();
    sqlx::query("SET LOCAL lock_timeout='100ms'")
        .execute(&mut *archive)
        .await
        .unwrap();
    let error =
        sqlx::query("UPDATE channels SET archived_at=now() WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(f.channel)
            .execute(&mut *archive)
            .await
            .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
    archive.rollback().await.unwrap();
    link.commit().await.unwrap();
    sqlx::query("UPDATE channels SET archived_at=now() WHERE community_id=$1 AND id=$2")
        .bind(f.community.as_uuid())
        .bind(f.channel)
        .execute(&f.pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_membership_removal_winner_rejects_new_association() {
    let f = Fixture::new().await;
    let a = f.event(100, false);
    f.write(&a, None).await.unwrap();
    let actor = Keys::generate();
    f.db.add_member(
        f.community,
        f.channel,
        actor.public_key().as_bytes(),
        crate::channel_members::MemberRole::Owner,
        Some(f.keys.public_key().as_bytes()),
    )
    .await
    .unwrap();
    f.db.remove_member(
        f.community,
        f.channel,
        f.keys.public_key().as_bytes(),
        actor.public_key().as_bytes(),
    )
    .await
    .unwrap();
    assert!(f.write(&f.event(101, true), Some(&a)).await.is_err());
    assert_eq!(f.live_ids().await, vec![a.id.as_bytes().to_vec()]);
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_unchanged_association_does_not_recheck_membership() {
    let f = Fixture::new().await;
    let a = f.event(100, true);
    f.write(&a, None).await.unwrap();
    sqlx::query(
        "UPDATE channel_members SET removed_at=now() WHERE community_id=$1 AND channel_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(f.channel)
    .execute(&f.pool)
    .await
    .unwrap();
    let b = f.event(101, true);
    assert_eq!(
        f.write(&b, Some(&a)).await.unwrap(),
        ParameterizedReplaceStatus::Inserted
    );
    assert_eq!(f.live_ids().await, vec![b.id.as_bytes().to_vec()]);
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_initial_home_requires_eligibility() {
    let f = Fixture::new().await;
    sqlx::query("UPDATE channels SET deleted_at=now() WHERE community_id=$1 AND id=$2")
        .bind(f.community.as_uuid())
        .bind(f.channel)
        .execute(&f.pool)
        .await
        .unwrap();
    let a = EventBuilder::new(Kind::Custom(30621), "")
        .tags([
            Tag::parse(["d", "project"]).unwrap(),
            Tag::parse(["buzz-channel", &f.channel.to_string()]).unwrap(),
        ])
        .sign_with_keys(&f.keys)
        .unwrap();
    assert!(f.write(&a, None).await.is_err());
    assert!(f.live_ids().await.is_empty());
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_actual_admin_kick_winner_rejects_association() {
    let f = Fixture::new().await;
    let a = f.event(100, false);
    f.write(&a, None).await.unwrap();
    let report = Uuid::new_v4();
    let action = Uuid::new_v4();
    let lease = Uuid::new_v4();
    sqlx::query("INSERT INTO moderation_reports (community_id,id,report_event_id,reporter_pubkey,target_kind,target_pubkey,channel_id,report_type) VALUES ($1,$2,$3,$4,'pubkey',$4,$5,'other')")
        .bind(f.community.as_uuid()).bind(report).bind(a.id.as_bytes().as_slice())
        .bind(f.keys.public_key().as_bytes().as_slice()).bind(f.channel)
        .execute(&f.pool).await.unwrap();
    sqlx::query("INSERT INTO relay_admin_actions (id,report_id,report_community_id,request_id,actor_pubkey,actor_role,action,state,action_lease_token,action_lease_expires_at) VALUES ($1,$2,$3,$4,$5,'operator','kick','enforcing',$6,now()+interval '1 minute')")
        .bind(action).bind(report).bind(f.community.as_uuid()).bind(Uuid::new_v4())
        .bind(f.keys.public_key().as_bytes().as_slice()).bind(lease)
        .execute(&f.pool).await.unwrap();
    let result = crate::relay_admin_actions::execute_kick_with_marker(
        &f.pool,
        action,
        lease,
        f.community,
        f.channel,
        f.keys.public_key().as_bytes(),
        f.keys.public_key().as_bytes(),
    )
    .await
    .unwrap();
    assert!(matches!(
        result,
        crate::relay_admin_actions::KickWithMarkerResult::Removed
    ));
    assert!(f.write(&f.event(101, true), Some(&a)).await.is_err());
    assert_eq!(f.live_ids().await, vec![a.id.as_bytes().to_vec()]);
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_rejects_wrong_author_foreign_target_and_nonstream() {
    let f = Fixture::new().await;
    let outsider = Keys::generate();
    let signed = EventBuilder::new(Kind::Custom(30621), "")
        .tags([
            Tag::parse(["d", "project"]).unwrap(),
            Tag::parse(["buzz-related-channel", &f.channel.to_string()]).unwrap(),
        ])
        .sign_with_keys(&outsider)
        .unwrap();
    assert!(f.write(&signed, None).await.is_err());
    let foreign = Fixture::new().await;
    let signed = EventBuilder::new(Kind::Custom(30621), "")
        .tags([
            Tag::parse(["d", "project"]).unwrap(),
            Tag::parse(["buzz-related-channel", &foreign.channel.to_string()]).unwrap(),
        ])
        .sign_with_keys(&f.keys)
        .unwrap();
    assert!(f.write(&signed, None).await.is_err());
    let forum =
        f.db.create_channel(
            f.community,
            "not-stream",
            ChannelType::Forum,
            ChannelVisibility::Open,
            None,
            f.keys.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap();
    let signed = EventBuilder::new(Kind::Custom(30621), "")
        .tags([
            Tag::parse(["d", "project"]).unwrap(),
            Tag::parse(["buzz-related-channel", &forum.id.to_string()]).unwrap(),
        ])
        .sign_with_keys(&f.keys)
        .unwrap();
    assert!(f.write(&signed, None).await.is_err());
    assert!(f.live_ids().await.is_empty());
}
