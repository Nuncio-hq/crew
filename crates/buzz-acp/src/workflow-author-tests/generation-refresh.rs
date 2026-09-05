#[tokio::test]
async fn test_authoritative_startup_result_completes_generation_zero() {
    let relay_keys = nostr::Keys::generate();
    let next_relay_keys = nostr::Keys::generate();
    let relay_hex = relay_keys.public_key().to_hex();
    let next_relay_hex = next_relay_keys.public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let owner_cache = OwnerCache::new(Some(workflow_owner.clone()));
    owner_cache.cache_sibling(relay_hex.clone(), false);
    owner_cache.cache_sibling(next_relay_hex.clone(), false);
    for identity in [Some(relay_hex.clone()), None] {
        let document = match &identity {
            Some(key) => serde_json::json!({ "self": key }),
            None => serde_json::json!({ "name": "relay without stable identity" }),
        };
        let mut responses = std::collections::VecDeque::from([Ok(document.clone())]);
        if identity.is_none() {
            // A missing `self` probes /info as well as the root.
            responses.push_back(Ok(document));
        }
        responses.push_back(Ok(serde_json::json!({ "self": next_relay_hex.clone() })));
        let (rest_client, server) = nip11_scripted_server(responses).await;
        let mut gate = InboundAuthorGate::connect(&rest_client, &agent, "startup").await;
        assert_eq!(gate.relay_identity_for_test(), identity.as_deref());
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
        let mut event = relay::BuzzEvent {
            connection_generation: 0,
            channel_id,
            event: relay_signed_workflow_dispatch(&relay_keys, &workflow_owner, &agent),
        };
        for _ in 0..2 {
            let decision = gate
                .evaluate_listener_event(
                    &event,
                    &RespondTo::OwnerOnly,
                    &HashSet::new(),
                    &owner_cache,
                    &channel_info,
                    &rest_client,
                )
                .await;
            assert_eq!(decision.allowed, identity.is_some());
            assert_eq!(
                gate.relay_identity_for_test(),
                identity.as_deref(),
                "an authoritative startup response must not be fetched again at generation 0"
            );
        }
        event.connection_generation = 1;
        event.event = relay_signed_workflow_dispatch(&next_relay_keys, &workflow_owner, &agent);
        let decision = gate
            .evaluate_listener_event(
                &event,
                &RespondTo::OwnerOnly,
                &HashSet::new(),
                &owner_cache,
                &channel_info,
                &rest_client,
            )
            .await;
        assert!(decision.allowed);
        assert_eq!(decision.effective_author, workflow_owner);
        assert_eq!(
            gate.relay_identity_for_test(),
            Some(next_relay_hex.as_str()),
            "a later connection must still refresh after authoritative startup"
        );
        server.abort();
    }
}

#[tokio::test]
async fn test_generation_refresh_retries_after_nip11_failure() {
    let old_relay = nostr::Keys::generate();
    let new_relay = nostr::Keys::generate();
    let old_relay_hex = old_relay.public_key().to_hex();
    let new_relay_hex = new_relay.public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let channel_id = uuid::Uuid::new_v4();
    let (rest_client, server) = nip11_scripted_server(std::collections::VecDeque::from([
        Ok(serde_json::json!({ "self": old_relay_hex.clone() })),
        Err(()),
        Err(()),
        Ok(serde_json::json!({ "self": new_relay_hex.clone() })),
    ]))
    .await;
    let mut gate = InboundAuthorGate::connect(&rest_client, &agent, "test").await;
    let owner_cache = OwnerCache::new(Some(workflow_owner.clone()));
    owner_cache.cache_sibling(old_relay_hex.clone(), false);
    owner_cache.cache_sibling(new_relay_hex.clone(), false);
    let channel_info = pool::ChannelInfoResolver::new(
        std::collections::HashMap::from([(
            channel_id,
            relay::ChannelInfo {
                name: "test".into(),
                channel_type: "stream".into(),
                description: None,
            },
        )]),
        rest_client.clone(),
    );

    assert_eq!(gate.relay_identity_for_test(), Some(old_relay_hex.as_str()));

    let new_event = relay::BuzzEvent {
        connection_generation: 2,
        channel_id,
        event: relay_signed_workflow_dispatch(&new_relay, &workflow_owner, &agent),
    };
    let first_new = gate
        .evaluate_listener_event(
            &new_event,
            &RespondTo::OwnerOnly,
            &HashSet::new(),
            &owner_cache,
            &channel_info,
            &rest_client,
        )
        .await;
    assert_eq!(gate.relay_identity_for_test(), Some(old_relay_hex.as_str()));
    assert!(
        !first_new.allowed,
        "the new signer must remain fail-closed while NIP-11 is unavailable"
    );

    let recovered = gate
        .evaluate_listener_event(
            &new_event,
            &RespondTo::OwnerOnly,
            &HashSet::new(),
            &owner_cache,
            &channel_info,
            &rest_client,
        )
        .await;
    assert_eq!(gate.relay_identity_for_test(), Some(new_relay_hex.as_str()));
    assert_eq!(recovered.effective_author, workflow_owner);
    assert!(
        recovered.allowed,
        "a later event on the same connection must use the refreshed relay key"
    );

    let stale_old_event = relay::BuzzEvent {
        connection_generation: 2,
        channel_id,
        event: relay_signed_workflow_dispatch(&old_relay, &workflow_owner, &agent),
    };
    let stale = gate
        .evaluate_listener_event(
            &stale_old_event,
            &RespondTo::OwnerOnly,
            &HashSet::new(),
            &owner_cache,
            &channel_info,
            &rest_client,
        )
        .await;
    assert!(!stale.allowed, "the rotated-away relay key must be evicted");

    server.abort();
}
