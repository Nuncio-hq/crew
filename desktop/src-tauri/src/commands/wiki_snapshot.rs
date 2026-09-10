//! Coherent, bounded read projection for repository Wiki snapshots.
//!
//! A repository Wiki is a replaceable head plus immutable dependencies. This
//! command reads that graph from one captured owner/community scope, verifies
//! the complete v1 graph with `crew-wiki`, and returns only one coherent
//! projection to the renderer. It deliberately does not use the renderer's
//! ambient WebSocket session or a global kind:30623 query.

use std::collections::{BTreeMap, BTreeSet};

use nostr::Event;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};

use crate::app_state::owner_scope::{assert_current, capture, OwnerScopeToken, OWNER_SCOPE_STALE};
use crate::app_state::AppState;
use crate::commands::identity_archive::fetch_relay_self_scoped;
use crate::commands::owner_operation_transport::OwnerOperationTransport;
use crate::commands::owner_operations::ScopedOperationResult;

const WIKI_EVENT_KIND: u16 = 30623;
const WIKI_STATE_KIND: u16 = 30618;
const MAX_PAGE_QUERY_BATCH: usize = 4;
const MAX_PAGES: usize = 256;

/// Native read state exposed to the renderer. `legacy` means the old TOC/page
/// shape was internally consistent; it does not imply v1 snapshot guarantees.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WikiSnapshotReadState {
    Complete,
    Legacy,
    Missing,
    Incomplete,
}

/// One bounded repository Wiki read. Raw signed events are returned only after
/// the corresponding graph has been checked; incomplete reads carry no page
/// bodies so callers cannot accidentally merge an unreferenced page.
#[derive(Debug, Serialize)]
pub(crate) struct WikiSnapshotRead {
    pub state: WikiSnapshotReadState,
    pub head: Option<Event>,
    pub manifest: Option<Event>,
    pub pages: Vec<Event>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Relay-authoritative ref state, when it can be trusted for this exact
    /// repository. Legacy relay-signed d-only states remain unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_state: Option<Event>,
}

#[derive(Debug)]
enum ReadError {
    Stale(String),
    Failed(String),
}

#[derive(Debug)]
enum HeadRead {
    Missing,
    Found(Event),
    Invalid(Option<Event>, String),
}

#[derive(Debug)]
enum ResolvedDependencies {
    Complete { manifest: Event, pages: Vec<Event> },
    Legacy { pages: Vec<Event> },
}

#[derive(Debug, Deserialize)]
struct LegacyToc {
    sections: Vec<LegacySection>,
    /// Older generators put the source revision in the body. Cadence-only
    /// updates may omit it because the signed `commit` tag remains required.
    #[serde(default)]
    commit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LegacySection {
    pages: Vec<LegacyPage>,
}

#[derive(Debug, Deserialize)]
struct LegacyPage {
    slug: String,
}

/// Read one exact repository Wiki graph under the caller's native scope.
#[tauri::command]
pub(crate) async fn wiki_snapshot_read(
    app: AppHandle,
    expected: OwnerScopeToken,
    coordinate: String,
) -> Result<ScopedOperationResult<WikiSnapshotRead>, String> {
    read_wiki_snapshot(app, expected, coordinate).await
}

/// The whole command body, minus the IPC attribute. Tests bind here so they
/// exercise scope capture, transport construction, and the graph read exactly
/// as the shipped command does.
pub(super) async fn read_wiki_snapshot<R: Runtime>(
    app: AppHandle<R>,
    expected: OwnerScopeToken,
    coordinate: String,
) -> Result<ScopedOperationResult<WikiSnapshotRead>, String> {
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(OWNER_SCOPE_STALE.into());
    }
    let (owner, repo_d) = coordinate_parts(&coordinate)?;

    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys,
        None,
    )
    .map_err(|error| error.to_string())?;
    read_snapshot(
        app,
        captured.token,
        transport,
        owner.to_owned(),
        repo_d.to_owned(),
    )
    .await
}

