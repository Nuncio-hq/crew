use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn http_fixture(
    responses: Vec<(u16, Value)>,
) -> (HarnessRelay, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut queries = Vec::new();
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let (header_end, length) = loop {
                let mut bytes = [0; 4096];
                let read = socket.read(&mut bytes).await.unwrap();
                assert!(read > 0, "request must include complete headers");
                request.extend_from_slice(&bytes[..read]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .expect("content length");
                    break (end + 4, length);
                }
            };
            while request.len() < header_end + length {
                let mut bytes = [0; 4096];
                let read = socket.read(&mut bytes).await.unwrap();
                assert!(read > 0, "request must include complete body");
                request.extend_from_slice(&bytes[..read]);
            }
            queries
                .push(serde_json::from_slice(&request[header_end..header_end + length]).unwrap());
            let body = body.to_string();
            let response = format!("HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            socket.write_all(response.as_bytes()).await.unwrap();
        }
        queries
    });
    let (_event_tx, event_rx) = mpsc::channel(1);
    let (cmd_tx, _cmd_rx) = mpsc::channel(1);
    let (_snapshot_tx, subscription_snapshot_rx) = tokio::sync::watch::channel(HashSet::new());
    let relay = HarnessRelay {
        event_rx,
        subscription_snapshot_rx,
        observer_control_rx: None,
        cmd_tx,
        http: reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
        relay_url: format!("ws://{address}"),
        keys: Keys::generate(),
        auth_tag: None,
        bg_handle: None,
    };
    (relay, server)
}

#[tokio::test]
async fn discovery_forbidden_is_an_error_while_empty_memberships_are_valid() {
    let (relay, server) = http_fixture(vec![(403, json!("restricted: insufficient scope"))]).await;
    assert!(
        matches!(
            relay.discover_channels().await,
            Err(RelayError::HttpStatus { status: 403, .. })
        ),
        "forbidden membership queries must never become a zero-channel snapshot"
    );
    assert_eq!(server.await.unwrap().len(), 1);
    let (relay, server) = http_fixture(vec![(200, json!([]))]).await;
    assert!(relay.discover_channels().await.unwrap().is_empty());
    let queries = server.await.unwrap();
    assert_eq!(
        queries.len(),
        1,
        "zero members must not trigger metadata lookup"
    );
    assert_eq!(queries[0][0]["kinds"], json!([39002]));
    assert_eq!(
        queries[0][0]["#p"],
        json!([relay.keys.public_key().to_hex()])
    );
}

#[tokio::test]
async fn discovery_archived_channels_are_excluded_but_dm_only_membership_is_preserved() {
    let archived = Uuid::new_v4();
    let dm = Uuid::new_v4();
    let memberships = json!([
        {"tags":[["d",archived.to_string()]]},
        {"tags":[["d",dm.to_string()]]}
    ]);
    let metadata = json!([
        {"tags":[["d",archived.to_string()],["name","archived"],["archived","true"]]},
        {"tags":[["d",dm.to_string()],["name","direct"],["t","dm"]]}
    ]);
    let (relay, server) = http_fixture(vec![(200, memberships), (200, metadata)]).await;
    let discovered = relay.discover_channels().await.unwrap();
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[&dm].channel_type, "dm");
    assert!(!discovered.contains_key(&archived));
    let queries = server.await.unwrap();
    assert_eq!(queries[1][0]["kinds"], json!([39000]));
}

#[tokio::test]
async fn discovery_malformed_member_response_is_not_an_empty_success() {
    let (relay, server) = http_fixture(vec![(200, json!({"unexpected":"object"}))]).await;
    assert!(matches!(
        relay.discover_channels().await,
        Err(RelayError::Http(_))
    ));
    server.await.unwrap();
}
