//! Deletion-versus-decision proofs for the conditional Wiki publication seam.
//!
//! These scenarios are children of `conditional_publication_postgres_tests`, so
//! the mandatory PostgreSQL lane discovers them through the same
//! `postgres_tests::` naming convention as their siblings and they reuse the
//! owned scratch database that [`scenario`] creates and drops.
//!
//! Each test holds one real production decision open on the fixture connection
//! and drives one real production deletion entry point on a second, separately
//! bounded connection. That connection's short `lock_timeout` turns "the
//! deletion waited for the decision" into a deterministic `55P03` instead of a
//! sleep, and no assertion here describes a lock the production path does not
//! itself take.
use super::*;

const WIKI_KIND: i32 = buzz_core::kind::KIND_REPO_WIKI_PAGE as i32;

/// Second pool onto the scenario's own scratch database, whose lock waits end
/// far below the fixture's overall bound.
async fn bounded_deleter_pool(db: &Db) -> PgPool {
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&db.pool)
        .await
        .expect("scratch database name");
    assert!(
        name.starts_with("crew_conditional_"),
        "refusing to open a second writer outside the owned scratch database"
    );
    let options: sqlx::postgres::PgConnectOptions = crate::test_support::database_url()
        .parse()
        .expect("fixture options");
    PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(
            options
                .database(&name)
                .options([("statement_timeout", "10s"), ("lock_timeout", "250ms")]),
        )
        .await
        .expect("bounded deleter connection")
}

fn assert_lock_timeout(error: DbError) {
    let DbError::Sqlx(error) = error else {
        panic!("expected a SQL lock timeout, got: {error}")
    };
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03"),
        "the deletion must wait on the open decision, not fail for another reason"
    );
}

async fn live_rows(db: &Db, community: CommunityId, owner: &[u8], d: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE community_id=$1 AND kind=30623 \
         AND pubkey=$2 AND d_tag=$3 AND deleted_at IS NULL",
    )
    .bind(community.as_uuid())
    .bind(owner)
    .bind(d)
    .fetch_one(&db.pool)
    .await
    .expect("live row count")
}

/// NIP-09 a-tag deletion through the production coordinate entry point.
async fn coordinate_delete(
    db: &Db,
    community: CommunityId,
    owner: &[u8],
    d: &str,
    secs: i64,
) -> std::result::Result<bool, DbError> {
    db.soft_delete_by_coordinate(community, WIKI_KIND, owner, d, secs)
        .await
}

/// Create-only publication through the production write path.
async fn expect_missing_write(
    db: &Db,
    community: CommunityId,
    event: &Event,
    d: &str,
) -> ParameterizedReplaceStatus {
    write(
        db,
        community,
        event,
        d,
        ParameterizedReplacePrecondition::ExpectedMissing,
    )
    .await
}

