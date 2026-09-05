#[tokio::test]
async fn production_listener_boundaries_recover_relay_identity() {
    for listener in [ListenerBoundary::Normal, ListenerBoundary::Setup] {
        let relay_keys = nostr::Keys::generate();
        let relay_hex = relay_keys.public_key().to_hex();
        let workflow_owner = nostr::Keys::generate().public_key().to_hex();
        let result = listener_boundary_scenario(ListenerBoundaryScenario {
            listener,
            relay_keys: &relay_keys,
            workflow_owner: &workflow_owner,
            responses: std::collections::VecDeque::from([
                Err(()),
                Err(()),
                Ok(serde_json::json!({ "self": relay_hex })),
            ]),
            event_generation: 0,
            channel_type: "stream",
            respond_to: RespondTo::OwnerOnly,
            allowlist: HashSet::new(),
            cache_owner: true,
            cache_sibling: false,
        })
        .await;
        assert!(
            result.1,
            "{} listener must recover identity before authorization",
            listener.name()
        );
        assert_eq!(
            result.0.as_deref(),
            Some(workflow_owner.as_str()),
            "{} listener must preserve the recovered workflow owner",
            listener.name()
        );
    }
}

/// The listener decision-boundary regression.
///
/// Both listeners call `evaluate_listener_event`; it owns identity refresh,
/// channel trust, workflow attribution, and policy, with no production-visible
/// raw-policy helper alongside it. This test drives that exact callable
/// against a live NIP-11 document, so it fails if identity loading,
/// effective-author resolution, DM classification, or policy application
/// regresses. Replacing either listener call with the former raw-signer
/// `author_allowed` path is now a compile error because that policy is
/// private to the gate module.
#[tokio::test]
async fn test_connected_gate_wakes_owner_only_agent_for_relay_signed_workflow() {
    let relay_keys = nostr::Keys::generate();
    let relay_hex = relay_keys.public_key().to_hex();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let (rest_client, server) = nip11_server(serde_json::json!({ "self": relay_hex })).await;

    let mut gate = InboundAuthorGate::connect(&rest_client, &agent, "test").await;
    assert!(
        gate.has_relay_identity(),
        "the gate must load the relay signing identity during construction"
    );

    let event = relay_signed_workflow_dispatch(&relay_keys, &workflow_owner, &agent);
    let cache = cache_with_sibling();
    cache.cache_sibling(workflow_owner.clone(), true);
    cache.cache_sibling(relay_hex.clone(), false);

    let channel_id = Uuid::new_v4();
    let channel_info = pool::ChannelInfoResolver::new(
        HashMap::from([(
            channel_id,
            relay::ChannelInfo {
                name: "workflow".into(),
                channel_type: "stream".into(),
                description: None,
            },
        )]),
        rest_client.clone(),
    );
    let buzz_event = relay::BuzzEvent {
        connection_generation: 0,
        channel_id,
        event,
    };
    let decision = gate
        .evaluate_listener_event(
            &buzz_event,
            &RespondTo::OwnerOnly,
            &HashSet::new(),
            &cache,
            &channel_info,
            &rest_client,
        )
        .await;

    assert_eq!(
        decision.effective_author, workflow_owner,
        "a connected gate must attribute a relay-signed workflow dispatch to its owner, not the relay signer"
    );
    assert!(
        decision.allowed,
        "an owner-only agent must wake for its own workflow's explicit mention"
    );
    server.abort();
}

/// A gate whose relay identity is unavailable must fall back to the raw
/// signer and stay closed — the documented fail-closed behavior, and the
/// exact state the wiring regression above proves the listeners avoid.
#[tokio::test]
async fn test_gate_without_relay_identity_fails_closed_to_raw_signer() {
    let relay_keys = nostr::Keys::generate();
    let relay_hex = relay_keys.public_key().to_hex();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    // A NIP-11 document with no `self` key: attribution is unavailable.
    let (rest_client, server) = nip11_server(serde_json::json!({ "name": "relay" })).await;

    let gate = InboundAuthorGate::connect(&rest_client, &agent, "test").await;
    assert!(
        !gate.has_relay_identity(),
        "a NIP-11 document without `self` must leave attribution unavailable"
    );

    let event = relay_signed_workflow_dispatch(&relay_keys, &workflow_owner, &agent);
    let cache = cache_with_sibling();
    cache.cache_sibling(workflow_owner, true);
    cache.cache_sibling(relay_hex.clone(), false);

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

    assert_eq!(
        decision.effective_author, relay_hex,
        "without a verified relay identity the gate must fall back to the raw signer"
    );
    assert!(
        !decision.allowed,
        "unattributed relay-signed output must not wake an owner-only agent"
    );
    server.abort();
}

// The first authorized event after reconnect must restore attribution
// through the same decision boundary both listeners use, without a
// separate identity-refresh call.
