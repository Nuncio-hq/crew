use super::*;
use crate::app_state::owner_scope::capture;
use crate::app_state::{build_app_state, AppState, IdentityStorage};
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::PageDraft;
use crew_wiki::snapshot_v1_build::{build_snapshot, SnapshotBuild, SnapshotPublication};
use crew_wiki::types::{PlannedPage, PlannedSection, WikiPlan};
use nostr::{EventBuilder, Keys, Kind, SecretKey, Tag, Timestamp};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn keys(seed: u8) -> Keys {
    let mut bytes = [0_u8; 32];
    bytes[31] = seed;
    Keys::new(SecretKey::from_slice(&bytes).expect("test key"))
}

fn state(keys: &Keys, tags: Vec<Vec<String>>, created_at: u64) -> Event {
    let tags = tags
        .into_iter()
        .map(|tag| Tag::parse(tag).expect("test tag"))
        .collect::<Vec<_>>();
    EventBuilder::new(Kind::Custom(WIKI_STATE_KIND), "")
        .tags(tags)
        .custom_created_at(Timestamp::from_secs(created_at))
        .sign_with_keys(keys)
        .expect("test event")
}

fn coordinate(owner: &str) -> String {
    format!("30617:{owner}:repo")
}

fn tag(name: &str, value: impl Into<String>) -> Tag {
    Tag::parse(vec![name.to_owned(), value.into()]).expect("test tag")
}

fn v1_fixture(keys: &Keys) -> SnapshotPublication {
    v1_fixture_with_page_count(keys, 1)
}

fn v1_fixture_with_page_count(keys: &Keys, page_count: usize) -> SnapshotPublication {
    let owner = keys.public_key().to_hex();
    let commit = "a".repeat(40);
    let files: Vec<_> = (0..page_count)
        .map(|index| format!("src/page-{index}.rs"))
        .collect();
    let contents = files
        .iter()
        .map(|file| {
            (
                file.clone(),
                format!(
                    "fn {}() {{}}\n",
                    file.replace('/', "_").replace('-', "_").replace('.', "_")
                ),
            )
        })
        .collect();
    let snapshot = RepoSnapshot {
        commit: commit.clone(),
        branch: "main".into(),
        source_revision: format!("git:{commit}"),
        files: files.clone(),
        contents,
        omissions: Vec::new(),
    };
    let pages: Vec<_> = (0..page_count)
        .map(|index| PlannedPage {
            slug: format!("page-{index}"),
            title: format!("Page {index}"),
            section: "overview".into(),
            source_files: vec![files[index].clone()],
        })
        .collect();
    let plan = WikiPlan {
        language: "en".into(),
        sections: vec![PlannedSection {
            id: "overview".into(),
            title: "Overview".into(),
            pages: pages.clone(),
        }],
    };
    let drafts: Vec<_> = pages
        .iter()
        .map(|page| PageDraft {
            slug: page.slug.clone(),
            title: page.title.clone(),
            section: page.section.clone(),
            source_files: page.source_files.clone(),
            commit: commit.clone(),
            language: "en".into(),
            content: format!("# {}\n", page.title),
        })
        .collect();
    build_snapshot(SnapshotBuild {
        owner: &owner,
        repo_d: "repo",
        snapshot: &snapshot,
        plan: &plan,
        drafts: &drafts,
        cadence: "manual",
        snapshot_id: Some("12345678-1234-4234-9234-123456789abc"),
        expected_revision: None,
        created_at: 10,
        keys,
    })
    .expect("v1 fixture publication")
}

async fn request(socket: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let headers_end;
    loop {
        let count = socket.read(&mut chunk).await.expect("read request");
        assert!(count > 0, "client closed before request headers");
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            headers_end = index;
            break;
        }
        assert!(bytes.len() < 2 * 1024 * 1024, "request is bounded");
    }
    let headers = String::from_utf8_lossy(&bytes[..headers_end]);
    let path = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_owned();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("content length"))
        })
        .unwrap_or(0);
    let body_start = headers_end + 4;
    while bytes.len() < body_start + content_length {
        let count = socket.read(&mut chunk).await.expect("read request body");
        assert!(count > 0, "client closed before request body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    (
        path,
        bytes[body_start..body_start + content_length].to_vec(),
    )
}

