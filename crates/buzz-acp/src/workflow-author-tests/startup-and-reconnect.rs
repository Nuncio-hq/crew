#[tokio::test]
async fn test_gate_refresh_arms_attribution_after_reconnect() {
    let relay_keys = nostr::Keys::generate();
    let relay_hex = relay_keys.public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();

    // Construct against an unreachable relay: no identity yet.
    let unreachable = relay::RestClient {
        http: reqwest::Client::new(),
        base_url: "http://127.0.0.1:1".into(),
        keys: nostr::Keys::generate(),
        auth_tag_json: None,
    };
    let mut gate = InboundAuthorGate::connect(&unreachable, &agent, "test").await;
    assert!(!gate.has_relay_identity());

    let (rest_client, server) = nip11_server(serde_json::json!({ "self": relay_hex })).await;
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    let event = relay_signed_workflow_dispatch(&relay_keys, &workflow_owner, &agent);
    let cache = cache_with_sibling();
    cache.cache_sibling(workflow_owner.clone(), true);
    cache.cache_sibling(relay_hex, false);
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
        connection_generation: 1,
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
        "a reconnect refresh must restore delegated workflow attribution"
    );
    assert!(decision.allowed);
    server.abort();
}

#[test]
fn refresh_needed_until_generation_completes() {
    use super::inbound_author_gate::refresh_needed;
    assert!(refresh_needed(None, 0));
    assert!(refresh_needed(None, 1));
    assert!(!refresh_needed(Some(0), 0));
    assert!(refresh_needed(Some(0), 1));
    assert!(!refresh_needed(Some(1), 1));
    assert!(!refresh_needed(Some(1), 0));
    assert!(refresh_needed(Some(1), 2));
}

#[tokio::test]
async fn test_generation_zero_retries_failed_startup_identity() {
    let relay_keys = nostr::Keys::generate();
    let relay_hex = relay_keys.public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let workflow_owner = nostr::Keys::generate().public_key().to_hex();
    // Both startup probes fail; HTTP then recovers without a WS reconnect.
    let (rest_client, server) = nip11_scripted_server(std::collections::VecDeque::from([
        Err(()),
        Err(()),
        Ok(serde_json::json!({ "self": relay_hex.clone() })),
    ]))
    .await;
    let mut gate = InboundAuthorGate::connect(&rest_client, &agent, "startup").await;
    assert!(!gate.has_relay_identity());
    let channel_id = Uuid::new_v4();
    let owner_cache = OwnerCache::new(Some(workflow_owner.clone()));
    owner_cache.cache_sibling(relay_hex.clone(), false);
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
    let event = relay::BuzzEvent {
        connection_generation: 0,
        channel_id,
        event: relay_signed_workflow_dispatch(&relay_keys, &workflow_owner, &agent),
    };
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
    server.abort();
    assert!(
        decision.allowed,
        "a generation-0 workflow wake must recover after the startup NIP-11 failure"
    );
    assert_eq!(decision.effective_author, workflow_owner);
}
