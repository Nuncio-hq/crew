use super::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct Store(Arc<Mutex<State>>);
struct State {
    record: ManagedAgentRecord,
    writes: usize,
    fail_write: Option<usize>,
}
impl RecordStore for Store {
    fn update(
        &mut self,
        change: impl FnOnce(&mut ManagedAgentRecord) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut state = self.0.lock().unwrap();
        let mut candidate = state.record.clone();
        change(&mut candidate)?;
        state.writes += 1;
        if state.fail_write == Some(state.writes) {
            return Err("injected write failure".into());
        }
        state.record = candidate;
        Ok(())
    }
}
fn fixture(fail_write: Option<usize>) -> (Store, ManagedAgentRecord) {
    let record: ManagedAgentRecord = serde_json::from_value(serde_json::json!({
        "pubkey": "a".repeat(64), "name": "Provider test", "relay_url": "wss://example.com",
        "acp_command": "", "agent_command": "", "agent_args": [], "mcp_command": "",
        "turn_timeout_seconds": 0, "system_prompt": null, "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z", "last_started_at": null, "last_stopped_at": null,
        "last_exit_code": null, "last_error": null,
        "instance_generation": uuid::Uuid::new_v4(),
        "backend": {"type": "provider", "id": "example", "config": {}}
    }))
    .unwrap();
    let store = Store(Arc::new(Mutex::new(State {
        record: record.clone(),
        writes: 0,
        fail_write,
    })));
    (store, record)
}
async fn guard() -> OwnedMutexGuard<()> {
    Arc::new(tokio::sync::Mutex::new(())).lock_owned().await
}

#[tokio::test]
async fn pending_write_failure_prevents_provider_invocation() {
    let (store, record) = fixture(Some(1));
    let invoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called = invoked.clone();
    let result = spawn(store.clone(), record, guard().await, move || {
        called.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok("handle".into())
    })
    .await
    .unwrap();
    assert!(result.is_err());
    assert!(!invoked.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!store.0.lock().unwrap().record.provider_policy_pending);
}

#[tokio::test]
async fn result_write_failure_retains_first_deploy_witness_without_handle() {
    let (store, record) = fixture(Some(2));
    assert!(
        spawn(store.clone(), record, guard().await, || Ok("handle".into()))
            .await
            .unwrap()
            .is_err()
    );
    let saved = &store.0.lock().unwrap().record;
    assert!(saved.provider_policy_pending);
    assert!(saved.backend_agent_id.is_none());
}

#[tokio::test]
async fn provider_crash_preserves_pending_witness() {
    let (store, record) = fixture(None);
    assert!(spawn(store.clone(), record, guard().await, || panic!(
        "injected provider crash"
    ))
    .await
    .is_err());
    assert!(store.0.lock().unwrap().record.provider_policy_pending);
}

#[tokio::test]
async fn canceled_waiter_does_not_release_guard_or_skip_result_persistence() {
    let (store, record) = fixture(None);
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = spawn(
        store.clone(),
        record,
        lock.clone().lock_owned().await,
        move || {
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok("surviving-handle".into())
        },
    );
    let waiter = tokio::spawn(async move { worker.await });
    tokio::time::timeout(Duration::from_secs(3), started_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(store.0.lock().unwrap().record.provider_policy_pending);
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert!(
        lock.try_lock().is_err(),
        "worker must retain delete exclusion"
    );
    release_tx.send(()).unwrap();
    let _finished = tokio::time::timeout(Duration::from_secs(3), lock.lock())
        .await
        .unwrap();
    let saved = &store.0.lock().unwrap().record;
    assert_eq!(saved.backend_agent_id.as_deref(), Some("surviving-handle"));
    assert!(!saved.provider_policy_pending);
}

#[tokio::test]
async fn changed_provider_cannot_receive_the_previous_provider_handle() {
    let (store, record) = fixture(None);
    let changed = store.clone();
    let result = spawn(store.clone(), record, guard().await, move || {
        changed.0.lock().unwrap().record.backend = crate::managed_agents::BackendKind::Provider {
            id: "replacement".into(),
            config: serde_json::json!({"different": true}),
        };
        Ok("old-provider-handle".into())
    })
    .await
    .unwrap();
    assert!(result.is_err());
    let saved = &store.0.lock().unwrap().record;
    assert!(saved.backend_agent_id.is_none());
    assert!(saved.provider_policy_pending);
}

#[tokio::test]
async fn policy_changed_during_invoke_preserves_handle_but_not_policy_acknowledgement() {
    for change_mode in [true, false] {
        let (store, mut record) = fixture(None);
        record.respond_to = crate::managed_agents::RespondTo::Allowlist;
        record.respond_to_allowlist = vec!["b".repeat(64)];
        store.0.lock().unwrap().record = record.clone();
        let changed = store.clone();
        let result = spawn(store.clone(), record, guard().await, move || {
            let mut state = changed.0.lock().unwrap();
            assert!(state.record.provider_policy_pending);
            if change_mode {
                state.record.respond_to = crate::managed_agents::RespondTo::Anyone;
            } else {
                state.record.respond_to_allowlist = vec!["c".repeat(64)];
            }
            Ok("same-provider-handle".into())
        })
        .await
        .unwrap();
        assert!(result.is_ok());
        let saved = &store.0.lock().unwrap().record;
        assert_eq!(
            saved.backend_agent_id.as_deref(),
            Some("same-provider-handle")
        );
        assert!(
            saved.provider_policy_pending,
            "a changed policy remains unacknowledged"
        );
        if change_mode {
            assert_eq!(saved.respond_to, crate::managed_agents::RespondTo::Anyone);
        } else {
            assert_eq!(saved.respond_to_allowlist, vec!["c".repeat(64)]);
        }
    }
}
