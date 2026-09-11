use super::*;

pub(super) fn canonical_channel_id(value: &str) -> Result<String, String> {
    if value != value.trim() {
        return Err("invalid_thread".to_string());
    }
    uuid::Uuid::parse_str(value)
        .map(|uuid| uuid.to_string())
        .map_err(|_| "invalid_thread".to_string())
}

pub(super) fn canonical_event_id(value: &str) -> Result<String, String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid_thread".to_string());
    }
    Ok(value.to_ascii_lowercase())
}

pub(super) fn valid_generation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_GENERATION_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn event_in_channel(event: &nostr::Event, channel_id: &str) -> bool {
    event.tags.iter().any(|tag| {
        let fields = tag.as_slice();
        fields.first().map(String::as_str) == Some("h")
            && fields.get(1).map(String::as_str) == Some(channel_id)
    })
}

fn event_is_thread_source(event: &nostr::Event) -> bool {
    THREAD_SOURCE_KINDS.contains(&(event.kind.as_u16() as u32))
}

fn event_footprint_bytes(event: &nostr::Event) -> usize {
    let tags_bytes = event.tags.iter().fold(0usize, |total, tag| {
        total.saturating_add(tag.as_slice().iter().fold(0usize, |tag_total, value| {
            tag_total.saturating_add(value.len())
        }))
    });
    event
        .content
        .len()
        .saturating_add(tags_bytes)
        .saturating_add(512)
}

pub(super) fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn settings_fingerprint(settings: &RecapSettings) -> Result<String, String> {
    let bytes = serde_json::to_vec(settings).map_err(|_| "invalid_settings".to_string())?;
    Ok(sha256_hex(bytes))
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

fn referenced_event_ids(event: &nostr::Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            let fields = tag.as_slice();
            (fields.first().map(String::as_str) == Some("e"))
                .then(|| fields.get(1))
                .flatten()
                .and_then(|value| canonical_event_id(value).ok())
        })
        .collect()
}

fn project_auxiliary_events(events: &mut Vec<nostr::Event>, auxiliary_events: &[nostr::Event]) {
    let deleted_event_ids: std::collections::HashSet<String> = auxiliary_events
        .iter()
        .filter(|event| {
            matches!(
                event.kind.as_u16() as u32,
                buzz_core_pkg::kind::KIND_DELETION | buzz_core_pkg::kind::KIND_NIP29_DELETE_EVENT
            )
        })
        .flat_map(referenced_event_ids)
        .collect();

    let mut latest_edits: HashMap<String, (u64, String)> = HashMap::new();
    for edit in auxiliary_events.iter().filter(|event| {
        event.kind.as_u16() as u32 == buzz_core_pkg::kind::KIND_STREAM_MESSAGE_EDIT
            && !deleted_event_ids.contains(&event.id.to_hex())
    }) {
        for target_id in referenced_event_ids(edit) {
            if !events.iter().any(|event| event.id.to_hex() == target_id) {
                continue;
            }
            let candidate = (edit.created_at.as_secs(), edit.content.clone());
            let replace = latest_edits
                .get(&target_id)
                .is_none_or(|(created_at, _)| candidate.0 > *created_at);
            if replace {
                latest_edits.insert(target_id, candidate);
            }
        }
    }

    events.retain(|event| !deleted_event_ids.contains(&event.id.to_hex()));
    for event in events.iter_mut() {
        if let Some((_, content)) = latest_edits.get(&event.id.to_hex()) {
            event.content.clone_from(content);
        }
    }
}

