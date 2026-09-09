//! Shared next-slice storage fixture. Never compiled into serving binaries.
use super::{insert, route, timestamp, ContactClass, Fixture};
use crate::error::{DbError, Result};
use chrono::{DateTime, Utc};
use nostr::Event;
use sqlx::{PgConnection, Postgres, Transaction};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Classified(i16),
    Legacy,
    Missing,
    Unavailable,
}

pub(super) async fn install(f: &Fixture) {
    super::install(f).await;
    // Fixture-only retention schema; never a production migration.
    sqlx::query("ALTER TABLE communities ADD COLUMN contact_replay_floor timestamptz")
        .execute(&f.pool)
        .await
        .expect("fixture replay floor");
    sqlx::raw_sql(include_str!("contact_r4_retention_guards.sql"))
        .execute(&f.pool)
        .await
        .expect("fixture retention guards");
    let mut connection = f.pool.acquire().await.expect("catalog setup connection");
    verify_catalog(&mut connection)
        .await
        .expect("healthy starting contact catalog");
}

/// The exact connection-bound predicate intended for later promotion. Tests
/// remove real catalog objects and invoke this function, not a constant result.
pub(super) async fn verify_catalog(connection: &mut PgConnection) -> Result<()> {
    let mut missing: Vec<String> = sqlx::query_scalar(
        r#"
        WITH rels AS (
            SELECT 'public.events'::regclass::oid AS oid
            UNION ALL SELECT inhrelid FROM pg_inherits WHERE inhparent='public.events'::regclass
        )
        SELECT c.relname::text FROM rels r JOIN pg_class c ON c.oid=r.oid
        WHERE NOT EXISTS (
            SELECT 1 FROM pg_attribute a WHERE a.attrelid=r.oid
                AND a.attname='contact_class' AND a.atttypid='smallint'::regtype
                AND NOT a.attnotnull AND NOT a.attisdropped
        ) OR NOT EXISTS (
            SELECT 1 FROM pg_constraint k WHERE k.conrelid=r.oid
                AND k.conname='contact_class_shape' AND k.convalidated
                AND regexp_replace(lower(pg_get_constraintdef(k.oid)), '[[:space:]()]', '', 'g')
                    = 'checkcontact_classisnullorkind=9andcontact_class>=0andcontact_class<=12'
        ) OR NOT EXISTS (
            SELECT 1 FROM pg_trigger t WHERE t.tgrelid=r.oid
                AND t.tgname='contact_classify_original'
                AND t.tgfoid=to_regprocedure('public.contact_classify_original()')
                AND t.tgenabled IN ('O','A') AND t.tgtype=23
        ) OR NOT EXISTS (
            SELECT 1 FROM pg_trigger t WHERE t.tgrelid=r.oid
                AND t.tgname='contact_guard_retention_event'
                AND t.tgfoid=to_regprocedure('public.contact_guard_retention_event()')
                AND t.tgenabled IN ('O','A') AND t.tgtype=27
        ) OR (r.oid <> 'public.events'::regclass AND NOT EXISTS (
            SELECT 1 FROM pg_trigger t WHERE t.tgrelid=r.oid
                AND t.tgname='contact_check_proof_delete'
                AND t.tgfoid=to_regprocedure('public.contact_check_proof_delete()')
                AND t.tgenabled IN ('O','A') AND t.tgtype=9
                AND t.tgdeferrable AND t.tginitdeferred
        )) OR (r.oid <> 'public.events'::regclass AND NOT EXISTS (
            SELECT 1 FROM pg_trigger t WHERE t.tgrelid=r.oid
                AND t.tgname='contact_check_original'
                AND t.tgfoid=to_regprocedure('public.contact_check_original()')
                AND t.tgenabled IN ('O','A') AND t.tgtype=5
                AND t.tgdeferrable AND t.tginitdeferred
        )) ORDER BY c.relname
        "#,
    )
    .fetch_all(&mut *connection)
    .await?;
    let retention_valid: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (SELECT 1 FROM pg_attribute WHERE attrelid='public.contact_routes'::regclass
            AND attname='decided_at' AND atttypid='timestamptz'::regtype AND attnotnull AND NOT attisdropped)
        AND EXISTS (SELECT 1 FROM pg_proc WHERE oid=to_regprocedure('public.contact_retention_now()')
            AND prorettype='timestamptz'::regtype)
        AND NOT EXISTS (
            SELECT 1 FROM (VALUES
                ('contact_routes','contact_stamp_decision','contact_stamp_decision()',7,false),
                ('contact_routes','contact_route_immutable','contact_route_immutable()',19,false),
                ('contact_routes','contact_guard_route_delete','contact_guard_route_delete()',11,false),
                ('contact_routes','contact_check_route_delete','contact_check_route_delete()',9,true),
                ('communities','contact_guard_replay_floor','contact_guard_replay_floor()',19,false)
            ) required(table_name,trigger_name,function_name,type_bits,deferred)
            WHERE NOT EXISTS (
                SELECT 1 FROM pg_trigger t WHERE t.tgrelid=to_regclass('public.'||required.table_name)
                    AND t.tgname=required.trigger_name AND t.tgfoid=to_regprocedure('public.'||required.function_name)
                    AND t.tgenabled IN ('O','A') AND t.tgtype=required.type_bits
                    AND t.tgdeferrable=required.deferred AND t.tginitdeferred=required.deferred
            )
        )
        "#,
    ).fetch_one(connection).await?;
    if !retention_valid {
        missing.push("retention anchor/guard coverage".into());
    }
    if !missing.is_empty() {
        return Err(DbError::InvalidData(format!(
            "contact catalog unavailable: {}",
            missing.join(",")
        )));
    }
    Ok(())
}