/// Idempotent replay of an already published event through the same path.
async fn exact_replay(
    db: &Db,
    community: CommunityId,
    event: &Event,
    d: &str,
) -> ParameterizedReplaceStatus {
    write(
        db,
        community,
        event,
        d,
        ParameterizedReplacePrecondition::ExactReplayOnly,
    )
    .await
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_coordinate_delete_waits_for_absent_head_decision() {
    scenario(|db, community, keys| async move {
        let owner = keys.public_key().to_bytes();
        let base = Timestamp::now().as_secs();
        // NIP-09 scopes an a-tag deletion to versions at or before the
        // tombstone's own `created_at`, so the same race runs once with a
        // timestamp that covers the head and once with one that does not.
        for (suffix, secs, should_delete) in [
            ("eligible", base as i64, true),
            ("stale", base as i64 - 1, false),
        ] {
            let d = format!("repo/delete-race-{suffix}");
            assert_eq!(live_rows(&db, community, &owner, &d).await, 0);

            // The coordinate starts absent, which is what makes this bind the
            // shared owner and coordinate advisory locks rather than any row
            // lock: an unfenced deletion finds no live row at all, reports
            // Ok(false), and lets the uncommitted head go live behind it.
            let head = event(&keys, &d, "head", base, true);
            let mut decision = db
                .begin_event_write_transaction()
                .await
                .expect("decision tx");
            let decided = db
                .replace_parameterized_event_in_transaction(
                    &mut decision,
                    community,
                    &head,
                    &d,
                    None,
                    ParameterizedReplacePrecondition::ExpectedMissing,
                )
                .await
                .expect("conditional insert");
            assert_eq!(decided.status, ParameterizedReplaceStatus::Inserted);

            let deleter_pool = bounded_deleter_pool(&db).await;
            let deleter = Db::from_pool(deleter_pool.clone());
            match coordinate_delete(&deleter, community, &owner, &d, secs).await {
                Err(error) => assert_lock_timeout(error),
                Ok(deleted) => panic!("coordinate delete must wait: {deleted}"),
            }
            assert_eq!(live_rows(&db, community, &owner, &d).await, 0);

            decision.commit().await.expect("commit decision");

            // The decision won, so the writer's own retry of the exact same
            // event is an idempotent replay of the head it just created.
            assert_eq!(
                exact_replay(&db, community, &head, &d).await,
                ParameterizedReplaceStatus::Duplicate
            );

            // The same production entry point now runs unblocked and decides
            // on its own timestamp rule against the head it waited for.
            let deleted = coordinate_delete(&deleter, community, &owner, &d, secs)
                .await
                .expect("unblocked coordinate delete");
            assert_eq!(deleted, should_delete);
            assert_eq!(
                live_rows(&db, community, &owner, &d).await,
                i64::from(!should_delete)
            );
            deleter_pool.close().await;
        }
    })
    .await;
}

#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_exact_id_delete_waits_for_live_replay_decision() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let owner = keys.public_key().to_bytes();
        let head = event(&keys, d, "head", Timestamp::now().as_secs(), true);
        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Inserted
        );

        // An exact live replay decides `Duplicate` out of the locking head read
        // alone: it updates no row, so it holds no lock a write would have left
        // behind. A deletion by event ID also takes none of the advisory locks.
        // The head read's `FOR UPDATE` is therefore the only fence left; drop
        // that clause and the deletion below succeeds immediately, tombstoning
        // the head this decision is still reasoning about.
        let mut decision = db
            .begin_event_write_transaction()
            .await
            .expect("decision tx");
        let decided = db
            .replace_parameterized_event_in_transaction(
                &mut decision,
                community,
                &head,
                d,
                None,
                ParameterizedReplacePrecondition::ExactReplayOnly,
            )
            .await
            .expect("exact replay decision");
        assert_eq!(decided.status, ParameterizedReplaceStatus::Duplicate);

        let deleter_pool = bounded_deleter_pool(&db).await;
        let deleter = Db::from_pool(deleter_pool.clone());
        match deleter
            .soft_delete_event(community, head.id.as_bytes())
            .await
        {
            Err(error) => assert_lock_timeout(error),
            Ok(deleted) => panic!("exact-ID delete must wait: {deleted}"),
        }
        assert_eq!(live_rows(&db, community, &owner, d).await, 1);

        decision.commit().await.expect("commit decision");
        assert_eq!(
            exact_replay(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Duplicate
        );

        let deleted = deleter
            .soft_delete_event(community, head.id.as_bytes())
            .await
            .expect("unblocked exact-ID delete");
        assert!(deleted);
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);

        // A tombstoned head must not be resurrected by the same replay that was
        // a live duplicate one statement earlier.
        let replay = expect_missing_write(&db, community, &head, d).await;
        assert!(
            !matches!(
                replay,
                ParameterizedReplaceStatus::Inserted | ParameterizedReplaceStatus::Duplicate
            ),
            "a deleted head must not ACK as a live duplicate"
        );
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);
        deleter_pool.close().await;
    })
    .await;
}