fn json_response(value: &impl serde::Serialize) -> Vec<u8> {
    json_response_with_status(200, value)
}

fn json_response_with_status(status: u16, value: &impl serde::Serialize) -> Vec<u8> {
    let body = serde_json::to_vec(value).expect("response JSON");
    let mut response = format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

struct ManifestGate {
    ready_tx: Option<tokio::sync::oneshot::Sender<()>>,
    release_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    fail: bool,
}

fn event_d(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .and_then(|tag| tag.as_slice().get(1).cloned())
}

async fn snapshot_relay(
    listener: TcpListener,
    head: Event,
    manifest: Event,
    pages: Vec<Event>,
    changed_head: Option<Event>,
    omit_manifest: bool,
    head_queries: Arc<AtomicUsize>,
    page_batch_sizes: Option<Arc<Mutex<Vec<usize>>>>,
    mut manifest_gate: Option<ManifestGate>,
) {
    loop {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let (path, body) = request(&mut socket).await;
        let response = if path == "/" {
            json_response(&serde_json::json!({}))
        } else {
            let filters: Vec<Value> = serde_json::from_slice(&body).expect("query filters");
            let filter = &filters[0];
            let ids = filter
                .get("ids")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let ds = filter
                .get("#d")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut status = 200;
            let head_d = format!(
                "{}/_toc",
                event_d(&head)
                    .expect("head d")
                    .strip_suffix("/_toc")
                    .expect("head repo")
            );
            let events = if ds.iter().any(|value| value.as_str() == Some(&head_d)) {
                let number = head_queries.fetch_add(1, Ordering::SeqCst);
                changed_head
                    .as_ref()
                    .filter(|_| number > 0)
                    .cloned()
                    .into_iter()
                    .chain(std::iter::once(head.clone()))
                    .take(1)
                    .collect::<Vec<_>>()
            } else if ids
                .iter()
                .any(|value| value.as_str() == Some(&manifest.id.to_hex()))
            {
                if let Some(gate) = manifest_gate.as_mut() {
                    if let Some(ready_tx) = gate.ready_tx.take() {
                        let _ = ready_tx.send(());
                    }
                    if let Some(release_rx) = gate.release_rx.take() {
                        let _ = release_rx.await;
                    }
                    if gate.fail {
                        status = 500;
                    }
                }
                if omit_manifest {
                    Vec::new()
                } else {
                    vec![manifest.clone()]
                }
            } else if !ids.is_empty() {
                if let Some(batch_sizes) = &page_batch_sizes {
                    batch_sizes
                        .lock()
                        .expect("batch sizes lock")
                        .push(ids.len());
                }
                pages
                    .iter()
                    .filter(|event| {
                        ids.iter()
                            .any(|value| value.as_str() == Some(&event.id.to_hex()))
                    })
                    .cloned()
                    .collect()
            } else {
                pages
                    .iter()
                    .filter(|event| {
                        event_d(event).is_some_and(|d| {
                            ds.iter().any(|value| value.as_str() == Some(d.as_str()))
                        })
                    })
                    .cloned()
                    .collect()
            };
            json_response_with_status(status, &events)
        };
        let _ = socket.write_all(&response).await;
    }
}

async fn owner_state_relay(listener: TcpListener, owner_state: Event) {
    loop {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let (path, _) = request(&mut socket).await;
        let response = if path == "/" {
            json_response(&serde_json::json!({}))
        } else {
            json_response(&vec![owner_state.clone()])
        };
        let _ = socket.write_all(&response).await;
    }
}

async fn empty_snapshot_relay(listener: TcpListener) {
    loop {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let (path, _) = request(&mut socket).await;
        let response = if path == "/" {
            json_response(&serde_json::json!({}))
        } else {
            json_response(&Vec::<Event>::new())
        };
        let _ = socket.write_all(&response).await;
    }
}

fn mock_app_with_origin(address: std::net::SocketAddr) -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    *app.state::<AppState>()
        .relay_url_override
        .lock()
        .expect("relay override lock") = Some(format!("ws://{address}"));
    app
}