pub(super) fn build_thread_source(
    mut events: Vec<nostr::Event>,
    auxiliary_events: Vec<nostr::Event>,
    channel_id: &str,
    root_event_id: &str,
) -> Result<ThreadSource, String> {
    project_auxiliary_events(&mut events, &auxiliary_events);
    let omitted_by_source_cap = events.len().saturating_sub(MAX_SOURCE_EVENTS) as u32;
    if events.len() > MAX_SOURCE_EVENTS {
        let root = events
            .iter()
            .find(|event| event.id.to_hex() == root_event_id)
            .cloned()
            .ok_or_else(|| "source_unavailable".to_string())?;
        events = events.into_iter().rev().take(MAX_SOURCE_EVENTS).collect();
        if !events
            .iter()
            .any(|event| event.id.to_hex() == root_event_id)
        {
            events.pop();
            events.push(root);
        }
        events.sort_by(|left, right| {
            left.created_at
                .as_secs()
                .cmp(&right.created_at.as_secs())
                .then_with(|| left.id.to_hex().cmp(&right.id.to_hex()))
        });
    }
    let Some(root_index) = events
        .iter()
        .position(|event| event.id.to_hex() == root_event_id)
    else {
        return Err("source_unavailable".to_string());
    };

    let prefix = RECAP_PROMPT_PREFIX.as_bytes();
    if prefix.len() > super::super::recap_adapter::RECAP_INPUT_LIMIT {
        return Err("input_limit".to_string());
    }
    let root_bytes = source_record(&events[root_index]).into_bytes();
    if root_bytes.len() > super::super::recap_adapter::RECAP_INPUT_LIMIT - prefix.len() {
        return Err("input_limit".to_string());
    }

    // Keep the root and then choose the newest whole messages that fit. The
    // final prompt is restored to chronological order before it is hashed and
    // sent to the selected one-shot adapter.
    let mut selected = vec![root_index];
    let mut remaining = super::super::recap_adapter::RECAP_INPUT_LIMIT
        .saturating_sub(prefix.len())
        .saturating_sub(root_bytes.len());
    let mut omitted_message_count = omitted_by_source_cap;
    for index in (0..events.len()).rev() {
        if index == root_index {
            continue;
        }
        let record = source_record(&events[index]);
        if record.len() <= remaining {
            remaining -= record.len();
            selected.push(index);
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

    let mut manifest_source_events = events.clone();
    manifest_source_events.extend(auxiliary_events);
    manifest_source_events.sort_by(|left, right| {
        left.created_at
            .as_secs()
            .cmp(&right.created_at.as_secs())
            .then_with(|| left.id.to_hex().cmp(&right.id.to_hex()))
    });
    manifest_source_events.dedup_by(|left, right| left.id == right.id);
    let manifest_events = manifest_source_events
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
        max_input_bytes: super::super::recap_adapter::RECAP_INPUT_LIMIT as u64,
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
        oldest_included_event_id,
        newest_included_event_id,
    })
}

