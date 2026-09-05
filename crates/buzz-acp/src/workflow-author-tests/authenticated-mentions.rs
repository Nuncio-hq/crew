use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag};

fn workflow_event(
    signer: &Keys,
    owner: Option<&str>,
    marker_tags: &[&[&str]],
    workflow_mentions: &[&[&str]],
    p_tags: &[&str],
) -> nostr::Event {
    let mut tags = Vec::new();
    for marker in marker_tags {
        tags.push(Tag::parse(marker.iter().copied()).expect("workflow marker"));
    }
    if let Some(owner) = owner {
        tags.push(Tag::parse(["buzz:workflow-owner", owner]).expect("workflow owner tag"));
    }
    for mention in workflow_mentions {
        tags.push(Tag::parse(mention.iter().copied()).expect("workflow mention tag"));
    }
    for recipient in p_tags {
        tags.push(Tag::parse(["p", *recipient]).expect("p tag"));
    }
    EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE as u16), "scheduled prompt")
        .tags(tags)
        .sign_with_keys(signer)
        .expect("signed event")
}

#[tokio::test]
async fn relay_identity_refresh_keeps_last_good_key_after_fetch_error() {
    let previous = Keys::generate().public_key().to_hex();
    let client = relay::RestClient {
        http: reqwest::Client::new(),
        base_url: "http://127.0.0.1:0".into(),
        keys: Keys::generate(),
        auth_tag_json: None,
    };

    let (refreshed, completed) = refresh_relay_self(&client, Some(previous.clone()), "test").await;
    assert_eq!(refreshed, Some(previous));
    assert!(!completed);
}

#[test]
fn trusted_relay_workflow_uses_owner_for_explicit_target() {
    let relay = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let agent = Keys::generate().public_key().to_hex();
    let event = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &[&["buzz:workflow-mention", agent.as_str()]],
        &[owner.as_str(), agent.as_str()],
    );

    assert_eq!(
        effective_prompt_author(&event, Some(&relay.public_key().to_hex()), &agent),
        owner
    );
}

#[test]
fn multiple_explicit_targets_each_use_owner() {
    let relay = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let agent_a = Keys::generate().public_key().to_hex();
    let agent_b = Keys::generate().public_key().to_hex();
    let event = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &[
            &["buzz:workflow-mention", agent_a.as_str()],
            &["buzz:workflow-mention", agent_b.as_str()],
        ],
        &[owner.as_str(), agent_a.as_str(), agent_b.as_str()],
    );

    for agent in [&agent_a, &agent_b] {
        assert_eq!(
            effective_prompt_author(&event, Some(&relay.public_key().to_hex()), agent),
            owner
        );
    }
}

#[test]
fn owner_as_explicit_target_uses_owner_without_duplicate_p_tag() {
    let relay = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let event = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &[&["buzz:workflow-mention", owner.as_str()]],
        &[owner.as_str()],
    );

    assert_eq!(
        effective_prompt_author(&event, Some(&relay.public_key().to_hex()), &owner),
        owner
    );
}

#[test]
fn legacy_owner_p_tag_without_explicit_target_keeps_relay_signer() {
    let relay = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let agent = owner.clone();
    let event = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &[],
        &[owner.as_str()],
    );

    assert_eq!(
        effective_prompt_author(&event, Some(&relay.public_key().to_hex()), &agent),
        relay.public_key().to_hex()
    );
}

#[test]
fn p_tag_without_matching_explicit_target_keeps_relay_signer() {
    let relay = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let agent = Keys::generate().public_key().to_hex();
    let other = Keys::generate().public_key().to_hex();
    let event = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &[&["buzz:workflow-mention", other.as_str()]],
        &[owner.as_str(), agent.as_str(), other.as_str()],
    );

    assert_eq!(
        effective_prompt_author(&event, Some(&relay.public_key().to_hex()), &agent),
        relay.public_key().to_hex()
    );
}

#[test]
fn forged_or_tampered_workflow_keeps_raw_signer() {
    let relay = Keys::generate();
    let attacker = Keys::generate();
    let owner = Keys::generate().public_key().to_hex();
    let agent = Keys::generate().public_key().to_hex();
    let mentions = [&["buzz:workflow-mention", agent.as_str()][..]];
    let forged = workflow_event(
        &attacker,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &mentions,
        &[agent.as_str()],
    );
    assert_eq!(
        effective_prompt_author(&forged, Some(&relay.public_key().to_hex()), &agent),
        attacker.public_key().to_hex()
    );

    let mut tampered = workflow_event(
        &relay,
        Some(&owner),
        &[&["buzz:workflow", "true"]],
        &mentions,
        &[agent.as_str()],
    );
    tampered.content = "tampered".into();
    assert_eq!(
        effective_prompt_author(&tampered, Some(&relay.public_key().to_hex()), &agent),
        relay.public_key().to_hex()
    );
}
