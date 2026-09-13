//! Exercise the production query plan against a loopback relay with signed fixtures.
use super::*;
use axum::{
    routing::{get, post},
    Json, Router,
};
use nostr::{EventBuilder, Keys, Kind, Tag};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

#[tokio::test]
async fn remote_owned_discovery_and_membership_do_not_require_local_records() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    crate::relay_admission::reset_rate_limit_gate();
    let relay = Keys::generate();
    let owner = Keys::generate();
    let agent = Keys::generate();
    let stranger = Keys::generate();
    let agent_key = agent.public_key().to_hex();
    let owner_key = owner.public_key().to_hex();
    let relay_key = relay.public_key().to_hex();
    let auth = buzz_sdk_pkg::nip_oa::compute_auth_tag(&owner, &agent.public_key(), "").unwrap();
    let auth: Vec<String> = serde_json::from_str(&auth).unwrap();
    let profile = EventBuilder::new(Kind::Metadata, r#"{"display_name":"Remote Scout"}"#)
        .tags([Tag::parse(auth).unwrap()])
        .sign_with_keys(&agent)
        .unwrap();
    let policy = |key: &str| {
        EventBuilder::new(
            Kind::Custom(30177),
            r#"{"name":"Remote Scout","parallelism":1,"respond_to":"owner-only"}"#,
        )
        .tags([Tag::parse(["d", key]).unwrap()])
        .sign_with_keys(&owner)
        .unwrap()
    };
    // An owner-authored coordinate is a discovery hint, not ownership proof.
    let forged = policy(&stranger.public_key().to_hex());
    let stranger_profile = EventBuilder::new(Kind::Metadata, "{}")
        .sign_with_keys(&stranger)
        .unwrap();
    let events = Arc::new(Mutex::new(vec![
        profile,
        policy(&agent_key),
        forged,
        stranger_profile,
    ]));
    let queries = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let query_events = events.clone();
    let query_log = queries.clone();
    let router = Router::new()
        .route(
            "/",
            get(move || {
                let key = relay_key.clone();
                async move { Json(serde_json::json!({"self": key})) }
            }),
        )
        .route(
            "/query",
            post(move |Json(filters): Json<Vec<serde_json::Value>>| {
                let events = query_events.clone();
                let queries = query_log.clone();
                async move {
                    queries.lock().unwrap().extend(filters.clone());
                    let events = events.lock().unwrap();
                    let result: Vec<_> = events
                        .iter()
                        .filter(|event| {
                            filters.iter().any(|filter| {
                                filter["kinds"]
                                    .as_array()
                                    .unwrap()
                                    .contains(&serde_json::json!(event.kind.as_u16()))
                                    && filter.get("authors").is_none_or(|authors| {
                                        authors
                                            .as_array()
                                            .unwrap()
                                            .contains(&serde_json::json!(event.pubkey.to_hex()))
                                    })
                                    && ["d", "p"].iter().all(|tag| {
                                        filter.get(format!("#{tag}")).is_none_or(|values| {
                                            event.tags.iter().any(|t| {
                                                t.as_slice().first().map(String::as_str)
                                                    == Some(*tag)
                                                    && t.as_slice().get(1).is_some_and(|value| {
                                                        values
                                                            .as_array()
                                                            .unwrap()
                                                            .contains(&serde_json::json!(value))
                                                    })
                                            })
                                        })
                                    })
                            })
                        })
                        .cloned()
                        .collect();
                    Json(result)
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let state = crate::app_state::build_app_state();
    *state.keys.lock().unwrap() = owner.clone();
    *state.relay_url_override.lock().unwrap() = Some(format!("ws://{address}"));

    let discovered = list_relay_agents_for_state(&state).await.unwrap();
    assert_eq!(discovered.len(), 1, "forged ownership must not be admitted");
    assert_eq!(discovered[0].pubkey, agent_key);
    assert_eq!(
        discovered[0].owner_pubkey.as_deref(),
        Some(owner_key.as_str())
    );
    assert!(
        discovered[0].channel_ids.is_empty(),
        "discovery is not membership"
    );

    let membership = EventBuilder::new(Kind::Custom(39002), "")
        .tags([
            Tag::parse(["d", "general"]).unwrap(),
            Tag::parse(["p", &owner_key, "", "member"]).unwrap(),
            Tag::parse(["p", &agent_key, "", "member"]).unwrap(),
        ])
        .sign_with_keys(&relay)
        .unwrap();
    events.lock().unwrap().push(membership);
    let requested = std::collections::HashSet::from([agent_key.clone()]);
    let admitted = list_relay_agents_for_selection(&state, Some(&requested), Some("general"))
        .await
        .unwrap();
    assert_eq!(admitted.len(), 1);
    assert_eq!(admitted[0].channel_ids, vec!["general".to_string()]);
    let outside = list_relay_agents_for_selection(&state, Some(&requested), Some("private-other"))
        .await
        .unwrap();
    assert_eq!(outside.len(), 1);
    assert!(
        outside[0].channel_ids.is_empty(),
        "ownership cannot fabricate destination membership"
    );
    // A newer signed snapshot revokes membership, even if an old snapshot
    // is also returned. The owned identity remains discoverable, not admitted.
    let removed = EventBuilder::new(Kind::Custom(39002), "")
        .tags([
            Tag::parse(["d", "general"]).unwrap(),
            Tag::parse(["p", &owner_key, "", "member"]).unwrap(),
        ])
        .custom_created_at(nostr::Timestamp::from(
            nostr::Timestamp::now().as_secs() + 1,
        ))
        .sign_with_keys(&relay)
        .unwrap();
    events.lock().unwrap().push(removed);
    let revoked = list_relay_agents_for_selection(&state, Some(&requested), Some("general"))
        .await
        .unwrap();
    assert!(revoked[0].channel_ids.is_empty());

    let deny = EventBuilder::new(
        Kind::Custom(30177),
        r#"{"name":"Remote Scout","parallelism":1,"respond_to":"nobody"}"#,
    )
    .tags([Tag::parse(["d", &agent_key]).unwrap()])
    .custom_created_at(nostr::Timestamp::from(
        nostr::Timestamp::now().as_secs() + 2,
    ))
    .sign_with_keys(&owner)
    .unwrap();
    events.lock().unwrap().push(deny);
    let denied = list_relay_agents_for_selection(&state, Some(&requested), Some("general"))
        .await
        .unwrap();
    assert!(
        denied.is_empty(),
        "latest unsupported policy cannot fall back to an older allow"
    );

    assert!(queries
        .lock()
        .unwrap()
        .iter()
        .any(|filter| filter["kinds"] == serde_json::json!([30177])
            && filter["authors"] == serde_json::json!([owner_key])
            && filter.get("#d").is_none()));
    server.abort();
    crate::relay_admission::reset_rate_limit_gate();
}

#[tokio::test]
async fn stopped_bot_membership_is_eligible_without_directory_projection() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    crate::relay_admission::reset_rate_limit_gate();
    let relay = Keys::generate();
    let owner = Keys::generate();
    let agent = Keys::generate();
    let known = Keys::generate();
    let nonbot = Keys::generate();
    let forged = Keys::generate();
    let relay_key = relay.public_key().to_hex();
    let owner_key = owner.public_key().to_hex();
    let agent_key = agent.public_key().to_hex();
    let known_key = known.public_key().to_hex();
    let nonbot_key = nonbot.public_key().to_hex();
    let forged_key = forged.public_key().to_hex();

    let current = EventBuilder::new(Kind::Custom(39002), "")
        .tags([
            Tag::parse(["d", "general"]).unwrap(),
            Tag::parse(["p", &owner_key, "", "member"]).unwrap(),
            Tag::parse(["p", &agent_key, "", "bot"]).unwrap(),
            Tag::parse(["p", &known_key, "", "member"]).unwrap(),
            Tag::parse(["p", &nonbot_key, "", "member"]).unwrap(),
        ])
        .sign_with_keys(&relay)
        .unwrap();
    let known_policy = EventBuilder::new(
        Kind::Custom(30177),
        r#"{"name":"Known Agent","parallelism":1,"respond_to":"owner-only"}"#,
    )
    .tags([Tag::parse(["d", &known_key]).unwrap()])
    .sign_with_keys(&owner)
    .unwrap();
    let forged_membership = EventBuilder::new(Kind::Custom(39002), "")
        .tags([
            Tag::parse(["d", "general"]).unwrap(),
            Tag::parse(["p", &forged_key, "", "bot"]).unwrap(),
        ])
        .sign_with_keys(&forged)
        .unwrap();
    let departed_before = EventBuilder::new(Kind::Custom(39002), "")
        .tags([
            Tag::parse(["d", "departed"]).unwrap(),
            Tag::parse(["p", &agent_key, "", "bot"]).unwrap(),
        ])
        .custom_created_at(nostr::Timestamp::from(1))
        .sign_with_keys(&relay)
        .unwrap();
    let departed_after = EventBuilder::new(Kind::Custom(39002), "")
        .tags([
            Tag::parse(["d", "departed"]).unwrap(),
            Tag::parse(["p", &owner_key, "", "member"]).unwrap(),
        ])
        .custom_created_at(nostr::Timestamp::from(2))
        .sign_with_keys(&relay)
        .unwrap();
    // Deliberately return no kind:0 or 10100 runtime projections. The signed
    // membership event establishes the stopped bot; the owner-signed policy
    // establishes the known identity whose cosmetic role is not bot.
    let events = Arc::new(Mutex::new(vec![
        current,
        known_policy,
        forged_membership,
        departed_before,
        departed_after,
    ]));
    let query_events = events.clone();
    let router = Router::new()
        .route(
            "/",
            get(move || {
                let relay_key = relay_key.clone();
                async move { Json(serde_json::json!({"self": relay_key})) }
            }),
        )
        .route(
            "/query",
            post(move || {
                let events = query_events.clone();
                async move { Json(events.lock().unwrap().clone()) }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let state = crate::app_state::build_app_state();
    *state.keys.lock().unwrap() = owner;
    *state.relay_url_override.lock().unwrap() = Some(format!("ws://{address}"));

    let forged_result =
        revalidate_channel_bot_members(std::slice::from_ref(&forged_key), "general", &state)
            .await
            .unwrap();
    assert!(
        forged_result.is_empty(),
        "forged membership must be rejected"
    );

    let nonbot_result =
        revalidate_channel_bot_members(std::slice::from_ref(&nonbot_key), "general", &state)
            .await
            .unwrap();
    assert!(
        nonbot_result.is_empty(),
        "non-bot membership must be rejected"
    );

    let departed_result =
        revalidate_channel_bot_members(std::slice::from_ref(&agent_key), "departed", &state)
            .await
            .unwrap();
    assert!(
        departed_result.is_empty(),
        "a newer relay membership snapshot must revoke the agent"
    );

    let stopped_result =
        revalidate_channel_bot_members(std::slice::from_ref(&agent_key), "general", &state)
            .await
            .unwrap();
    assert_eq!(
        stopped_result,
        std::collections::HashSet::from([agent_key.clone()]),
        "a signed bot member must not require a runtime directory projection"
    );

    let known_result =
        revalidate_channel_bot_members(std::slice::from_ref(&known_key), "general", &state)
            .await
            .unwrap();
    assert_eq!(
        known_result,
        std::collections::HashSet::from([known_key]),
        "a known owner-authenticated identity may use a cosmetic non-bot role"
    );

    server.abort();
    crate::relay_admission::reset_rate_limit_gate();
}

#[tokio::test]
async fn directory_deadline_cancels_delayed_policy_fanout_without_partial_success() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    crate::relay_admission::reset_rate_limit_gate();
    let relay = Keys::generate();
    let owner = Keys::generate();
    let agent = Keys::generate();
    let agent_key = agent.public_key().to_hex();
    let auth = buzz_sdk_pkg::nip_oa::compute_auth_tag(&owner, &agent.public_key(), "").unwrap();
    let auth: Vec<String> = serde_json::from_str(&auth).unwrap();
    let profile = EventBuilder::new(Kind::Metadata, r#"{"display_name":"Deadline Scout"}"#)
        .tags([Tag::parse(auth).unwrap()])
        .sign_with_keys(&agent)
        .unwrap();
    let events = Arc::new(Mutex::new(vec![profile]));
    let delayed_policy_queries = Arc::new(AtomicUsize::new(0));
    let query_events = events.clone();
    let query_delays = delayed_policy_queries.clone();
    let relay_key = relay.public_key().to_hex();
    let router = Router::new()
        .route(
            "/",
            get(move || {
                let relay_key = relay_key.clone();
                async move { Json(serde_json::json!({"self": relay_key})) }
            }),
        )
        .route(
            "/query",
            post(move |Json(filters): Json<Vec<serde_json::Value>>| {
                let events = query_events.clone();
                let query_delays = query_delays.clone();
                async move {
                    let delayed_policy = filters.iter().any(|filter| {
                        filter["kinds"] == serde_json::json!([30177])
                            && filter["limit"] == serde_json::json!(1)
                            && filter.get("#d").is_some()
                    });
                    if delayed_policy {
                        query_delays.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                    let events = events.lock().unwrap();
                    let result: Vec<_> = events
                        .iter()
                        .filter(|event| {
                            filters.iter().any(|filter| {
                                filter["kinds"].as_array().is_some_and(|kinds| {
                                    kinds.contains(&serde_json::json!(event.kind.as_u16()))
                                }) && filter.get("authors").is_none_or(|authors| {
                                    authors.as_array().is_some_and(|authors| {
                                        authors.contains(&serde_json::json!(event.pubkey.to_hex()))
                                    })
                                })
                            })
                        })
                        .cloned()
                        .collect();
                    Json(result)
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let state = crate::app_state::build_app_state();
    *state.keys.lock().unwrap() = owner;
    *state.relay_url_override.lock().unwrap() = Some(format!("ws://{address}"));
    let requested = std::collections::HashSet::from([agent_key]);

    let result = list_relay_agents_for_selection_with_timeout(
        &state,
        Some(&requested),
        Some("general"),
        std::time::Duration::from_millis(500),
    )
    .await;

    let error = result.expect_err("a delayed policy fanout must not return partial agents");
    assert_eq!(error, "relay agent directory lookup timed out");
    assert_eq!(
        delayed_policy_queries.load(Ordering::SeqCst),
        1,
        "the timed-out policy request started exactly once"
    );
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(
        delayed_policy_queries.load(Ordering::SeqCst),
        1,
        "cancellation must prevent any next policy/fanout request"
    );

    server.abort();
    crate::relay_admission::reset_rate_limit_gate();
}