async fn read_snapshot<R: Runtime>(
    app: AppHandle<R>,
    token: OwnerScopeToken,
    transport: OwnerOperationTransport,
    owner: String,
    repo_d: String,
) -> Result<ScopedOperationResult<WikiSnapshotRead>, String> {
    let head_d = format!("{repo_d}/_toc");
    let head = match query_head(app.clone(), &token, &transport, &owner, &repo_d, &head_d).await {
        Ok(head) => head,
        Err(ReadError::Stale(error)) => return Err(error),
        Err(ReadError::Failed(error)) => {
            return Ok(scoped(token, incomplete(None, None, error)));
        }
    };

    let head_event = match head {
        HeadRead::Missing => {
            let repo_state =
                match trusted_repo_state(app.clone(), &token, &transport, &owner, &repo_d).await {
                    Ok(state) => state,
                    Err(ReadError::Stale(error)) => return Err(error),
                    Err(ReadError::Failed(_)) => None,
                };
            return Ok(scoped(
                token,
                WikiSnapshotRead {
                    state: WikiSnapshotReadState::Missing,
                    head: None,
                    manifest: None,
                    pages: Vec::new(),
                    error: None,
                    repo_state,
                },
            ));
        }
        HeadRead::Invalid(event, error) => {
            return Ok(scoped(token, incomplete(event, None, error)));
        }
        HeadRead::Found(event) => event,
    };

    let dependencies = match resolve_dependencies(
        app.clone(),
        &token,
        &transport,
        &owner,
        &repo_d,
        &head_event,
    )
    .await
    {
        Ok(dependencies) => dependencies,
        Err(ReadError::Stale(error)) => return Err(error),
        Err(ReadError::Failed(error)) => {
            return Ok(scoped(token, incomplete(Some(head_event), None, error)));
        }
    };

    // Ref state is useful for freshness only when its author can be bound to
    // this exact repository. It is deliberately advisory to the content read:
    // an unavailable or legacy relay-signed state leaves manual/time cadence
    // usable without invalidating an otherwise complete Wiki snapshot.
    let repo_state =
        match trusted_repo_state(app.clone(), &token, &transport, &owner, &repo_d).await {
            Ok(state) => state,
            Err(ReadError::Stale(error)) => return Err(error),
            Err(ReadError::Failed(_)) => None,
        };

    // Dependencies are read asynchronously. Re-read the exact head before
    // exposing them so a head replacement or deletion cannot be presented as
    // the current snapshot. This single bounded reread is intentional; a
    // moving relay yields an explicit incomplete result instead of an
    // unbounded retry loop.
    let current_head =
        match query_head(app.clone(), &token, &transport, &owner, &repo_d, &head_d).await {
            Ok(HeadRead::Found(event)) => event,
            Ok(HeadRead::Missing) => {
                return Ok(scoped(
                    token,
                    incomplete(
                        Some(head_event),
                        None,
                        "Wiki head was deleted while reading its dependencies".into(),
                    ),
                ));
            }
            Ok(HeadRead::Invalid(event, error)) => {
                return Ok(scoped(token, incomplete(event, None, error)));
            }
            Err(ReadError::Stale(error)) => return Err(error),
            Err(ReadError::Failed(error)) => {
                return Ok(scoped(
                    token,
                    incomplete(
                        Some(head_event),
                        None,
                        format!("Wiki head could not be revalidated: {error}"),
                    ),
                ));
            }
        };

    if current_head.id != head_event.id {
        return Ok(scoped(
            token,
            incomplete(
                Some(head_event),
                None,
                "Wiki head changed while reading its dependencies".into(),
            ),
        ));
    }
    assert_current(app, &token).await?;

    let value = match dependencies {
        ResolvedDependencies::Complete { manifest, pages } => WikiSnapshotRead {
            state: WikiSnapshotReadState::Complete,
            head: Some(head_event),
            manifest: Some(manifest),
            pages,
            error: None,
            repo_state,
        },
        ResolvedDependencies::Legacy { pages } => WikiSnapshotRead {
            state: WikiSnapshotReadState::Legacy,
            head: Some(head_event),
            manifest: None,
            pages,
            error: None,
            repo_state,
        },
    };
    Ok(scoped(token, value))
}

