//! Ignored PostgreSQL tests require an explicitly supplied owned fixture URL.
use super::*;
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::future::Future;
use std::sync::Arc;

pub(crate) async fn scenario<F, Fut>(body: F)
where
    F: FnOnce(Db, CommunityId, Keys) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let base = std::env::var("CREW_CONDITIONAL_TEST_DATABASE_URL")
        .expect("explicit owned fixture URL is required; no localhost fallback");
    let options: sqlx::postgres::PgConnectOptions = base.parse().expect("fixture options");
    let options = options.options([("statement_timeout", "15s"), ("lock_timeout", "5s")]);
    assert!(
        options
            .get_database()
            .is_some_and(|name| name.starts_with("crew_")),
        "owned Crew fixture required"
    );
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(options.clone())
        .await
        .expect("fixture admin");
    let name = format!("crew_conditional_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .expect("create owned scratch");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(options.database(&name))
        .await
        .expect("scratch connection");
    let migrations = crate::migration::run_migrations(&pool).await;
    if let Err(error) = migrations {
        cleanup(&admin, &pool, &name).await;
        panic!("fixture migrations failed: {error}");
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO communities(id,host) VALUES($1,$2)")
        .bind(id)
        .bind(format!("conditional-{}.example", id.simple()))
        .execute(&pool)
        .await
        .expect("community");
    let mut task = tokio::spawn(body(
        Db::from_pool(pool.clone()),
        CommunityId::from_uuid(id),
        Keys::generate(),
    ));
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(45), &mut task).await;
    if outcome.is_err() {
        task.abort();
        let _ = task.await;
    }
    cleanup(&admin, &pool, &name).await;
    outcome
        .expect("bounded scenario")
        .expect("scenario assertion passed");
}

