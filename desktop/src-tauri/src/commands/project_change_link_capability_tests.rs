use super::*;

#[tokio::test]
async fn project_link_capability_disabled_after_preflight_retains_exact_retry() {
    let (runtime, operation) = Runtime::fixture();
    let id = operation.id.clone();
    let signed = operation.payload["signed_patch"].clone();
    runtime
        .disable_capability_at_send
        .store(true, Ordering::SeqCst);
    let rejected = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert_eq!(rejected.id, id);
    assert_eq!(rejected.status, OperationStatus::Failed);
    assert!(!rejected.reconciled);
    assert_eq!(rejected.payload["signed_patch"], signed);
    assert_eq!(rejected.payload["publication_attempted"], true);
    assert_eq!(
        rejected.payload["last_error"],
        "unsupported: crew-project-channel-link-v1"
    );
    assert_eq!(runtime.head.load(Ordering::SeqCst), 0);
    assert_eq!(runtime.sent.lock().unwrap().len(), 1);

    // The relay stays disabled on explicit retry: retain the same journal and
    // signed event without an unconditional fallback or an additional send.
    let waiting = drive(&runtime, runtime.load(&id), runtime.owner, true)
        .await
        .unwrap();
    assert_eq!(waiting.id, id);
    assert!(!waiting.reconciled);
    assert_eq!(waiting.payload["signed_patch"], signed);
    assert_eq!(runtime.sent.lock().unwrap().len(), 1);

    runtime
        .disable_capability_at_send
        .store(false, Ordering::SeqCst);
    runtime.capability.store(true, Ordering::SeqCst);
    let completed = drive(&runtime, runtime.load(&id), runtime.owner, true)
        .await
        .unwrap();
    assert_eq!(completed.id, id);
    assert_eq!(completed.status, OperationStatus::Complete);
    assert!(completed.reconciled);
    assert_eq!(completed.payload["signed_patch"], signed);
    let sent = runtime.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        sent[0], sent[1],
        "retry must preserve the exact signed envelope"
    );
}
