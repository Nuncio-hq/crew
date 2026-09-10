use std::collections::BTreeSet;

use buzz_core_pkg::crew_role::CrewConfigDraft;
use nostr::{Event, Keys};

use super::record::{Outcome, Payload};

#[derive(Debug)]
pub(super) enum Preparation {
    Unchanged(Option<String>),
    Conflict(Option<String>),
    ReviewRequired(String),
    Ready(Box<Payload>),
}

pub(super) struct Input<'a> {
    pub keys: &'a Keys,
    pub channel_id: &'a str,
    pub relay_url: &'a str,
    pub expected_head: Option<&'a str>,
    pub current: Option<&'a Event>,
    pub known_members: &'a BTreeSet<String>,
    pub now: i64,
}

pub(super) fn prepare(input: Input<'_>, draft: &CrewConfigDraft) -> Result<Preparation, String> {
    let prepared = prepare_with(input, |content, known| {
        let updated = buzz_core_pkg::crew_role::update_canvas_crew_config(content, draft, 65536)
            .map_err(|error| error.to_string())?;
        for selected in draft.assignments.keys().chain(draft.contact.iter()) {
            let key = nostr::PublicKey::parse(selected.trim())
                .map_err(|_| "invalid selected agent")?
                .to_hex();
            if !known.contains(&key) {
                return Err(
                    "selected agent is not a current channel member; reload the agent list".into(),
                );
            }
        }
        Ok(updated)
    })?;
    Ok(match prepared {
        Preparation::Ready(mut payload) => {
            payload.draft = Some(draft.clone());
            Preparation::Ready(payload)
        }
        other => other,
    })
}

pub(super) fn prepare_cleanup(input: Input<'_>, members: &[String]) -> Result<Preparation, String> {
    let members = canonical_members(members)?;
    validate_head(&input)?;
    let old = input
        .current
        .map(|event| event.content.as_str())
        .unwrap_or("");
    let updated = match buzz_core_pkg::crew_role::remove_canvas_crew_members(old, &members, 65536) {
        Ok(updated) => updated,
        Err(_)
            if input
                .current
                .is_some_and(|event| event.pubkey != input.keys.public_key()) =>
        {
            return Ok(Preparation::ReviewRequired(
                input
                    .current
                    .map(|event| event.id.to_hex())
                    .unwrap_or_default(),
            ))
        }
        Err(error) => return Err(error.to_string()),
    };
    if updated == old {
        return Ok(Preparation::Unchanged(
            input.current.map(|event| event.id.to_hex()),
        ));
    }
    let prepared = prepare_with(input, |_, _| Ok(updated))?;
    Ok(match prepared {
        Preparation::Ready(mut payload) => {
            payload.cleanup_members = Some(members);
            Preparation::Ready(payload)
        }
        other => other,
    })
}

pub(super) fn canonical_members(members: &[String]) -> Result<Vec<String>, String> {
    if members.is_empty() || members.len() > 1024 {
        return Err("cleanup requires a bounded nonempty member set".into());
    }
    members
        .iter()
        .map(|member| {
            nostr::PublicKey::parse(member.trim())
                .map(|key| key.to_hex())
                .map_err(|_| "invalid cleanup member".to_string())
        })
        .collect::<Result<BTreeSet<_>, _>>()
        .map(|members| members.into_iter().collect())
}

fn validate_head(input: &Input<'_>) -> Result<(), String> {
    let channel = uuid::Uuid::parse_str(input.channel_id).map_err(|_| "invalid channel UUID")?;
    if channel.to_string() != input.channel_id {
        return Err("channel UUID must be canonical".into());
    }
    if let Some(event) = input.current {
        event
            .verify()
            .map_err(|_| "invalid current canvas signature")?;
        if event.kind.as_u16() != 40100
            || !event
                .tags
                .iter()
                .any(|tag| tag.as_slice() == ["h", input.channel_id])
        {
            return Err("canvas response does not match the requested channel".into());
        }
    }
    Ok(())
}