async fn cleanup(admin: &PgPool, pool: &PgPool, name: &str) {
    assert!(
        name.starts_with("crew_conditional_")
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    );
    pool.close().await;
    let mut last_error = None;
    for attempt in 0..5 {
        match sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE {name}")))
            .execute(admin)
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
    panic!("drop owned scratch database failed: {last_error:?}");
}

pub(crate) fn event(keys: &Keys, d: &str, content: &str, time: u64, version: bool) -> Event {
    let mut tags = vec![Tag::parse(["d", d]).unwrap()];
    if version {
        tags.push(Tag::parse(["wiki-version", "1"]).unwrap());
    }
    EventBuilder::new(Kind::Custom(30623), content)
        .tags(tags)
        .custom_created_at(Timestamp::from(time))
        .sign_with_keys(keys)
        .unwrap()
}

async fn write(
    db: &Db,
    community: CommunityId,
    event: &Event,
    d: &str,
    precondition: ParameterizedReplacePrecondition<'_>,
) -> ParameterizedReplaceStatus {
    let mut tx = db.begin_event_write_transaction().await.unwrap();
    let result = db
        .replace_parameterized_event_in_transaction(
            &mut tx,
            community,
            event,
            d,
            None,
            precondition,
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    result.status
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn conditional_publication_expected_missing_rejects_second_writer_and_accepts_exact_replay() {
    scenario(|db, community, keys| async move {
        let first = event(
            &keys,
            "repo/_toc",
            "first",
            Timestamp::now().as_secs(),
            false,
        );
        let second = event(
            &keys,
            "repo/_toc",
            "second",
            first.created_at.as_secs() + 1,
            false,
        );
        assert_eq!(
            write(
                &db,
                community,
                &first,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedMissing
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(
                &db,
                community,
                &first,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedMissing
            )
            .await,
            ParameterizedReplaceStatus::Duplicate
        );
        assert_eq!(
            write(
                &db,
                community,
                &second,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedMissing
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn conditional_publication_tag_guard_reads_locked_live_head() {
    scenario(|db, community, keys| async move {
        let first = event(&keys, "repo/_toc", "v1", Timestamp::now().as_secs(), true);
        let legacy = event(
            &keys,
            "repo/_toc",
            "legacy",
            first.created_at.as_secs() + 1,
            false,
        );
        assert_eq!(
            write(
                &db,
                community,
                &first,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedMissing
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(
                &db,
                community,
                &legacy,
                "repo/_toc",
                ParameterizedReplacePrecondition::RejectIfLiveHeadHasTag("wiki-version", "1")
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn conditional_publication_deleted_replay_is_not_live_duplicate() {
    scenario(|db, community, keys| async move {
        let first = event(
            &keys,
            "repo/_toc",
            "first",
            Timestamp::now().as_secs(),
            false,
        );
        assert_eq!(
            write(
                &db,
                community,
                &first,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedMissing
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, first.id.as_bytes())
            .await
            .unwrap());
        let replay = write(
            &db,
            community,
            &first,
            "repo/_toc",
            ParameterizedReplacePrecondition::ExpectedMissing,
        )
        .await;
        assert!(
            !matches!(
                replay,
                ParameterizedReplaceStatus::Inserted | ParameterizedReplaceStatus::Duplicate
            ),
            "deleted event must not ACK as a live duplicate"
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn conditional_publication_existing_revision_race_has_one_winner() {
    scenario(|db, community, keys| async move {
        let base = event(
            &keys,
            "repo/_toc",
            "base",
            Timestamp::now().as_secs(),
            false,
        );
        assert_eq!(
            write(
                &db,
                community,
                &base,
                "repo/_toc",
                ParameterizedReplacePrecondition::Unconditional
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        let db = Arc::new(db);
        let start = Arc::new(tokio::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for index in 1..=2 {
            let db = db.clone();
            let start = start.clone();
            let expected = base.id;
            let next = event(
                &keys,
                "repo/_toc",
                &format!("next-{index}"),
                base.created_at.as_secs() + index,
                false,
            );
            handles.push(tokio::spawn(async move {
                start.wait().await;
                write(
                    &db,
                    community,
                    &next,
                    "repo/_toc",
                    ParameterizedReplacePrecondition::ExpectedRevision(expected.as_bytes()),
                )
                .await
            }));
        }
        start.wait().await;
        let mut statuses = Vec::new();
        for handle in handles {
            statuses.push(handle.await.unwrap());
        }
        assert_eq!(
            statuses
                .iter()
                .filter(|status| **status == ParameterizedReplaceStatus::Inserted)
                .count(),
            1
        );
        assert_eq!(
            statuses
                .iter()
                .filter(|status| **status == ParameterizedReplaceStatus::RevisionMismatch)
                .count(),
            1
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn conditional_publication_expected_missing_race_has_one_winner() {
    scenario(|db, community, keys| async move {
        let db = Arc::new(db);
        let start = Arc::new(tokio::sync::Barrier::new(3));
        let mut handles = Vec::new();
        let now = Timestamp::now().as_secs();
        for index in 0..2 {
            let db = db.clone();
            let start = start.clone();
            let next = event(&keys, "repo/_toc", &format!("race-{index}"), now, true);
            handles.push(tokio::spawn(async move {
                start.wait().await;
                write(
                    &db,
                    community,
                    &next,
                    "repo/_toc",
                    ParameterizedReplacePrecondition::ExpectedMissing,
                )
                .await
            }));
        }
        start.wait().await;
        let mut statuses = Vec::new();
        for handle in handles {
            statuses.push(handle.await.unwrap());
        }
        assert_eq!(
            statuses
                .iter()
                .filter(|s| **s == ParameterizedReplaceStatus::Inserted)
                .count(),
            1
        );
        assert_eq!(
            statuses
                .iter()
                .filter(|s| **s == ParameterizedReplaceStatus::RevisionMismatch)
                .count(),
            1
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires explicit isolated PostgreSQL"]
async fn conditional_publication_same_second_cas_requires_exact_current_revision() {
    scenario(|db, community, keys| async move {
        let now = Timestamp::now().as_secs();
        let mut candidates: Vec<_> = (0..3)
            .map(|i| event(&keys, "repo/_toc", &format!("same-second-{i}"), now, true))
            .collect();
        candidates.sort_by_key(|event| event.id);
        let (last, middle, base) = (&candidates[0], &candidates[1], &candidates[2]);
        assert_eq!(
            write(
                &db,
                community,
                base,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedMissing
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(
                &db,
                community,
                middle,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedRevision(base.id.as_bytes())
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        // Better NIP-33 ordering cannot bypass a stale CAS token.
        assert_eq!(
            write(
                &db,
                community,
                last,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedRevision(base.id.as_bytes())
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch
        );
        assert_eq!(
            write(
                &db,
                community,
                middle,
                "repo/_toc",
                ParameterizedReplacePrecondition::ExpectedRevision(base.id.as_bytes())
            )
            .await,
            ParameterizedReplaceStatus::Duplicate
        );
    })
    .await;
}
