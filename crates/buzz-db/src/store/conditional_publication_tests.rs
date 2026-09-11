//! Ignored PostgreSQL tests create one owned scratch database per scenario.
use super::*;
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::future::Future;
use std::sync::Arc;

/// Deletion races against an open conditional decision. Kept in this namespace
/// so the registered PostgreSQL lane discovers them and they reuse [`scenario`].
#[path = "conditional_publication_deletion_tests.rs"]
mod postgres_tests;

pub(crate) async fn scenario<F, Fut>(body: F)
where
    F: FnOnce(Db, CommunityId, Keys) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let base = crate::test_support::database_url();
    let options: sqlx::postgres::PgConnectOptions = base.parse().expect("fixture options");
    let options = options.options([("statement_timeout", "15s"), ("lock_timeout", "5s")]);
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
    try_write(db, community, event, d, precondition)
        .await
        .expect("parameterized write")
}

async fn try_write(
    db: &Db,
    community: CommunityId,
    event: &Event,
    d: &str,
    precondition: ParameterizedReplacePrecondition<'_>,
) -> std::result::Result<ParameterizedReplaceStatus, DbError> {
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
        .await?;
    tx.commit().await?;
    Ok(result.status)
}

/// Read the exact logical byte accounting used by the production Wiki quota
/// query. Keeping this in the PostgreSQL fixture makes the boundary assertion
/// falsifiable when the stored JSONB envelope changes shape.
async fn reserved_live_usage(db: &Db, community: CommunityId, owner: &[u8]) -> (i64, i64) {
    sqlx::query_as(
        "SELECT COUNT(*)::bigint, COALESCE(SUM(octet_length(jsonb_build_object(\
             'id', encode(id, 'hex'), \
             'pubkey', encode(pubkey, 'hex'), \
             'created_at', EXTRACT(EPOCH FROM created_at)::bigint, \
             'kind', kind, \
             'tags', tags, \
             'content', content, \
             'sig', encode(sig, 'hex')\
         )::text)), 0)::bigint \
         FROM events \
         WHERE community_id=$1 AND kind=30623 AND pubkey=$2 AND deleted_at IS NULL \
           AND d_tag ~ '/(p1|m1)-[0-9a-f]{64}$'",
    )
    .bind(community.as_uuid())
    .bind(owner)
    .fetch_one(&db.pool)
    .await
    .expect("reserved live usage")
}

