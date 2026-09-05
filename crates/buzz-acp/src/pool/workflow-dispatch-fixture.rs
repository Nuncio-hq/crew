struct DispatchFixture {
    root: PathBuf,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for DispatchFixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl DispatchFixture {
    async fn new(relay_key: &str) -> (Self, RestClient) {
        let (rest, server) =
            crate::author_gate_tests::nip11_server(serde_json::json!({"self": relay_key})).await;
        let root = std::env::temp_dir().join(format!("crew-workflow-dispatch-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("fixture directory");
        std::fs::write(root.join("peer.py"), PEER).expect("protocol peer");
        (Self { root, server }, rest)
    }

    async fn agent(&self, index: usize, conversation: Uuid) -> OwnedAgent {
        let mut acp = AcpClient::spawn(
            "python3",
            &[self.root.join("peer.py").display().to_string()],
            &[
                ("DISPATCH_DIR".into(), self.root.display().to_string()),
                ("DISPATCH_WORKER".into(), index.to_string()),
            ],
            false,
        )
        .await
        .expect("spawn protocol peer");
        acp.initialize().await.expect("initialize protocol peer");
        let mut state = SessionState::default();
        state
            .sessions
            .insert(conversation, format!("session-{index}"));
        OwnedAgent {
            index,
            acp,
            state,
            model_capabilities: None,
            desired_model: None,
            model_overridden: false,
            desired_model_request_id: None,
            desired_model_pending_ack: false,
            startup_effort: None,
            agent_name: "workflow-dispatch-test".into(),
            goose_system_prompt_supported: None,
            protocol_version: 1,
            load_session_supported: false,
        }
    }
}

// Both provider prompts must be live before either completes. This observes
// actual ACP wire requests while remaining independent of an external model.
const PEER: &str = r#"
import json, os, pathlib, sys, time
root = pathlib.Path(os.environ['DISPATCH_DIR'])
worker = os.environ['DISPATCH_WORKER']
for line in sys.stdin:
    frame = json.loads(line)
    method = frame.get('method')
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {}}
    elif method == 'session/prompt':
        (root / ('prompt-' + worker + '.json')).write_text(line)
        deadline = time.monotonic() + 5
        while len(list(root.glob('prompt-*.json'))) < 2:
            if time.monotonic() > deadline:
                raise RuntimeError('second thread did not run concurrently')
            time.sleep(0.01)
        result = {'stopReason': 'end_turn'}
    else:
        result = {}
    if 'id' in frame:
        print(json.dumps({'jsonrpc': '2.0', 'id': frame['id'], 'result': result}), flush=True)
"#;
