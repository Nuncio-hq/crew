use super::*;

#[tokio::test]
#[ignore = "owned fixture bootstrap only; wrapper runs this before proof filters"]
async fn project_link_fixture_bootstrap() {
    let url = std::env::var("CREW_PROJECT_LINK_TEST_DATABASE_URL").unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
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
    assert!(name.starts_with("crew_project_link_"));
    crate::migration::run_migrations(&pool).await.unwrap();
    pool.close().await;
}

async fn bounded_writer_pool() -> PgPool {
    let url = std::env::var("CREW_PROJECT_LINK_TEST_DATABASE_URL").unwrap();
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET statement_timeout='10s'")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("SET lock_timeout='100ms'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap()
}

fn assert_lock_timeout(error: DbError) {
    let DbError::Sqlx(error) = error else {
        panic!("expected SQL lock timeout: {error}")
    };
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_noninsert_statuses_skip_writes_and_eligibility() {
    let f = Fixture::new().await;
    let current = f.event(100, false);
    f.write(&current, None).await.unwrap();
    let missing = Uuid::new_v4();
    let build = |offset| {
        EventBuilder::new(Kind::Custom(30621), "")
            .tags([
                Tag::parse(["d", "project"]).unwrap(),
                Tag::parse(["buzz-related-channel", &missing.to_string()]).unwrap(),
            ])
            .custom_created_at(Timestamp::from(f.timestamp + offset))
            .sign_with_keys(&f.keys)
            .unwrap()
    };
    let stale = build(99);
    let next = build(101);
    // A real DB trigger makes any attempted event mutation fail, including a
    // tentative write that would otherwise be hidden by savepoint rollback.
    let name = format!("project_no_write_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF (CASE WHEN TG_OP='DELETE' THEN OLD.community_id ELSE NEW.community_id END) = '{}'::uuid THEN RAISE EXCEPTION 'unexpected Project mutation'; END IF; IF TG_OP='DELETE' THEN RETURN OLD; END IF; RETURN NEW; END $$", f.community.as_uuid()
    ))).execute(&f.pool).await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER {name} BEFORE INSERT OR UPDATE OR DELETE ON events FOR EACH ROW EXECUTE FUNCTION {name}()"
    ))).execute(&f.pool).await.unwrap();
    let cases = [
        (
            &stale,
            ParameterizedReplacePrecondition::Unconditional,
            ParameterizedReplaceStatus::Superseded,
        ),
        (
            &next,
            ParameterizedReplacePrecondition::ExpectedRevision(stale.id.as_bytes()),
            ParameterizedReplaceStatus::RevisionMismatch,
        ),
        (
            &next,
            ParameterizedReplacePrecondition::ExactReplayOnly,
            ParameterizedReplaceStatus::ReplayOnlyMiss,
        ),
    ];
    for (event, precondition, status) in cases {
        let mut tx = f.pool.begin().await.unwrap();
        let result =
            f.db.replace_project_event_in_transaction(
                &mut tx,
                f.community,
                event,
                "project",
                precondition,
            )
            .await
            .unwrap();
        assert_eq!(result.status, status);
        tx.commit().await.unwrap();
    }
    assert_eq!(f.live_ids().await, vec![current.id.as_bytes().to_vec()]);
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {name} ON events"
    )))
    .execute(&f.pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP FUNCTION {name}()")))
        .execute(&f.pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_actual_admin_kick_waits_for_commit() {
    let f = Fixture::new().await;
    let a = f.event(100, true);
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
    let pool = bounded_writer_pool().await;
    let result = crate::relay_admin_actions::execute_kick_with_marker(
        &pool,
        action,
        lease,
        f.community,
        f.channel,
        f.keys.public_key().as_bytes(),
        f.keys.public_key().as_bytes(),
    )
    .await;
    let Err(error) = result else {
        panic!("admin kick must wait for the Project transaction");
    };
    assert_lock_timeout(error);
    let marker: Option<String> =
        sqlx::query_scalar("SELECT step_marker FROM relay_admin_actions WHERE id=$1")
            .bind(action)
            .fetch_one(&f.pool)
            .await
            .unwrap();
    assert!(marker.is_none());
    link.commit().await.unwrap();
    let result = crate::relay_admin_actions::execute_kick_with_marker(
        &pool,
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
    assert_eq!(f.live_ids().await, vec![a.id.as_bytes().to_vec()]);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_actual_archive_delete_and_ttl_wait_for_commit() {
    for writer in 0..3 {
        let f = Fixture::new().await;
        if writer == 2 {
            sqlx::query("UPDATE channels SET ttl_seconds=60,ttl_deadline=now()-interval '1 minute' WHERE community_id=$1 AND id=$2")
                .bind(f.community.as_uuid()).bind(f.channel).execute(&f.pool).await.unwrap();
        }
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
        let pool = bounded_writer_pool().await;
        let attempt = match writer {
            0 => crate::channel::archive_channel(&pool, f.community, f.channel).await,
            1 => crate::channel::soft_delete_channel(&pool, f.community, f.channel)
                .await
                .map(|_| ()),
            _ => crate::channel::reap_expired_ephemeral_channels(&pool)
                .await
                .map(|_| ()),
        };
        assert_lock_timeout(attempt.unwrap_err());
        link.commit().await.unwrap();
        match writer {
            0 => crate::channel::archive_channel(&pool, f.community, f.channel)
                .await
                .unwrap(),
            1 => assert!(
                crate::channel::soft_delete_channel(&pool, f.community, f.channel)
                    .await
                    .unwrap()
            ),
            _ => assert!(crate::channel::reap_expired_ephemeral_channels(&pool)
                .await
                .unwrap()
                .iter()
                .any(|row| row.channel_id == f.channel)),
        }
        assert_eq!(f.live_ids().await, vec![a.id.as_bytes().to_vec()]);
        pool.close().await;
    }
}

#[tokio::test]
#[ignore = "requires explicitly allocated Project-link PostgreSQL fixture"]
async fn project_link_db_reversed_target_order_completes_without_deadlock() {
    let f = Fixture::new().await;
    let second =
        f.db.create_channel(
            f.community,
            "second-target",
            ChannelType::Stream,
            ChannelVisibility::Open,
            None,
            f.keys.public_key().as_bytes(),
            None,
        )
        .await
        .unwrap()
        .id;
    let build = |d: &str, channels: [Uuid; 2]| {
        EventBuilder::new(Kind::Custom(30621), "")
            .tags([
                Tag::parse(["d", d]).unwrap(),
                Tag::parse(["buzz-related-channel", &channels[0].to_string()]).unwrap(),
                Tag::parse(["buzz-related-channel", &channels[1].to_string()]).unwrap(),
            ])
            .sign_with_keys(&f.keys)
            .unwrap()
    };
    let a = build("first", [f.channel, second]);
    let b = build("second", [second, f.channel]);
    let barrier = tokio::sync::Barrier::new(2);
    let persist = |event: Event, d: &'static str| {
        let f = &f;
        let barrier = &barrier;
        async move {
            let mut tx = f.pool.begin().await.unwrap();
            barrier.wait().await;
            let result =
                f.db.replace_project_event_in_transaction(
                    &mut tx,
                    f.community,
                    &event,
                    d,
                    ParameterizedReplacePrecondition::Unconditional,
                )
                .await
                .unwrap();
            assert_eq!(result.status, ParameterizedReplaceStatus::Inserted);
            tx.commit().await.unwrap();
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(persist(a, "first"), persist(b, "second"));
    })
    .await
    .expect("bounded multi-target completion");
    assert_eq!(f.live_ids().await.len(), 2);
}