async fn resolve_dependencies<R: Runtime>(
    app: AppHandle<R>,
    token: &OwnerScopeToken,
    transport: &OwnerOperationTransport,
    owner: &str,
    repo_d: &str,
    head: &Event,
) -> Result<ResolvedDependencies, ReadError> {
    if has_any_tag(head, &["wiki-version", "wiki-snapshot", "wiki-manifest"]) {
        resolve_v1(app, token, transport, owner, repo_d, head).await
    } else {
        resolve_legacy(app, token, transport, owner, repo_d, head).await
    }
}

async fn resolve_v1<R: Runtime>(
    app: AppHandle<R>,
    token: &OwnerScopeToken,
    transport: &OwnerOperationTransport,
    owner: &str,
    repo_d: &str,
    head: &Event,
) -> Result<ResolvedDependencies, ReadError> {
    let manifest_tag = exact_tag(head, "wiki-manifest", 3).map_err(ReadError::Failed)?;
    let manifest_id = manifest_tag[1].clone();
    let manifest_hash = manifest_tag[2].clone();
    if !is_lower_hex(&manifest_id, 64) || !is_lower_hex(&manifest_hash, 64) {
        return Err(ReadError::Failed(
            "Wiki manifest reference is invalid".into(),
        ));
    }
    let manifest_d = format!("{repo_d}/m1-{manifest_hash}");
    let manifests = scoped_query(
        app.clone(),
        token,
        transport,
        json!({
            "kinds": [WIKI_EVENT_KIND],
            "authors": [owner],
            "ids": [manifest_id],
            "#d": [manifest_d],
            "limit": 2,
        }),
    )
    .await?;
    let manifest =
        exact_event(&manifests, WIKI_EVENT_KIND, owner, &manifest_d).map_err(ReadError::Failed)?;
    if manifest.id.to_hex() != manifest_id {
        return Err(ReadError::Failed(
            "Wiki manifest identity changed while reading".into(),
        ));
    }

    let head_value = serde_json::to_value(head)
        .map_err(|_| ReadError::Failed("Wiki head is not serializable".into()))?;
    let manifest_value = serde_json::to_value(&manifest)
        .map_err(|_| ReadError::Failed("Wiki manifest is not serializable".into()))?;
    let index =
        crew_wiki::snapshot_v1::verify_snapshot_index(owner, repo_d, &head_value, &manifest_value)
            .map_err(|error| ReadError::Failed(error.to_string()))?;
    let references = &index.manifest().7;
    if references.len() > MAX_PAGES {
        return Err(ReadError::Failed("Wiki snapshot has too many pages".into()));
    }

    let mut raw_pages = Vec::new();
    for chunk in references.chunks(MAX_PAGE_QUERY_BATCH) {
        let ids: Vec<_> = chunk.iter().map(|reference| reference.2.clone()).collect();
        let ds: Vec<_> = chunk
            .iter()
            .map(|reference| format!("{repo_d}/{}", reference.1))
            .collect();
        let events = scoped_query(
            app.clone(),
            token,
            transport,
            json!({
                "kinds": [WIKI_EVENT_KIND],
                "authors": [owner],
                "ids": ids,
                "#d": ds,
                "limit": 8,
            }),
        )
        .await?;
        raw_pages.extend(events);
    }
    let pages: Vec<Value> = raw_pages
        .into_iter()
        .map(|event| serde_json::to_value(event))
        .collect::<Result<_, _>>()
        .map_err(|_| ReadError::Failed("Wiki page is not serializable".into()))?;
    crew_wiki::snapshot_v1::verify_snapshot(owner, repo_d, &head_value, &manifest_value, &pages)
        .map_err(|error| ReadError::Failed(error.to_string()))?;

    let mut ordered = pages
        .into_iter()
        .map(|value| {
            serde_json::from_value::<Event>(value)
                .map_err(|_| ReadError::Failed("Wiki page is not serializable".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    // `verify_snapshot` authenticated membership; restore manifest traversal
    // order for the renderer without reimplementing its grammar.
    let mut by_id: BTreeMap<String, Event> = ordered
        .drain(..)
        .map(|event| (event.id.to_hex(), event))
        .collect();
    let pages = references
        .iter()
        .map(|reference| {
            by_id.remove(&reference.2).ok_or_else(|| {
                ReadError::Failed("Wiki snapshot page disappeared after verification".into())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ResolvedDependencies::Complete { manifest, pages })
}

async fn resolve_legacy<R: Runtime>(
    app: AppHandle<R>,
    token: &OwnerScopeToken,
    transport: &OwnerOperationTransport,
    owner: &str,
    repo_d: &str,
    head: &Event,
) -> Result<ResolvedDependencies, ReadError> {
    let toc: LegacyToc = serde_json::from_str(&head.content)
        .map_err(|_| ReadError::Failed("Legacy Wiki TOC is malformed".into()))?;
    let signed_commit =
        legacy_commit_for_head(&head, toc.commit.as_deref()).map_err(ReadError::Failed)?;
    let mut slugs = Vec::new();
    let mut seen = BTreeSet::new();
    for section in toc.sections {
        for page in section.pages {
            if !valid_legacy_slug(&page.slug) || !seen.insert(page.slug.clone()) {
                return Err(ReadError::Failed(
                    "Legacy Wiki TOC page membership is invalid".into(),
                ));
            }
            slugs.push(page.slug);
            if slugs.len() > MAX_PAGES {
                return Err(ReadError::Failed(
                    "Legacy Wiki TOC has too many pages".into(),
                ));
            }
        }
    }

    let mut pages_by_d = BTreeMap::new();
    for chunk in slugs.chunks(MAX_PAGE_QUERY_BATCH) {
        let ds: Vec<_> = chunk
            .iter()
            .map(|slug| format!("{repo_d}/{slug}"))
            .collect();
        let events = scoped_query(
            app.clone(),
            token,
            transport,
            json!({
                "kinds": [WIKI_EVENT_KIND],
                "authors": [owner],
                "#d": ds,
                "limit": 8,
            }),
        )
        .await?;
        let expected_ds: BTreeSet<String> = ds.iter().cloned().collect();
        for event in events {
            if event.kind.as_u16() != WIKI_EVENT_KIND
                || event.pubkey.to_hex() != owner
                || event.verify().is_err()
            {
                return Err(ReadError::Failed(
                    "Legacy Wiki query returned an invalid signed page".into(),
                ));
            }
            let d = exact_d_tag(&event).map_err(ReadError::Failed)?;
            if !expected_ds.contains(&d) {
                return Err(ReadError::Failed(
                    "Legacy Wiki query returned an unrelated page".into(),
                ));
            }
            let slug = d.strip_prefix(&format!("{repo_d}/")).unwrap_or_default();
            validate_optional_coordinate(&event, owner, repo_d).map_err(ReadError::Failed)?;
            if slug.is_empty() {
                return Err(ReadError::Failed(
                    "Legacy Wiki page d tag is invalid".into(),
                ));
            }
            if pages_by_d.insert(d, event).is_some() {
                return Err(ReadError::Failed(
                    "Legacy Wiki query returned duplicate pages".into(),
                ));
            }
        }
    }

    let mut pages = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let d = format!("{repo_d}/{slug}");
        let event = pages_by_d
            .remove(&d)
            .ok_or_else(|| ReadError::Failed("Legacy Wiki TOC references a missing page".into()))?;
        legacy_page_commit(&event, &signed_commit).map_err(ReadError::Failed)?;
        pages.push(event);
    }
    if !pages_by_d.is_empty() {
        return Err(ReadError::Failed(
            "Legacy Wiki query returned an unreferenced page".into(),
        ));
    }
    Ok(ResolvedDependencies::Legacy { pages })
}

/// Resolve the legacy source revision from the replaceable signed head. The
/// body field is optional for cadence-only rewrites, but if present it is only
/// a consistency check against the signed tag.
fn legacy_commit_for_head(head: &Event, body_commit: Option<&str>) -> Result<String, String> {
    let signed_commit = exact_tag(head, "commit", 2)?
        .get(1)
        .cloned()
        .ok_or_else(|| "Legacy Wiki TOC commit is invalid".to_owned())?;
    if signed_commit.is_empty() || signed_commit.len() > 128 {
        return Err("Legacy Wiki TOC commit is invalid".into());
    }
    if body_commit.is_some_and(|commit| commit != signed_commit) {
        return Err("Legacy Wiki TOC body commit differs from its signed commit".into());
    }
    Ok(signed_commit)
}

fn legacy_page_commit(event: &Event, expected: &str) -> Result<(), String> {
    let commit = exact_tag(event, "commit", 2)?
        .get(1)
        .ok_or_else(|| "Legacy Wiki page commit is invalid".to_owned())?;
    if commit.as_str() != expected {
        return Err("Legacy Wiki page commit differs from its TOC".into());
    }
    Ok(())
}

async fn query_head<R: Runtime>(
    app: AppHandle<R>,
    token: &OwnerScopeToken,
    transport: &OwnerOperationTransport,
    owner: &str,
    repo_d: &str,
    head_d: &str,
) -> Result<HeadRead, ReadError> {
    let events = scoped_query(
        app,
        token,
        transport,
        json!({
            "kinds": [WIKI_EVENT_KIND],
            "authors": [owner],
            "#d": [head_d],
            "limit": 2,
        }),
    )
    .await?;
    let event = match exact_event_optional(&events, WIKI_EVENT_KIND, owner, head_d) {
        Ok(Some(event)) => event,
        Ok(None) => return Ok(HeadRead::Missing),
        Err(error) => return Ok(HeadRead::Invalid(events.first().cloned(), error)),
    };
    if let Err(error) = validate_optional_coordinate(&event, owner, repo_d) {
        return Ok(HeadRead::Invalid(Some(event), error));
    }
    Ok(HeadRead::Found(event))
}

async fn scoped_query<R: Runtime>(
    app: AppHandle<R>,
    token: &OwnerScopeToken,
    transport: &OwnerOperationTransport,
    filter: Value,
) -> Result<Vec<Event>, ReadError> {
    let events = match transport
        .query(filter, assert_current(app.clone(), token))
        .await
    {
        Ok(events) => events,
        Err(error) => {
            // A transport failure can race an owner/workspace replacement.
            // Check the fence before classifying it as an ordinary incomplete
            // read, so an old failure cannot be published under a new scope.
            assert_current(app, token)
                .await
                .map_err(|error| ReadError::Stale(error.to_string()))?;
            return Err(read_error(error.to_string()));
        }
    };
    assert_current(app, token)
        .await
        .map_err(|error| ReadError::Stale(error.to_string()))?;
    Ok(events)
}

fn exact_event<'a>(events: &'a [Event], kind: u16, owner: &str, d: &str) -> Result<Event, String> {
    let [event] = events else {
        return Err("Wiki query returned an unexpected number of events".into());
    };
    if event.kind.as_u16() != kind
        || event.pubkey.to_hex() != owner
        || event.verify().is_err()
        || !exact_d_tag(event).is_ok_and(|value| value == d)
    {
        return Err("Wiki query returned an invalid signed event".into());
    }
    Ok(event.clone())
}

fn exact_event_optional(
    events: &[Event],
    kind: u16,
    owner: &str,
    d: &str,
) -> Result<Option<Event>, String> {
    match events {
        [] => Ok(None),
        [event] => {
            if event.kind.as_u16() != kind
                || event.pubkey.to_hex() != owner
                || event.verify().is_err()
                || !exact_d_tag(event).is_ok_and(|value| value == d)
            {
                return Err("Wiki head is not a valid signed event".into());
            }
            Ok(Some(event.clone()))
        }
        _ => Err("Wiki head query returned duplicate events".into()),
    }
}

fn exact_d_tag(event: &Event) -> Result<String, String> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    let [tag] = tags.as_slice() else {
        return Err("Wiki event has an invalid d tag".into());
    };
    if tag.as_slice().len() != 2 {
        return Err("Wiki event has an invalid d tag".into());
    }
    Ok(tag.as_slice()[1].clone())
}

fn validate_optional_coordinate(event: &Event, owner: &str, repo_d: &str) -> Result<(), String> {
    let coordinate = format!("30617:{owner}:{repo_d}");
    let a_tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "a"))
        .collect();
    match a_tags.as_slice() {
        [] => Ok(()),
        [tag] if tag.as_slice().len() == 2 && tag.as_slice()[1] == coordinate => Ok(()),
        _ => Err("Wiki event has a conflicting repository coordinate".into()),
    }
}

fn exact_tag<'a>(event: &'a Event, name: &str, width: usize) -> Result<&'a [String], String> {
    let mut tags = event.tags.iter().filter_map(|tag| {
        (tag.as_slice().first().is_some_and(|value| value == name)).then_some(tag.as_slice())
    });
    let tag = tags
        .next()
        .ok_or_else(|| format!("Wiki event is missing {name} tag"))?;
    if tag.len() != width || tags.next().is_some() {
        return Err(format!("Wiki event has an invalid {name} tag"));
    }
    Ok(tag)
}

