#[tokio::test]
async fn production_listener_boundaries_apply_workflow_owner_policy() {
    for listener in [ListenerBoundary::Normal, ListenerBoundary::Setup] {
        let relay_keys = nostr::Keys::generate();
        let relay_hex = relay_keys.public_key().to_hex();
        let accepted_workflow_owner = nostr::Keys::generate().public_key().to_hex();
        let accepted = listener_boundary_scenario(ListenerBoundaryScenario {
            listener,
            relay_keys: &relay_keys,
            workflow_owner: &accepted_workflow_owner,
            responses: std::collections::VecDeque::from([Ok(
                serde_json::json!({ "self": relay_hex }),
            )]),
            event_generation: 0,
            channel_type: "stream",
            respond_to: RespondTo::OwnerOnly,
            allowlist: HashSet::new(),
            cache_owner: true,
            cache_sibling: false,
        })
        .await;
        assert!(
            accepted.1,
            "{} listener must allow the workflow owner",
            listener.name()
        );
        assert_eq!(
            accepted.0.as_deref(),
            Some(accepted_workflow_owner.as_str()),
            "{} listener must preserve the effective workflow owner",
            listener.name()
        );

        let relay_keys = nostr::Keys::generate();
        let relay_hex = relay_keys.public_key().to_hex();
        let denied_workflow_owner = nostr::Keys::generate().public_key().to_hex();
        let denied = listener_boundary_scenario(ListenerBoundaryScenario {
            listener,
            relay_keys: &relay_keys,
            workflow_owner: &denied_workflow_owner,
            responses: std::collections::VecDeque::from([Ok(
                serde_json::json!({ "self": relay_hex }),
            )]),
            event_generation: 0,
            channel_type: "stream",
            respond_to: RespondTo::Nobody,
            allowlist: HashSet::new(),
            cache_owner: true,
            cache_sibling: false,
        })
        .await;
        assert!(
            !denied.1,
            "{} listener must enforce respond-to=nobody",
            listener.name()
        );
    }
}

/// Both production boundaries must retain DM classification when composing
/// trusted workflow attribution with configured author policy. External
/// allowlist entries and `Anyone` stay denied in a DM; owner and sibling
/// principals remain allowed; `Nobody` remains absolute.
#[tokio::test]
async fn production_listener_boundaries_enforce_dm_author_policy() {
    for listener in [ListenerBoundary::Normal, ListenerBoundary::Setup] {
        let relay_keys = nostr::Keys::generate();
        let relay_hex = relay_keys.public_key().to_hex();
        let external = nostr::Keys::generate().public_key().to_hex();
        let external_allowlist = HashSet::from([external.clone()]);
        let denied_external = listener_boundary_scenario(ListenerBoundaryScenario {
            listener,
            relay_keys: &relay_keys,
            workflow_owner: &external,
            responses: std::collections::VecDeque::from([Ok(
                serde_json::json!({ "self": relay_hex }),
            )]),
            event_generation: 0,
            channel_type: "dm",
            respond_to: RespondTo::Allowlist,
            allowlist: external_allowlist,
            cache_owner: false,
            cache_sibling: false,
        })
        .await;
        assert!(
            !denied_external.1,
            "{} listener must deny an external allowlist entry in a DM",
            listener.name()
        );

        let relay_keys = nostr::Keys::generate();
        let relay_hex = relay_keys.public_key().to_hex();
        let stranger = nostr::Keys::generate().public_key().to_hex();
        let denied_stranger = listener_boundary_scenario(ListenerBoundaryScenario {
            listener,
            relay_keys: &relay_keys,
            workflow_owner: &stranger,
            responses: std::collections::VecDeque::from([Ok(
                serde_json::json!({ "self": relay_hex }),
            )]),
            event_generation: 0,
            channel_type: "dm",
            respond_to: RespondTo::Anyone,
            allowlist: HashSet::new(),
            cache_owner: false,
            cache_sibling: false,
        })
        .await;
        assert!(
            !denied_stranger.1,
            "{} listener must deny a stranger in a DM under Anyone",
            listener.name()
        );

        for (principal, cache_owner, cache_sibling, label) in [
            (
                nostr::Keys::generate().public_key().to_hex(),
                true,
                false,
                "owner",
            ),
            (
                nostr::Keys::generate().public_key().to_hex(),
                false,
                true,
                "sibling",
            ),
        ] {
            let relay_keys = nostr::Keys::generate();
            let relay_hex = relay_keys.public_key().to_hex();
            let allowed = listener_boundary_scenario(ListenerBoundaryScenario {
                listener,
                relay_keys: &relay_keys,
                workflow_owner: &principal,
                responses: std::collections::VecDeque::from([Ok(
                    serde_json::json!({ "self": relay_hex }),
                )]),
                event_generation: 0,
                channel_type: "dm",
                respond_to: RespondTo::Anyone,
                allowlist: HashSet::new(),
                cache_owner,
                cache_sibling,
            })
            .await;
            assert!(
                allowed.1,
                "{} listener must allow the {label} in a DM",
                listener.name()
            );
        }

        let relay_keys = nostr::Keys::generate();
        let relay_hex = relay_keys.public_key().to_hex();
        let owner = nostr::Keys::generate().public_key().to_hex();
        let denied_nobody = listener_boundary_scenario(ListenerBoundaryScenario {
            listener,
            relay_keys: &relay_keys,
            workflow_owner: &owner,
            responses: std::collections::VecDeque::from([Ok(
                serde_json::json!({ "self": relay_hex }),
            )]),
            event_generation: 0,
            channel_type: "dm",
            respond_to: RespondTo::Nobody,
            allowlist: HashSet::new(),
            cache_owner: true,
            cache_sibling: false,
        })
        .await;
        assert!(
            !denied_nobody.1,
            "{} listener must enforce Nobody in a DM",
            listener.name()
        );
    }
}

// Both production boundaries must perform the pending generation-zero
// refresh before policy evaluation. Bypassing the gate invocation leaves
// the relay signer denied and makes this recovery assertion fail.
