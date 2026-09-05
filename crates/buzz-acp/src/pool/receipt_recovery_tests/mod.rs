use super::*;
use nostr::{EventBuilder, Keys};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod durable_spool;
mod turn_completion;

#[derive(Clone)]
enum Response {
    Accept,
    Reject(u16, String),
    Disconnect,
}

struct Fixture {
    outbox: PathBuf,
    rest: RestClient,
    event: nostr::Event,
    response: Arc<Mutex<Response>>,
    submissions: Arc<Mutex<Vec<nostr::Event>>>,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new(response: Response) -> Self {
        let keys = Keys::generate();
        let trigger = EventBuilder::text_note("trigger")
            .sign_with_keys(&keys)
            .expect("trigger");
        let event = buzz_sdk::build_agent_receipt(
            Uuid::new_v4(),
            &buzz_sdk::ThreadRef {
                root_event_id: trigger.id,
                parent_event_id: trigger.id,
            },
            "{\"summary\":\"completed\"}",
        )
        .expect("receipt")
        .sign_with_keys(&keys)
        .expect("signed receipt");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listen");
        let rest = RestClient {
            http: reqwest::Client::new(),
            base_url: format!("http://{}", listener.local_addr().expect("address")),
            keys,
            auth_tag_json: None,
        };
        let response = Arc::new(Mutex::new(response));
        let submissions = Arc::new(Mutex::new(Vec::new()));
        let reply_state = Arc::clone(&response);
        let received = Arc::clone(&submissions);
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let (path, body) = read_request(&mut socket).await;
                let (status, response) = if path == "/events" {
                    let event: nostr::Event =
                        serde_json::from_slice(&body).expect("signed event body");
                    assert!(event.verify().is_ok());
                    let mode = if event.kind.as_u16() as u32 == buzz_core::kind::KIND_AGENT_RECEIPT
                    {
                        received.lock().expect("submissions").push(event.clone());
                        reply_state.lock().expect("response").clone()
                    } else {
                        Response::Accept
                    };
                    match mode {
                        Response::Disconnect => continue,
                        Response::Reject(status, body) => (status, body),
                        Response::Accept => (
                            200,
                            serde_json::json!({"event_id": event.id.to_hex(), "accepted": true})
                                .to_string(),
                        ),
                    }
                } else if path == "/count" {
                    (200, "{\"count\":0}".into())
                } else {
                    (200, "[]".into())
                };
                let wire = format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len());
                let _ = socket.write_all(wire.as_bytes()).await;
            }
        });
        let outbox = std::fs::canonicalize(std::env::temp_dir())
            .expect("temp root")
            .join(format!("crew-receipt-recovery-{}", Uuid::new_v4()));
        Self {
            outbox,
            rest,
            event,
            response,
            submissions,
            server,
        }
    }

    async fn persist(&self) {
        assert!(
            persist_receipt_events(&self.outbox, std::slice::from_ref(&self.event))
                .await
                .is_empty()
        );
    }

    async fn flush(&self) -> ReceiptFlushReport {
        tokio::time::timeout(
            Duration::from_secs(10),
            flush_receipt_outbox(
                &self.outbox,
                &self.rest,
                self.rest.keys.public_key(),
                &[],
                None,
                1,
            ),
        )
        .await
        .expect("bounded flush")
        .expect("flush")
    }

    fn submitted(&self) -> Vec<nostr::Event> {
        self.submissions.lock().expect("submissions").clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.outbox);
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    let (headers_end, content_length) = loop {
        let mut buffer = [0; 4096];
        let count = socket.read(&mut buffer).await.expect("request");
        assert!(count > 0, "complete request headers");
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() <= 65536, "bounded fixture request");
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&bytes[..end]).expect("HTTP headers");
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().expect("length"))
                })
                .expect("content length");
            break (end + 4, length);
        }
    };
    while bytes.len() < headers_end + content_length {
        let mut buffer = [0; 4096];
        let count = socket.read(&mut buffer).await.expect("request body");
        assert!(count > 0, "complete request body");
        bytes.extend_from_slice(&buffer[..count]);
    }
    let path = std::str::from_utf8(&bytes[..headers_end])
        .expect("headers")
        .lines()
        .next()
        .expect("request line")
        .split_whitespace()
        .nth(1)
        .expect("path")
        .to_string();
    (
        path,
        bytes[headers_end..headers_end + content_length].to_vec(),
    )
}
