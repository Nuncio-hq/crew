//! Manual `retry_turn` observer control: re-dispatch a dead-lettered batch.
//!
//! Desktop never re-publishes the user's kind:9. It sends event ids from the
//! failure notice's `e`/`failed` tags; this module re-fetches those events,
//! re-resolves kind:40003 edits (including per-event `p-removed` vetoes),
//! derives conversation ids the same way the live path does, rematches
//! `prompt_tag` against current subscription rules, and pushes hold-exempt
//! queue entries so the next flush re-runs the work.

use std::collections::HashMap;
use std::time::Instant;

use buzz_core::kind::KIND_STREAM_MESSAGE_EDIT;
use nostr::{Alphabet, SingleLetterTag};
use serde_json::Value;
use uuid::Uuid;

use crate::conversation;
use crate::filter::{self, SubscriptionRule};
use crate::observer;
use crate::pool::{AgentPool, ChannelInfoResolver};
use crate::queue::{EventQueue, QueuedEvent};
use crate::relay;
use crate::{edit_removes_agent, edit_target_event_id, is_dm_channel};

/// Handle a `retry_turn` control frame.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_retry_turn_control(
    payload: &Value,
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    rest_client: Option<&relay::RestClient>,
    observer: Option<&observer::ObserverHandle>,
    agent_pubkey_hex: &str,
    rules: &[SubscriptionRule],
    channel_info: &ChannelInfoResolver,
) {
    let Some(channel_id) = payload
        .get("channelId")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<Uuid>().ok())
    else {
        tracing::warn!("observer retry_turn control frame missing valid channelId");
        return;
    };
    // Client-supplied conversationId is only used to correlate control_result.
    // Queue routing is derived from fetched events (see select_retry_entries).
    let result_conversation_id = payload
        .get("conversationId")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<Uuid>().ok());

    let event_ids = parse_event_ids(payload.get("eventIds"));
    if event_ids.is_empty() {
        emit_retry_result(
            observer,
            channel_id,
            result_conversation_id,
            "events_gone",
            0,
            0,
            0,
        );
        return;
    }

    let Some(rest) = rest_client else {
        tracing::warn!("observer retry_turn control frame but no RestClient — cannot fetch");
        emit_retry_result(
            observer,
            channel_id,
            result_conversation_id,
            "events_gone",
            0,
            0,
            event_ids.len(),
        );
        return;
    };

    let fetched = match fetch_events_and_edits(rest, &event_ids).await {
        Ok(fetched) => fetched,
        Err(error) => {
            tracing::warn!(%error, "retry_turn relay query failed");
            emit_retry_result(
                observer,
                channel_id,
                result_conversation_id,
                "events_gone",
                0,
                0,
                event_ids.len(),
            );
            return;
        }
    };

    let is_dm = is_dm_channel(channel_id, channel_info).await;
    let mut prompt_tags = HashMap::new();
    for event in fetched.events.values() {
        match filter::match_event(event, channel_id, rules, agent_pubkey_hex).await {
            Some(matched) => {
                prompt_tags.insert(event.id, matched.prompt_tag);
            }
            None => {
                tracing::warn!(
                    event_id = %event.id.to_hex(),
                    "retry_turn: event no longer matches subscription rules — skipping"
                );
            }
        }
    }

    let selection = select_retry_entries(
        &event_ids,
        &fetched,
        agent_pubkey_hex,
        |event| conversation::id_for_event(channel_id, event, is_dm),
        |event| prompt_tags.get(&event.id).cloned(),
    );

    if selection.entries.is_empty() {
        let status = empty_retry_status(selection.withheld, selection.missing);
        emit_retry_result(
            observer,
            channel_id,
            result_conversation_id,
            status,
            0,
            selection.withheld,
            event_ids.len(),
        );
        return;
    }

    // One failure notice comes from one FlushBatch, so surviving events share a
    // conversation. Use the first derived key for already-running / backoff.
    let conversation_key = selection.entries[0].conversation_id;
    if conversation_already_running(pool, queue, conversation_key, channel_id) {
        emit_retry_result(
            observer,
            channel_id,
            result_conversation_id.or(Some(conversation_key)),
            "already_running",
            0,
            selection.withheld,
            event_ids.len(),
        );
        return;
    }

    // Manual Retry means "run now" — drop any leftover automatic backoff.
    let mut cleared = std::collections::HashSet::new();
    for entry in &selection.entries {
        if cleared.insert(entry.conversation_id) {
            queue.clear_retry_after(entry.conversation_id);
        }
    }

    let dispatched = selection.entries.len();
    let now = Instant::now();
    for entry in selection.entries {
        queue.push(QueuedEvent {
            channel_id: entry.conversation_id,
            event: entry.event,
            received_at: now,
            prompt_tag: entry.prompt_tag,
            edited_content: entry.edited_content,
            // Already waited (and possibly ran); never re-apply the hold.
            hold_exempt: true,
        });
    }

    let status = if selection.withheld > 0 {
        "dispatched_partial"
    } else {
        "dispatched"
    };
    emit_retry_result(
        observer,
        channel_id,
        result_conversation_id.or(Some(conversation_key)),
        status,
        dispatched,
        selection.withheld,
        event_ids.len(),
    );
}

