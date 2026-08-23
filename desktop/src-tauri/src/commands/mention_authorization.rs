//! Send-time authorization for relay agent mentions.
//!
//! Autocomplete reads the full relay directory, but a send must only trust the
//! agents it is actually addressing in the destination channel. Revalidation is
//! therefore bounded by the mention set and, when known, the target channel:
//! only `bot`-role membership the viewer can itself see admits an agent.

use std::collections::{HashMap, HashSet};

use nostr::Event;
use serde_json::{json, Value};
use tauri::State;

use crate::{
    app_state::AppState, managed_agents::RelayAgentInfo, nostr_convert, relay::query_relay,
};

/// Exactly one replaceable event per author, so a flooded relay page cannot
/// crowd out the authentic record for a mentioned agent.
fn exact_author_filters(pubkeys: &[String], kind: u16) -> Vec<Value> {
    pubkeys
        .iter()
        .map(|pubkey| {
            json!({
                "authors": [pubkey],
                "kinds": [kind],
                "limit": 1,
            })
        })
        .collect()
}

/// Membership visible to `viewer_pubkey`, narrowed to `channel_id` when the
/// send destination is known.
fn membership_filter(viewer_pubkey: &str, channel_id: Option<&str>) -> Value {
    let mut filter = json!({
        "kinds": [39002],
        "#p": [viewer_pubkey],
    });
    if let Some(channel_id) = channel_id {
        filter["#d"] = json!([channel_id]);
    }
    filter
}

/// Map each requested agent to the channels where it is a `bot` member.
///
/// The membership events are the ones the viewer is a member of, so an agent is
/// only admitted where the sender can legitimately address it.
fn bot_member_channel_ids(
    events: &[Event],
    requested_pubkeys: &HashSet<String>,
) -> HashMap<String, Vec<String>> {
    let mut by_agent: HashMap<String, Vec<String>> = HashMap::new();
    for event in events {
        let Some(channel_id) = event.tags.iter().find_map(|tag| {
            let slice = tag.as_slice();
            (slice.len() >= 2 && slice[0] == "d").then(|| slice[1].clone())
        }) else {
            continue;
        };
        for tag in event.tags.iter() {
            let slice = tag.as_slice();
            if slice.len() < 2 || slice[0] != "p" {
                continue;
            }
            let pubkey = &slice[1];
            if slice.get(3).map(String::as_str) != Some("bot")
                || !requested_pubkeys.contains(pubkey)
            {
                continue;
            }
            let channels = by_agent.entry(pubkey.clone()).or_default();
            if !channels.contains(&channel_id) {
                channels.push(channel_id.clone());
            }
        }
    }
    by_agent
}

fn relay_agents_from_directory_events(events: &[Event]) -> Result<Vec<RelayAgentInfo>, String> {
    let value = nostr_convert::agents_from_events(events);
    let agents = value.get("agents").cloned().unwrap_or_else(|| json!([]));
    serde_json::from_value(agents).map_err(|error| format!("agent parse failed: {error}"))
}

fn normalized_pubkeys(pubkeys: Vec<String>) -> HashSet<String> {
    pubkeys
        .iter()
        .filter_map(|pubkey| nostr::PublicKey::from_hex(pubkey).ok())
        .map(|pubkey| pubkey.to_hex())
        .collect()
}