fn prepare_with(
    input: Input<'_>,
    update: impl FnOnce(&str, &BTreeSet<String>) -> Result<String, String>,
) -> Result<Preparation, String> {
    validate_head(&input)?;
    let channel = uuid::Uuid::parse_str(input.channel_id).map_err(|_| "invalid channel UUID")?;
    let current_id = input.current.map(|event| event.id.to_hex());
    if current_id.as_deref() != input.expected_head {
        return Ok(Preparation::Conflict(current_id));
    }
    if let Some(event) = input.current {
        event
            .verify()
            .map_err(|_| "invalid current canvas signature")?;
        if event.kind.as_u16() != 40100
            || !event
                .tags
                .iter()
                .any(|tag| tag.as_slice() == ["h", input.channel_id])
        {
            return Err("canvas response does not match the requested channel".into());
        }
        if event.pubkey != input.keys.public_key() {
            return Ok(Preparation::ReviewRequired(event.id.to_hex()));
        }
    }
    let old = input
        .current
        .map(|event| event.content.as_str())
        .unwrap_or("");
    let updated = update(old, input.known_members)?;
    let time = u64::try_from(input.now).map_err(|_| "invalid save clock")?;
    // Never advance the clock to overtake a future-dated remote canvas.
    let canvas = crate::events::build_set_canvas(channel, &updated)?
        .custom_created_at(nostr::Timestamp::from(time))
        .sign_with_keys(input.keys)
        .map_err(|_| "could not sign canvas")?;
    let notice = announcement(old, &updated, &canvas.id.to_hex())?;
    let announcement = crate::events::build_message(
        channel,
        &notice,
        None,
        &[],
        &[],
        &[],
        &[],
        &[],
        None,
        input.relay_url,
    )?
    .custom_created_at(nostr::Timestamp::from(time))
    .sign_with_keys(input.keys)
    .map_err(|_| "could not sign canvas announcement")?;
    Ok(Preparation::Ready(Box::new(Payload {
        version: 1,
        relay_url: input.relay_url.to_string(),
        channel_id: channel.to_string(),
        expected_head: current_id,
        canvas,
        announcement,
        canvas_attempted: false,
        canvas_acknowledged: false,
        announcement_attempted: false,
        announcement_acknowledged: false,
        failures: 0,
        next_retry_at: None,
        lease: None,
        outcome: Outcome::NotCommitted,
        draft: None,
        cleanup_members: None,
    })))
}