fn has_any_tag(event: &Event, names: &[&str]) -> bool {
    event.tags.iter().any(|tag| {
        tag.as_slice()
            .first()
            .is_some_and(|name| names.iter().any(|candidate| name == candidate))
    })
}

fn coordinate_parts(coordinate: &str) -> Result<(&str, &str), String> {
    let mut parts = coordinate.splitn(3, ':');
    let kind = parts.next();
    let owner = parts.next().unwrap_or_default();
    let repo_d = parts.next().unwrap_or_default();
    if kind != Some("30617")
        || !is_lower_hex(owner, 64)
        || repo_d.is_empty()
        || repo_d.len() > 64
        || repo_d.starts_with('.')
        || repo_d.contains("..")
        || !repo_d
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("Wiki repository coordinate is invalid".into());
    }
    Ok((owner, repo_d))
}

fn valid_legacy_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 80
        && slug != "_toc"
        && !slug.contains('/')
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_lower_hex(value: &str, width: usize) -> bool {
    value.len() == width
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_error(error: String) -> ReadError {
    if error == OWNER_SCOPE_STALE {
        ReadError::Stale(error)
    } else {
        ReadError::Failed(error)
    }
}

fn incomplete(head: Option<Event>, manifest: Option<Event>, error: String) -> WikiSnapshotRead {
    WikiSnapshotRead {
        state: WikiSnapshotReadState::Incomplete,
        head,
        manifest,
        pages: Vec::new(),
        error: Some(error),
        repo_state: None,
    }
}

fn scoped<T: Serialize>(token: OwnerScopeToken, value: T) -> ScopedOperationResult<T> {
    ScopedOperationResult { token, value }
}

// Kept adjacent to the reader contract so the state-authentication rule is
// visible to callers that later add freshness/push automation. The relay's
// 30618 signer is authoritative only after this helper and the NIP-11 `self`
// lookup agree on one exact coordinate.
async fn trusted_repo_state<R: Runtime>(
    app: AppHandle<R>,
    token: &OwnerScopeToken,
    transport: &OwnerOperationTransport,
    owner: &str,
    repo_d: &str,
) -> Result<Option<Event>, ReadError> {
    let coordinate = format!("30617:{owner}:{repo_d}");
    let owner_events = scoped_query(
        app.clone(),
        token,
        transport,
        json!({
            "kinds": [WIKI_STATE_KIND],
            "authors": [owner],
            "#d": [repo_d],
            "limit": 2,
        }),
    )
    .await?;
    let owner_candidate =
        select_owner_state(&owner_events, owner, repo_d, &coordinate).map_err(ReadError::Failed)?;

    let relay_self = match fetch_relay_self_scoped(app.clone(), token)
        .await
        .map_err(read_error)
    {
        Ok(relay_self) => relay_self,
        Err(ReadError::Stale(error)) => return Err(ReadError::Stale(error)),
        // An owner-signed state is independently authoritative. A failed
        // discovery must not hide it, while a relay-only state remains absent.
        Err(ReadError::Failed(_)) => return Ok(owner_candidate),
    };
    let Some(relay_self) = relay_self else {
        return Ok(owner_candidate);
    };

    let relay_events = match scoped_query(
        app,
        token,
        transport,
        json!({
            "kinds": [WIKI_STATE_KIND],
            "authors": [&relay_self],
            "#d": [repo_d],
            "#a": [coordinate],
            "limit": 2,
        }),
    )
    .await
    {
        Ok(events) => events,
        Err(ReadError::Stale(error)) => return Err(ReadError::Stale(error)),
        Err(ReadError::Failed(_)) => return Ok(owner_candidate),
    };
    let relay_candidate = select_relay_state(&relay_events, &relay_self, repo_d, &coordinate)
        .map_err(ReadError::Failed)?;

    Ok(newest_state(owner_candidate, relay_candidate))
}

fn select_owner_state(
    events: &[Event],
    owner: &str,
    repo_d: &str,
    coordinate: &str,
) -> Result<Option<Event>, String> {
    let mut candidates = Vec::new();
    for event in events {
        if event.kind.as_u16() != WIKI_STATE_KIND
            || event.pubkey.to_hex() != owner
            || event.verify().is_err()
        {
            continue;
        }
        validate_ref_state_tags(event)?;
        if exact_d_tag(event).ok().as_deref() != Some(repo_d) {
            continue;
        }
        let a_tags = event
            .tags
            .iter()
            .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "a"))
            .collect::<Vec<_>>();
        match a_tags.as_slice() {
            [] => candidates.push(event.clone()),
            [tag] if tag.as_slice().len() == 2 && tag.as_slice()[1] == coordinate => {
                candidates.push(event.clone())
            }
            _ => {
                return Err("Repository state has a conflicting association".into());
            }
        }
    }
    Ok(newest_state_list(candidates))
}