pub(super) async fn contact_lock(f: &Fixture, tx: &mut Transaction<'_, Postgres>) {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "crew_contact:{}:{}",
            f.community.as_uuid(),
            f.channel
        ))
        .execute(&mut **tx)
        .await
        .expect("contact serialization");
}

pub(super) async fn observe(f: &Fixture, connection: &mut PgConnection, event: &Event) -> Outcome {
    if verify_catalog(connection).await.is_err() {
        return Outcome::Unavailable;
    }
    let class: Option<Option<i16>> =
        sqlx::query_scalar("SELECT contact_class FROM events WHERE community_id=$1 AND id=$2")
            .bind(f.community.as_uuid())
            .bind(event.id.as_bytes().as_slice())
            .fetch_optional(&mut *connection)
            .await
            .expect("persisted original, including deleted");
    match class {
        None => Outcome::Missing,
        Some(None) => Outcome::Legacy,
        Some(Some(1)) => {
            let retained: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM contact_routes r JOIN events e \
                 ON e.community_id=r.community_id AND e.id=r.decision_id \
                 AND e.created_at=r.decision_created_at AND e.kind=46044 \
                 AND e.pubkey=r.relay_pubkey AND e.deleted_at IS NULL \
                 WHERE r.community_id=$1 AND r.original_id=$2)",
            )
            .bind(f.community.as_uuid())
            .bind(event.id.as_bytes().as_slice())
            .fetch_one(connection)
            .await
            .expect("retained proof lookup");
            if retained {
                Outcome::Classified(1)
            } else {
                Outcome::Unavailable
            }
        }
        Some(Some(value)) => Outcome::Classified(value),
    }
}

pub(super) async fn route_or_observe(
    f: &Fixture,
    tx: &mut Transaction<'_, Postgres>,
    event: &Event,
    limit: i32,
) -> Outcome {
    contact_lock(f, tx).await;
    let existing = observe(f, tx, event).await;
    if existing != Outcome::Missing {
        return existing;
    }
    let floor: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT contact_replay_floor FROM communities WHERE id=$1")
            .bind(f.community.as_uuid())
            .fetch_one(&mut **tx)
            .await
            .expect("floor");
    if floor.is_some_and(|floor| timestamp(event) <= floor) {
        assert!(insert(f, tx, event, Some(ContactClass::ReplayFloor)).await);
        return Outcome::Classified(12);
    }
    Outcome::Classified(route(f, tx, event, true, limit).await as i16)
}

