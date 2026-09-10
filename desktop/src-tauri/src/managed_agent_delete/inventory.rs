//! Coverage is the captured relay's authorized/known view, never global absence.
use nostr::Event;
use std::collections::BTreeSet;

pub(super) const CHANNEL_LIMIT: usize = 64;
pub(super) const CHANNEL_SENTINEL: usize = CHANNEL_LIMIT + 1;

fn channel_id(event: &Event, kind: u16) -> Result<String, String> {
    event
        .verify()
        .map_err(|_| "Channel coverage contains an invalid signed event")?;
    if event.kind.as_u16() != kind {
        return Err("Channel coverage returned an unexpected event kind".into());
    }
    let coordinates: Vec<_> = event
        .tags
        .iter()
        .filter_map(|tag| {
            let fields = tag.as_slice();
            (fields.first().map(String::as_str) == Some("d")).then_some(fields)
        })
        .collect();
    let [coordinate] = coordinates.as_slice() else {
        return Err("Channel coverage has an ambiguous coordinate".into());
    };
    let [_, id] = *coordinate else {
        return Err("Channel coverage has an invalid coordinate".into());
    };
    let parsed =
        uuid::Uuid::parse_str(id).map_err(|_| "Channel coverage has an invalid channel")?;
    if parsed.to_string() != *id {
        return Err("Channel coverage has a noncanonical channel".into());
    }
    Ok(id.clone())
}

fn bounded_page(events: &[Event]) -> Result<(), String> {
    // Each caller asks for 65, not 64. A 65th result is proof of overflow;
    // partial/denied transport responses never reach this success-only parser.
    if events.len() >= CHANNEL_SENTINEL {
        return Err("Channel coverage exceeds 64 channels; review removal scope".into());
    }
    Ok(())
}

