use std::sync::atomic::{AtomicBool, Ordering};

use super::{
    sha256_hex, ThreadSource, MAX_SOURCE_EVENTS, MAX_SOURCE_SCAN_EVENTS, RECAP_CANCEL_POLL_MS,
    RECAP_PROMPT_PREFIX, RECAP_PROMPT_VERSION, THREAD_SOURCE_KINDS,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct SourceManifestEvent {
    event_id: String,
    created_at: u64,
    kind: u32,
    content_sha256: String,
    tags_sha256: String,
}

#[derive(Debug, Serialize)]
struct SourceManifest {
    version: u8,
    prompt_version: u8,
    channel_id: String,
    root_event_id: String,
    events: Vec<SourceManifestEvent>,
    included_event_ids: Vec<String>,
    omitted_message_count: u32,
    source_overflow: bool,
    max_input_bytes: u64,
}

fn event_in_channel(event: &nostr::Event, channel_id: &str) -> bool {
    event.tags.iter().any(|tag| {
        let fields = tag.as_slice();
        fields.first().map(String::as_str) == Some("h")
            && fields.get(1).map(String::as_str) == Some(channel_id)
    })
}

fn source_record(event: &nostr::Event) -> String {
    format!(
        "[{} @{}] {}\n",
        event.id.to_hex(),
        event.created_at.as_secs(),
        event.content
    )
}

fn source_manifest_event(event: &nostr::Event) -> Result<SourceManifestEvent, String> {
    let tags = serde_json::to_vec(&event.tags).map_err(|_| "source_unavailable".to_string())?;
    Ok(SourceManifestEvent {
        event_id: event.id.to_hex(),
        created_at: event.created_at.as_secs(),
        kind: event.kind.as_u16() as u32,
        content_sha256: sha256_hex(event.content.as_bytes()),
        tags_sha256: sha256_hex(tags),
    })
}

pub(super) fn append_source_scan_page(
    replies: &mut Vec<nostr::Event>,
    page: Vec<nostr::Event>,
) -> (usize, bool) {
    let page_len = page.len();
    replies.extend(page);
    if replies.len() <= MAX_SOURCE_SCAN_EVENTS {
        return (page_len, false);
    }
    let drop_count = replies.len() - MAX_SOURCE_SCAN_EVENTS;
    replies.drain(..drop_count);
    (page_len, true)
}

#[cfg(test)]
pub(super) fn build_thread_source(
    events: Vec<nostr::Event>,
    channel_id: &str,
    root_event_id: &str,
) -> Result<ThreadSource, String> {
    build_thread_source_with_overflow(events, channel_id, root_event_id, false)
}

pub(super) fn build_thread_source_with_overflow(
    mut events: Vec<nostr::Event>,
    channel_id: &str,
    root_event_id: &str,
    source_overflow: bool,
) -> Result<ThreadSource, String> {
    events.sort_by(|left, right| {
        left.created_at
            .as_secs()
            .cmp(&right.created_at.as_secs())
            .then_with(|| left.id.to_hex().cmp(&right.id.to_hex()))
    });
    events.dedup_by(|left, right| left.id == right.id);
    let Some(root_index) = events
        .iter()
        .position(|event| event.id.to_hex() == root_event_id)
    else {
        return Err("source_unavailable".to_string());
    };

    let prefix = RECAP_PROMPT_PREFIX.as_bytes();
    if prefix.len() > crate::managed_agents::recap_adapter::RECAP_INPUT_LIMIT {
        return Err("input_limit".to_string());
    }
    let root_bytes = source_record(&events[root_index]).into_bytes();
    if root_bytes.len() > crate::managed_agents::recap_adapter::RECAP_INPUT_LIMIT - prefix.len() {
        return Err("input_limit".to_string());
    }

    // Keep the root and then choose the newest whole messages that fit. The
    // final prompt is restored to chronological order before it is hashed and
    // sent to the selected one-shot adapter.
    let mut selected = vec![root_index];
    let mut remaining = crate::managed_agents::recap_adapter::RECAP_INPUT_LIMIT
        .saturating_sub(prefix.len())
        .saturating_sub(root_bytes.len());
    let mut omitted_message_count = 0u32;
    let mut available_slots = MAX_SOURCE_EVENTS.saturating_sub(1);
    for index in (0..events.len()).rev() {
        if index == root_index {
            continue;
        }
        if available_slots == 0 {
            omitted_message_count = omitted_message_count.saturating_add(1);
            continue;
        }
        let record = source_record(&events[index]);
        if record.len() <= remaining {
            remaining -= record.len();
            selected.push(index);
            available_slots -= 1;
        } else {
            omitted_message_count = omitted_message_count.saturating_add(1);
        }
    }
    selected.sort_unstable();

    let mut prompt = prefix.to_vec();
    let mut event_ids = Vec::with_capacity(selected.len());
    for index in selected {
        let event = &events[index];
        prompt.extend_from_slice(source_record(event).as_bytes());
        event_ids.push(event.id.to_hex());
    }
    if event_ids.is_empty() {
        return Err("input_limit".to_string());
    }

    let manifest_events = events
        .iter()
        .map(source_manifest_event)
        .collect::<Result<Vec<_>, _>>()?;
    let manifest = SourceManifest {
        version: 1,
        prompt_version: RECAP_PROMPT_VERSION,
        channel_id: channel_id.to_string(),
        root_event_id: root_event_id.to_string(),
        events: manifest_events,
        included_event_ids: event_ids.clone(),
        omitted_message_count,
        source_overflow,
        max_input_bytes: crate::managed_agents::recap_adapter::RECAP_INPUT_LIMIT as u64,
    };
    let manifest_bytes =
        serde_json::to_vec(&manifest).map_err(|_| "source_unavailable".to_string())?;
    let manifest_hash = sha256_hex(manifest_bytes);
    let oldest_included_event_id = event_ids.first().cloned();
    let newest_included_event_id = event_ids.last().cloned();
    Ok(ThreadSource {
        prompt: String::from_utf8(prompt).map_err(|_| "source_unavailable".to_string())?,
        manifest_hash,
        event_ids,
        omitted_message_count,
        source_overflow,
        oldest_included_event_id,
        newest_included_event_id,
    })
}

pub(super) async fn collect_thread_source(
    state: &crate::app_state::AppState,
    owner_scope: &crate::app_state::owner_scope::CapturedOwnerScope,
    channel_id: &str,
    root_event_id: &str,
) -> Result<ThreadSource, String> {
    collect_thread_source_with_cancel(state, owner_scope, channel_id, root_event_id, None).await
}

async fn wait_for_source_cancellation(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(std::time::Duration::from_millis(RECAP_CANCEL_POLL_MS)).await;
    }
}

