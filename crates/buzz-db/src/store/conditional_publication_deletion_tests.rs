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

/// D-079 head retirement: the exact submitted head was accepted and later
/// deleted, so a replay against absence is permanently refused with proof.
#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_reports_retired_head_after_accept_then_delete() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let owner = keys.public_key().to_bytes();
        let head = event(&keys, d, "head", Timestamp::now().as_secs(), true);

        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        // The exact live head still ACKs as a successful duplicate. This
        // ordering matters: proof must never pre-empt a live replay.
        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Duplicate
        );

        assert!(db
            .soft_delete_event(community, head.id.as_bytes())
            .await
            .expect("delete head"));
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);

        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::WikiHeadRetired,
            "a historically accepted, now deleted head is permanently retired"
        );
        assert_eq!(
            live_rows(&db, community, &owner, d).await,
            0,
            "a refused write must not insert anything"
        );

        // A head that was never accepted has no such evidence: absence alone
        // is not retirement, and this attempt must stay admissible.
        let fresh = event(&keys, d, "fresh", Timestamp::now().as_secs() + 1, true);
        assert_eq!(
            expect_missing_write(&db, community, &fresh, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(live_rows(&db, community, &owner, d).await, 1);
    })
    .await;
}

/// Item 5: deletion-first versus send-first around a retired head, with the
/// deletion held open on a real second connection so the ordering is the
/// production lock ordering rather than a sequence of independent statements.
#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn retired_head_classification_survives_both_commit_orders() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let owner = keys.public_key().to_bytes();
        let head = event(&keys, d, "head", Timestamp::now().as_secs(), true);
        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Inserted
        );

        // Old send first: a decision is already open on the live head when the
        // deletion arrives, so the deletion must wait rather than tombstone a
        // head the open decision is still reasoning about.
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
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await
            .expect("live replay decision");
        assert_eq!(
            decided.status,
            ParameterizedReplaceStatus::Duplicate,
            "the exact live head still ACKs before any retirement can be claimed"
        );

        let deleter_pool = bounded_deleter_pool(&db).await;
        let deleter = Db::from_pool(deleter_pool.clone());
        match deleter
            .soft_delete_event(community, head.id.as_bytes())
            .await
        {
            Err(error) => assert_lock_timeout(error),
            Ok(deleted) => panic!("the deletion must wait for the open decision: {deleted}"),
        }
        decision.commit().await.expect("commit decision");

        // Deletion first: once it commits, the same exact write is retired and
        // no late replay can make it live again.
        assert!(deleter
            .soft_delete_event(community, head.id.as_bytes())
            .await
            .expect("unblocked delete"));
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);
        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::WikiHeadRetired
        );
        assert_eq!(
            live_rows(&db, community, &owner, d).await,
            0,
            "a late exact writer must not become live after the proof"
        );

        // A competing head may legitimately take the coordinate afterwards,
        // and a further rejected retired write must not remove it.
        let competing = event(&keys, d, "competing", Timestamp::now().as_secs() + 1, true);
        assert_eq!(
            expect_missing_write(&db, community, &competing, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(
                &db,
                community,
                &head,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(competing.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::WikiHeadRetired,
            "a retired head is classified even under an ExpectedRevision precondition"
        );
        assert_eq!(
            live_rows(&db, community, &owner, d).await,
            1,
            "a rejected write must never remove the competing live head"
        );
        deleter_pool.close().await;
    })
    .await;
}

