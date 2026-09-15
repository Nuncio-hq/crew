// Test-only credential I/O seam. The production keyring remains the default;
// tests register only the exact pubkey they own so an unavailable OS keyring
// cannot hide the local deletion/recovery behavior under test.
struct TestAgentKeyDelete {
    result: Result<(), String>,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    token: std::sync::Arc<()>,
}

fn test_agent_key_delete_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, TestAgentKeyDelete>> {
    static REGISTRY: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, TestAgentKeyDelete>>,
    > = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// RAII handle for a test-only override of one agent key deletion.
pub(crate) struct TestAgentKeyDeleteGuard {
    pubkey: String,
    token: std::sync::Arc<()>,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl TestAgentKeyDeleteGuard {
    /// Return the number of times the production deletion seam used this key.
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for TestAgentKeyDeleteGuard {
    fn drop(&mut self) {
        let mut registry = test_agent_key_delete_registry()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if registry
            .get(&self.pubkey)
            .is_some_and(|entry| std::sync::Arc::ptr_eq(&entry.token, &self.token))
        {
            registry.remove(&self.pubkey);
        }
    }
}

/// Inject one exact agent-key deletion result for a production-bound test.
///
/// The override is keyed by pubkey and removed when the returned guard drops;
/// all other keys continue through the real OS-keyring path.
pub(crate) fn install_test_agent_key_delete(
    pubkey: &str,
    result: Result<(), String>,
) -> TestAgentKeyDeleteGuard {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let token = std::sync::Arc::new(());
    let mut registry = test_agent_key_delete_registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert!(
        !registry.contains_key(pubkey),
        "test agent-key deletion override already installed"
    );
    registry.insert(
        pubkey.to_string(),
        TestAgentKeyDelete {
            result,
            calls: std::sync::Arc::clone(&calls),
            token: std::sync::Arc::clone(&token),
        },
    );
    TestAgentKeyDeleteGuard {
        pubkey: pubkey.to_string(),
        token,
        calls,
    }
}

pub(super) fn test_agent_key_delete(pubkey: &str) -> Option<Result<(), String>> {
    let registry = test_agent_key_delete_registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let entry = registry.get(pubkey)?;
    entry
        .calls
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Some(entry.result.clone())
}