/// Status when nothing can be re-dispatched.
fn empty_retry_status(withheld: usize, missing: usize) -> &'static str {
    match (withheld > 0, missing > 0) {
        (true, false) => "agent_removed",
        (false, true) => "events_gone",
        (true, true) => "events_gone",
        (false, false) => "events_gone",
    }
}

struct RetryEntry {
    event: nostr::Event,
    conversation_id: Uuid,
    prompt_tag: String,
    edited_content: Option<String>,
}

struct RetrySelection {
    entries: Vec<RetryEntry>,
    withheld: usize,
    missing: usize,
}

/// Pure selection of which failed ids become queue entries.
///
/// `conversation_for` / `prompt_tag_for` are injected so unit tests do not need
/// the async matcher or channel-type resolver.
fn select_retry_entries(
    event_ids: &[nostr::EventId],
    fetched: &FetchedRetryMaterial,
    agent_pubkey_hex: &str,
    conversation_for: impl Fn(&nostr::Event) -> Uuid,
    prompt_tag_for: impl Fn(&nostr::Event) -> Option<String>,
) -> RetrySelection {
    let mut entries = Vec::new();
    let mut withheld = 0usize;
    let mut missing = 0usize;

    for id in event_ids {
        let Some(event) = fetched.events.get(id) else {
            missing += 1;
            continue;
        };
        let edit = fetched.latest_edit_for(id);
        if edit
            .as_ref()
            .is_some_and(|edit| edit_removes_agent(edit, agent_pubkey_hex))
        {
            withheld += 1;
            continue;
        }
        let Some(prompt_tag) = prompt_tag_for(event) else {
            // No longer matches current rules — same class as "not for this agent".
            withheld += 1;
            continue;
        };
        let edited_content = edit.map(|edit| edit.content.clone());
        entries.push(RetryEntry {
            conversation_id: conversation_for(event),
            event: event.clone(),
            prompt_tag,
            edited_content,
        });
    }

    RetrySelection {
        entries,
        withheld,
        missing,
    }
}

fn conversation_already_running(
    pool: &AgentPool,
    queue: &EventQueue,
    conversation_key: Uuid,
    channel_id: Uuid,
) -> bool {
    if queue.is_channel_in_flight(conversation_key) {
        return true;
    }
    pool.task_map().values().any(|meta| {
        meta.channel_id == Some(conversation_key)
            || (meta.channel_id.is_none() && meta.routing_channel_id == Some(channel_id))
    })
}

fn parse_event_ids(value: Option<&Value>) -> Vec<nostr::EventId> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let Some(hex) = item.as_str() else {
            continue;
        };
        let Ok(id) = nostr::EventId::from_hex(hex) else {
            continue;
        };
        if seen.insert(id) {
            ids.push(id);
        }
    }
    ids
}

struct FetchedRetryMaterial {
    events: HashMap<nostr::EventId, nostr::Event>,
    /// Latest edit per target event id (hex lowercase).
    edits_by_target: HashMap<String, nostr::Event>,
}

impl FetchedRetryMaterial {
    fn latest_edit_for(&self, id: &nostr::EventId) -> Option<&nostr::Event> {
        self.edits_by_target.get(&id.to_hex())
    }
}

