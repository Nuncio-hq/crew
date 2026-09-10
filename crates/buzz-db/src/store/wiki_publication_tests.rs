use super::*;
use crate::store::replaceable::conditional_publication_postgres_tests::{event, scenario};
use nostr::{Keys, Timestamp};
use std::sync::Arc;

async fn write(
    db: &Db,
    community: CommunityId,
    event: &Event,
    d: &str,
    limits: (i64, i64),
) -> Result<ParameterizedReplaceStatus> {
    let mut tx = db.begin_event_write_transaction().await.unwrap();
    let result = replace_with_limits(
        db,
        &mut tx,
        community,
        ImmutableWrite {
            event,
            d,
            channel: None,
            replay_only: false,
        },
        limits,
    )
    .await;
    // Quota failure must have rolled back its savepoint, even if the caller
    // commits unrelated outer work instead of propagating that failure.
    tx.commit().await.unwrap();
    result.map(|result| result.status)
}

fn page(keys: &Keys, letter: char, body: &str) -> (String, Event) {
    let d = format!("repo/p1-{}", letter.to_string().repeat(64));
    let event = event(keys, &d, body, Timestamp::now().as_secs(), false);
    (d, event)
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn wiki_admission_count_quota_rolls_back_and_exact_replay_adds_nothing() {
    scenario(|db, community, keys| async move {
        let (d, first) = page(&keys, 'a', "first");
        assert_eq!(write(&db, community, &first, &d, (1, MAX_LIVE_BYTES)).await.unwrap(), ParameterizedReplaceStatus::Inserted);
        assert_eq!(write(&db, community, &first, &d, (1, MAX_LIVE_BYTES)).await.unwrap(), ParameterizedReplaceStatus::Duplicate);
        let (next_d, second) = page(&keys, 'b', "second");
        assert!(matches!(write(&db, community, &second, &next_d, (1, MAX_LIVE_BYTES)).await, Err(DbError::WikiStorageQuota)));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE community_id=$1 AND pubkey=$2 AND kind=30623 AND deleted_at IS NULL")
            .bind(community.as_uuid()).bind(keys.public_key().to_bytes().as_slice()).fetch_one(&db.pool).await.unwrap();
        assert_eq!(count, 1);
    }).await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn wiki_admission_bytes_count_full_escaped_event_and_preserve_existing() {
    scenario(|db, community, keys| async move {
        let (d, first) = page(&keys, 'a', "small");
        assert_eq!(
            write(&db, community, &first, &d, (100, 2500))
                .await
                .unwrap(),
            ParameterizedReplaceStatus::Inserted
        );
        let (next_d, second) = page(&keys, 'b', &"\"".repeat(1500));
        assert!(matches!(
            write(&db, community, &second, &next_d, (100, 2500)).await,
            Err(DbError::WikiStorageQuota)
        ));
        assert_eq!(
            write(&db, community, &first, &d, (100, 2500))
                .await
                .unwrap(),
            ParameterizedReplaceStatus::Duplicate
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn wiki_admission_concurrent_coordinates_share_owner_quota() {
    scenario(|db, community, keys| async move {
        let db = Arc::new(db);
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for letter in ['a', 'b'] {
            let db = db.clone();
            let barrier = barrier.clone();
            let (d, event) = page(&keys, letter, "page");
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                write(&db, community, &event, &d, (1, MAX_LIVE_BYTES)).await
            }));
        }
        barrier.wait().await;
        let mut accepted = 0;
        let mut refused = 0;
        for handle in handles {
            match handle.await.unwrap() {
                Ok(ParameterizedReplaceStatus::Inserted) => accepted += 1,
                Err(DbError::WikiStorageQuota) => refused += 1,
                other => panic!("unexpected admission: {other:?}"),
            }
        }
        assert_eq!((accepted, refused), (1, 1));
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn wiki_admission_deleted_rows_do_not_count_but_deleted_replay_is_not_live() {
    scenario(|db, community, keys| async move {
        let (d, first) = page(&keys, 'a', "first");
        write(&db, community, &first, &d, (1, MAX_LIVE_BYTES))
            .await
            .unwrap();
        assert!(db
            .soft_delete_event(community, first.id.as_bytes())
            .await
            .unwrap());
        let (next_d, second) = page(&keys, 'b', "second");
        assert_eq!(
            write(&db, community, &second, &next_d, (1, MAX_LIVE_BYTES))
                .await
                .unwrap(),
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(&db, community, &first, &d, (1, MAX_LIVE_BYTES))
                .await
                .unwrap(),
            ParameterizedReplaceStatus::DuplicateNotLive
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn wiki_admission_owner_lock_serializes_before_coordinate_and_quota_read() {
    scenario(|db, community, keys| async move {
        let (d, first) = page(&keys, 'a', "first");
        let mut first_tx = db.begin_event_write_transaction().await.unwrap();
        assert_eq!(
            replace_with_limits(
                &db,
                &mut first_tx,
                community,
                ImmutableWrite {
                    event: &first,
                    d: &d,
                    channel: None,
                    replay_only: false,
                },
                (1, MAX_LIVE_BYTES)
            )
            .await
            .unwrap()
            .status,
            ParameterizedReplaceStatus::Inserted
        );
        let (next_d, second) = page(&keys, 'b', "second");
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let mut second_write = tokio::spawn(async move {
            let mut tx = db.begin_event_write_transaction().await.unwrap();
            started_tx.send(()).unwrap();
            let result = replace_with_limits(
                &db,
                &mut tx,
                community,
                ImmutableWrite {
                    event: &second,
                    d: &next_d,
                    channel: None,
                    replay_only: false,
                },
                (1, MAX_LIVE_BYTES),
            )
            .await;
            tx.commit().await.unwrap();
            result
        });
        started_rx.await.unwrap();
        let early =
            tokio::time::timeout(std::time::Duration::from_millis(300), &mut second_write).await;
        // A different coordinate must wait for the first owner's transaction,
        // otherwise both could read a one-event quota and commit two events.
        assert!(
            early.is_err(),
            "second coordinate bypassed owner admission lock"
        );
        first_tx.commit().await.unwrap();
        assert!(matches!(
            second_write.await.unwrap(),
            Err(DbError::WikiStorageQuota)
        ));
    })
    .await;
}
