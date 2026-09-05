use super::*;

#[tokio::test]
async fn unicode_http_rejection_dead_letters_without_panicking() {
    let fixture = Fixture::new(Response::Reject(400, format!("{}é", "a".repeat(511)))).await;
    fixture.persist().await;
    let report = fixture.flush().await;
    let id = fixture.event.id.to_hex();
    assert!(report.rejected.contains(&id));
    assert!(!report.queued.contains(&id));
    assert!(fixture.outbox.join(format!("{id}.rejected")).is_file());
}

#[tokio::test]
async fn unsupported_kind_invalid_payload_and_registration_refusal_stay_permanent() {
    for (status, body) in [
        (400, "restricted: unknown event kind"),
        (400, "invalid: event signature verification failed"),
        (
            403,
            "restricted: agent receipt must be authored by a registered agent",
        ),
    ] {
        let fixture = Fixture::new(Response::Reject(status, body.into())).await;
        fixture.persist().await;
        let original = tokio::fs::read(fixture.outbox.join(format!("{}.json", fixture.event.id)))
            .await
            .expect("durable receipt");
        let report = fixture.flush().await;
        let id = fixture.event.id.to_hex();
        assert!(report.rejected.contains(&id), "HTTP {status}: {body}");
        assert!(!report.queued.contains(&id));
        assert_eq!(fixture.submitted(), vec![fixture.event.clone()]);
        assert_eq!(
            tokio::fs::read(fixture.outbox.join(format!("{id}.rejected")))
                .await
                .expect("retained rejection"),
            original
        );
        *fixture.response.lock().expect("mode") = Response::Accept;
        let retry = fixture.flush().await;
        assert!(
            retry.acked.is_empty(),
            "permanent errors must not become an unlimited replay loop"
        );
        assert_eq!(fixture.submitted().len(), 1);
    }
}

async fn transient_then_exact_ack(response: Response) {
    let fixture = Fixture::new(response).await;
    fixture.persist().await;
    let id = fixture.event.id.to_hex();
    let entry = fixture.outbox.join(format!("{id}.json"));
    let original = tokio::fs::read(&entry).await.expect("durable receipt");
    let first = fixture.flush().await;
    assert!(first.queued.contains(&id));
    assert!(first.rejected.is_empty());
    assert_eq!(
        tokio::fs::read(&entry).await.expect("still queued"),
        original
    );
    let failed_attempts = fixture.submitted();
    assert!(!failed_attempts.is_empty());
    assert!(failed_attempts.len() <= 4, "bounded inline HTTP attempts");
    assert!(failed_attempts.iter().all(|event| event == &fixture.event));
    *fixture.response.lock().expect("mode") = Response::Accept;
    let recovered = fixture.flush().await;
    assert!(recovered.acked.contains(&id));
    assert!(recovered.queued.is_empty());
    assert!(
        !entry.exists(),
        "only an exact accepted ACK retires the durable receipt"
    );
    assert!(fixture
        .submitted()
        .iter()
        .all(|event| event == &fixture.event));
}

#[tokio::test]
async fn unavailable_relay_queues_receipt_then_retires_same_signed_event_on_ack() {
    transient_then_exact_ack(Response::Reject(503, "unavailable".into())).await;
}

#[tokio::test]
async fn connection_drop_queues_receipt_then_retires_same_signed_event_on_ack() {
    transient_then_exact_ack(Response::Disconnect).await;
}

#[tokio::test]
async fn startup_worker_recovers_a_durable_receipt_after_network_repair() {
    let mut fixture = Fixture::new(Response::Disconnect).await;
    let mut ctx = super::super::tests::make_prompt_context_no_owner();
    ctx.agent_keys = fixture.rest.keys.clone();
    ctx.rest_client = fixture.rest.clone();
    ctx.relay_url = fixture.rest.base_url.clone();
    ctx.agent_receipts_enabled = true;
    fixture.outbox = receipt_outbox_dir(&ctx).expect("scoped outbox");
    fixture.persist().await;
    let id = fixture.event.id.to_hex();
    let first = fixture.flush().await;
    assert!(first.queued.contains(&id));
    let path = fixture.outbox.join(format!("{id}.json"));
    assert!(path.is_file());
    *fixture.response.lock().expect("mode") = Response::Accept;
    let ctx = Arc::new(ctx);
    resume_receipt_outbox(Arc::clone(&ctx));
    tokio::time::timeout(Duration::from_secs(5), async {
        while path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("startup worker retires the recovered durable receipt");
    assert!(fixture.submitted().len() >= 2);
    assert!(fixture
        .submitted()
        .iter()
        .all(|event| event == &fixture.event));
    ctx.receipt_outbox_workers.close();
    tokio::time::timeout(Duration::from_secs(5), ctx.receipt_outbox_workers.wait())
        .await
        .expect("worker exits after its outbox is empty");
}