pub(super) async fn second_channel(f: &Fixture) -> Fixture {
    let mut second = f.clone();
    second.channel = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO channels (community_id,id,name,created_by) VALUES ($1,$2,'second',$3)",
    )
    .bind(f.community.as_uuid())
    .bind(second.channel)
    .bind(f.owner.public_key().to_bytes().to_vec())
    .execute(&f.pool)
    .await
    .expect("second channel");
    second
}

pub(super) async fn counts(f: &Fixture) -> (i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM events WHERE community_id=$1 AND kind=9), \
         (SELECT count(*) FROM contact_routes WHERE community_id=$1), \
         (SELECT sum(used)::bigint FROM contact_quota WHERE community_id=$1)",
    )
    .bind(f.community.as_uuid())
    .fetch_one(&f.pool)
    .await
    .expect("counts")
}

pub(super) async fn wait_blocked(connection: &mut PgConnection, waiter: i32) -> bool {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let blocked: bool =
                sqlx::query_scalar("SELECT pg_backend_pid()=ANY(pg_blocking_pids($1))")
                    .bind(waiter)
                    .fetch_one(&mut *connection)
                    .await
                    .expect("actual lock wait graph");
            if blocked {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok()
}

pub(super) async fn writer_guard(f: &Fixture, tx: &mut Transaction<'_, Postgres>) {
    sqlx::query("SELECT assert_community_write_allowed($1)")
        .bind(f.community.as_uuid())
        .execute(&mut **tx)
        .await
        .expect("existing community write guard");
}

// GC never deletes an original. Database guards independently enforce the
// server-time retention anchor; stage boundaries allow rollback evidence.
pub(super) async fn gc_one(
    f: &Fixture,
    tx: &mut Transaction<'_, Postgres>,
    original: &Event,
    stop_after: u8,
) -> std::result::Result<(), sqlx::Error> {
    writer_guard(f, tx).await;
    contact_lock(f, tx).await;
    sqlx::query(
        "UPDATE communities SET contact_replay_floor=GREATEST(contact_replay_floor,$2) WHERE id=$1",
    )
    .bind(f.community.as_uuid())
    .bind(timestamp(original))
    .execute(&mut **tx)
    .await?;
    if stop_after == 1 {
        return Ok(());
    }
    sqlx::query(
        "DELETE FROM events WHERE community_id=$1 AND id IN \
        (SELECT decision_id FROM contact_routes WHERE community_id=$1 AND original_id=$2)",
    )
    .bind(f.community.as_uuid())
    .bind(original.id.as_bytes().as_slice())
    .execute(&mut **tx)
    .await?;
    let removed =
        sqlx::query("DELETE FROM contact_routes WHERE community_id=$1 AND original_id=$2")
            .bind(f.community.as_uuid())
            .bind(original.id.as_bytes().as_slice())
            .execute(&mut **tx)
            .await?
            .rows_affected();
    if stop_after == 2 {
        return Ok(());
    }
    if removed == 1 {
        sqlx::query("UPDATE contact_quota SET used=used-1 WHERE community_id=$1 AND stripe=0")
            .bind(f.community.as_uuid())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub(super) async fn rejected(
    tx: Transaction<'_, Postgres>,
    result: std::result::Result<(), sqlx::Error>,
    context: &str,
) {
    match result {
        Err(error) => {
            super::constraint_error(Err(error), context);
            tx.rollback().await.expect("rollback rejected mutation");
        }
        Ok(()) => super::constraint_error(tx.commit().await, context),
    }
}

pub(super) async fn aged_timestamp(f: &Fixture) -> u64 {
    let epoch: i64 =
        sqlx::query_scalar("SELECT extract(epoch FROM now()-interval '91 days')::bigint")
            .fetch_one(&f.pool)
            .await
            .expect("server-aged fixture timestamp");
    epoch as u64
}

pub(super) async fn maintenance_lock(tx: &mut Transaction<'_, Postgres>) {
    // Same namespace as the real migrator, transaction lifetime and existing
    // fixture deadlines retained. No timeout exemption or extra connection.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::deletion::SCHEMA_DESTRUCTION_LOCK_KEY)
        .execute(&mut **tx)
        .await
        .expect("maintenance fence");
}

pub(super) async fn install_leaf_guard(connection: &mut PgConnection, leaf: &str) {
    let attached:bool=sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_inherits WHERE inhparent='public.events'::regclass AND inhrelid=to_regclass($1))",
    ).bind(leaf).fetch_one(&mut *connection).await.expect("attached child");
    assert!(
        attached,
        "only an attached event leaf may receive this guard"
    );
    let quoted = leaf.replace('"', "\"\"");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE CONSTRAINT TRIGGER contact_check_original AFTER INSERT ON public.\"{quoted}\" \
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.contact_check_original()"
    ))).execute(&mut *connection).await.expect("install actual deferred guard");
    // Rebuild only this explicit leaf trigger inside the same DDL transaction.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER IF EXISTS contact_check_proof_delete ON public.\"{quoted}\"; \
         CREATE CONSTRAINT TRIGGER contact_check_proof_delete AFTER DELETE ON public.\"{quoted}\" \
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION public.contact_check_proof_delete()"
    ))).execute(connection).await.expect("install deferred retention guard");
}

