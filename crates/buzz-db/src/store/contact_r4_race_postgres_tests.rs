use super::next_support::{self as next, Outcome};
use super::{constraint_error, insert, message, route, ContactClass, Fixture};

// Two connection-owned futures, no detached tasks or third observer connection.
// Join finishes both actors before assertions; deadlines live in the fixture.
async fn overlap(
    f: &Fixture,
    other: &Fixture,
    duplicate: bool,
    rollback: bool,
    at: u64,
    limit: i32,
) -> (Outcome, bool) {
    let a = message(f, "actor A", at);
    let b = if duplicate {
        a.clone()
    } else {
        message(other, "actor B", at + 1)
    };
    let mut first = f.pool.begin().await.expect("actor A connection");
    assert_eq!(
        next::route_or_observe(f, &mut first, &a, limit).await,
        Outcome::Classified(1)
    );
    let mut second = f.pool.begin().await.expect("actor B connection");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *second)
        .await
        .expect("actor B backend");
    let (blocked, result) = tokio::join!(
        async {
            let blocked = next::wait_blocked(&mut first, pid).await;
            if rollback {
                first.rollback().await.expect("actor A rollback");
            } else {
                first.commit().await.expect("actor A commit");
            }
            blocked
        },
        async {
            let outcome = next::route_or_observe(other, &mut second, &b, limit).await;
            second.commit().await.expect("actor B commit");
            outcome
        }
    );
    (result, blocked)
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_a1_same_stripe_last_slot_accepts_both_originals() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let second = next::second_channel(&f).await;
    let (outcome, blocked) = overlap(&f, &second, false, false, 1_800_001_000, 1).await;
    assert!(blocked, "must observe actual quota-row contention");
    assert_eq!(outcome, Outcome::Classified(10));
    assert_eq!(next::counts(&f).await, (2, 1, 1));
    // Reverse the winning channel with exactly one additional available slot.
    let (outcome, blocked) = overlap(&second, &f, false, false, 1_800_001_010, 2).await;
    assert!(blocked);
    assert_eq!(outcome, Outcome::Classified(10));
    assert_eq!(next::counts(&f).await, (4, 2, 2));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_a2_rolled_back_reservation_is_reusable() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let second = next::second_channel(&f).await;
    let (outcome, blocked) = overlap(&f, &second, false, true, 1_800_002_000, 1).await;
    assert!(blocked);
    assert_eq!(outcome, Outcome::Classified(1));
    assert_eq!(next::counts(&f).await, (1, 1, 1));
    let a = message(&f, "rollback after original before proof", 1_800_002_100);
    let mut tx = f.pool.begin().await.expect("staged rollback");
    sqlx::query("UPDATE contact_quota SET used=used+1 WHERE community_id=$1 AND stripe=0")
        .bind(f.community.as_uuid())
        .execute(&mut *tx)
        .await
        .expect("reserve");
    assert!(insert(&f, &mut tx, &a, Some(ContactClass::Routed)).await);
    tx.rollback().await.expect("rollback before proof");
    assert_eq!(next::counts(&f).await, (1, 1, 1));
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_a3_duplicate_waiter_reuses_committed_decision() {
    let f = Fixture::new().await;
    next::install(&f).await;
    for (index, rollback) in [false, true].into_iter().enumerate() {
        let (outcome, blocked) =
            overlap(&f, &f, true, rollback, 1_800_003_000 + index as u64, 8192).await;
        assert!(blocked, "duplicate must wait at the contact key");
        assert_eq!(outcome, Outcome::Classified(1));
        let committed = index as i64 + 1;
        assert_eq!(next::counts(&f).await, (committed, committed, committed));
        let proofs: i64 =
            sqlx::query_scalar("SELECT count(*) FROM events WHERE community_id=$1 AND kind=46044")
                .bind(f.community.as_uuid())
                .fetch_one(&f.pool)
                .await
                .expect("one proof per committed original");
        assert_eq!(proofs, committed);
    }
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_a4_suppressed_and_legacy_replay_never_reserve() {
    let f = Fixture::new().await;
    let legacy = message(&f, "legacy", 1_800_004_000);
    f.insert(&legacy).await;
    next::install(&f).await;
    let suppressed = message(&f, "disabled", 1_800_004_001);
    let mut tx = f.pool.begin().await.expect("disabled original");
    assert!(insert(&f, &mut tx, &suppressed, Some(ContactClass::Disabled)).await);
    tx.commit().await.expect("disabled commit");
    for (event, expected) in [
        (&legacy, Outcome::Legacy),
        (&suppressed, Outcome::Classified(4)),
    ] {
        let mut tx = f.pool.begin().await.expect("replay");
        assert_eq!(
            next::route_or_observe(&f, &mut tx, event, 8192).await,
            expected
        );
        tx.commit()
            .await
            .expect("historical classification unchanged");
    }
    assert_eq!(next::counts(&f).await, (2, 0, 0));
    let mut tx = f.pool.begin().await.expect("orphan quota");
    sqlx::query("UPDATE contact_quota SET used=1 WHERE community_id=$1 AND stripe=0")
        .bind(f.community.as_uuid())
        .execute(&mut *tx)
        .await
        .expect("quota write");
    constraint_error(tx.commit().await, "reservation without route must rollback");
    let fresh = message(&f, "retry after rollback", 1_800_004_002);
    let mut tx = f.pool.begin().await.expect("healthy retry");
    assert_eq!(
        route(&f, &mut tx, &fresh, true, 8192).await,
        ContactClass::Routed
    );
    tx.commit().await.expect("retry remains usable");
    assert_eq!(next::counts(&f).await, (3, 1, 1));
}
