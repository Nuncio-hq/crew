//! #355 counterexamples at the existing receipt predicate; no fallback policy.

use super::receipt_parent_targets_agent;
use nostr::{EventBuilder, Keys, Kind, Tag};

#[test]
fn contact_proof_mentionless_and_other_target_never_authorize_receipt() {
    let owner = Keys::generate();
    let contact = Keys::generate();
    let other = Keys::generate();
    let channel = uuid::Uuid::new_v4().to_string();
    for extra_tags in [
        vec![],
        vec![Tag::parse(["p"]).expect("malformed p fixture")],
        vec![Tag::parse(["p", &other.public_key().to_hex()]).expect("other target")],
    ] {
        let mut tags = vec![Tag::parse(["h", &channel]).expect("channel")];
        tags.extend(extra_tags);
        let original = EventBuilder::new(Kind::Custom(9), "unmentioned contact")
            .tags(tags)
            .sign_with_keys(&owner)
            .expect("signed human original");
        original.verify().expect("valid fixture signature");
        assert!(
            !receipt_parent_targets_agent(&original, &contact.public_key().to_hex()),
            "current receipt predicate must not invent a contact from an absent, malformed, or other p target"
        );
    }
}

#[test]
fn contact_proof_self_target_receipt_ancestry_does_not_prove_human_delegation() {
    let agent = Keys::generate();
    let original = EventBuilder::new(Kind::Custom(9), "agent authored note")
        // The SDK strips self-p tags by default; this fixture deliberately
        // represents a valid signed event from a producer that retains one.
        .allow_self_tagging()
        .tags([
            Tag::parse(["h", &uuid::Uuid::new_v4().to_string()]).expect("channel"),
            Tag::parse(["p", &agent.public_key().to_hex()]).expect("self target"),
        ])
        .sign_with_keys(&agent)
        .expect("signed agent original");
    original.verify().expect("valid fixture signature");
    assert_eq!(original.pubkey, agent.public_key());
    assert!(original.tags.iter().any(|tag| tag.as_slice()[0] == "p"));
    assert!(receipt_parent_targets_agent(
        &original,
        &agent.public_key().to_hex()
    ));
    // This is allowed by the existing explicit-agent flow. The predicate
    // alone says nothing about users.agent_owner_pubkey or human delegation.
}
