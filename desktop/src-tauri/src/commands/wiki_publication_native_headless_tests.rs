//! Headless acceptance of the native Wiki publication worker.
//!
//! This deliberately starts at the production worker seam: the test creates a
//! real SQLite operation, constructs a `NativeWikiPublication<MockRuntime>`
//! through its explicit journal context, and runs the same bounded recovery
//! tick used after an app restart. The loopback relay is only a transport
//! fixture; all identity, lease, graph validation, CAS, and publication order
//! decisions remain in the production code.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nostr::Event;
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::wiki_publication_commands::{
    dispatch_row_with_context, reserve_generated_publication_at_path, WikiPublicationBuildInput,
};
use super::wiki_publication_native_reads::{NativeClock, NativeJournal};
use super::wiki_publication_test_fixture as fixture;
use super::wiki_publication_worker::run_due_with_context;
use crate::app_state::owner_scope::capture;
use crate::app_state::{build_app_state, AppState};
use crate::owner_operations::{CreateResult, Limits, OperationStore};

const WIKI_KIND: u16 = 30623;
const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 128;

#[derive(Default)]
struct RelayState {
    events: Vec<Event>,
    head: Option<Event>,
    fail_next_publish: bool,
    publish_requests: usize,
}

struct ServerGuard(tokio::task::JoinHandle<()>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn request(socket: &mut tokio::net::TcpStream) -> Result<(String, Vec<u8>), String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let headers_end = loop {
        let count = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut chunk))
            .await
            .map_err(|_| "request header timeout".to_string())?
            .map_err(|_| "request header read failed".to_string())?;
        if count == 0 {
            return Err("client closed before request headers".into());
        }
        if bytes.len().saturating_add(count) > MAX_REQUEST_BYTES {
            return Err("request exceeds test bound".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break index;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..headers_end]).into_owned();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = headers_end + 4;
    let body_end = body_start
        .checked_add(content_length)
        .ok_or_else(|| "request length overflow".to_string())?;
    if body_end > MAX_REQUEST_BYTES {
        return Err("request exceeds test bound".into());
    }
    while bytes.len() < body_end {
        let count = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut chunk))
            .await
            .map_err(|_| "request body timeout".to_string())?
            .map_err(|_| "request body read failed".to_string())?;
        if count == 0 {
            return Err("client closed before request body".into());
        }
        if bytes.len().saturating_add(count) > MAX_REQUEST_BYTES {
            return Err("request exceeds test bound".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let path = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_owned();
    Ok((path, bytes[body_start..body_end].to_vec()))
}

fn response(status: u16, value: &impl serde::Serialize) -> Vec<u8> {
    let body = serde_json::to_vec(value).expect("test response JSON");
    let reason = if status == 200 { "OK" } else { "Bad Request" };
    let mut bytes = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

fn matches_filter(event: &Event, filter: &serde_json::Value) -> bool {
    let kind = event.kind.as_u16() as u64;
    let kinds_match = filter
        .get("kinds")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_u64() == Some(kind)));
    let owner = event.pubkey.to_hex();
    let author_match = filter
        .get("authors")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|values| {
            values
                .iter()
                .any(|value| value.as_str() == Some(owner.as_str()))
        });
    let d_match = filter
        .get("#d")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|values| {
            event.tags.iter().any(|tag| {
                tag.as_slice().first().is_some_and(|name| name == "d")
                    && tag
                        .as_slice()
                        .get(1)
                        .is_some_and(|d| values.iter().any(|value| value.as_str() == Some(d)))
            })
        });
    let id_match = filter
        .get("ids")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|values| {
            let id = event.id.to_hex();
            values
                .iter()
                .any(|value| value.as_str() == Some(id.as_str()))
        });
    kinds_match && author_match && d_match && id_match
}