fn replace_identity_a_b_a(app: &tauri::App<tauri::test::MockRuntime>, original_keys: &Keys) {
    let directory = tempfile::tempdir().expect("identity directory");
    let state = app.state::<AppState>();
    let mutation = state.identity_mutation.lock().expect("identity lock");
    for keys in [Keys::generate(), original_keys.clone()] {
        crate::commands::commit_imported_identity(
            &state,
            &mutation,
            directory.path(),
            keys,
            |_| Ok(IdentityStorage::LocalFile),
        )
        .expect("identity replacement");
    }
}

#[tokio::test]
async fn reader_returns_a_complete_v1_graph_for_a_different_repository_owner() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let repository_keys = Keys::generate();
    let publication = v1_fixture(&repository_keys);
    let head_queries = Arc::new(AtomicUsize::new(0));
    let server = tokio::spawn(snapshot_relay(
        listener,
        publication.head.clone(),
        publication.manifest.clone(),
        publication.pages.clone(),
        None,
        false,
        head_queries.clone(),
        None,
        None,
    ));
    let repository_owner = repository_keys.public_key().to_hex();
    let result = read_wiki_snapshot(
        app.handle().clone(),
        captured.token.clone(),
        format!("30617:{repository_owner}:repo"),
    )
    .await
    .expect("scoped reader");
    assert!(matches!(
        result.value.state,
        WikiSnapshotReadState::Complete
    ));
    assert_eq!(
        result.value.head.as_ref().map(|event| event.id.to_hex()),
        Some(publication.head.id.to_hex())
    );
    assert_eq!(
        result
            .value
            .manifest
            .as_ref()
            .map(|event| event.id.to_hex()),
        Some(publication.manifest.id.to_hex())
    );
    assert_eq!(result.value.pages.len(), publication.pages.len());
    assert_eq!(result.token, captured.token);
    assert_eq!(
        head_queries.load(Ordering::SeqCst),
        2,
        "exact head must be reread"
    );
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn reader_fences_identity_aba_while_a_dependency_query_is_in_flight() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let repository_keys = Keys::generate();
    let publication = v1_fixture(&repository_keys);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(snapshot_relay(
        listener,
        publication.head,
        publication.manifest,
        publication.pages,
        None,
        false,
        Arc::new(AtomicUsize::new(0)),
        None,
        Some(ManifestGate {
            ready_tx: Some(ready_tx),
            release_rx: Some(release_rx),
            fail: false,
        }),
    ));
    let lookup_app = app.handle().clone();
    let lookup_token = captured.token.clone();
    let repository_owner = repository_keys.public_key().to_hex();
    let lookup = tokio::spawn(async move {
        read_wiki_snapshot(
            lookup_app,
            lookup_token,
            format!("30617:{repository_owner}:repo"),
        )
        .await
    });
    ready_rx.await.expect("manifest query reached relay");
    replace_identity_a_b_a(&app, &captured.keys);
    release_tx.send(()).expect("release manifest query");
    let result = lookup.await.expect("reader task");
    assert!(matches!(result, Err(ref error) if error == OWNER_SCOPE_STALE));
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn reader_fences_identity_aba_before_classifying_a_transport_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let repository_keys = Keys::generate();
    let publication = v1_fixture(&repository_keys);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(snapshot_relay(
        listener,
        publication.head,
        publication.manifest,
        publication.pages,
        None,
        false,
        Arc::new(AtomicUsize::new(0)),
        None,
        Some(ManifestGate {
            ready_tx: Some(ready_tx),
            release_rx: Some(release_rx),
            fail: true,
        }),
    ));
    let lookup_app = app.handle().clone();
    let lookup_token = captured.token.clone();
    let repository_owner = repository_keys.public_key().to_hex();
    let lookup = tokio::spawn(async move {
        read_wiki_snapshot(
            lookup_app,
            lookup_token,
            format!("30617:{repository_owner}:repo"),
        )
        .await
    });
    ready_rx.await.expect("manifest query reached relay");
    replace_identity_a_b_a(&app, &captured.keys);
    release_tx.send(()).expect("release failed manifest query");
    let result = lookup.await.expect("reader task");
    assert!(matches!(result, Err(ref error) if error == OWNER_SCOPE_STALE));
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn reader_returns_incomplete_when_the_head_changes_during_dependency_reads() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let repository_keys = Keys::generate();
    let publication = v1_fixture(&repository_keys);
    let changed_head = EventBuilder::new(
        Kind::Custom(WIKI_EVENT_KIND),
        publication.head.content.clone(),
    )
    .tags(publication.head.tags.clone())
    .custom_created_at(Timestamp::from_secs(20))
    .sign_with_keys(&repository_keys)
    .expect("changed head");
    let head_queries = Arc::new(AtomicUsize::new(0));
    let server = tokio::spawn(snapshot_relay(
        listener,
        publication.head.clone(),
        publication.manifest.clone(),
        publication.pages.clone(),
        Some(changed_head),
        false,
        head_queries.clone(),
        None,
        None,
    ));
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("captured transport");
    let result = read_snapshot(
        app.handle().clone(),
        captured.token.clone(),
        transport,
        repository_keys.public_key().to_hex(),
        "repo".into(),
    )
    .await
    .expect("head race should be represented as a read result")
    .value;
    assert!(matches!(result.state, WikiSnapshotReadState::Incomplete));
    assert_eq!(result.pages.len(), 0);
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("changed")));
    assert_eq!(head_queries.load(Ordering::SeqCst), 2);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn reader_returns_incomplete_when_a_manifest_dependency_is_missing() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let repository_keys = Keys::generate();
    let publication = v1_fixture(&repository_keys);
    let head_queries = Arc::new(AtomicUsize::new(0));
    let server = tokio::spawn(snapshot_relay(
        listener,
        publication.head.clone(),
        publication.manifest.clone(),
        publication.pages.clone(),
        None,
        true,
        head_queries,
        None,
        None,
    ));
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("captured transport");
    let result = read_snapshot(
        app.handle().clone(),
        captured.token,
        transport,
        repository_keys.public_key().to_hex(),
        "repo".into(),
    )
    .await
    .expect("missing dependency should be represented as a read result")
    .value;
    assert!(matches!(result.state, WikiSnapshotReadState::Incomplete));
    assert!(result.manifest.is_none());
    assert!(result.pages.is_empty());
    assert!(result.error.is_some());
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn reader_returns_incomplete_when_a_manifest_dependency_is_malformed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let repository_keys = Keys::generate();
    let publication = v1_fixture(&repository_keys);
    let manifest_hash = "b".repeat(64);
    let malformed_manifest = EventBuilder::new(Kind::Custom(WIKI_EVENT_KIND), "not-json")
        .tags(vec![tag("d", format!("repo/m1-{manifest_hash}"))])
        .custom_created_at(Timestamp::from_secs(11))
        .sign_with_keys(&repository_keys)
        .expect("malformed manifest event");
    let replacement = Tag::parse(vec![
        "wiki-manifest".into(),
        malformed_manifest.id.to_hex(),
        manifest_hash,
    ])
    .expect("manifest reference");
    let mut replaced = false;
    let head_tags: Vec<Tag> = publication
        .head
        .tags
        .iter()
        .cloned()
        .map(|tag| {
            if tag
                .as_slice()
                .first()
                .is_some_and(|name| name == "wiki-manifest")
            {
                replaced = true;
                replacement.clone()
            } else {
                tag
            }
        })
        .collect();
    assert!(replaced, "fixture head must carry a manifest reference");
    let malformed_head = EventBuilder::new(
        Kind::Custom(WIKI_EVENT_KIND),
        publication.head.content.clone(),
    )
    .tags(head_tags)
    .custom_created_at(Timestamp::from_secs(12))
    .sign_with_keys(&repository_keys)
    .expect("malformed manifest head");
    let server = tokio::spawn(snapshot_relay(
        listener,
        malformed_head,
        malformed_manifest,
        Vec::new(),
        None,
        false,
        Arc::new(AtomicUsize::new(0)),
        None,
        None,
    ));
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("transport");
    let result = read_snapshot(
        app.handle().clone(),
        captured.token,
        transport,
        repository_keys.public_key().to_hex(),
        "repo".into(),
    )
    .await
    .expect("malformed dependency should be represented as a read result")
    .value;
    assert!(matches!(result.state, WikiSnapshotReadState::Incomplete));
    assert!(result.pages.is_empty());
    assert!(result.error.is_some());
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn reader_batches_v1_page_queries_in_groups_of_four() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let repository_keys = Keys::generate();
    let publication = v1_fixture_with_page_count(&repository_keys, 5);
    let page_batch_sizes = Arc::new(Mutex::new(Vec::new()));
    let server = tokio::spawn(snapshot_relay(
        listener,
        publication.head.clone(),
        publication.manifest.clone(),
        publication.pages.clone(),
        None,
        false,
        Arc::new(AtomicUsize::new(0)),
        Some(page_batch_sizes.clone()),
        None,
    ));
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("transport");
    let result = read_snapshot(
        app.handle().clone(),
        captured.token,
        transport,
        repository_keys.public_key().to_hex(),
        "repo".into(),
    )
    .await
    .expect("batched graph");
    assert!(matches!(
        result.value.state,
        WikiSnapshotReadState::Complete
    ));
    assert_eq!(result.value.pages.len(), 5);
    assert_eq!(
        *page_batch_sizes.lock().expect("batch sizes lock"),
        vec![4, 1],
        "page queries must obey the native batch bound"
    );
    server.abort();
    let _ = server.await;
}

