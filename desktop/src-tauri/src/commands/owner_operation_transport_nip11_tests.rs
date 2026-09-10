use super::*;

#[tokio::test]
async fn owner_transport_nip11_uses_captured_origin_and_accept_header() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    for (body, expected) in [
        (
            r#"{"supported_extensions":["crew-conditional-publication-v1"]}"#,
            Some(vec!["crew-conditional-publication-v1".to_owned()]),
        ),
        ("{}", None),
        (r#"{"supported_extensions":null}"#, None),
    ] {
        let (origin, worker) = relay(response(200, body.as_bytes())).await;
        let info = transport(origin, Keys::generate())
            .relay_information(async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(info.supported_extensions, expected);
        let request = worker.await.unwrap();
        let headers = String::from_utf8_lossy(&request).to_ascii_lowercase();
        assert!(headers.starts_with("get / http/1.1\r\n"));
        assert!(headers.contains("\r\naccept: application/nostr+json\r\n"));
    }
}

#[tokio::test]
async fn owner_transport_nip11_stale_scope_is_not_attempted() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let result = transport(origin, Keys::generate())
        .relay_information(async { Err("OWNER_SCOPE_STALE".into()) })
        .await;
    assert!(
        matches!(result, Err(OperationTransportError::NotAttempted(reason)) if reason == "OWNER_SCOPE_STALE")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}