/// Item 5: scope negatives. Retirement is scoped to one exact
/// community/owner/kind/`d` coordinate and to conditional v1 `_toc` writes.
#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn retirement_is_scoped_to_owner_community_kind_and_conditional_toc() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let head = event(&keys, d, "head", Timestamp::now().as_secs(), true);
        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, head.id.as_bytes())
            .await
            .expect("delete head"));

        // A different owner's identical-shaped attempt has no history here.
        let other_keys = Keys::generate();
        let other_head = event(&other_keys, d, "head", Timestamp::now().as_secs(), true);
        assert_eq!(
            expect_missing_write(&db, community, &other_head, d).await,
            ParameterizedReplaceStatus::Inserted,
            "another owner's coordinate is untouched by this retirement"
        );

        // A retired expected revision belonging to ANOTHER OWNER at the same
        // `d` must not prove anything for this owner's precondition. This is
        // the foreign-owner E case, distinct from the fresh-insert case above.
        assert!(db
            .soft_delete_event(community, other_head.id.as_bytes())
            .await
            .expect("delete the other owner's head"));
        let successor = event(&keys, d, "successor", Timestamp::now().as_secs() + 1, true);
        assert_eq!(
            write(
                &db,
                community,
                &successor,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(other_head.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::RevisionMissing,
            "a retired event under a different owner is not this owner's precondition"
        );

        // The same fact across communities: an event retired in one community
        // says nothing about the same coordinate in another. The scenario owns
        // exactly one scratch database, so use a second community inside it.
        let other_id = Uuid::new_v4();
        sqlx::query("INSERT INTO communities(id,host) VALUES($1,$2)")
            .bind(other_id)
            .bind(format!("scoped-{}.example", other_id.simple()))
            .execute(&db.pool)
            .await
            .expect("owned second community");
        let other_community = CommunityId::from_uuid(other_id);
        let elsewhere = event(&keys, d, "elsewhere", Timestamp::now().as_secs() + 2, true);
        assert_eq!(
            expect_missing_write(&db, other_community, &elsewhere, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(other_community, elsewhere.id.as_bytes())
            .await
            .expect("delete in the other community"));
        assert_eq!(
            write(
                &db,
                community,
                &successor,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(elsewhere.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::RevisionMissing,
            "a retired event in another community proves nothing here"
        );
        assert_eq!(
            expect_missing_write(&db, community, &elsewhere, d).await,
            ParameterizedReplaceStatus::Inserted,
            "nor does it retire the same head identity in this community"
        );
        assert!(db
            .soft_delete_event(community, elsewhere.id.as_bytes())
            .await
            .expect("clean up the same-coordinate scope probe"));

        // A non-Wiki kind never carries this classification. Vary ONLY the
        // kind: this event keeps the v1 tag and the `_toc` address, so the
        // assertion isolates the kind check instead of passing because some
        // other guard rejected the shape first.
        let other_kind_d = "kindcheck/_toc";
        let other_kind = EventBuilder::new(Kind::Custom(30078), "other-kind")
            .tags(vec![
                Tag::parse(["d", other_kind_d]).unwrap(),
                Tag::parse(["wiki-version", "1"]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(Timestamp::now().as_secs() + 3))
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(
            expect_missing_write(&db, community, &other_kind, other_kind_d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, other_kind.id.as_bytes())
            .await
            .expect("delete other kind"));
        assert_ne!(
            expect_missing_write(&db, community, &other_kind, other_kind_d).await,
            ParameterizedReplaceStatus::WikiHeadRetired,
            "only kind 30623 participates in Wiki head retirement"
        );

        // A non-`_toc` address is not the conditional head coordinate.
        let page_d = "repo/overview";
        let page = event(&keys, page_d, "page", Timestamp::now().as_secs(), true);
        assert_eq!(
            expect_missing_write(&db, community, &page, page_d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, page.id.as_bytes())
            .await
            .expect("delete page"));
        assert_ne!(
            expect_missing_write(&db, community, &page, page_d).await,
            ParameterizedReplaceStatus::WikiHeadRetired,
            "only the replaceable _toc head carries this classification"
        );

        // A legacy, non-v1 `_toc` write keeps its existing classification.
        let legacy_d = "legacy/_toc";
        let legacy = event(&keys, legacy_d, "legacy", Timestamp::now().as_secs(), false);
        assert_eq!(
            expect_missing_write(&db, community, &legacy, legacy_d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, legacy.id.as_bytes())
            .await
            .expect("delete legacy"));
        assert_ne!(
            expect_missing_write(&db, community, &legacy, legacy_d).await,
            ParameterizedReplaceStatus::WikiHeadRetired,
            "a non-v1 head is not part of the conditional contract"
        );

        // Unconditional replacement is unchanged by any of this.
        let unconditional = event(
            &keys,
            d,
            "unconditional",
            Timestamp::now().as_secs() + 5,
            true,
        );
        assert_eq!(
            write(
                &db,
                community,
                &unconditional,
                d,
                ParameterizedReplacePrecondition::Unconditional,
            )
            .await,
            ParameterizedReplaceStatus::Inserted
        );
    })
    .await;
}

/// Item 5, second real order: the DELETION commits first and an exact writer
/// then arrives while a decision is held open. The waiting writer must observe
/// retirement, and must not resurrect the head behind the open decision.
#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn deletion_first_then_a_waiting_exact_writer_observes_retirement() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let owner = keys.public_key().to_bytes();
        let head = event(&keys, d, "head", Timestamp::now().as_secs(), true);
        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Inserted
        );

        // Deletion first, committed.
        assert!(db
            .soft_delete_event(community, head.id.as_bytes())
            .await
            .expect("delete head"));
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);

        // A competing head now takes the coordinate inside a held transaction,
        // so the late exact writer below must wait on the coordinate lock
        // rather than racing it.
        let competing = event(&keys, d, "competing", Timestamp::now().as_secs() + 1, true);
        let mut holder = db.begin_event_write_transaction().await.expect("holder tx");
        let held = db
            .replace_parameterized_event_in_transaction(
                &mut holder,
                community,
                &competing,
                d,
                None,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await
            .expect("competing insert decision");
        assert_eq!(held.status, ParameterizedReplaceStatus::Inserted);

        let writer_pool = bounded_deleter_pool(&db).await;
        let writer = Db::from_pool(writer_pool.clone());
        let mut waiting = writer
            .begin_event_write_transaction()
            .await
            .expect("waiting tx");
        match writer
            .replace_parameterized_event_in_transaction(
                &mut waiting,
                community,
                &head,
                d,
                None,
                ParameterizedReplacePrecondition::ExpectedMissing,
            )
            .await
        {
            Err(error) => assert_lock_timeout(error),
            Ok(result) => panic!("the late writer must wait: {:?}", result.status),
        }
        let _ = waiting.rollback().await;
        holder.commit().await.expect("commit competing head");

        // Once unblocked, the competing live head takes the ordinary conflict
        // branch, while the rejected write leaves that head untouched.
        assert_eq!(
            expect_missing_write(&db, community, &head, d).await,
            ParameterizedReplaceStatus::RevisionMismatch,
            "a live competing head is an ordinary conflict, not retirement proof"
        );
        assert_eq!(live_rows(&db, community, &owner, d).await, 1);
        let live: Vec<u8> = sqlx::query_scalar(
            "SELECT id FROM events WHERE community_id=$1 AND kind=30623 \
             AND pubkey=$2 AND d_tag=$3 AND deleted_at IS NULL",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .bind(d)
        .fetch_one(&db.pool)
        .await
        .expect("live head id");
        assert_eq!(
            live,
            competing.id.as_bytes().to_vec(),
            "a rejected retired write must not replace the competing head"
        );
        writer_pool.close().await;
    })
    .await;
}

/// Item 2, old-send-first: an already accepted H cannot be retired by a later
/// deletion of its predecessor E. The H decision is held open in a real
/// transaction, so the deletion must block on the shared coordinate lock.
#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn an_accepted_head_is_not_retired_by_a_later_predecessor_deletion() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let owner = keys.public_key().to_bytes();
        let predecessor = event(&keys, d, "predecessor", Timestamp::now().as_secs(), true);
        let head = event(&keys, d, "head", Timestamp::now().as_secs() + 1, true);
        assert_eq!(
            expect_missing_write(&db, community, &predecessor, d).await,
            ParameterizedReplaceStatus::Inserted
        );

        // H's conditional decision against E succeeds and is held open.
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
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await
            .expect("conditional decision");
        assert_eq!(decided.status, ParameterizedReplaceStatus::Inserted);

        // Deleting E now must wait for that open decision.
        let deleter_pool = bounded_deleter_pool(&db).await;
        let deleter = Db::from_pool(deleter_pool.clone());
        match deleter
            .soft_delete_event(community, predecessor.id.as_bytes())
            .await
        {
            Err(error) => assert_lock_timeout(error),
            Ok(deleted) => panic!("predecessor deletion must wait: {deleted}"),
        }
        decision.commit().await.expect("commit head");
        // Committing H already soft-deleted the superseded E as part of the
        // production replacement, so the now-unblocked delete finds no live
        // row and reports false. Assert the durable tombstone directly rather
        // than inferring it from this return value.
        assert!(
            !deleter
                .soft_delete_event(community, predecessor.id.as_bytes())
                .await
                .expect("unblocked predecessor delete"),
            "an already superseded predecessor is no longer live to delete"
        );
        let predecessor_tombstoned: bool = sqlx::query_scalar(
            "SELECT deleted_at IS NOT NULL FROM events \
             WHERE community_id=$1 AND kind=30623 AND pubkey=$2 AND d_tag=$3 AND id=$4",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .bind(d)
        .bind(predecessor.id.as_bytes())
        .fetch_one(&db.pool)
        .await
        .expect("predecessor row");
        assert!(
            predecessor_tombstoned,
            "the exact predecessor is durably non-live at this coordinate"
        );

        // H is live and stays the unique live head. Its exact replay ACKs as a
        // duplicate and must not be reclassified as retired.
        assert_eq!(live_rows(&db, community, &owner, d).await, 1);
        assert_eq!(
            exact_replay(&db, community, &head, d).await,
            ParameterizedReplaceStatus::Duplicate,
            "an accepted live head keeps ACKing successfully"
        );
        assert_eq!(
            write(
                &db,
                community,
                &head,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::Duplicate,
            "a retired predecessor must not retire an already accepted head"
        );
        let live: Vec<u8> = sqlx::query_scalar(
            "SELECT id FROM events WHERE community_id=$1 AND kind=30623 \
             AND pubkey=$2 AND d_tag=$3 AND deleted_at IS NULL",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .bind(d)
        .fetch_one(&db.pool)
        .await
        .expect("live head id");
        assert_eq!(live, head.id.as_bytes().to_vec());
        deleter_pool.close().await;
    })
    .await;
}

/// Item 2, deletion-first: E is deleted, then the H/E retirement decision is
/// held open in its real transaction while a late conditional H writer blocks
/// on the same coordinate lock.
#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn expected_head_retirement_serializes_with_a_late_conditional_writer() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let owner = keys.public_key().to_bytes();
        let predecessor = event(&keys, d, "predecessor", Timestamp::now().as_secs(), true);
        let head = event(&keys, d, "head", Timestamp::now().as_secs() + 1, true);
        assert_eq!(
            expect_missing_write(&db, community, &predecessor, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, predecessor.id.as_bytes())
            .await
            .expect("delete predecessor"));
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);

        // The refusal decision is held open in a real transaction.
        let mut refusal = db
            .begin_event_write_transaction()
            .await
            .expect("refusal tx");
        let refused = db
            .replace_parameterized_event_in_transaction(
                &mut refusal,
                community,
                &head,
                d,
                None,
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await
            .expect("retirement decision");
        assert_eq!(
            refused.status,
            ParameterizedReplaceStatus::WikiExpectedHeadRetired
        );

        // A late conditional writer for the same coordinate must block on that
        // decision's lock rather than race it.
        let writer_pool = bounded_deleter_pool(&db).await;
        let writer = Db::from_pool(writer_pool.clone());
        let mut late = writer
            .begin_event_write_transaction()
            .await
            .expect("late tx");
        match writer
            .replace_parameterized_event_in_transaction(
                &mut late,
                community,
                &head,
                d,
                None,
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await
        {
            Err(error) => assert_lock_timeout(error),
            Ok(result) => panic!("the late writer must wait: {:?}", result.status),
        }
        let _ = late.rollback().await;

        // The relay rolls the refusal back; the classification is stable and
        // nothing was written.
        refusal.rollback().await.expect("rollback refusal");
        assert_eq!(
            write(
                &db,
                community,
                &head,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::WikiExpectedHeadRetired,
            "the decision is stable across the rolled-back refusal"
        );
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);

        // A competing head is a separate fact: with C live, the same request
        // takes the ordinary current-head conflict branch and C is untouched.
        let competing = event(&keys, d, "competing", Timestamp::now().as_secs() + 2, true);
        assert_eq!(
            expect_missing_write(&db, community, &competing, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(
                &db,
                community,
                &head,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch,
            "a live competing head is an ordinary conflict, not retirement proof"
        );
        let live: Vec<u8> = sqlx::query_scalar(
            "SELECT id FROM events WHERE community_id=$1 AND kind=30623 \
             AND pubkey=$2 AND d_tag=$3 AND deleted_at IS NULL",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .bind(d)
        .fetch_one(&db.pool)
        .await
        .expect("live head id");
        assert_eq!(live, competing.id.as_bytes().to_vec());
        writer_pool.close().await;
    })
    .await;
}

/// D-079 precondition retirement: the exact expected predecessor was accepted
/// and later deleted while the coordinate has no live head.
#[tokio::test]
#[ignore = "requires isolated PostgreSQL"]
async fn conditional_publication_reports_retired_expected_head_only_for_the_exact_predecessor() {
    scenario(|db, community, keys| async move {
        let d = "repo/_toc";
        let owner = keys.public_key().to_bytes();
        let predecessor = event(&keys, d, "predecessor", Timestamp::now().as_secs(), true);
        let successor = event(&keys, d, "successor", Timestamp::now().as_secs() + 1, true);

        assert_eq!(
            expect_missing_write(&db, community, &predecessor, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, predecessor.id.as_bytes())
            .await
            .expect("delete predecessor"));
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);

        assert_eq!(
            write(
                &db,
                community,
                &successor,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::WikiExpectedHeadRetired,
            "the exact expected predecessor can never be live again"
        );
        assert_eq!(live_rows(&db, community, &owner, d).await, 0);

        // An expected revision that was never accepted at this coordinate is
        // unknown, not retired, and keeps the generic classification.
        let unknown = event(&keys, d, "unknown", Timestamp::now().as_secs() + 2, true);
        assert_eq!(
            write(
                &db,
                community,
                &successor,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(unknown.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::RevisionMissing,
            "an unknown expected revision is never upgraded into proof"
        );

        // Neither is an event that exists only at a different coordinate.
        let foreign_d = "other/_toc";
        let foreign = event(
            &keys,
            foreign_d,
            "foreign",
            Timestamp::now().as_secs() + 3,
            true,
        );
        assert_eq!(
            expect_missing_write(&db, community, &foreign, foreign_d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert!(db
            .soft_delete_event(community, foreign.id.as_bytes())
            .await
            .expect("delete foreign"));
        assert_eq!(
            write(
                &db,
                community,
                &successor,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(foreign.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::RevisionMissing,
            "a retired event at another coordinate proves nothing here"
        );

        // With a live head restored, a retired predecessor is an ordinary
        // mismatch again rather than a permanent retirement.
        let live = event(&keys, d, "live", Timestamp::now().as_secs() + 4, true);
        assert_eq!(
            expect_missing_write(&db, community, &live, d).await,
            ParameterizedReplaceStatus::Inserted
        );
        assert_eq!(
            write(
                &db,
                community,
                &successor,
                d,
                ParameterizedReplacePrecondition::ExpectedRevision(predecessor.id.as_bytes()),
            )
            .await,
            ParameterizedReplaceStatus::RevisionMismatch
        );
    })
    .await;
}