fn select_relay_state(
    events: &[Event],
    relay_self: &str,
    repo_d: &str,
    coordinate: &str,
) -> Result<Option<Event>, String> {
    let mut candidates = Vec::new();
    for event in events {
        if event.kind.as_u16() != WIKI_STATE_KIND
            || event.pubkey.to_hex() != relay_self
            || event.verify().is_err()
        {
            continue;
        }
        validate_ref_state_tags(event)?;
        if exact_d_tag(event).ok().as_deref() != Some(repo_d) {
            continue;
        }
        let a_tags = event
            .tags
            .iter()
            .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "a"))
            .collect::<Vec<_>>();
        match a_tags.as_slice() {
            [tag] if tag.as_slice().len() == 2 && tag.as_slice()[1] == coordinate => {
                candidates.push(event.clone())
            }
            _ => return Err("Relay repository state has a conflicting association".into()),
        }
    }
    Ok(newest_state_list(candidates))
}

fn newest_state(owner: Option<Event>, relay: Option<Event>) -> Option<Event> {
    newest_state_list(owner.into_iter().chain(relay).collect())
}

fn newest_state_list(mut events: Vec<Event>) -> Option<Event> {
    events.sort_by(|left, right| {
        left.created_at
            .as_secs()
            .cmp(&right.created_at.as_secs())
            .then_with(|| left.id.to_hex().cmp(&right.id.to_hex()))
    });
    events.pop()
}