/// Revalidate only the mentioned relay agents in the destination channel.
///
/// Keeps the unbounded directory command for autocomplete while making
/// send-time authorization depend on membership the sender can observe.
#[tauri::command]
pub async fn revalidate_relay_agents(
    pubkeys: Vec<String>,
    channel_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<RelayAgentInfo>, String> {
    let requested_pubkeys = normalized_pubkeys(pubkeys);
    if requested_pubkeys.is_empty() {
        return Ok(Vec::new());
    }

    let viewer_pubkey = state
        .keys
        .lock()
        .map(|keys| keys.public_key().to_hex())
        .map_err(|error| error.to_string())?;
    let membership_events = query_relay(
        &state,
        &[membership_filter(&viewer_pubkey, channel_id.as_deref())],
    )
    .await
    .map_err(|error| format!("relay agent channel-membership query failed: {error}"))?;

    let member_channel_ids = bot_member_channel_ids(&membership_events, &requested_pubkeys);
    let candidate_pubkeys: Vec<String> = member_channel_ids.keys().cloned().collect();
    if candidate_pubkeys.is_empty() {
        return Ok(Vec::new());
    }

    let directory_events = query_relay(&state, &exact_author_filters(&candidate_pubkeys, 10100))
        .await
        .map_err(|error| format!("relay agent runtime-directory query failed: {error}"))?;

    let mut agents = relay_agents_from_directory_events(&directory_events)?;
    agents.retain(|agent| member_channel_ids.contains_key(&agent.pubkey));
    for agent in &mut agents {
        agent.channel_ids = member_channel_ids
            .get(&agent.pubkey)
            .cloned()
            .unwrap_or_default();
    }
    Ok(agents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn membership_event(channel_id: &str, members: &[(&str, &str)]) -> Event {
        let tags = std::iter::once(Tag::parse(["d", channel_id]).unwrap())
            .chain(members.iter().map(|(pubkey, role)| {
                Tag::parse(["p", pubkey, "", role]).unwrap()
            }))
            .collect::<Vec<_>>();
        EventBuilder::new(Kind::Custom(39002), "")
            .tags(tags)
            .sign_with_keys(&Keys::generate())
            .unwrap()
    }

    #[test]
    fn directory_queries_are_bounded_to_the_mention_set() {
        let pubkeys = vec!["a".repeat(64), "b".repeat(64)];

        let filters = exact_author_filters(&pubkeys, 10100);

        assert_eq!(filters.len(), 2);
        for (filter, pubkey) in filters.iter().zip(pubkeys) {
            assert_eq!(filter["authors"], json!([pubkey]));
            assert_eq!(filter["kinds"], json!([10100]));
            assert_eq!(filter["limit"], 1);
        }
    }

    #[test]
    fn membership_filter_narrows_to_the_destination_channel() {
        let viewer = "a".repeat(64);

        let scoped = membership_filter(&viewer, Some("channel-1"));
        assert_eq!(scoped["kinds"], json!([39002]));
        assert_eq!(scoped["#p"], json!([viewer]));
        assert_eq!(scoped["#d"], json!(["channel-1"]));

        let unscoped = membership_filter(&viewer, None);
        assert!(unscoped.get("#d").is_none());
    }

    #[test]
    fn only_bot_members_in_the_mention_set_are_admitted() {
        let agent = "a".repeat(64);
        let human = "b".repeat(64);
        let unmentioned_agent = "c".repeat(64);
        let events = vec![membership_event(
            "channel-1",
            &[
                (&agent, "bot"),
                (&human, "member"),
                (&unmentioned_agent, "bot"),
            ],
        )];
        let requested = HashSet::from([agent.clone(), human.clone()]);

        let admitted = bot_member_channel_ids(&events, &requested);

        assert_eq!(admitted.len(), 1);
        assert_eq!(admitted.get(&agent), Some(&vec!["channel-1".to_string()]));
    }

    #[test]
    fn spoofed_membership_without_bot_role_is_rejected() {
        let agent = "a".repeat(64);
        let events = vec![membership_event("channel-1", &[(&agent, "member")])];

        let admitted = bot_member_channel_ids(&events, &HashSet::from([agent]));

        assert!(admitted.is_empty());
    }

    #[test]
    fn admitted_channels_accumulate_across_membership_events() {
        let agent = "a".repeat(64);
        let events = vec![
            membership_event("channel-1", &[(&agent, "bot")]),
            membership_event("channel-2", &[(&agent, "bot")]),
        ];

        let admitted = bot_member_channel_ids(&events, &HashSet::from([agent.clone()]));

        assert_eq!(
            admitted.get(&agent),
            Some(&vec!["channel-1".to_string(), "channel-2".to_string()])
        );
    }
}
