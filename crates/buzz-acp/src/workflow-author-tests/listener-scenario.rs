struct ListenerBoundaryScenario<'a> {
    listener: ListenerBoundary,
    relay_keys: &'a nostr::Keys,
    workflow_owner: &'a str,
    responses: std::collections::VecDeque<Result<serde_json::Value, ()>>,
    event_generation: u64,
    channel_type: &'a str,
    respond_to: RespondTo,
    allowlist: HashSet<String>,
    cache_owner: bool,
    cache_sibling: bool,
}

async fn listener_boundary_scenario(
    scenario: ListenerBoundaryScenario<'_>,
) -> (Option<String>, bool) {
    let ListenerBoundaryScenario {
        listener,
        relay_keys,
        workflow_owner,
        responses,
        event_generation,
        channel_type,
        respond_to,
        allowlist,
        cache_owner,
        cache_sibling,
    } = scenario;
    let relay_hex = relay_keys.public_key().to_hex();
    let agent = nostr::Keys::generate().public_key().to_hex();
    let (rest_client, server) = nip11_scripted_server(responses).await;
    let mut gate = InboundAuthorGate::connect(&rest_client, &agent, "listener startup").await;
    let configured_owner = if cache_owner {
        Some(workflow_owner.to_string())
    } else if cache_sibling {
        Some(nostr::Keys::generate().public_key().to_hex())
    } else {
        None
    };
    let owner_cache = OwnerCache::new(configured_owner);
    owner_cache.cache_sibling(relay_hex, false);
    owner_cache.cache_sibling(workflow_owner.to_string(), cache_sibling);
    let channel_id = Uuid::new_v4();
    let channel_info = pool::ChannelInfoResolver::new(
        HashMap::from([(
            channel_id,
            relay::ChannelInfo {
                name: "workflow".into(),
                channel_type: channel_type.into(),
                description: None,
            },
        )]),
        rest_client.clone(),
    );
    let event = relay::BuzzEvent {
        connection_generation: event_generation,
        channel_id,
        event: relay_signed_workflow_dispatch(relay_keys, workflow_owner, &agent),
    };
    let authorized = match listener {
        ListenerBoundary::Normal => {
            authorize_normal_listener_event(
                &mut gate,
                event,
                &respond_to,
                &allowlist,
                &owner_cache,
                &channel_info,
                &rest_client,
            )
            .await
        }
        ListenerBoundary::Setup => {
            setup_mode::authorize_setup_listener_event(
                &mut gate,
                event,
                &respond_to,
                &allowlist,
                &owner_cache,
                &channel_info,
                &rest_client,
            )
            .await
        }
    };
    let result = authorized.map(|event| event.into_parts().1);
    server.abort();
    let allowed = result.is_some();
    (result, allowed)
}

#[derive(Clone, Copy, Debug)]
enum ListenerBoundary {
    Normal,
    Setup,
}

impl ListenerBoundary {
    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Setup => "setup",
        }
    }
}

// Both production listener callables must attribute relay-signed workflow
// events to the workflow owner and enforce policy there. A local
// `allowed: true` replacement at either call site makes the Nobody case
// fail; using the raw relay signer makes the OwnerOnly case fail.
