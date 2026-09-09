use super::next_support::{self as next, Outcome};
use super::{classification, insert, message, route, timestamp, ContactClass, Fixture};

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_b1_soft_delete_preserves_original_raw_delete_is_denied() {
    let f = Fixture::new().await;
    let old = next::aged_timestamp(&f).await;
    let legacy = message(&f, "legacy retained", old);
    f.insert(&legacy).await;
    next::install(&f).await;
    let original = message(&f, "soft-deleted routed original", old + 1);
    let mut tx = f.pool.begin().await.expect("route");
    assert_eq!(
        route(&f, &mut tx, &original, true, 8192).await,
        ContactClass::Routed
    );
    let suppressed = message(&f, "suppressed retained", old + 2);
    assert!(insert(&f, &mut tx, &suppressed, Some(ContactClass::Disabled)).await);
    tx.commit().await.expect("commit route");
    assert!(
        crate::event::soft_delete_event(&f.pool, f.community, original.id.as_bytes())
            .await
            .expect("actual soft delete")
    );
    assert_eq!(classification(&f, &original).await, Some(1));
    let mut tx = f.pool.begin().await.expect("deleted replay");
    assert_eq!(
        next::route_or_observe(&f, &mut tx, &original, 8192).await,
        Outcome::Classified(1)
    );
    tx.commit().await.expect("historical replay");
    // Age and floor do not authorize raw kind9 hard erasure, even for legacy.
    sqlx::query("UPDATE communities SET contact_replay_floor=$2 WHERE id=$1")
        .bind(f.community.as_uuid())
        .bind(timestamp(&suppressed))
        .execute(&f.pool)
        .await
        .expect("old floor fixture");
    for event in [&legacy, &original, &suppressed] {
        let mut tx = f.pool.begin().await.expect("unauthorized hard erasure");
        let result = sqlx::query("DELETE FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(event.id.as_bytes().as_slice())
            .execute(&mut *tx)
            .await
            .map(|_| ());
        next::rejected(
            tx,
            result,
            "ordinary raw original DELETE must remain denied despite floor+age",
        )
        .await;
    }
    // Legacy NULL rows must fail at the rewrite itself, before their kind can
    // escape the DELETE guard. Exercise the following DELETE on a missing-guard
    // build, then roll back even that counterexample before failing the test.
    let mut rewrite = f.pool.begin().await.expect("legacy rewrite then erase");
    let changed = sqlx::query("UPDATE events SET kind=10 WHERE community_id=$1 AND id=$2")
        .bind(f.community.as_uuid())
        .bind(legacy.id.as_bytes().as_slice())
        .execute(&mut *rewrite)
        .await;
    match changed {
        Err(error) => {
            super::constraint_error(Err(error), "legacy kind rewrite must reject immediately");
            rewrite.rollback().await.expect("rollback rejected rewrite");
        }
        Ok(_) => {
            let deleted = sqlx::query("DELETE FROM events WHERE community_id=$1 AND id=$2")
                .bind(f.community.as_uuid())
                .bind(legacy.id.as_bytes().as_slice())
                .execute(&mut *rewrite)
                .await
                .map(|result| result.rows_affected());
            rewrite
                .rollback()
                .await
                .expect("restore missing-guard counterexample");
            panic!("legacy kind rewrite was accepted; following DELETE result: {deleted:?}");
        }
    }
    for mutation in [
        "UPDATE events SET id=decode(repeat('ab',32),'hex') WHERE community_id=$1 AND id=$2",
        "UPDATE events SET created_at=created_at+interval '1 second' WHERE community_id=$1 AND id=$2",
    ] {
        let mut tx = f.pool.begin().await.expect("legacy identity mutation");
        let result=sqlx::query(mutation).bind(f.community.as_uuid()).bind(legacy.id.as_bytes().as_slice())
            .execute(&mut *tx).await.map(|_|());
        // Require the UPDATE itself to fail, not a subsequent deferred check.
        super::constraint_error(result,"legacy signed identity cannot move");
        tx.rollback().await.expect("rollback legacy identity mutation");
    }
    let other_community = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO communities (id,host) VALUES ($1,$2)")
        .bind(other_community)
        .bind(format!("legacy-rewrite-{other_community}.example"))
        .execute(&f.pool)
        .await
        .expect("valid alternative community fixture");
    let mut moved = f.pool.begin().await.expect("legacy community rewrite");
    let result = sqlx::query("UPDATE events SET community_id=$3 WHERE community_id=$1 AND id=$2")
        .bind(f.community.as_uuid())
        .bind(legacy.id.as_bytes().as_slice())
        .bind(other_community)
        .execute(&mut *moved)
        .await
        .map(|_| ());
    super::constraint_error(
        result,
        "legacy original cannot move to another valid community",
    );
    moved.rollback().await.expect("rollback community rewrite");
    assert_eq!(classification(&f, &legacy).await, None);
    assert_eq!(next::counts(&f).await, (3, 1, 1));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_b2_proof_mutation_and_untracked_evidence_delete_reject() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let original = message(&f, "retained immutable evidence", 1_800_011_000);
    let mut tx = f.pool.begin().await.expect("route");
    route(&f, &mut tx, &original, true, 8192).await;
    tx.commit().await.expect("route commit");
    let before: (Vec<u8>, String, serde_json::Value, Vec<u8>) = sqlx::query_as(
        "SELECT id,content,tags,sig FROM events WHERE community_id=$1 AND kind=46044",
    )
    .bind(f.community.as_uuid())
    .fetch_one(&f.pool)
    .await
    .expect("signed proof snapshot");
    for mutation in [
        "UPDATE events SET content='forged' WHERE community_id=$1 AND kind=46044",
        "UPDATE events SET tags='[]'::jsonb WHERE community_id=$1 AND kind=46044",
        "DELETE FROM events WHERE community_id=$1 AND kind=46044",
        "DELETE FROM contact_routes WHERE community_id=$1",
    ] {
        let mut tx = f.pool.begin().await.expect("evidence mutation");
        let result = sqlx::query(mutation)
            .bind(f.community.as_uuid())
            .execute(&mut *tx)
            .await
            .map(|_| ());
        next::rejected(tx, result, mutation).await;
        let after: (Vec<u8>, String, serde_json::Value, Vec<u8>) = sqlx::query_as(
            "SELECT id,content,tags,sig FROM events WHERE community_id=$1 AND kind=46044",
        )
        .bind(f.community.as_uuid())
        .fetch_one(&f.pool)
        .await
        .expect("retained signed bytes");
        assert_eq!(after, before);
        assert_eq!(next::counts(&f).await, (1, 1, 1));
    }
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_b3_route_gc_preserves_original_and_requires_ninety_days() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let old = next::aged_timestamp(&f).await;
    let present = next::retention_now(&f).await;
    // A newly recorded decision cannot expire merely because its signed
    // original/proof timestamps are old. Require rejection before commit, so
    // deferred INSERT checks cannot masquerade as the age guard.
    let old_original = message(&f, "old original, new decision", old - 1);
    let mut probe = f.pool.begin().await.expect("new evidence for old original");
    next::contact_lock(&f, &mut probe).await;
    let reserved = sqlx::query(
        "UPDATE contact_quota SET used=used+1 WHERE community_id=$1 AND stripe=0 AND used<8192",
    )
    .bind(f.community.as_uuid())
    .execute(&mut *probe)
    .await
    .expect("reserve real fixture stripe");
    assert_eq!(reserved.rows_affected(), 1);
    assert!(insert(&f, &mut probe, &old_original, Some(ContactClass::Routed)).await);
    let relay = nostr::Keys::generate();
    let contact = nostr::Keys::generate();
    let proof = nostr::EventBuilder::new(
        nostr::Kind::Custom(super::CONTACT_PROOF),
        "fixture decision",
    )
    .tags([
        nostr::Tag::parse(["h", &f.channel.to_string()]).expect("h"),
        nostr::Tag::parse(["p", &contact.public_key().to_hex()]).expect("p"),
        nostr::Tag::parse(["original", &old_original.id.to_hex()]).expect("original"),
        nostr::Tag::parse(["phase", "decision"]).expect("phase"),
    ])
    .custom_created_at(old_original.created_at)
    .sign_with_keys(&relay)
    .expect("signed proof");
    assert!(insert(&f, &mut probe, &proof, None).await);
    // Supply decided_at explicitly. A DEFAULT cannot protect this INSERT;
    // removing its stamp assignment (keeping the catalog trigger present)
    // must fail the immediate readback below.
    sqlx::query(
        "INSERT INTO contact_routes (community_id,original_id,original_created_at,channel_id,\
         contact_pubkey,relay_pubkey,decision_id,decision_created_at,stripe,decided_at) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,0,$9)",
    )
    .bind(f.community.as_uuid())
    .bind(old_original.id.as_bytes().as_slice())
    .bind(timestamp(&old_original))
    .bind(f.channel)
    .bind(contact.public_key().to_bytes().to_vec())
    .bind(relay.public_key().to_bytes().to_vec())
    .bind(proof.id.as_bytes().as_slice())
    .bind(timestamp(&proof))
    .bind(timestamp(&old_original))
    .execute(&mut *probe)
    .await
    .expect("explicitly backdated route INSERT");
    let recorded: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT decided_at FROM contact_routes WHERE community_id=$1 AND original_id=$2",
    )
    .bind(f.community.as_uuid())
    .bind(old_original.id.as_bytes().as_slice())
    .fetch_one(&mut *probe)
    .await
    .expect("server-created age anchor");
    assert!(recorded >= present && recorded > timestamp(&old_original));
    let error = next::gc_one(&f, &mut probe, &old_original, 3)
        .await
        .expect_err("new evidence for an old original has not aged ninety days");
    super::constraint_error(
        Err(error),
        "retention uses server decision age, not original age",
    );
    probe.rollback().await.expect("rollback age probe");
    // Create evidence at a past SERVER clock, then advance that clock. Never
    // rewrite decided_at or infer retention from the original's signature.
    next::set_retention_clock(&f, present - chrono::Duration::days(91)).await;
    let original = message(&f, "ordinary original survives GC", old);
    let mut tx = f.pool.begin().await.expect("route");
    route(&f, &mut tx, &original, true, 8192).await;
    tx.commit().await.expect("route commit");
    next::set_retention_clock(&f, present).await;
    let mut tx = f.pool.begin().await.expect("eligible GC");
    next::gc_one(&f, &mut tx, &original, 3)
        .await
        .expect("eligible cleanup");
    tx.commit().await.expect("atomic GC");
    assert_eq!(next::counts(&f).await, (1, 0, 0));
    assert_eq!(classification(&f, &original).await, Some(1));
    let content: String =
        sqlx::query_scalar("SELECT content FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(original.id.as_bytes().as_slice())
            .fetch_one(&f.pool)
            .await
            .expect("original retained");
    assert_eq!(content, original.content);
    let mut tx = f.pool.begin().await.expect("replay after GC");
    assert_eq!(
        next::route_or_observe(&f, &mut tx, &original, 8192).await,
        Outcome::Unavailable
    );
    let missing_old = message(&f, "new old ID at durable floor", old - 1);
    assert_eq!(
        next::route_or_observe(&f, &mut tx, &missing_old, 8192).await,
        Outcome::Classified(12)
    );
    tx.commit()
        .await
        .expect("old-history suppression without route");
    let recent = message(&f, "recent original", nostr::Timestamp::now().as_secs());
    let mut tx = f.pool.begin().await.expect("recent route");
    route(&f, &mut tx, &recent, true, 8192).await;
    tx.commit().await.expect("recent commit");
    let mut tx = f.pool.begin().await.expect("ineligible GC");
    let result = next::gc_one(&f, &mut tx, &recent, 3).await;
    next::rejected(
        tx,
        result,
        "GC cannot remove recent proof or advance recent floor",
    )
    .await;
    assert_eq!(next::counts(&f).await, (3, 1, 1));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_b4_gc_rollback_replay_and_repeat_are_atomic() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let present = next::retention_now(&f).await;
    next::set_retention_clock(&f, present - chrono::Duration::days(91)).await;
    let original = message(&f, "GC rollback", next::aged_timestamp(&f).await);
    let mut tx = f.pool.begin().await.expect("route");
    route(&f, &mut tx, &original, true, 8192).await;
    tx.commit().await.expect("route commit");
    next::set_retention_clock(&f, present).await;
    for stage in [1, 2, 3] {
        let mut tx = f.pool.begin().await.expect("GC stage");
        next::gc_one(&f, &mut tx, &original, stage)
            .await
            .expect("stage executes");
        tx.rollback().await.expect("inject rollback");
        assert_eq!(next::counts(&f).await, (1, 1, 1));
        let floor: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT contact_replay_floor FROM communities WHERE id=$1")
                .bind(f.community.as_uuid())
                .fetch_one(&f.pool)
                .await
                .expect("floor rollback");
        assert!(floor.is_none());
    }
    let mut gc = f.pool.begin().await.expect("GC actor");
    next::gc_one(&f, &mut gc, &original, 3)
        .await
        .expect("pending GC");
    let mut replay = f.pool.begin().await.expect("replay actor");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *replay)
        .await
        .expect("backend");
    let (blocked, outcome) = tokio::join!(
        async {
            let blocked = next::wait_blocked(&mut gc, pid).await;
            gc.commit().await.expect("GC commit");
            blocked
        },
        async {
            let result = next::route_or_observe(&f, &mut replay, &original, 8192).await;
            replay.commit().await.expect("replay commit");
            result
        }
    );
    assert!(blocked);
    assert_eq!(outcome, Outcome::Unavailable);
    let mut repeat = f.pool.begin().await.expect("repeat GC");
    next::gc_one(&f, &mut repeat, &original, 3)
        .await
        .expect("idempotent GC");
    repeat.commit().await.expect("repeat commit");
    assert_eq!(next::counts(&f).await, (1, 0, 0));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_b5_actual_deletion_catalog_accepts_registered_proof_tables() {
    let f = Fixture::new().await;
    let db = crate::Db::from_pool(f.pool.clone());
    // The production storage foundation is part of the reviewed deletion
    // manifest. The real executor must accept it before the proof-only guards
    // are installed.
    db.validate_deletion_catalog()
        .await
        .expect("registered production contact tables must pass the catalog guard");
    next::install(&f).await;
    db.validate_deletion_catalog()
        .await
        .expect("proof-only contact guards must preserve the catalog contract");
}