#[test]
fn malformed_default_head_is_not_a_trusted_freshness_source() {
    let owner_keys = keys(3);
    let owner = owner_keys.public_key().to_hex();
    let coordinate = coordinate(&owner);
    let event = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo".into()],
            vec!["a".into(), coordinate.clone()],
            vec!["HEAD".into(), "refs/heads/main".into()],
            vec!["refs/heads/main".into(), "c".repeat(40)],
        ],
        1,
    );
    assert!(select_owner_state(&[event], &owner, "repo", &coordinate).is_err());
}

#[test]
fn legacy_cadence_body_may_omit_commit_but_signed_head_tag_is_separate() {
    let owner_keys = keys(4);
    let head = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo/_toc".into()],
            vec!["commit".into(), "a".repeat(40)],
        ],
        1,
    );
    assert_eq!(
        legacy_commit_for_head(&head, None).expect("signed commit"),
        "a".repeat(40)
    );
    assert!(legacy_commit_for_head(&head, Some("b".repeat(40).as_str())).is_err());
    let no_signed_commit = state(&owner_keys, vec![vec!["d".into(), "repo/_toc".into()]], 1);
    assert!(legacy_commit_for_head(&no_signed_commit, None).is_err());
}

#[tokio::test]
async fn legacy_reader_binds_signed_commit_and_optional_body_commit_in_production_path() {
    let keys = Keys::generate();
    let owner = keys.public_key().to_hex();
    let commit = "a".repeat(40);
    let page = EventBuilder::new(Kind::Custom(WIKI_EVENT_KIND), "# Overview\n")
        .tags(vec![
            tag("d", "repo/overview"),
            tag("commit", commit.clone()),
        ])
        .custom_created_at(Timestamp::from_secs(2))
        .sign_with_keys(&keys)
        .expect("page");
    let head_content = serde_json::json!({
        "sections": [{"pages": [{"slug": "overview"}]}]
    })
    .to_string();
    let head = EventBuilder::new(Kind::Custom(WIKI_EVENT_KIND), head_content)
        .tags(vec![tag("d", "repo/_toc"), tag("commit", commit.clone())])
        .custom_created_at(Timestamp::from_secs(1))
        .sign_with_keys(&keys)
        .expect("head");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("transport");
    let server = tokio::spawn(snapshot_relay(
        listener,
        head.clone(),
        head.clone(),
        vec![page.clone()],
        None,
        false,
        Arc::new(AtomicUsize::new(0)),
        None,
        None,
    ));
    let result = resolve_legacy(
        app.handle().clone(),
        &captured.token,
        &transport,
        &owner,
        "repo",
        &head,
    )
    .await
    .expect("legacy graph");
    assert!(matches!(result, ResolvedDependencies::Legacy { pages } if pages.len() == 1));
    server.abort();
    let _ = server.await;

    let missing_signed_commit = EventBuilder::new(
        Kind::Custom(WIKI_EVENT_KIND),
        serde_json::json!({"sections": []}).to_string(),
    )
    .tags(vec![tag("d", "repo/_toc")])
    .sign_with_keys(&keys)
    .expect("head without commit");
    let missing_error = resolve_legacy(
        app.handle().clone(),
        &captured.token,
        &transport,
        &owner,
        "repo",
        &missing_signed_commit,
    )
    .await
    .expect_err("missing signed commit must fail in production reader");
    assert!(matches!(missing_error, ReadError::Failed(error) if error.contains("commit")));

    let mismatched_body = EventBuilder::new(
        Kind::Custom(WIKI_EVENT_KIND),
        serde_json::json!({
            "sections": [{"pages": [{"slug": "overview"}]}],
            "commit": "b".repeat(40)
        })
        .to_string(),
    )
    .tags(vec![tag("d", "repo/_toc"), tag("commit", commit)])
    .sign_with_keys(&keys)
    .expect("mismatched body head");
    let mismatch_error = resolve_legacy(
        app.handle().clone(),
        &captured.token,
        &transport,
        &owner,
        "repo",
        &mismatched_body,
    )
    .await
    .expect_err("body commit mismatch must fail in production reader");
    assert!(matches!(mismatch_error, ReadError::Failed(error) if error.contains("differs")));
}

