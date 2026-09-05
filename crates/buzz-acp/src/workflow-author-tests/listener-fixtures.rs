/// Serve a NIP-11 document on a loopback port so `InboundAuthorGate` can be
/// built through the *same* constructor the listeners use, rather than by
/// injecting an already-resolved relay identity. This is what makes the
/// listener-to-gate wiring testable: a gate that never loads its identity
/// fails these tests instead of silently degrading to the raw signer.
pub(super) async fn nip11_server(
    document: serde_json::Value,
) -> (relay::RestClient, tokio::task::JoinHandle<()>) {
    nip11_scripted_server(std::collections::VecDeque::from([Ok(document)])).await
}

/// Serve scripted NIP-11 responses. `Err(())` returns HTTP 500.
async fn nip11_scripted_server(
    responses: std::collections::VecDeque<Result<serde_json::Value, ()>>,
) -> (relay::RestClient, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind NIP-11 test server");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let responses = std::sync::Arc::new(tokio::sync::Mutex::new((responses, None)));
    let server = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut request = vec![0; 8192];
            let _ = socket.read(&mut request).await;
            if request.starts_with(b"POST ") {
                let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]").await;
                continue;
            }
            let response = {
                let mut scripted = responses.lock().await;
                let response = if let Some(next) = scripted.0.pop_front() {
                    Some(next)
                } else {
                    scripted.1.clone()
                };
                if let Some(Ok(document)) = &response {
                    scripted.1 = Some(Ok(document.clone()));
                }
                response
            };
            let Some(response) = response else {
                continue;
            };
            let Ok(document) = response else {
                let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(response.as_bytes()).await;
                continue;
            };
            let body = document.to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/nostr+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    let rest = relay::RestClient {
        http: reqwest::Client::new(),
        base_url,
        keys: nostr::Keys::generate(),
        auth_tag_json: None,
    };
    (rest, server)
}

/// Build a gate through the real `connect` path against a NIP-11 document
/// advertising `relay_hex` as the relay signer. Tests use this instead of
/// constructing `InboundAuthorGate` literally so that the identity load
/// stays part of what they cover.
async fn connected_gate(
    relay_hex: &str,
    agent: &str,
) -> (
    InboundAuthorGate,
    relay::RestClient,
    tokio::task::JoinHandle<()>,
) {
    let (rest_client, server) = nip11_server(serde_json::json!({ "self": relay_hex })).await;
    let gate = InboundAuthorGate::connect(&rest_client, agent, "test").await;
    (gate, rest_client, server)
}

/// A genuine relay-signed workflow dispatch that explicitly targets `agent`
/// on behalf of `owner` — the exact event shape a scheduled workflow emits.
pub(super) fn relay_signed_workflow_dispatch(
    relay_keys: &nostr::Keys,
    owner: &str,
    agent: &str,
) -> nostr::Event {
    nostr::EventBuilder::new(nostr::Kind::Custom(KIND_STREAM_MESSAGE as u16), "dispatch")
        .tags([
            nostr::Tag::parse(["buzz:workflow", "true"]).expect("workflow marker"),
            nostr::Tag::parse(["buzz:workflow-owner", owner]).expect("workflow owner tag"),
            nostr::Tag::parse(["buzz:workflow-mention", agent]).expect("workflow mention tag"),
            nostr::Tag::parse(["p", agent]).expect("recipient tag"),
        ])
        .sign_with_keys(relay_keys)
        .expect("signed workflow event")
}
