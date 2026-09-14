//! Opt-in native acceptance of one Wiki publication against an owned relay.
//!
//! This test intentionally has no embedded relay or provider.  It is ignored
//! by default and only runs when the caller supplies an owned relay, signer,
//! repository coordinate, SQLite journal path, and receipt path.  The test
//! drives the production generation, reservation, and restart worker seams; a
//! second native app handle then resumes the unresolved row and verifies the
//! same durable row and exact signed graph.

use std::path::{Path, PathBuf};

use nostr::{Event, Keys};
use serde_json::{json, Value};
use std::time::Duration;
use tauri::Manager;

use super::owner_operations::owner_operation_load_at_path;
use super::wiki_publication_commands::{
    reserve_generated_publication_at_path, WikiPublicationBuildInput,
};
use super::wiki_publication_native_reads::{NativeClock, NativeJournal};
use super::wiki_publication_record::WikiPublicationRecord;
use super::wiki_publication_runtime::NativeWikiPublication;
use super::wiki_publication_test_fixture;
use super::wiki_publication_worker::start_with_context;
use crate::app_state::owner_scope::{capture, OwnerScopeToken};
use crate::app_state::{build_app_state, AppState};
use crate::owner_operations::{CreateResult, Operation, OperationStatus};
use tokio::sync::oneshot;

const RECEIPT_LIMIT: usize = 1024 * 1024;

struct Config {
    relay_http: String,
    relay_ws: String,
    keys: Keys,
    repo_d: String,
    journal: PathBuf,
    receipt: PathBuf,
    expected_head: Option<String>,
}

fn required(name: &str) -> Result<String, String> {
    let value = std::env::var(name).map_err(|_| format!("missing {name}"))?;
    if value.trim().is_empty() {
        return Err(format!("empty {name}"));
    }
    Ok(value)
}

fn config() -> Result<Config, String> {
    let relay = required("CREW_NATIVE_WIKI_RELAY_ORIGIN")?;
    let mut url = url::Url::parse(&relay).map_err(|_| "invalid relay origin".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !matches!(url.path(), "" | "/")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("relay origin must be a canonical http(s) origin".into());
    }
    let origin = url.origin().ascii_serialization();
    let ws_scheme = if url.scheme() == "https" { "wss" } else { "ws" };
    url.set_scheme(ws_scheme)
        .map_err(|_| "invalid relay websocket origin".to_string())?;
    let relay_ws = url
        .origin()
        .ascii_serialization()
        .replace("http://", "ws://")
        .replace("https://", "wss://");

    let secret = required("CREW_NATIVE_WIKI_OWNER_SECRET_KEY")?;
    let keys = Keys::parse(&secret).map_err(|_| "invalid owner secret key".to_string())?;
    let repo_d = required("CREW_NATIVE_WIKI_REPO_D")?;
    let journal = PathBuf::from(required("CREW_NATIVE_WIKI_JOURNAL_PATH")?);
    let receipt = PathBuf::from(required("CREW_NATIVE_WIKI_RECEIPT_PATH")?);
    if !journal.is_absolute() || !receipt.is_absolute() {
        return Err("journal and receipt paths must be absolute".into());
    }
    if let Some(parent) = journal.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "cannot create journal parent".to_string())?;
    }
    if let Some(parent) = receipt.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "cannot create receipt parent".to_string())?;
    }
    Ok(Config {
        relay_http: origin,
        relay_ws,
        keys,
        repo_d,
        journal,
        receipt,
        expected_head: std::env::var("CREW_NATIVE_WIKI_EXPECTED_HEAD_ID").ok(),
    })
}

fn app_with_identity(keys: &Keys, relay_ws: &str) -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("native acceptance app");
    let state = app.state::<AppState>();
    let guard = state.identity_mutation.lock().expect("identity lock");
    state
        .replace_workspace_identity(&guard, Some(keys.clone()), relay_ws.to_owned())
        .expect("install acceptance identity");
    drop(guard);
    app
}

fn event_value(event: &Event) -> Value {
    serde_json::to_value(event).expect("signed event JSON")
}

fn graph_value(publication: &crew_wiki::snapshot_v1_build::SnapshotPublication) -> Value {
    json!({
        "head": event_value(&publication.head),
        "manifest": event_value(&publication.manifest),
        "pages": publication.pages.iter().map(event_value).collect::<Vec<_>>(),
        "snapshotId": publication.snapshot_id,
        "sourceRevision": publication.source_revision,
        "expectedRevision": publication.expected_revision,
    })
}

fn operation_value(operation: &Operation) -> Value {
    json!({
        "id": operation.id,
        "revision": operation.revision,
        "status": operation.status,
        "reconciled": operation.reconciled,
        "resourceKey": operation.resource_key,
    })
}

fn write_receipt(path: &Path, value: &Value) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "receipt encoding failed".to_string())?;
    if bytes.len() > RECEIPT_LIMIT {
        return Err("native acceptance receipt exceeds 1 MiB".into());
    }
    std::fs::write(path, bytes).map_err(|_| "native acceptance receipt write failed".into())
}

async fn scope_for(app: &tauri::App<tauri::test::MockRuntime>) -> Result<OwnerScopeToken, String> {
    Ok(capture(app.handle().clone()).await?.token)
}

async fn load_operation(
    app: tauri::AppHandle<tauri::test::MockRuntime>,
    path: PathBuf,
    expected: &OwnerScopeToken,
    id: &str,
) -> Result<Operation, String> {
    Ok(
        owner_operation_load_at_path(app, path, expected.clone(), id.to_owned(), None)
            .await?
            .value,
    )
}

