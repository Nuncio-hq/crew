//! Wire regressions for the proposed unadvertised strict adapter contract.
//! These execute the real subprocess and run loop; native RED is not yet run.
use super::*;

fn strict_params(sid: &str, turn: &str, request: &str) -> Value {
    json!({"sessionId":sid,"expectedTurnId":turn,"requestId":request,
        "prompt":[{"type":"text","text":"STRICT-STEER-CANARY"}]})
}

async fn strict_run(
    terminal_first_round: bool,
) -> (
    Harness,
    String,
    i64,
    tokio::sync::oneshot::Sender<()>,
    Arc<Mutex<Vec<Value>>>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let captures = Arc::new(Mutex::new(Vec::new()));
    let responses = vec![
        CannedResponse {
            status: 200,
            body: if terminal_first_round {
                openai_text("done")
            } else {
                openai_tool_call("strict-call", "fake__noop", json!({}))
            },
        },
        CannedResponse {
            status: 200,
            body: openai_text("done after next boundary"),
        },
    ];
    let (url, _) =
        spawn_gated_capturing_fake_llm(responses, captures.clone(), Arc::new(Mutex::new(Some(rx))))
            .await;
    let mut h = Harness::spawn(&url).await;
    let sid = init_session(&mut h).await;
    let prompt = h
        .send(
            "session/prompt",
            json!({"sessionId":sid,
        "prompt":[{"type":"text","text":"original task"}],
        "_meta":{"crew":{"invocationId":"11111111-1111-4111-8111-111111111111"}}}),
        )
        .await;
    recv_active_run_id(&mut h).await;
    // Pin the test after the first drain, before its provider response.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !captures.lock().await.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first provider request");
    (h, sid, prompt, tx, captures)
}

async fn enqueue_and_prove_pending(h: &mut Harness, sid: &str) -> i64 {
    let params = strict_params(
        sid,
        "11111111-1111-4111-8111-111111111111",
        "33333333-3333-4333-8333-333333333333",
    );
    let original = h.send("_session/steering", params.clone()).await;
    let duplicate = h.send("_session/steering", params).await;
    // Pending duplicate is bounded, does not extend deadline, and must not
    // block the ACP dispatch loop behind the original's eventual append.
    let response = h
        .recv_until(|v| v["id"] == json!(original) || v["id"] == json!(duplicate))
        .await;
    assert_eq!(
        response["id"], duplicate,
        "enqueue must not acknowledge an append"
    );
    assert_eq!(response["result"]["outcome"], "busy");
    assert_eq!(
        response["result"]["requestId"],
        "33333333-3333-4333-8333-333333333333"
    );
    original
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_steer_acknowledges_actual_selected_run_append() {
    let (mut h, sid, prompt, release, captures) = strict_run(false).await;
    let steer = enqueue_and_prove_pending(&mut h, &sid).await;
    release.send(()).expect("release first round");
    let mut appended = false;
    let mut finished = false;
    for _ in 0..50 {
        let v = h.recv().await;
        if v["id"] == steer {
            assert_eq!(v["result"]["outcome"], "appended");
            assert_eq!(
                v["result"]["turnId"],
                "11111111-1111-4111-8111-111111111111"
            );
            appended = true;
        }
        if v["id"] == prompt {
            finished = true;
        }
        if appended && finished {
            break;
        }
    }
    assert!(appended && finished);
    let requests = captures.lock().await;
    let occurrences = requests.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| {
            m["role"] == "user"
                && m["content"]
                    .as_str()
                    .is_some_and(|s| s.contains("STRICT-STEER-CANARY"))
        })
        .count();
    assert_eq!(
        occurrences, 1,
        "pending duplicate must not duplicate history"
    );
    drop(requests);
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_steer_queued_before_final_response_is_not_an_append() {
    let (mut h, sid, prompt, release, captures) = strict_run(true).await;
    let steer = enqueue_and_prove_pending(&mut h, &sid).await;
    release.send(()).expect("release final response");
    let mut settled = false;
    let mut finished = false;
    for _ in 0..30 {
        let v = h.recv().await;
        if v["id"] == steer {
            assert_eq!(v["result"]["outcome"], "stale_target");
            settled = true;
        }
        if v["id"] == prompt {
            finished = true;
        }
        if settled && finished {
            break;
        }
    }
    assert!(
        settled && finished,
        "every queued request must settle on finish"
    );
    assert_eq!(
        captures.lock().await.len(),
        1,
        "strict input must not start another turn"
    );
    h.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strict_steer_rejects_other_invocation_without_touching_live_work() {
    let (mut h, sid, prompt, release, captures) = strict_run(true).await;
    let steer = h
        .send(
            "_session/steering",
            strict_params(
                &sid,
                "22222222-2222-4222-8222-222222222222",
                "44444444-4444-4444-8444-444444444444",
            ),
        )
        .await;
    let response = h.recv_until(|v| v["id"] == steer).await;
    assert_eq!(response["result"]["outcome"], "stale_target");
    release.send(()).expect("release original");
    let response = h.recv_until(|v| v["id"] == prompt).await;
    assert_eq!(response["result"]["stopReason"], "end_turn");
    assert_eq!(captures.lock().await.len(), 1);
    h.shutdown().await;
}