pub(super) async fn collect_thread_source(
    state: &super::super::super::app_state::AppState,
    owner_scope: &crate::app_state::owner_scope::CapturedOwnerScope,
    channel_id: &str,
    root_event_id: &str,
) -> Result<ThreadSource, String> {
    let relay_http = super::super::super::relay::relay_http_base_url(&owner_scope.relay_url);
    let root_events = super::super::super::relay::query_relay_at_with_keys(
        state,
        &relay_http,
        &[serde_json::json!({
            "ids": [root_event_id],
            "kinds": THREAD_SOURCE_KINDS,
            "limit": 1,
        })],
        &owner_scope.keys,
        None,
    )
    .await
    .map_err(|_| "source_unavailable".to_string())?;
    let Some(root) = root_events.into_iter().find(|event| {
        event.id.to_hex() == root_event_id
            && event_is_thread_source(event)
            && event_in_channel(event, channel_id)
    }) else {
        return Err("source_unavailable".to_string());
    };

    let mut replies = Vec::new();
    let mut reply_bytes = event_footprint_bytes(&root);
    if reply_bytes > MAX_SOURCE_SCAN_BYTES {
        return Err("source_limit".to_string());
    }
    let mut cursor: Option<(u64, String)> = None;
    let mut scan_complete = false;
    loop {
        let mut filter = serde_json::json!({
            "#e": [root_event_id],
            "#h": [channel_id],
            "kinds": THREAD_SOURCE_KINDS,
            "depth_limit": 64,
            "limit": SOURCE_PAGE_LIMIT as u32,
        });
        if let Some((created_at, event_id)) = cursor.as_ref() {
            filter["thread_cursor"] = serde_json::json!(created_at);
            filter["thread_cursor_id"] = serde_json::json!(event_id);
        }
        let page = super::super::super::relay::query_relay_at_with_keys(
            state,
            &relay_http,
            &[filter],
            &owner_scope.keys,
            None,
        )
        .await
        .map_err(|_| "source_unavailable".to_string())?;
        let page_len = page.len();
        let last = page
            .last()
            .map(|event| (event.created_at.as_secs(), event.id.to_hex()));
        for event in page {
            if !event_is_thread_source(&event) || !event_in_channel(&event, channel_id) {
                continue;
            }
            if replies.len() >= MAX_SOURCE_SCAN_EVENTS {
                return Err("source_limit".to_string());
            }
            reply_bytes = reply_bytes.saturating_add(event_footprint_bytes(&event));
            if reply_bytes > MAX_SOURCE_SCAN_BYTES {
                return Err("source_limit".to_string());
            }
            replies.push(event);
        }
        if page_len < SOURCE_PAGE_LIMIT {
            scan_complete = true;
            break;
        }
        if replies.len() >= MAX_SOURCE_SCAN_EVENTS {
            break;
        }
        let Some(next_cursor) = last else {
            break;
        };
        if cursor.as_ref() == Some(&next_cursor) {
            break;
        }
        cursor = Some(next_cursor);
    }
    if !scan_complete {
        return Err("source_limit".to_string());
    }

    let mut events = Vec::with_capacity(replies.len() + 1);
    events.push(root);
    events.extend(replies);
    events.sort_by(|left, right| {
        left.created_at
            .as_secs()
            .cmp(&right.created_at.as_secs())
            .then_with(|| left.id.to_hex().cmp(&right.id.to_hex()))
    });
    events.dedup_by(|left, right| left.id == right.id);
    let target_ids = events
        .iter()
        .map(|event| event.id.to_hex())
        .collect::<Vec<_>>();
    let mut auxiliary_events = Vec::new();
    let mut auxiliary_bytes = 0usize;
    let mut aux_cursor: Option<(u64, String)> = None;
    if !target_ids.is_empty() {
        loop {
            let mut filter = serde_json::json!({
                "#e": target_ids.clone(),
                "kinds": THREAD_AUX_KINDS,
                "limit": SOURCE_PAGE_LIMIT as u32,
            });
            if let Some((created_at, event_id)) = aux_cursor.as_ref() {
                filter["until"] = serde_json::json!(created_at);
                filter["before_id"] = serde_json::json!(event_id);
            }
            let page = super::super::super::relay::query_relay_at_with_keys(
                state,
                &relay_http,
                &[filter],
                &owner_scope.keys,
                None,
            )
            .await
            .map_err(|_| "source_unavailable".to_string())?;
            let page_len = page.len();
            let last = page
                .last()
                .map(|event| (event.created_at.as_secs(), event.id.to_hex()));
            for event in page {
                if auxiliary_events.len() >= MAX_SOURCE_SCAN_EVENTS {
                    return Err("source_limit".to_string());
                }
                auxiliary_bytes = auxiliary_bytes.saturating_add(event_footprint_bytes(&event));
                if reply_bytes.saturating_add(auxiliary_bytes) > MAX_SOURCE_SCAN_BYTES {
                    return Err("source_limit".to_string());
                }
                auxiliary_events.push(event);
            }
            if page_len < SOURCE_PAGE_LIMIT {
                break;
            }
            let Some(next_cursor) = last else {
                return Err("source_limit".to_string());
            };
            if aux_cursor.as_ref() == Some(&next_cursor) {
                return Err("source_limit".to_string());
            }
            aux_cursor = Some(next_cursor);
        }
    }
    build_thread_source(events, auxiliary_events, channel_id, root_event_id)
}