async fn load_record(
    app: tauri::AppHandle<tauri::test::MockRuntime>,
    path: PathBuf,
    expected: &OwnerScopeToken,
    id: &str,
) -> Result<WikiPublicationRecord, String> {
    let operation = load_operation(app, path, expected, id).await?;
    serde_json::from_value(operation.payload)
        .map_err(|_| "invalid acceptance Wiki record".to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires an owned relay, signer, journal path, and receipt path"]
async fn native_wiki_publication_roundtrip_against_owned_relay() -> Result<(), String> {
    let cfg = config()?;
    let app_a = app_with_identity(&cfg.keys, &cfg.relay_ws);
    let scope_a = scope_for(&app_a).await?;
    let coordinate = wiki_publication_test_fixture::coordinate(&cfg.keys, &cfg.repo_d);
    let journal = NativeJournal::Path(cfg.journal.clone());
    let clock = NativeClock::System;
    let runtime_a = NativeWikiPublication::new_with_context(
        app_a.handle().clone(),
        scope_a.clone(),
        &coordinate,
        journal.clone(),
        clock.clone(),
    )
    .await?;
    let repository = runtime_a.repository_head().await?;
    let initial = runtime_a
        .current_publication()
        .await?
        .ok_or_else(|| "owned relay has no complete v1 Wiki graph".to_string())?;
    if let Some(expected) = &cfg.expected_head {
        if initial.head.id.to_hex() != *expected {
            return Err("initial head does not match CREW_NATIVE_WIKI_EXPECTED_HEAD_ID".into());
        }
    }

    let current_time = u64::try_from(super::wiki_publication_runtime::now()?)
        .map_err(|_| "clock conversion failed".to_string())?;
    let created_at = initial
        .head
        .created_at
        .as_secs()
        .saturating_add(1)
        .max(current_time);
    let operation_id = uuid::Uuid::new_v4().to_string();
    let reserved = reserve_generated_publication_at_path(
        app_a.handle().clone(),
        cfg.journal.clone(),
        scope_a.clone(),
        operation_id,
        coordinate.clone(),
        WikiPublicationBuildInput {
            owner: cfg.keys.public_key().to_hex(),
            repo_d: cfg.repo_d.clone(),
            generation: wiki_publication_test_fixture::generation(),
            cadence: "manual".into(),
            expected_revision: Some(initial.head.id.to_hex()),
            created_at,
            keys: cfg.keys.clone(),
        },
    )
    .await?;
    let operation = match reserved.value {
        CreateResult::Created(operation) => operation,
        CreateResult::Existing(_) => {
            return Err(
                "acceptance journal already contains a Wiki operation for this coordinate".into(),
            )
        }
    };
    let reserved_record = load_record(
        app_a.handle().clone(),
        cfg.journal.clone(),
        &scope_a,
        &operation.id,
    )
    .await?;
    let row_before_restart = load_operation(
        app_a.handle().clone(),
        cfg.journal.clone(),
        &scope_a,
        &operation.id,
    )
    .await?;
    if row_before_restart.status != OperationStatus::Preparing || row_before_restart.reconciled {
        return Err("app A did not leave a genuinely unresolved Wiki row".into());
    }
    drop(runtime_a);
    drop(app_a);

    let app_b = app_with_identity(&cfg.keys, &cfg.relay_ws);
    let scope_b = scope_for(&app_b).await?;
    let (stop_tx, stop_rx) = oneshot::channel();
    let worker_task = start_with_context(
        app_b.handle().clone(),
        journal.clone(),
        clock.clone(),
        Some(stop_rx),
    )
    .ok_or_else(|| "fresh app did not start its Wiki recovery worker".to_string())?;
    let recovered = async {
        for _ in 0..240 {
            let row = load_operation(
                app_b.handle().clone(),
                cfg.journal.clone(),
                &scope_b,
                &operation.id,
            )
            .await?;
            if row.status == OperationStatus::Complete && row.reconciled {
                return Ok::<_, String>(row);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err("fresh app did not reconcile the unresolved Wiki row in time".into())
    }
    .await;
    let _ = stop_tx.send(());
    worker_task
        .await
        .map_err(|_| "fresh Wiki recovery worker did not stop".to_string())?;
    let row_b = recovered?;
    let runtime_b = NativeWikiPublication::new_with_context(
        app_b.handle().clone(),
        scope_b.clone(),
        &coordinate,
        journal,
        clock,
    )
    .await?;
    let final_b = runtime_b
        .current_publication()
        .await?
        .ok_or_else(|| "recreated native app cannot verify v1 graph".to_string())?;
    if final_b.head.id != reserved_record.head.id
        || final_b.manifest.id != reserved_record.manifest.id
        || final_b
            .pages
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>()
            != reserved_record
                .pages
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>()
    {
        return Err("recreated app observed a different exact graph".into());
    }
    if row_b.id != operation.id
        || row_b.revision <= row_before_restart.revision
        || row_b.status != OperationStatus::Complete
        || !row_b.reconciled
    {
        return Err("recreated app did not resume and reconcile the durable row".into());
    }

    write_receipt(
        &cfg.receipt,
        &json!({
            "schema": "crew-362-native-wiki-acceptance-v2",
            "relayOrigin": cfg.relay_http,
            "owner": cfg.keys.public_key().to_hex(),
            "repository": {"coordinate": coordinate, "d": cfg.repo_d},
            "initial": {"repository": event_value(&repository), "graph": graph_value(&initial)},
            "final": graph_value(&final_b),
            "operation": operation_value(&row_b),
            "restart": {
                "before": operation_value(&row_before_restart),
                "resumed": true,
            },
            "scopeGenerations": {
                "a": {"workspace": scope_a.workspace_generation, "identity": scope_a.identity_generation},
                "b": {"workspace": scope_b.workspace_generation, "identity": scope_b.identity_generation},
            },
        }),
    )
}
