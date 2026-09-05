use super::tests::{
    init_git_fixture, make_prompt_context_no_owner, make_prompt_context_with_owner,
    owner_project_workspace_batch_with,
};
use super::*;
use nostr::{EventBuilder, Keys, Kind};

mod continuation;
mod workspace;

async fn inert_owned_agent(index: usize) -> OwnedAgent {
    OwnedAgent {
        index,
        acp: AcpClient::spawn("cat", &[], &[], false)
            .await
            .expect("spawn cat as inert agent"),
        state: SessionState::default(),
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "unknown".into(),
        goose_system_prompt_supported: None,
        protocol_version: 1,
        load_session_supported: false,
    }
}