async fn relay(listener: TcpListener, state: Arc<Mutex<RelayState>>) {
    for _ in 0..MAX_CONNECTIONS {
        let Ok(Ok((mut socket, _))) =
            tokio::time::timeout(Duration::from_secs(20), listener.accept()).await
        else {
            return;
        };
        let Ok((path, body)) = request(&mut socket).await else {
            return;
        };
        let payload = match path.as_str() {
            "/" => response(
                200,
                &serde_json::json!({
                    "supported_extensions": ["crew-conditional-publication-v1"]
                }),
            ),
            "/query" => {
                let filters: Vec<serde_json::Value> =
                    serde_json::from_slice(&body).expect("query JSON");
                let filter = filters.first().expect("one query filter");
                let state = state.lock().expect("relay state");
                let mut events = state
                    .events
                    .iter()
                    .filter(|event| matches_filter(event, filter))
                    .cloned()
                    .collect::<Vec<_>>();
                if filter
                    .get("#d")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|values| {
                        values
                            .iter()
                            .any(|value| value.as_str().is_some_and(|d| d.ends_with("/_toc")))
                    })
                {
                    events = state.head.clone().into_iter().collect();
                }
                response(200, &events)
            }
            "/events" => {
                let event: Event = serde_json::from_slice(&body).expect("event JSON");
                let mut state = state.lock().expect("relay state");
                state.publish_requests = state.publish_requests.saturating_add(1);
                if state.fail_next_publish {
                    state.fail_next_publish = false;
                    response(
                        503,
                        &serde_json::json!({ "error": "temporary test outage" }),
                    )
                } else {
                    if !state.events.iter().any(|existing| existing.id == event.id) {
                        state.events.push(event.clone());
                    }
                    if event.kind.as_u16() == WIKI_KIND
                        && event.tags.iter().any(|tag| {
                            tag.as_slice().first().is_some_and(|name| name == "d")
                                && tag.as_slice().get(1).is_some_and(|d| d.ends_with("/_toc"))
                        })
                    {
                        state.head = Some(event.clone());
                    }
                    response(
                        200,
                        &serde_json::json!({
                            "event_id": event.id.to_hex(),
                            "accepted": true,
                            "message": "stored"
                        }),
                    )
                }
            }
            _ => response(
                400,
                &serde_json::json!({ "error": "unsupported test path" }),
            ),
        };
        let _ = tokio::time::timeout(Duration::from_secs(10), socket.write_all(&payload)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_recovers_a_reserved_publication_headlessly_after_restart() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("relay address");
    let relay_state = Arc::new(Mutex::new(RelayState::default()));
    let _server = ServerGuard(tokio::spawn(relay(listener, relay_state.clone())));

    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    *app.state::<AppState>()
        .relay_url_override
        .lock()
        .expect("relay override lock") = Some(format!("ws://{address}"));
    let captured = capture(app.handle().clone()).await.expect("native scope");
    let repo_d = "crew.headless.acceptance";
    let coordinate = fixture::coordinate(&captured.keys, repo_d);
    let dir = tempfile::tempdir().expect("journal directory");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical journal path")
        .join("owner-operations/recovery.db");
    let operation_id = uuid::Uuid::new_v4().to_string();
    let created = reserve_generated_publication_at_path(
        app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        operation_id,
        coordinate.clone(),
        WikiPublicationBuildInput {
            owner: captured.keys.public_key().to_hex(),
            repo_d: repo_d.to_owned(),
            generation: fixture::generation(),
            cadence: "manual".into(),
            expected_revision: None,
            created_at: 10,
            keys: captured.keys.clone(),
        },
    )
    .await
    .expect("reserve publication")
    .value;
    let operation = match created {
        CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
    };
    let expected_head = operation
        .payload
        .get("head")
        .and_then(|head| head.get("id"))
        .and_then(serde_json::Value::as_str)
        .and_then(|id| nostr::EventId::from_hex(id).ok())
        .expect("reserved head id");
    let clock = Arc::new(std::sync::atomic::AtomicI64::new(100));

    // Exercise the foreground dispatch seam first. Its first relay write is a
    // bounded, ambiguous outage; the durable row must remain retryable.
    relay_state.lock().expect("relay state").fail_next_publish = true;
    let failed = dispatch_row_with_context(
        app.handle().clone(),
        captured.token.clone(),
        operation.id.clone(),
        operation.revision,
        false,
        false,
        (
            NativeJournal::Path(path.clone()),
            NativeClock::Fixed(clock.clone()),
        ),
    )
    .await
    .expect("dispatch records the outage")
    .value;
    assert_eq!(
        failed.status,
        crate::owner_operations::OperationStatus::Failed
    );
    clock.store(1_000, std::sync::atomic::Ordering::SeqCst);

    // This is the same worker tick used on app startup. It creates the native
    // publication from the captured scope and drives the real transport/CAS
    // loop against the disposable relay.
    run_due_with_context(
        app.handle().clone(),
        captured.token.clone(),
        NativeJournal::Path(path.clone()),
        NativeClock::Fixed(clock),
    )
    .await
    .expect("worker recovery");

    let stored = OperationStore::open(&path, Limits::default())
        .expect("reopen journal")
        .load(&captured.token.scope, &operation.id)
        .expect("recovered row");
    assert!(stored.reconciled, "worker must settle the durable row");
    assert_eq!(
        stored.status,
        crate::owner_operations::OperationStatus::Complete
    );
    {
        let relay = relay_state.lock().expect("relay state");
        assert_eq!(
            relay.head.as_ref().map(|event| event.id),
            Some(expected_head),
            "the exact persisted head must be retained by the relay"
        );
        assert!(
            relay.events.iter().any(|event| event.id == expected_head),
            "the worker must publish the persisted graph rather than regenerate it"
        );
    }
    // A fresh tick sees the completed projection and performs no second write.
    let publish_requests = relay_state.lock().expect("relay state").publish_requests;
    run_due_with_context(
        app.handle().clone(),
        captured.token,
        NativeJournal::Path(path),
        NativeClock::System,
    )
    .await
    .expect("completed row remains quiescent");
    assert_eq!(
        relay_state.lock().expect("relay state").publish_requests,
        publish_requests,
        "completed row must not issue a second publish request"
    );
}
