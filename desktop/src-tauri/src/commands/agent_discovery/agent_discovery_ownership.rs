use crate::{
    app_state::AppState, managed_agents::RelayAgentInfo, nostr_convert, relay::query_relay,
};
use std::sync::Arc;

pub(crate) const PROFILE_QUERY_BATCH_SIZE: usize = 10;
pub(crate) const PROFILE_QUERY_CONCURRENCY: usize = 8;

pub(crate) fn profile_filters_for_agents(pubkeys: &[String]) -> Vec<serde_json::Value> {
    pubkeys
        .iter()
        .map(|pubkey| serde_json::json!({"authors": [pubkey], "kinds": [0], "limit": 1}))
        .collect()
}

pub(crate) fn retain_agents_allowed_by_build(
    agents: &mut Vec<RelayAgentInfo>,
    require_verified_owner: bool,
) {
    if require_verified_owner {
        agents.retain(|agent| agent.owner_pubkey.is_some());
    }
}

pub(crate) async fn apply_verified_agent_owner_fields(
    state: &AppState,
    agents: &mut [RelayAgentInfo],
) -> Result<(), String> {
    if agents.is_empty() {
        return Ok(());
    }
    let pubkeys = agents
        .iter()
        .map(|agent| agent.pubkey.clone())
        .collect::<Vec<_>>();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(PROFILE_QUERY_CONCURRENCY));
    let profile_queries = pubkeys.chunks(PROFILE_QUERY_BATCH_SIZE).map(|batch| {
        let filters = profile_filters_for_agents(batch);
        let semaphore = Arc::clone(&semaphore);
        async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|error| format!("profile query semaphore closed: {error}"))?;
            query_relay(state, &filters).await
        }
    });
    let profile_events = futures_util::future::try_join_all(profile_queries)
        .await?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let owners = nostr_convert::verified_agent_owners_from_profiles(&profile_events);
    for agent in agents {
        agent.owner_pubkey = owners.get(&agent.pubkey).cloned();
    }
    Ok(())
}

pub(crate) async fn list_relay_agents_inner(
    state: &AppState,
) -> Result<Vec<RelayAgentInfo>, String> {
    let events = query_relay(state, &[serde_json::json!({"kinds": [10100]})]).await?;
    let value = nostr_convert::agents_from_events(&events);
    let mut agents: Vec<RelayAgentInfo> = serde_json::from_value(
        value
            .get("agents")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )
    .map_err(|e| format!("agent parse failed: {e}"))?;
    apply_verified_agent_owner_fields(state, &mut agents).await?;
    retain_agents_allowed_by_build(
        &mut agents,
        crate::managed_agents::owner_only_access_build(),
    );
    Ok(agents)
}

#[cfg(test)]
mod tests {
    use super::{profile_filters_for_agents, retain_agents_allowed_by_build};
    use crate::managed_agents::RelayAgentInfo;

    #[test]
    fn profile_queries_use_one_exact_author_filter_per_agent() {
        let pubkeys = vec!["agent-a".to_string(), "agent-b".to_string()];
        assert_eq!(
            profile_filters_for_agents(&pubkeys),
            vec![
                serde_json::json!({"authors": ["agent-a"], "kinds": [0], "limit": 1}),
                serde_json::json!({"authors": ["agent-b"], "kinds": [0], "limit": 1}),
            ]
        );
    }

    #[test]
    fn marked_build_requires_verified_owner_without_viewer_equality() {
        let mut agents = vec![
            RelayAgentInfo {
                pubkey: "a".repeat(64),
                owner_pubkey: Some("b".repeat(64)),
                name: "verified".into(),
                agent_type: "agent".into(),
                channels: vec![],
                channel_ids: vec![],
                capabilities: vec![],
                status: "offline".into(),
                respond_to: None,
                respond_to_allowlist: vec![],
            },
            test_agent(None),
        ];
        retain_agents_allowed_by_build(&mut agents, true);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "verified");
    }

    #[test]
    fn oss_build_preserves_ownerless_legacy_agents() {
        let mut agents = vec![test_agent(None)];
        retain_agents_allowed_by_build(&mut agents, false);
        assert_eq!(agents.len(), 1);
        assert!(agents[0].owner_pubkey.is_none());
    }

    fn test_agent(owner_pubkey: Option<String>) -> RelayAgentInfo {
        RelayAgentInfo {
            pubkey: "c".repeat(64),
            owner_pubkey,
            name: "ownerless".into(),
            agent_type: "agent".into(),
            channels: vec![],
            channel_ids: vec![],
            capabilities: vec![],
            status: "online".into(),
            respond_to: None,
            respond_to_allowlist: vec![],
        }
    }
}
