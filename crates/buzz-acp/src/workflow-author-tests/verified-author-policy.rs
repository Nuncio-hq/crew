#[tokio::test]
async fn test_combined_gate_accepts_explicit_trusted_workflow_target_only() {
    let relay = nostr::Keys::generate();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let event =
        nostr::EventBuilder::new(nostr::Kind::Custom(KIND_STREAM_MESSAGE as u16), "dispatch")
            .tags([
                nostr::Tag::parse(["buzz:workflow", "true"]).expect("workflow marker"),
                nostr::Tag::parse(["buzz:workflow-owner", workflow_owner.as_str()])
                    .expect("workflow owner tag"),
                nostr::Tag::parse(["buzz:workflow-mention", agent.as_str()])
                    .expect("workflow mention tag"),
                nostr::Tag::parse(["p", agent.as_str()]).expect("recipient tag"),
            ])
            .sign_with_keys(&relay)
            .expect("signed workflow event");
    let cache = cache_with_sibling();
    cache.cache_sibling(workflow_owner.clone(), true);

    let (gate, rest_client, server) = connected_gate(&relay.public_key().to_hex(), &agent).await;
    let decision = gate
        .evaluate_for_test(
            &event,
            &RespondTo::OwnerOnly,
            &HashSet::new(),
            false,
            &cache,
            &rest_client,
        )
        .await;
    assert_eq!(decision.effective_author, workflow_owner);
    assert!(
        decision.allowed,
        "a verified workflow owner for an explicitly targeted agent must flow through the existing sibling policy"
    );
    server.abort();
}

#[tokio::test]
async fn test_combined_gate_rejects_owner_p_tag_without_explicit_workflow_target() {
    let relay = nostr::Keys::generate();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let agent = workflow_owner.clone();
    let event =
        nostr::EventBuilder::new(nostr::Kind::Custom(KIND_STREAM_MESSAGE as u16), "dispatch")
            .tags([
                nostr::Tag::parse(["buzz:workflow", "true"]).expect("workflow marker"),
                nostr::Tag::parse(["buzz:workflow-owner", workflow_owner.as_str()])
                    .expect("workflow owner tag"),
                nostr::Tag::parse(["p", agent.as_str()]).expect("legacy owner p tag"),
            ])
            .sign_with_keys(&relay)
            .expect("signed workflow event");
    let cache = cache_with_sibling();
    cache.cache_sibling(workflow_owner, true);
    cache.cache_sibling(relay.public_key().to_hex(), false);

    let (gate, rest_client, server) = connected_gate(&relay.public_key().to_hex(), &agent).await;
    let decision = gate
        .evaluate_for_test(
            &event,
            &RespondTo::OwnerOnly,
            &HashSet::new(),
            false,
            &cache,
            &rest_client,
        )
        .await;
    server.abort();
    assert_eq!(decision.effective_author, relay.public_key().to_hex());
    assert!(
        !decision.allowed,
        "the legacy owner p tag alone must not wake an agent-owned workflow"
    );
}

#[tokio::test]
async fn test_combined_gate_rejects_forged_workflow_attribution() {
    let relay = nostr::Keys::generate();
    let attacker = nostr::Keys::generate();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let event =
        nostr::EventBuilder::new(nostr::Kind::Custom(KIND_STREAM_MESSAGE as u16), "dispatch")
            .tags([
                nostr::Tag::parse(["buzz:workflow", "true"]).expect("workflow marker"),
                nostr::Tag::parse(["buzz:workflow-owner", workflow_owner.as_str()])
                    .expect("workflow owner tag"),
                nostr::Tag::parse(["buzz:workflow-mention", agent.as_str()])
                    .expect("workflow mention tag"),
                nostr::Tag::parse(["p", agent.as_str()]).expect("recipient tag"),
            ])
            .sign_with_keys(&attacker)
            .expect("signed forged event");
    let cache = cache_with_sibling();
    cache.cache_sibling(workflow_owner, true);
    cache.cache_sibling(attacker.public_key().to_hex(), false);

    let (gate, rest_client, server) = connected_gate(&relay.public_key().to_hex(), &agent).await;
    let decision = gate
        .evaluate_for_test(
            &event,
            &RespondTo::OwnerOnly,
            &HashSet::new(),
            false,
            &cache,
            &rest_client,
        )
        .await;
    server.abort();
    assert_eq!(decision.effective_author, attacker.public_key().to_hex());
    assert!(
        !decision.allowed,
        "an attacker-signed workflow event must not borrow trusted owner authority"
    );
}
