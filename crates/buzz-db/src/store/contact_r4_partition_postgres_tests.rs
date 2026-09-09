use super::next_support::{self as next, Outcome};
use super::{classification, constraint_error, insert, message, route, ContactClass, Fixture};

fn future_time() -> u64 {
    chrono::DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z")
        .expect("fixed range")
        .timestamp() as u64
}

async fn empty_future_range(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) {
    next::maintenance_lock(tx).await;
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM events_p_future")
        .fetch_one(&mut **tx)
        .await
        .expect("empty fixture range");
    assert_eq!(rows, 0, "prepare range only before any evidence exists");
    sqlx::query("DROP TABLE events_p_future")
        .execute(&mut **tx)
        .await
        .expect("owned empty fixture setup");
    // Never use this setup operation as a supported production retention path.
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_c1_real_partition_create_needs_deferred_guard_before_commit() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let mut tx = f.pool.begin().await.expect("DDL transaction");
    empty_future_range(&mut tx).await;
    crate::partition::contact_proof_ensure_partition_tx(
        &mut tx,
        "2099-01-01",
        "2099-02-01",
        "2099_01",
    )
    .await
    .expect("actual partition manager CREATE");
    let error = next::verify_catalog(&mut tx)
        .await
        .expect_err("new leaf must not be considered protected yet");
    assert!(error.to_string().contains("events_p2099_01"));
    next::install_leaf_guard(&mut tx, "events_p2099_01").await;
    next::verify_catalog(&mut tx)
        .await
        .expect("exact coverage before DDL commit");
    tx.commit().await.expect("atomic maintenance");
    let parent = message(&f, "new parent path", future_time());
    f.insert(&parent).await;
    assert_eq!(classification(&f, &parent).await, Some(0));
    let direct = message(&f, "new direct child path", future_time() + 1);
    let mut connection = f.pool.acquire().await.expect("direct child connection");
    next::direct_original(&f, &mut connection, "events_p2099_01", &direct, None).await;
    drop(connection);
    assert_eq!(classification(&f, &direct).await, Some(0));
    let torn = message(&f, "new leaf torn route", future_time() + 2);
    let mut tx = f.pool.begin().await.expect("new leaf classified original");
    assert!(insert(&f, &mut tx, &torn, Some(ContactClass::Routed)).await);
    constraint_error(
        tx.commit().await,
        "new leaf must reject missing route/proof at commit",
    );
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_c2_attach_and_wrong_or_disabled_guards_fail_closed() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let mut tx = f.pool.begin().await.expect("attach transaction");
    empty_future_range(&mut tx).await;
    sqlx::query("CREATE TABLE events_r4_attach (LIKE events INCLUDING DEFAULTS INCLUDING CONSTRAINTS INCLUDING GENERATED)")
        .execute(&mut *tx).await.expect("preexisting empty child");
    next::verify_attach_rows(&mut tx, "events_r4_attach")
        .await
        .expect("empty import");
    sqlx::query("ALTER TABLE events ATTACH PARTITION events_r4_attach FOR VALUES FROM ('2099-01-01') TO ('2099-02-01')")
        .execute(&mut *tx).await.expect("actual ATTACH");
    assert!(
        next::verify_catalog(&mut tx).await.is_err(),
        "ATTACH does not supply deferred protection"
    );
    next::install_leaf_guard(&mut tx, "events_r4_attach").await;
    next::verify_catalog(&mut tx).await.expect("covered attach");
    sqlx::raw_sql("CREATE FUNCTION contact_wrong_guard() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$; \
        DROP TRIGGER contact_check_original ON events_r4_attach; \
        CREATE CONSTRAINT TRIGGER contact_check_original AFTER INSERT ON events_r4_attach \
        DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION contact_wrong_guard();")
        .execute(&mut *tx).await.expect("wrong-function same-name fault");
    assert!(
        next::verify_catalog(&mut tx).await.is_err(),
        "must check function OID, not only name/type"
    );
    sqlx::query("DROP TRIGGER contact_check_original ON events_r4_attach")
        .execute(&mut *tx)
        .await
        .expect("remove fault");
    next::install_leaf_guard(&mut tx, "events_r4_attach").await;
    sqlx::query("ALTER TABLE events_r4_attach DISABLE TRIGGER contact_classify_original")
        .execute(&mut *tx)
        .await
        .expect("disabled guard fault");
    assert!(
        next::verify_catalog(&mut tx).await.is_err(),
        "disabled protection is unavailable"
    );
    sqlx::query("ALTER TABLE events_r4_attach ENABLE TRIGGER contact_classify_original")
        .execute(&mut *tx)
        .await
        .expect("restore exact guard");
    next::verify_catalog(&mut tx)
        .await
        .expect("restored attach");
    tx.commit()
        .await
        .expect("attach and verify in one transaction");
    let direct = message(&f, "attached direct child", future_time());
    let mut connection = f.pool.acquire().await.expect("direct child connection");
    next::direct_original(&f, &mut connection, "events_r4_attach", &direct, None).await;
    drop(connection);
    assert_eq!(classification(&f, &direct).await, Some(0));
    // A second, unattached import cannot smuggle prior classified decisions.
    let mut tx = f.pool.begin().await.expect("dirty attach candidate");
    next::maintenance_lock(&mut tx).await;
    sqlx::query("CREATE TABLE events_r4_dirty (LIKE events INCLUDING DEFAULTS INCLUDING CONSTRAINTS INCLUDING GENERATED)")
        .execute(&mut *tx).await.expect("dirty import table");
    let dirty = message(&f, "untrusted classified import", future_time() + 1);
    next::direct_original(&f, &mut tx, "events_r4_dirty", &dirty, Some(1)).await;
    assert!(next::verify_attach_rows(&mut tx, "events_r4_dirty")
        .await
        .is_err());
    tx.rollback().await.expect("reject entire dirty import");
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn contact_proof_r4_next_c3_detected_catalog_or_material_loss_is_unavailable() {
    let f = Fixture::new().await;
    next::install(&f).await;
    let original = message(&f, "frozen decision", 1_800_020_000);
    let mut tx = f.pool.begin().await.expect("route");
    route(&f, &mut tx, &original, true, 8192).await;
    tx.commit().await.expect("route commit");
    let mut fault = f.pool.begin().await.expect("owned fixture catalog fault");
    next::maintenance_lock(&mut fault).await;
    sqlx::query("ALTER TABLE events_p_future DISABLE TRIGGER contact_check_original")
        .execute(&mut *fault)
        .await
        .expect("fault injection");
    assert_eq!(
        next::route_or_observe(&f, &mut fault, &original, 8192).await,
        Outcome::Unavailable
    );
    fault.rollback().await.expect("restore catalog by rollback");
    // This is privileged loss outside supported maintenance, not a new erasure
    // API. It occurs only in an owned fixture and is rolled back, never repaired
    // by recomputing or re-signing the original decision.
    let mut fault = f.pool.begin().await.expect("owned fixture proof loss");
    next::maintenance_lock(&mut fault).await;
    let guards:Vec<(String,String)>=sqlx::query_as(
        "SELECT t.tgname::text,t.tgenabled::text FROM pg_trigger t JOIN pg_proc p ON p.oid=t.tgfoid \
         WHERE t.tgrelid='events_p_future'::regclass AND p.proname LIKE 'contact_%' AND t.tgenabled IN ('O','A')",
    ).fetch_all(&mut *fault).await.expect("actual owned fixture contact guards");
    assert!(
        !guards.is_empty() && guards.len() <= 8,
        "bounded fault injection"
    );
    for (name, _) in &guards {
        let quoted = name.replace('"', "\"\"");
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE events_p_future DISABLE TRIGGER \"{quoted}\""
        )))
        .execute(&mut *fault)
        .await
        .expect("privileged fixture loss injection");
    }
    sqlx::query("DELETE FROM events WHERE community_id=$1 AND kind=46044")
        .bind(f.community.as_uuid())
        .execute(&mut *fault)
        .await
        .expect("erase proof only in fault transaction");
    for (name, enabled) in &guards {
        let quoted = name.replace('"', "\"\"");
        let mode = if enabled == "A" {
            "ENABLE ALWAYS"
        } else {
            "ENABLE"
        };
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE events_p_future {mode} TRIGGER \"{quoted}\""
        )))
        .execute(&mut *fault)
        .await
        .expect("restore exact catalog before observing material loss");
    }
    next::verify_catalog(&mut fault)
        .await
        .expect("coverage restored; material still missing");
    assert_eq!(
        next::route_or_observe(&f, &mut fault, &original, 8192).await,
        Outcome::Unavailable
    );
    fault
        .rollback()
        .await
        .expect("restore material by rollback");
    assert_eq!(next::counts(&f).await, (1, 1, 1));
    assert_eq!(classification(&f, &original).await, Some(1));
}