async fn fetch_events_and_edits(
    rest: &relay::RestClient,
    event_ids: &[nostr::EventId],
) -> Result<FetchedRetryMaterial, String> {
    let id_hexes: Vec<String> = event_ids.iter().map(|id| id.to_hex()).collect();
    let events_filter = nostr::Filter::new().ids(event_ids.iter().copied());
    let e_tag = SingleLetterTag::lowercase(Alphabet::E);
    let edits_filter = nostr::Filter::new()
        .kind(nostr::Kind::Custom(KIND_STREAM_MESSAGE_EDIT as u16))
        .custom_tags(e_tag, id_hexes.iter().map(String::as_str));

    let resp = rest
        .query(&[events_filter, edits_filter])
        .await
        .map_err(|e| e.to_string())?;
    let rows = resp
        .as_array()
        .ok_or_else(|| "retry_turn query response was not an array".to_string())?;

    let mut events = HashMap::new();
    let mut edits_by_target: HashMap<String, nostr::Event> = HashMap::new();

    for row in rows {
        let event = match serde_json::from_value::<nostr::Event>(row.clone()) {
            Ok(event) => event,
            Err(error) => {
                tracing::warn!(%error, "retry_turn skipping malformed query event");
                continue;
            }
        };

        if event.kind == nostr::Kind::Custom(KIND_STREAM_MESSAGE_EDIT as u16) {
            let Some(target) = edit_target_event_id(&event) else {
                continue;
            };
            match edits_by_target.get(&target) {
                Some(existing) if existing.created_at >= event.created_at => {}
                _ => {
                    edits_by_target.insert(target, event);
                }
            }
            continue;
        }

        if event_ids.contains(&event.id) {
            events.insert(event.id, event);
        }
    }

    Ok(FetchedRetryMaterial {
        events,
        edits_by_target,
    })
}

fn emit_retry_result(
    observer: Option<&observer::ObserverHandle>,
    channel_id: Uuid,
    conversation_id: Option<Uuid>,
    status: &str,
    dispatched_count: usize,
    withheld_count: usize,
    requested_count: usize,
) {
    let Some(observer) = observer else {
        return;
    };
    let context = observer::context_for_conversation(Some(channel_id), conversation_id, None, None);
    observer.emit(
        "control_result",
        None,
        &context,
        serde_json::json!({
            "type": "retry_turn",
            "status": status,
            "dispatchedCount": dispatched_count,
            "withheldCount": withheld_count,
            "requestedCount": requested_count,
            "conversationId": conversation_id.map(|id| id.to_string()),
        }),
    );
}