#[tokio::test]
async fn legacy_reader_rejects_a_page_with_a_different_signed_commit() {
    let keys = Keys::generate();
    let owner = keys.public_key().to_hex();
    let head_commit = "a".repeat(40);
    let page = EventBuilder::new(Kind::Custom(WIKI_EVENT_KIND), "# Overview\n")
        .tags(vec![
            tag("d", "repo/overview"),
            tag("commit", "b".repeat(40)),
        ])
        .sign_with_keys(&keys)
        .expect("page");
    let head = EventBuilder::new(
        Kind::Custom(WIKI_EVENT_KIND),
        serde_json::json!({
            "sections": [{"pages": [{"slug": "overview"}]}]
        })
        .to_string(),
    )
    .tags(vec![tag("d", "repo/_toc"), tag("commit", head_commit)])
    .sign_with_keys(&keys)
    .expect("head");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("transport");
    let server = tokio::spawn(snapshot_relay(
        listener,
        head.clone(),
        head.clone(),
        vec![page],
        None,
        false,
        Arc::new(AtomicUsize::new(0)),
        None,
        None,
    ));
    let error = resolve_legacy(
        app.handle().clone(),
        &captured.token,
        &transport,
        &owner,
        "repo",
        &head,
    )
    .await
    .expect_err("page revision must be bound to the signed head");
    assert!(matches!(error, ReadError::Failed(error) if error.contains("differs")));
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn legacy_reader_rejects_a_missing_referenced_page() {
    let keys = Keys::generate();
    let owner = keys.public_key().to_hex();
    let commit = "a".repeat(40);
    let head = EventBuilder::new(
        Kind::Custom(WIKI_EVENT_KIND),
        serde_json::json!({
            "sections": [{"pages": [{"slug": "missing"}]}]
        })
        .to_string(),
    )
    .tags(vec![tag("d", "repo/_toc"), tag("commit", commit)])
    .sign_with_keys(&keys)
    .expect("head");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let app = mock_app_with_origin(listener.local_addr().expect("address"));
    let captured = capture(app.handle().clone()).await.expect("scope");
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys.clone(),
        None,
    )
    .expect("transport");
    let server = tokio::spawn(snapshot_relay(
        listener,
        head.clone(),
        head.clone(),
        Vec::new(),
        None,
        false,
        Arc::new(AtomicUsize::new(0)),
        None,
        None,
    ));
    let error = resolve_legacy(
        app.handle().clone(),
        &captured.token,
        &transport,
        &owner,
        "repo",
        &head,
    )
    .await
    .expect_err("missing legacy page must fail closed");
    assert!(matches!(error, ReadError::Failed(error) if error.contains("missing page")));
    server.abort();
    let _ = server.await;
}

#[test]
fn legacy_page_commit_must_match_the_signed_head_revision() {
    let owner_keys = keys(5);
    let page = state(
        &owner_keys,
        vec![
            vec!["d".into(), "repo/overview".into()],
            vec!["commit".into(), "a".repeat(40)],
        ],
        1,
    );
    assert!(legacy_page_commit(&page, &"a".repeat(40)).is_ok());
    assert!(legacy_page_commit(&page, &"b".repeat(40)).is_err());
}

#[path = "wiki_snapshot_state_tests.rs"]
mod state_tests;
