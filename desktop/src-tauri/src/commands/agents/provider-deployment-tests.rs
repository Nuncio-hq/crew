use super::*;
use serde_json::json;

fn scope() -> AgentStartScope {
    AgentStartScope {
        expected_relay_url: Some("wss://tenant-a.example".into()),
        expected_signer_pubkey: Some("owner-a".into()),
        replay_floor_unix: Some(1000),
    }
}
fn payload() -> serde_json::Value {
    json!({"relay_url":"wss://tenant-a.example", "launch":{"owner_pubkey":"owner-a", "env":{}, "policy_env":{}}})
}

#[test]
fn provider_replay_floor_overrides_every_case_variant_without_changing_other_policy() {
    let mut data = payload();
    data["launch"]["env"] = json!({"BUZZ_ACP_REPLAY_FLOOR":"9999", "buzz_acp_replay_floor":"8888", "HERMES_PROFILE":"scout"});
    data["launch"]["policy_env"] = json!({"buzz_acp_replay_floor":"7777", "BUZZ_ACP_AGENTS":"10"});
    prepare_scoped_payload(&mut data, &scope()).unwrap();
    assert_eq!(
        data["launch"]["policy_env"],
        json!({"BUZZ_ACP_REPLAY_FLOOR":"1000", "BUZZ_ACP_AGENTS":"10"})
    );
    assert_eq!(data["launch"]["env"], json!({"HERMES_PROFILE":"scout"}));
    // The caller's record/payload is not assigned a floor until this invocation.
    assert!(payload()["launch"]["policy_env"]
        .get(REPLAY_FLOOR_ENV_VAR)
        .is_none());
}

#[test]
fn ordinary_provider_deploy_preserves_saved_environment_without_adding_a_floor() {
    let mut data = payload();
    data["launch"]["env"] = json!({"BUZZ_ACP_REPLAY_FLOOR":"800"});
    let original = data.clone();
    prepare_scoped_payload(&mut data, &AgentStartScope::default()).unwrap();
    assert_eq!(data, original);
}

#[test]
fn changed_or_unverifiable_provider_scope_is_rejected_before_floor_or_deploy() {
    for path in ["/relay_url", "/launch/owner_pubkey"] {
        let mut data = payload();
        *data.pointer_mut(path).unwrap() = json!("different");
        let before = data.clone();
        assert!(prepare_scoped_payload(&mut data, &scope()).is_err());
        assert_eq!(data, before, "scope must fail before preparing a launch");
        *data.pointer_mut(path).unwrap() = serde_json::Value::Null;
        assert!(prepare_scoped_payload(&mut data, &scope()).is_err());
    }
}

#[tokio::test]
async fn provider_payload_rebuilt_after_a_wait_cannot_adopt_a_different_community_or_signer() {
    // A queued deploy receives a newly resolved payload after the preceding
    // deploy releases its lock. The production assertion checks that payload.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let mut rebuilt = rx.await.unwrap();
        prepare_scoped_payload(&mut rebuilt, &scope())
    });
    let mut changed = payload();
    changed["relay_url"] = json!("wss://tenant-b.example");
    changed["launch"]["owner_pubkey"] = json!("owner-b");
    tx.send(changed).unwrap();
    assert!(task
        .await
        .unwrap()
        .unwrap_err()
        .contains("active community changed"));
}