/// Validate NIP-34 ref metadata before using a state event as a freshness
/// source. An invalid `HEAD` must not silently become the renderer's default
/// branch, and malformed ref tags must not be treated as a current tip.
fn validate_ref_state_tags(event: &Event) -> Result<(), String> {
    let mut seen_refs = BTreeSet::new();
    let mut seen_head = false;
    for tag in event.tags.iter() {
        let values = tag.as_slice();
        let Some(name) = values.first() else { continue };
        if name == "HEAD" {
            if seen_head || values.len() != 2 || !valid_default_head(&values[1]) {
                return Err("Repository state has an invalid HEAD ref".into());
            }
            seen_head = true;
            continue;
        }
        if name.starts_with("refs/heads/") || name.starts_with("refs/tags/") {
            if values.len() != 2
                || !valid_ref_name(name)
                || !valid_ref_oid(&values[1])
                || !seen_refs.insert(name)
            {
                return Err("Repository state has an invalid ref".into());
            }
        }
    }
    Ok(())
}

fn valid_default_head(value: &str) -> bool {
    value
        .strip_prefix("ref: refs/heads/")
        .is_some_and(valid_ref_tail)
}

fn valid_ref_name(value: &str) -> bool {
    (value.starts_with("refs/heads/") || value.starts_with("refs/tags/"))
        && !value.ends_with('/')
        && !value.contains("//")
        && !value.contains("..")
        && valid_ref_tail(value.rsplit_once('/').map_or("", |(_, tail)| tail))
}

fn valid_ref_tail(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'.' | b'-'))
}

fn valid_ref_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
#[path = "wiki_snapshot_tests.rs"]
mod tests;