/// Union complete sentinel-bounded owner-visible metadata, the agent's own
/// memberships, and retained known coordinates. Membership is coverage only;
/// every resulting channel still needs separate owner authority validation.
pub(super) fn covered_channels(
    owner_metadata: &[Event],
    agent_memberships: &[Event],
    retained: &[String],
    agent: &str,
) -> Result<BTreeSet<String>, String> {
    bounded_page(owner_metadata)?;
    bounded_page(agent_memberships)?;
    if retained.len() > CHANNEL_LIMIT {
        return Err("Retained channel coverage exceeds 64 channels".into());
    }
    let mut channels = BTreeSet::new();
    for event in owner_metadata {
        channels.insert(channel_id(event, 39000)?);
    }
    for event in agent_memberships {
        let id = channel_id(event, 39002)?;
        if !event.tags.iter().any(|tag| {
            let fields = tag.as_slice();
            fields.first().map(String::as_str) == Some("p")
                && fields.get(1).map(String::as_str) == Some(agent)
        }) {
            return Err("Agent membership query returned unrelated coverage".into());
        }
        channels.insert(id);
    }
    for id in retained {
        let parsed =
            uuid::Uuid::parse_str(id).map_err(|_| "Retained coverage has an invalid channel")?;
        if parsed.to_string() != *id {
            return Err("Retained coverage has a noncanonical channel".into());
        }
        channels.insert(id.clone());
    }
    if channels.len() > CHANNEL_LIMIT {
        return Err("Combined channel coverage exceeds 64 channels".into());
    }
    Ok(channels)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ChannelCoverage {
    pub channel_id: String,
    pub expected_head: Option<String>,
    pub cleanup_needed: bool,
    pub membership_present: bool,
}

/// Inspect current signed state before any stop or write. Agent possession
/// never grants the human permission to rewrite a foreign/private canvas.
pub(super) fn inspect_channel(
    id: &str,
    roster: &Event,
    canvas: Option<&Event>,
    owner: &str,
    agent: &str,
) -> Result<ChannelCoverage, String> {
    if channel_id(roster, 39002)? != id {
        return Err("Membership coverage returned another channel".into());
    }
    let members: Vec<_> = roster
        .tags
        .iter()
        .filter_map(|tag| {
            let fields = tag.as_slice();
            (fields.first().map(String::as_str) == Some("p")).then_some(fields)
        })
        .collect();
    let member = |pubkey: &str| {
        members
            .iter()
            .any(|fields| fields.get(1).map(String::as_str) == Some(pubkey))
    };
    let membership_present = member(agent);
    let mut cleanup_needed = false;
    let mut expected_head = None;
    if let Some(canvas) = canvas {
        canvas.verify().map_err(|_| "Invalid canvas coverage")?;
        if canvas.kind.as_u16() != 40100
            || !canvas.tags.iter().any(|tag| tag.as_slice() == ["h", id])
        {
            return Err("Canvas coverage returned another channel".into());
        }
        expected_head = Some(canvas.id.to_hex());
        let metadata =
            buzz_core_pkg::crew_role::read_canvas_crew_metadata(Some(&canvas.content), None, "");
        if metadata.crew_parse_state == "invalid" {
            return Err("Known channel assignments require review".into());
        }
        cleanup_needed = metadata.contact_pubkey.as_deref() == Some(agent)
            || metadata.stored_assignments.keys().any(|raw| {
                nostr::PublicKey::parse(raw.trim()).is_ok_and(|key| key.to_hex() == agent)
            });
        if cleanup_needed && canvas.pubkey.to_hex() != owner {
            return Err("A known assignment belongs to a canvas this owner cannot update".into());
        }
    }
    if (membership_present || cleanup_needed) && !member(owner) {
        return Err("A known channel requires owner access before instance removal".into());
    }
    // Preserve the relay's last-owner rule before attempting the membership write.
    let owners: Vec<_> = members
        .iter()
        .filter(|fields| fields.get(3).map(String::as_str) == Some("owner"))
        .collect();
    if owners.len() == 1 && owners[0].get(1).map(String::as_str) == Some(agent) {
        return Err("A known channel requires ownership transfer before instance removal".into());
    }
    Ok(ChannelCoverage {
        channel_id: id.into(),
        expected_head,
        cleanup_needed,
        membership_present,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn snapshot(keys: &Keys, kind: u16, id: &str, members: &[&str]) -> Event {
        let mut tags = vec![Tag::parse(["d", id]).unwrap()];
        tags.extend(
            members
                .iter()
                .map(|member| Tag::parse(["p", *member, "", "member"]).unwrap()),
        );
        EventBuilder::new(Kind::Custom(kind), "")
            .tags(tags)
            .sign_with_keys(keys)
            .unwrap()
    }
    #[test]
    fn coverage_unions_authorized_membership_and_retained_views() {
        let relay = Keys::generate();
        let agent = Keys::generate().public_key().to_hex();
        let owner_channel = uuid::Uuid::new_v4().to_string();
        let private_channel = uuid::Uuid::new_v4().to_string();
        let retained_channel = uuid::Uuid::new_v4().to_string();
        let union = covered_channels(
            &[snapshot(&relay, 39000, &owner_channel, &[])],
            &[snapshot(&relay, 39002, &private_channel, &[&agent])],
            &[retained_channel.clone()],
            &agent,
        )
        .unwrap();
        assert_eq!(
            union,
            BTreeSet::from([owner_channel, private_channel, retained_channel])
        );
    }
    #[test]
    fn sixty_fifth_result_is_not_a_complete_empty_or_truncated_inventory() {
        let relay = Keys::generate();
        let agent = Keys::generate().public_key().to_hex();
        let events: Vec<_> = (0..65)
            .map(|_| snapshot(&relay, 39000, &uuid::Uuid::new_v4().to_string(), &[]))
            .collect();
        assert!(covered_channels(&events, &[], &[], &agent).is_err());
        assert_eq!(
            covered_channels(&events[..64], &[], &[], &agent)
                .unwrap()
                .len(),
            64
        );
    }
    #[test]
    fn agent_credentials_do_not_authorize_owner_to_remove_private_membership() {
        let relay = Keys::generate();
        let owner = Keys::generate().public_key().to_hex();
        let agent = Keys::generate().public_key().to_hex();
        let channel = uuid::Uuid::new_v4().to_string();
        let roster = snapshot(&relay, 39002, &channel, &[&agent]);
        assert!(inspect_channel(&channel, &roster, None, &owner, &agent).is_err());
        let accessible = snapshot(&relay, 39002, &channel, &[&agent, &owner]);
        assert!(
            inspect_channel(&channel, &accessible, None, &owner, &agent)
                .unwrap()
                .membership_present
        );
        // Revocation observed before local commit returns the same explicit refusal.
        assert!(inspect_channel(&channel, &roster, None, &owner, &agent).is_err());
    }
    #[test]
    fn unrelated_agent_roster_cannot_satisfy_coverage() {
        let relay = Keys::generate();
        let agent = Keys::generate().public_key().to_hex();
        let channel = uuid::Uuid::new_v4().to_string();
        assert!(
            covered_channels(&[], &[snapshot(&relay, 39002, &channel, &[])], &[], &agent).is_err()
        );
    }
}