pub(super) async fn direct_original(
    f: &Fixture,
    connection: &mut PgConnection,
    table: &str,
    event: &Event,
    class: Option<i16>,
) {
    let quoted = table.replace('"', "\"\"");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO public.\"{quoted}\" (community_id,id,pubkey,created_at,kind,tags,content,sig,channel_id,contact_class) \
         VALUES ($1,$2,$3,$4,9,$5,$6,$7,$8,$9)"
    ))).bind(f.community.as_uuid()).bind(event.id.as_bytes().as_slice())
        .bind(event.pubkey.to_bytes().to_vec()).bind(timestamp(event))
        .bind(serde_json::to_value(&event.tags).expect("tags"))
        .bind(&event.content).bind(event.sig.serialize().to_vec()).bind(f.channel).bind(class)
        .execute(connection).await.expect("direct original fixture insert");
}

pub(super) async fn verify_attach_rows(connection: &mut PgConnection, table: &str) -> Result<()> {
    let quoted = table.replace('"', "\"\"");
    let classified: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS(SELECT 1 FROM public.\"{quoted}\" WHERE contact_class IS NOT NULL)"
    )))
    .fetch_one(connection)
    .await?;
    if classified {
        return Err(DbError::InvalidData(
            "attach requires empty or legacy-NULL originals; classified import unavailable".into(),
        ));
    }
    Ok(())
}

/// Default is the real database clock. Only isolated tests replace this function
/// to model elapsed server time without altering an immutable recorded anchor.
pub(super) async fn retention_now(f: &Fixture) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT contact_retention_now()")
        .fetch_one(&f.pool)
        .await
        .expect("server retention clock")
}

pub(super) async fn set_retention_clock(f: &Fixture, instant: DateTime<Utc>) {
    // RFC3339 is generated from DateTime, not external SQL/text; it contains no
    // quote delimiter. This function exists exclusively in owned test schemas.
    let value = instant.to_rfc3339();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE OR REPLACE FUNCTION contact_retention_now() RETURNS timestamptz \
         LANGUAGE sql VOLATILE AS $$ SELECT TIMESTAMPTZ '{value}' $$"
    )))
    .execute(&f.pool)
    .await
    .expect("advance isolated server test clock");
}