async fn query_thread_source_page(
    state: &crate::app_state::AppState,
    relay_http: &str,
    filters: &[serde_json::Value],
    keys: &nostr::Keys,
    cancelled: Option<&AtomicBool>,
) -> Result<Vec<nostr::Event>, String> {
    let query = crate::relay::query_relay_at_with_keys(state, relay_http, filters, keys, None);
    match cancelled {
        Some(cancelled) => tokio::select! {
            result = query => result.map_err(|_| "source_unavailable".to_string()),
            _ = wait_for_source_cancellation(cancelled) => Err("cancelled".to_string()),
        },
        None => query.await.map_err(|_| "source_unavailable".to_string()),
    }
}

pub(super) async fn collect_thread_source_with_cancel(
    state: &crate::app_state::AppState,
    owner_scope: &crate::app_state::owner_scope::CapturedOwnerScope,
    channel_id: &str,
    root_event_id: &str,
    cancelled: Option<&AtomicBool>,
) -> Result<ThreadSource, String> {
    if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err("cancelled".to_string());
    }
    #[cfg(test)]
    if let Some(source) = super::test_source_override() {
        return Ok(source);
    }
    let relay_http = crate::relay::relay_http_base_url(&owner_scope.relay_url);
    let root_events = query_thread_source_page(
        state,
        &relay_http,
        &[serde_json::json!({
            "ids": [root_event_id],
            "kinds": THREAD_SOURCE_KINDS,
            "limit": 1,
        })],
        &owner_scope.keys,
        cancelled,
    )
    .await?;
    if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        return Err("cancelled".to_string());
    }
    let Some(root) = root_events
        .into_iter()
        .find(|event| event.id.to_hex() == root_event_id && event_in_channel(event, channel_id))
    else {
        return Err("source_unavailable".to_string());
    };

    // `get_thread_replies` is intentionally chronological (ASC) because the
    // desktop timeline walks forward with a composite cursor. Do the same here
    // until a short page proves EOF, retaining at most a bounded tail. A full
    // page after the scan ceiling is a sentinel that marks the source as
    // incomplete; it never gets reported as a complete thread snapshot.
    let page_limit = MAX_SOURCE_EVENTS as u32;
    let mut replies = Vec::new();
    let mut source_overflow = false;
    let mut cursor: Option<(u64, String)> = None;
    let mut last_page_tail: Option<String> = None;

    loop {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err("cancelled".to_string());
        }
        let mut filter = serde_json::json!({
            "#e": [root_event_id],
            "#h": [channel_id],
            "kinds": THREAD_SOURCE_KINDS,
            "depth_limit": 64,
            "limit": page_limit,
            "include_aux": false,
        });
        if let Some((created_at, event_id)) = &cursor {
            filter["thread_cursor"] = serde_json::json!(created_at);
            filter["thread_cursor_id"] = serde_json::json!(event_id);
        }

        let page =
            query_thread_source_page(state, &relay_http, &[filter], &owner_scope.keys, cancelled)
                .await?;
        if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return Err("cancelled".to_string());
        }
        let page_len = page.len();
        let page_tail = page.last().map(|event| event.id.to_hex());
        if page_len == 0 {
            break;
        }

        if page_tail == last_page_tail {
            // A relay that ignores the cursor would otherwise make this read
            // loop forever while repeatedly appending the same page.
            return Err("source_unavailable".to_string());
        }
        last_page_tail = page_tail.clone();
        let (page_len, page_overflow) = append_source_scan_page(&mut replies, page);

        if page_overflow {
            source_overflow = true;
            // Replies are ASC, so this is the newest window observed before
            // the bounded scan stopped. Newer replies may still exist; the
            // overflow bit prevents callers from treating this as current.
            break;
        }
        if page_len < page_limit as usize {
            break;
        }

        let Some(tail) = replies.last() else {
            break;
        };
        cursor = Some((tail.created_at.as_secs(), tail.id.to_hex()));
    }

    let mut events = Vec::with_capacity(replies.len() + 1);
    events.push(root);
    events.extend(
        replies
            .into_iter()
            .filter(|event| event_in_channel(event, channel_id)),
    );
    build_thread_source_with_overflow(events, channel_id, root_event_id, source_overflow)
}

pub(super) fn recap_status(
    source_overflow: bool,
    settings_is_valid: bool,
    settings_match: bool,
    source_match: bool,
) -> (&'static str, Option<&'static str>) {
    if source_overflow {
        // The bounded ASC scan cannot prove that its observed tail is the
        // thread's newest tail. Preserve the artifact for explicit review, but
        // never report it as current while newer replies may be unobserved.
        ("stale", Some("source_overflow"))
    } else if !settings_is_valid || !settings_match || !source_match {
        ("stale", None)
    } else {
        ("current", None)
    }
}