fn announcement(old: &str, updated: &str, id: &str) -> Result<String, String> {
    use buzz_core_pkg::crew_role::parse_canvas_assignments;
    let before = parse_canvas_assignments(old).map_err(|error| error.to_string())?;
    let after = parse_canvas_assignments(updated).map_err(|error| error.to_string())?;
    let empty = std::collections::BTreeMap::new();
    let count = |before: &std::collections::BTreeMap<String, String>,
                 after: &std::collections::BTreeMap<String, String>| {
        let added = after
            .keys()
            .filter(|key| !before.contains_key(*key))
            .count();
        let removed = before
            .keys()
            .filter(|key| !after.contains_key(*key))
            .count();
        let changed = after
            .iter()
            .filter(|(key, value)| before.get(*key).is_some_and(|old| old != *value))
            .count();
        format!("{added} added, {changed} changed, {removed} removed")
    };
    let roles = count(
        before
            .as_ref()
            .map(|block| &block.definitions)
            .unwrap_or(&empty),
        after
            .as_ref()
            .map(|block| &block.definitions)
            .unwrap_or(&empty),
    );
    let assignments = count(
        before
            .as_ref()
            .map(|block| &block.assignments)
            .unwrap_or(&empty),
        after
            .as_ref()
            .map(|block| &block.assignments)
            .unwrap_or(&empty),
    );
    let old_contact = before
        .as_ref()
        .and_then(|block| block.contact_pubkey.as_deref());
    let new_contact = after
        .as_ref()
        .and_then(|block| block.contact_pubkey.as_deref());
    let contact = if old_contact == new_contact {
        "unchanged".to_string()
    } else {
        new_contact
            .map(|key| format!("selected {key}"))
            .unwrap_or_else(|| "cleared".into())
    };
    Ok(format!("AGENT-WORKING-AGREEMENT: configuration snapshot recorded. Canvas: {id}\nRoles: {roles}. Assignments: {assignments}. Contact: {contact}.\nExisting sessions keep their current configuration until explicitly restarted."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_core_pkg::crew_role::CrewRoleDefinition;

    #[test]
    fn channel_crew_prepare_absent_expectation_never_overwrites_an_existing_head() {
        let keys = Keys::generate();
        let channel = uuid::Uuid::new_v4();
        let event = crate::events::build_set_canvas(channel, "old")
            .unwrap()
            .sign_with_keys(&keys)
            .unwrap();
        let known = BTreeSet::new();
        let input = Input {
            keys: &keys,
            channel_id: &channel.to_string(),
            relay_url: "http://fixture.invalid",
            expected_head: None,
            current: Some(&event),
            known_members: &known,
            now: 1000,
        };
        assert!(
            matches!(prepare(input,&CrewConfigDraft::default()).unwrap(),Preparation::Conflict(Some(id)) if id==event.id.to_hex())
        );
    }

    #[test]
    fn channel_crew_prepare_foreign_head_requires_review_without_resigning() {
        let keys = Keys::generate();
        let channel = uuid::Uuid::new_v4();
        let event = crate::events::build_set_canvas(channel, "foreign")
            .unwrap()
            .sign_with_keys(&Keys::generate())
            .unwrap();
        let known = BTreeSet::new();
        let id = event.id.to_hex();
        let input = Input {
            keys: &keys,
            channel_id: &channel.to_string(),
            relay_url: "http://fixture.invalid",
            expected_head: Some(&id),
            current: Some(&event),
            known_members: &known,
            now: 1000,
        };
        assert!(
            matches!(prepare(input,&CrewConfigDraft::default()).unwrap(),Preparation::ReviewRequired(current) if current==id)
        );
    }

    #[test]
    fn channel_crew_prepare_validates_agent_membership_before_producing_events() {
        let keys = Keys::generate();
        let channel = uuid::Uuid::new_v4().to_string();
        let agent = Keys::generate().public_key().to_hex();
        let known = BTreeSet::new();
        let draft = CrewConfigDraft {
            definitions: vec![CrewRoleDefinition {
                label: "Review".into(),
                definition: "Inspect".into(),
            }],
            assignments: [(agent.clone(), "Review".into())].into(),
            contact: Some(agent.clone()),
            ..Default::default()
        };
        let input = |known| Input {
            keys: &keys,
            channel_id: &channel,
            relay_url: "http://fixture.invalid",
            expected_head: None,
            current: None,
            known_members: known,
            now: 1000,
        };
        assert!(prepare(input(&known), &draft).is_err());
        let known = [agent].into();
        let Preparation::Ready(payload) = prepare(input(&known), &draft).unwrap() else {
            panic!("ready")
        };
        assert_eq!(
            serde_json::to_value(payload.draft.as_ref()).unwrap(),
            serde_json::to_value(Some(&draft)).unwrap()
        );
        assert_eq!(payload.canvas.created_at.as_secs(), 1000);
        assert_eq!(payload.canvas.pubkey, keys.public_key());
        assert_eq!(payload.announcement.kind.as_u16(), 9);
        assert!(payload
            .announcement
            .content
            .contains(&payload.canvas.id.to_hex()));
        assert_eq!(payload.announcement.tags.len(), 1);
    }
}