/// Return PostgreSQL's canonical JSONB text size for an incoming signed event.
async fn jsonb_event_bytes(db: &Db, event: &Event) -> i64 {
    sqlx::query_scalar("SELECT octet_length($1::jsonb::text)::bigint")
        .bind(serde_json::to_value(event).expect("event JSON"))
        .fetch_one(&db.pool)
        .await
        .expect("incoming JSONB size")
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

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_v1_head_rejects_legacy_downgrade_and_preserves_head() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let first = event(&keys, d, "v1-head", Timestamp::now().as_secs(), true);
        assert_eq!(
            write(
                &db,
                community,
                &first,
                d,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );

        // A v1-shaped event sent through the legacy/unconditional path must
        // still be rejected. This binds the downgrade guard itself rather
        // than only the legacy-shape parser: removing the intrinsic
        // `Unconditional` check would let this event replace the v1 head.
        let unconditioned_v1 = event(
            &keys,
            d,
            "unconditioned-v1-downgrade",
            first.created_at.as_secs() + 1,
            true,
        );
        assert_eq!(
            write(
                &db,
                community,
                &unconditioned_v1,
                d,
                ParameterizedReplacePrecondition::Unconditional,
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch
        );

        let legacy = event(
            &keys,
            d,
            "legacy-downgrade",
            first.created_at.as_secs() + 1,
            false,
        );
        assert_eq!(
            write(
                &db,
                community,
                &legacy,
                d,
                ParameterizedReplacePrecondition::Unconditional,
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch
        );
        let (content, deleted): (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
            "SELECT content, deleted_at FROM events \
                 WHERE community_id=$1 AND kind=30623 AND pubkey=$2 AND d_tag=$3 \
                   AND deleted_at IS NULL",
        )
        .bind(community.as_uuid())
        .bind(keys.public_key().to_bytes())
        .bind(d)
        .fetch_one(&db.pool)
        .await
        .expect("live v1 head");
        assert_eq!(content, "v1-head");
        assert!(deleted.is_none());
    })
    .await;
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_immutable_address_is_create_only_without_version_tags() {
    scenario(|db, community, keys| async move {
        let d = format!("repo/m1-{}", "c".repeat(64));
        let first = event(&keys, &d, "manifest", Timestamp::now().as_secs(), false);
        assert_eq!(
            write(
                &db,
                community,
                &first,
                &d,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(
                &db,
                community,
                &first,
                &d,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await,
            ParameterizedReplaceStatus::Duplicate
        );
        let replacement = event(
            &keys,
            &d,
            "replacement",
            first.created_at.as_secs() + 1,
            false,
        );
        assert_eq!(
            write(
                &db,
                community,
                &replacement,
                &d,
                ParameterizedReplacePrecondition::Unconditional,
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch
        );
        let live_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE community_id=$1 AND kind=30623 \
             AND pubkey=$2 AND d_tag=$3 AND deleted_at IS NULL",
        )
        .bind(community.as_uuid())
        .bind(keys.public_key().to_bytes())
        .bind(&d)
        .fetch_one(&db.pool)
        .await
        .expect("immutable live count");
        assert_eq!(live_count, 1);
    })
    .await;
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_reserved_quota_is_scoped_and_replays_exactly() {
    scenario(|db, community, keys| async move {
        let owner = keys.public_key().to_bytes();
        let other_keys = Keys::generate();
        let other_owner = other_keys.public_key().to_bytes();
        let other_community = Uuid::new_v4();
        sqlx::query("INSERT INTO communities(id,host) VALUES($1,$2)")
            .bind(other_community)
            .bind(format!("conditional-other-{}.example", other_community.simple()))
            .execute(&db.pool)
            .await
            .expect("other community");

        let reserved_digest = |index: i64| format!("repo/p1-{:0>64x}", index);
        // Fill the current owner's reserved live-row budget without creating
        // 4095 signed Events in Rust. These rows model already accepted v1
        // immutable publications and exercise the same SQL quota predicate.
        sqlx::query(
            "INSERT INTO events \
             (community_id,id,pubkey,created_at,kind,tags,content,sig,d_tag) \
             SELECT $1, decode(lpad(to_hex(i),64,'0'),'hex'), $2, \
                    now() - (i * interval '1 second'), 30623, \
                    jsonb_build_array(jsonb_build_array('d', concat('repo/p1-', lpad(to_hex(i),64,'0')))), \
                    'seed', decode(repeat('00',64),'hex'), \
                    concat('repo/p1-', lpad(to_hex(i),64,'0')) \
             FROM generate_series(1::bigint, $3::bigint) AS series(i)",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .bind(MAX_WIKI_LIVE_EVENTS - 1)
        .execute(&db.pool)
        .await
        .expect("seed current owner quota");
        // A malformed-length p1 slug must not consume the reserved budget.
        sqlx::query(
            "INSERT INTO events \
             (community_id,id,pubkey,created_at,kind,tags,content,sig,d_tag) \
             VALUES ($1, decode(repeat('ef',32),'hex'), $2, now(), 30623, \
                     jsonb_build_array(jsonb_build_array('d', $3)), 'escaped \\\"content', \
                     decode(repeat('00',64),'hex'), $3)",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .bind(format!("repo/p1-{}", "a".repeat(63)))
        .execute(&db.pool)
        .await
        .expect("seed non-reserved escaping row");
        // Owner and community are both part of the quota key.
        sqlx::query(
            "INSERT INTO events \
             (community_id,id,pubkey,created_at,kind,tags,content,sig,d_tag) \
             VALUES ($1, decode(repeat('dd',32),'hex'), $2, now(), 30623, \
                     jsonb_build_array(jsonb_build_array('d', $3)), 'other owner', \
                     decode(repeat('00',64),'hex'), $3), \
                    ($4, decode(repeat('cc',32),'hex'), $5, now(), 30623, \
                     jsonb_build_array(jsonb_build_array('d', $3)), 'other community', \
                     decode(repeat('00',64),'hex'), $3)",
        )
        .bind(community.as_uuid())
        .bind(other_owner.as_slice())
        .bind(reserved_digest(50_000))
        .bind(other_community)
        .bind(owner.as_slice())
        .execute(&db.pool)
        .await
        .expect("seed out-of-scope reserved rows");

        let first_d = reserved_digest(60_000);
        let first = event(
            &keys,
            &first_d,
            "quoted \\\"slash\\\\ emoji 🚀",
            Timestamp::now().as_secs(),
            true,
        );
        assert_eq!(
            write(
                &db,
                community,
                &first,
                &first_d,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
        // Replaying the exact current immutable event remains idempotent even
        // though the reserved quota is now full.
        assert_eq!(
            write(
                &db,
                community,
                &first,
                &first_d,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await,
            ParameterizedReplaceStatus::Duplicate
        );

        let second_d = reserved_digest(60_001);
        let second = event(
            &keys,
            &second_d,
            "second",
            first.created_at.as_secs() + 1,
            true,
        );
        assert!(matches!(
            try_write(
                &db,
                community,
                &second,
                &second_d,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await,
            Err(DbError::WikiStorageQuotaExceeded)
        ));

        // Legacy-shaped rows do not consume the immutable reserved quota.
        let non_reserved_d = format!("repo/p1-{}", "b".repeat(63));
        let non_reserved = event(
            &keys,
            &non_reserved_d,
            "legacy after quota",
            second.created_at.as_secs() + 1,
            false,
        );
        assert_eq!(
            write(
                &db,
                community,
                &non_reserved,
                &non_reserved_d,
                ParameterizedReplacePrecondition::Unconditional,
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL; allocates the 512 MiB logical Wiki boundary"]
async fn conditional_publication_reserved_quota_enforces_jsonb_bytes_below_row_cap() {
    // Fixture resource manifest: 3,000 rows × 178,500-byte repetitive bodies
    // are about 536.8 MB of logical JSONB text (just under 512 MiB), with one
    // <=192 KiB signed UTF-8 candidate held in memory. PostgreSQL TOAST/WAL
    // may transiently use up to roughly 1.2 GiB; nextest serializes this
    // quota group so the disposable service does not multiply that peak.
    const SEED_ROWS: i64 = 3_000;
    const SEED_REPEAT: i64 = 44_625;
    const SEED_CHUNK_ROWS: i64 = 250;
    const MAX_SIGNED_EVENT_BYTES: i64 = 192 * 1024;

    scenario(|db, community, keys| async move {
        let owner = keys.public_key().to_bytes();
        for start in (1..=SEED_ROWS).step_by(SEED_CHUNK_ROWS as usize) {
            let end = (start + SEED_CHUNK_ROWS - 1).min(SEED_ROWS);
            sqlx::query(
                "INSERT INTO events \
                 (community_id,id,pubkey,created_at,kind,tags,content,sig,d_tag) \
                 SELECT $1, decode(lpad(to_hex(i),64,'0'),'hex'), $2, \
                        now() - (i * interval '1 second'), 30623, \
                        jsonb_build_array(jsonb_build_array('d', concat('repo/p1-', lpad(to_hex(i),64,'0')))), \
                        repeat($4, $5::int), decode(repeat('00',64),'hex'), \
                        concat('repo/p1-', lpad(to_hex(i),64,'0')) \
                 FROM generate_series($3::bigint, $6::bigint) AS series(i)",
            )
            .bind(community.as_uuid())
            .bind(owner.as_slice())
            .bind(start)
            .bind("seed")
            .bind(SEED_REPEAT)
            .bind(end)
            .execute(&db.pool)
            .await
            .expect("seed TOAST-backed Wiki rows");
        }

        let before = reserved_live_usage(&db, community, owner.as_slice()).await;
        assert_eq!(before.0, SEED_ROWS);
        assert!(before.0 < MAX_WIKI_LIVE_EVENTS);
        assert!(before.1 < MAX_WIKI_LIVE_BYTES);

        // Use escaped and non-ASCII content so the test observes PostgreSQL's
        // JSONB text accounting rather than assuming raw UTF-8 body length.
        // Calibrate the smallest crossing payload through the actual JSONB
        // cast. This keeps the fixture valid if PostgreSQL's Unicode escaping
        // policy changes, while bounding the probe to 15 database reads and
        // keeping the final signed event under the production limit.
        let pattern = ["é", "🚀", "\"", "\\"].concat();
        let d = format!("repo/p1-{}", "f".repeat(64));
        let candidate_time = Timestamp::now().as_secs();
        let remaining = MAX_WIKI_LIVE_BYTES - before.1;
        let mut low = 1usize;
        let mut high = 16_000usize;
        let mut chosen = None;
        while low <= high {
            let repeats = low + (high - low) / 2;
            let content = pattern.repeat(repeats);
            let candidate = event(&keys, &d, &content, candidate_time, true);
            let candidate_bytes = jsonb_event_bytes(&db, &candidate).await;
            if candidate_bytes > remaining {
                let signed_bytes = serde_json::to_vec(&candidate)
                    .expect("signed event JSON")
                    .len() as i64;
                if signed_bytes <= MAX_SIGNED_EVENT_BYTES
                    && candidate_bytes <= MAX_SIGNED_EVENT_BYTES
                {
                    chosen = Some((candidate, signed_bytes, candidate_bytes));
                }
                high = repeats.saturating_sub(1);
            } else {
                low = repeats + 1;
            }
        }
        let (candidate, signed_bytes, candidate_bytes) = chosen
            .expect("a bounded UTF-8 candidate must cross the remaining byte quota");
        let content = candidate.content.clone();
        assert!(signed_bytes <= MAX_SIGNED_EVENT_BYTES);
        assert!(candidate_bytes <= MAX_SIGNED_EVENT_BYTES);
        assert!(candidate_bytes > content.len() as i64);
        assert!(before.1 + candidate_bytes > MAX_WIKI_LIVE_BYTES);

        assert!(matches!(
            try_write(
                &db,
                community,
                &candidate,
                &d,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await,
            Err(DbError::WikiStorageQuotaExceeded)
        ));
        assert_eq!(
            reserved_live_usage(&db, community, owner.as_slice()).await,
            before,
            "quota rejection must preserve the live JSONB usage"
        );
    })
    .await;
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_reserved_quota_concurrent_final_slot_has_one_winner() {
    const SEED_ROWS: i64 = MAX_WIKI_LIVE_EVENTS - 1;

    scenario(|db, community, keys| async move {
        let owner = keys.public_key().to_bytes();
        sqlx::query(
            "INSERT INTO events \
             (community_id,id,pubkey,created_at,kind,tags,content,sig,d_tag) \
             SELECT $1, decode(lpad(to_hex(i),64,'0'),'hex'), $2, \
                    now() - (i * interval '1 second'), 30623, \
                    jsonb_build_array(jsonb_build_array('d', concat('repo/p1-', lpad(to_hex(i),64,'0')))), \
                    'seed', decode(repeat('00',64),'hex'), \
                    concat('repo/p1-', lpad(to_hex(i),64,'0')) \
             FROM generate_series(1::bigint, $3::bigint) AS series(i)",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .bind(SEED_ROWS)
        .execute(&db.pool)
        .await
        .expect("seed final Wiki quota slot");

        let db = Arc::new(db);
        let start = Arc::new(tokio::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for index in 0..2 {
            let db = db.clone();
            let start = start.clone();
            let d = format!("repo/p1-{}", format!("e{index}").repeat(32));
            let candidate = event(
                &keys,
                &d,
                &format!("concurrent-final-slot-{index}"),
                Timestamp::now().as_secs(),
                true,
            );
            handles.push(tokio::spawn(async move {
                start.wait().await;
                try_write(
                    &db,
                    community,
                    &candidate,
                    &d,
                    ParameterizedReplacePrecondition::ExpectedMissing,
                )
                .await
            }));
        }
        start.wait().await;

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.expect("quota race task"));
        }
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Ok(ParameterizedReplaceStatus::Inserted)))
                .count(),
            1,
            "owner lock must admit exactly one final quota slot"
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(DbError::WikiStorageQuotaExceeded)))
                .count(),
            1,
            "the losing writer must receive the durable quota error"
        );
        assert_eq!(
            reserved_live_usage(&db, community, owner.as_slice()).await.0,
            MAX_WIKI_LIVE_EVENTS
        );
    })
    .await;
}