/// Emit a `turn_retrying` observer frame after a successful automatic requeue.
pub(crate) fn emit_turn_retrying(
    observer: Option<&observer::ObserverHandle>,
    routing_channel_id: Uuid,
    conversation_id: Uuid,
    turn_id: Option<String>,
    attempt: u32,
    max_attempts: u32,
) {
    let Some(observer) = observer else {
        return;
    };
    let context = observer::context_for_conversation(
        Some(routing_channel_id),
        Some(conversation_id),
        None,
        turn_id,
    );
    observer.emit(
        "turn_retrying",
        None,
        &context,
        serde_json::json!({
            "attempt": attempt,
            "maxAttempts": max_attempts,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DedupMode;
    use crate::{edit_removes_agent, edit_target_event_id};
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn signed(content: &str, tags: Vec<Tag>) -> nostr::Event {
        EventBuilder::new(Kind::Custom(9), content)
            .tags(tags)
            .sign_with_keys(&Keys::generate())
            .expect("sign")
    }

    fn edit_for(target: &nostr::Event, content: &str, tags: Vec<Tag>) -> nostr::Event {
        let mut all = vec![Tag::parse(["e", &target.id.to_hex()]).expect("e tag")];
        all.extend(tags);
        EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE_EDIT as u16), content)
            .tags(all)
            .sign_with_keys(&Keys::generate())
            .expect("sign edit")
    }

    #[test]
    fn parse_event_ids_dedupes_and_skips_junk() {
        let a = signed("a", vec![]);
        let b = signed("b", vec![]);
        let value = serde_json::json!([a.id.to_hex(), "not-an-id", a.id.to_hex(), b.id.to_hex(),]);
        let ids = parse_event_ids(Some(&value));
        assert_eq!(ids, vec![a.id, b.id]);
    }

    #[test]
    fn clear_retry_after_unblocks_manual_retry() {
        let mut queue = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();
        queue.set_retry_after_for_test(ch, Instant::now() + std::time::Duration::from_secs(60));
        assert!(queue.has_retry_after_for_test(ch));
        queue.clear_retry_after(ch);
        assert!(!queue.has_retry_after_for_test(ch));
    }

    #[test]
    fn empty_retry_status_distinguishes_gone_and_removed() {
        assert_eq!(empty_retry_status(1, 0), "agent_removed");
        assert_eq!(empty_retry_status(0, 1), "events_gone");
        // Mixed: event disappearance is the honest primary report.
        assert_eq!(empty_retry_status(1, 1), "events_gone");
        assert_eq!(empty_retry_status(0, 0), "events_gone");
    }

    #[test]
    fn select_skips_p_removed_per_event_and_reports_partial() {
        let agent = "aa".repeat(32);
        let keep = signed("keep me", vec![]);
        let drop = signed("drop me", vec![]);
        let remove = edit_for(
            &drop,
            "no longer for you",
            vec![Tag::parse(["p-removed", &agent]).expect("p-removed")],
        );
        assert!(edit_removes_agent(&remove, &agent));
        assert_eq!(
            edit_target_event_id(&remove).as_deref(),
            Some(drop.id.to_hex().as_str())
        );

        let fetched = FetchedRetryMaterial {
            events: HashMap::from([(keep.id, keep.clone()), (drop.id, drop.clone())]),
            edits_by_target: HashMap::from([(drop.id.to_hex(), remove)]),
        };
        let conv = Uuid::new_v4();
        let selection = select_retry_entries(
            &[keep.id, drop.id],
            &fetched,
            &agent,
            |_| conv,
            |_| Some("@mention".into()),
        );
        assert_eq!(selection.entries.len(), 1);
        assert_eq!(selection.entries[0].event.id, keep.id);
        assert_eq!(selection.withheld, 1);
        assert_eq!(selection.missing, 0);
    }

    #[test]
    fn select_all_p_removed_is_agent_removed() {
        let agent = "bb".repeat(32);
        let only = signed("gone", vec![]);
        let remove = edit_for(
            &only,
            "withdrawn",
            vec![Tag::parse(["p-removed", &agent]).expect("p-removed")],
        );
        let fetched = FetchedRetryMaterial {
            events: HashMap::from([(only.id, only.clone())]),
            edits_by_target: HashMap::from([(only.id.to_hex(), remove)]),
        };
        let selection = select_retry_entries(
            &[only.id],
            &fetched,
            &agent,
            |_| Uuid::new_v4(),
            |_| Some("@mention".into()),
        );
        assert!(selection.entries.is_empty());
        assert_eq!(
            empty_retry_status(selection.withheld, selection.missing),
            "agent_removed"
        );
    }

    #[test]
    fn select_missing_events_reports_events_gone() {
        let present = signed("here", vec![]);
        let missing_id = signed("absent", vec![]).id;
        let fetched = FetchedRetryMaterial {
            events: HashMap::from([(present.id, present.clone())]),
            edits_by_target: HashMap::new(),
        };
        let selection = select_retry_entries(
            &[present.id, missing_id],
            &fetched,
            "cc".repeat(32).as_str(),
            |_| Uuid::new_v4(),
            |_| Some("all".into()),
        );
        // One present → not empty; missing counted.
        assert_eq!(selection.entries.len(), 1);
        assert_eq!(selection.missing, 1);

        let selection_all_missing = select_retry_entries(
            &[missing_id],
            &fetched,
            "cc".repeat(32).as_str(),
            |_| Uuid::new_v4(),
            |_| Some("all".into()),
        );
        assert!(selection_all_missing.entries.is_empty());
        assert_eq!(
            empty_retry_status(
                selection_all_missing.withheld,
                selection_all_missing.missing
            ),
            "events_gone"
        );
    }

    #[test]
    fn select_applies_edited_content_from_latest_edit() {
        let event = signed("original", vec![]);
        let edit = edit_for(&event, "edited body", vec![]);
        let fetched = FetchedRetryMaterial {
            events: HashMap::from([(event.id, event.clone())]),
            edits_by_target: HashMap::from([(event.id.to_hex(), edit)]),
        };
        let selection = select_retry_entries(
            &[event.id],
            &fetched,
            "dd".repeat(32).as_str(),
            |_| Uuid::new_v4(),
            |_| Some("@mention".into()),
        );
        assert_eq!(
            selection.entries[0].edited_content.as_deref(),
            Some("edited body")
        );
    }

    #[test]
    fn select_uses_injected_conversation_and_prompt_tag() {
        let event = signed("hi", vec![]);
        let conv = Uuid::new_v4();
        let fetched = FetchedRetryMaterial {
            events: HashMap::from([(event.id, event.clone())]),
            edits_by_target: HashMap::new(),
        };
        let selection = select_retry_entries(
            &[event.id],
            &fetched,
            "ee".repeat(32).as_str(),
            |_| conv,
            |_| Some("keyword-rule".into()),
        );
        assert_eq!(selection.entries[0].conversation_id, conv);
        assert_eq!(selection.entries[0].prompt_tag, "keyword-rule");
    }

    #[test]
    fn select_skips_when_prompt_tag_resolver_returns_none() {
        let event = signed("hi", vec![]);
        let fetched = FetchedRetryMaterial {
            events: HashMap::from([(event.id, event.clone())]),
            edits_by_target: HashMap::new(),
        };
        let selection = select_retry_entries(
            &[event.id],
            &fetched,
            "ff".repeat(32).as_str(),
            |_| Uuid::new_v4(),
            |_| None,
        );
        assert!(selection.entries.is_empty());
        assert_eq!(selection.withheld, 1);
    }
}
