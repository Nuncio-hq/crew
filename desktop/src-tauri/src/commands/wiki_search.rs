//! Scoped NIP-50 body search for one verified Project Wiki snapshot.

use nostr::Event;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Manager, Runtime};

use super::owner_operation_transport::OwnerOperationTransport;
use super::owner_operations::ScopedOperationResult;
use super::wiki_snapshot::{
    exact_d_tag, exact_tag, scoped_query, valid_legacy_slug, ReadError, WIKI_EVENT_KIND,
};
use crate::app_state::owner_scope::{assert_current, capture, OwnerScopeToken, OWNER_SCOPE_STALE};
use crate::app_state::AppState;

const MAX_SEARCH_QUERY_CHARS: usize = 256;
const MAX_SEARCH_RESULTS: usize = 50;

#[derive(Debug, Serialize)]
pub(crate) struct WikiSearchRead {
    pub events: Vec<Event>,
    pub truncated: bool,
}

/// Search the body of one verified v1 repository snapshot through the relay's
/// existing NIP-50 query path. The renderer still authenticates membership
/// against its coherent snapshot before displaying any returned body.
#[tauri::command]
pub(crate) async fn wiki_search(
    app: AppHandle,
    expected: OwnerScopeToken,
    coordinate: String,
    snapshot_id: String,
    query: String,
) -> Result<ScopedOperationResult<WikiSearchRead>, String> {
    search_wiki(app, expected, coordinate, snapshot_id, query).await
}

/// Production seam for the scoped Wiki body search command. Tests call this
/// helper through the same capture, transport, and relay query path as IPC.
pub(super) async fn search_wiki<R: Runtime>(
    app: AppHandle<R>,
    expected: OwnerScopeToken,
    coordinate: String,
    snapshot_id: String,
    query: String,
) -> Result<ScopedOperationResult<WikiSearchRead>, String> {
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(OWNER_SCOPE_STALE.into());
    }
    let (owner, repo_d) = super::wiki_snapshot::coordinate_parts(&coordinate)?;
    if !valid_snapshot_id(&snapshot_id) {
        return Err("Wiki snapshot identity is invalid".into());
    }
    let query = query.trim();
    if query.is_empty() || query.chars().count() > MAX_SEARCH_QUERY_CHARS {
        return Err("Wiki search query must contain 1-256 characters".into());
    }
    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        captured.keys,
        None,
    )
    .map_err(|error| error.to_string())?;
    // NIP-01 filters only accept single-letter `#X` keys. The longer
    // `wiki-snapshot` protocol tag cannot be sent through the typed relay
    // filter without being silently dropped by nostr's deserializer. Keep the
    // exact snapshot check below as a strict post-filter; an old/foreign hit
    // fails closed instead of being rendered.
    let events = scoped_query(
        app.clone(),
        &captured.token,
        &transport,
        json!({
            "kinds": [WIKI_EVENT_KIND],
            "authors": [owner],
            "#a": [coordinate],
            "search": query,
            "limit": MAX_SEARCH_RESULTS + 1,
        }),
    )
    .await
    .map_err(|error| match error {
        ReadError::Stale(error) | ReadError::Failed(error) => error,
    })?;
    for event in &events {
        validate_search_event(event, owner, repo_d, &snapshot_id)?;
    }
    let truncated = events.len() > MAX_SEARCH_RESULTS;
    let events = events.into_iter().take(MAX_SEARCH_RESULTS).collect();
    assert_current(app, &captured.token).await?;
    Ok(ScopedOperationResult {
        token: captured.token,
        value: WikiSearchRead { events, truncated },
    })
}

fn valid_snapshot_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|&index| bytes[index] == b'-')
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index)
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(byte)
        })
}

fn validate_search_event(
    event: &Event,
    owner: &str,
    repo_d: &str,
    snapshot_id: &str,
) -> Result<(), String> {
    if event.kind.as_u16() != WIKI_EVENT_KIND
        || event.pubkey.to_hex() != owner
        || event.verify().is_err()
    {
        return Err("Wiki search returned an invalid signed event".into());
    }
    let d = exact_d_tag(event)?;
    d.strip_prefix(&format!("{repo_d}/"))
        .filter(|slug| valid_legacy_slug(slug) && *slug != "_toc")
        .ok_or_else(|| "Wiki search returned an event for another repository".to_owned())?;
    let coordinate = format!("30617:{owner}:{repo_d}");
    if exact_tag(event, "a", 2)?[1] != coordinate
        || exact_tag(event, "wiki-version", 2)?[1] != "1"
        || exact_tag(event, "wiki-snapshot", 2)?[1] != snapshot_id
    {
        return Err("Wiki search returned an event outside the requested snapshot".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "wiki_search_tests.rs"]
mod tests;
