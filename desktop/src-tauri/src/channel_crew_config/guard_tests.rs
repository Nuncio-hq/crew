use super::*;
use crate::channel_crew_config::{
    driver::Backend,
    record::{Lease, Payload},
    tests::Fixture,
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct ResetAdmission;
impl Drop for ResetAdmission {
    fn drop(&mut self) {
        crate::relay_admission::reset_rate_limit_gate();
    }
}

#[tokio::test]
async fn channel_crew_guard_reloads_lease_after_admission_and_never_sends_stale_worker() {
    let _serial = crate::relay_admission::TEST_SERIAL.lock().await;
    let _reset = ResetAdmission;
    crate::relay_admission::reset_rate_limit_gate();
    let (fixture, operation) = Fixture::new();
    let mut payload: Payload = serde_json::from_value(operation.payload.clone()).unwrap();
    let worker = uuid::Uuid::new_v4().to_string();
    payload.lease = Some(Lease {
        worker: worker.clone(),
        expires_at: 1060,
    });
    let leased = fixture
        .persist(&operation, &payload, OperationStatus::Reconciling, false)
        .await
        .unwrap();
    let stamp = DispatchStamp {
        id: leased.id.clone(),
        revision: leased.revision,
        worker,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let accepted = Arc::new(AtomicUsize::new(0));
    let seen = accepted.clone();
    let event_id = payload.canvas.id.to_hex();
    let server = tokio::spawn(async move {
        if let Ok(Ok((mut socket, _))) =
            tokio::time::timeout(Duration::from_secs(3), listener.accept()).await
        {
            seen.fetch_add(1, Ordering::SeqCst);
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            loop {
                let count = socket.read(&mut chunk).await.unwrap();
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let length = header
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let body = serde_json::json!({"event_id":event_id,"accepted":true,"message":"fixture"})
                .to_string();
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        }
    });
    let state = crate::app_state::build_app_state();
    crate::relay_admission::activate_rate_limit(Some(1));
    let send = crate::commands::channel_crew_transport_publish(
        &state,
        origin,
        fixture.keys.clone(),
        &payload.canvas,
        async {
            reload_and_verify(async { Ok(fixture.load(&stamp.id)) }, &stamp, || Ok(1000)).await
        },
    );
    let takeover = async {
        tokio::task::yield_now().await;
        let mut changed = payload.clone();
        changed.lease = Some(Lease {
            worker: uuid::Uuid::new_v4().to_string(),
            expires_at: 1060,
        });
        fixture
            .persist(&leased, &changed, OperationStatus::Reconciling, false)
            .await
            .unwrap();
    };
    let (result, ()) = tokio::join!(send, takeover);
    assert!(
        result.is_err(),
        "stale persisted lease must block actual HTTP send"
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 0);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn channel_crew_guard_rejects_expired_lease_at_exact_deadline() {
    let (fixture, operation) = Fixture::new();
    let mut payload: Payload = serde_json::from_value(operation.payload.clone()).unwrap();
    let worker = uuid::Uuid::new_v4().to_string();
    payload.lease = Some(Lease {
        worker: worker.clone(),
        expires_at: 1060,
    });
    let leased = fixture
        .persist(&operation, &payload, OperationStatus::Reconciling, false)
        .await
        .unwrap();
    assert!(verify_dispatch_record(&leased, &leased.id, leased.revision, &worker, 1059).is_ok());
    assert!(verify_dispatch_record(&leased, &leased.id, leased.revision, &worker, 1060).is_err());
}
