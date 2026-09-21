//! E2E walkthrough of the contact-fallback production seam (issue #412,
//! successor contract v4).
//!
//! Run against a local relay with migrations applied:
//!   BUZZ_ALLOW_NIP_OA_AUTH=true cargo run -p buzz-relay
//!   BUZZ_POSTGRES_URL=postgres://buzz:buzz_dev@127.0.0.1:5432/buzz \
//!     cargo test -p buzz-test-client --test e2e_contact_fallback -- --ignored
//!
//! The tests drive the real relay ingest path, real Postgres, real NIP-OA auth
//! backfill, and real WebSocket frames — no test seams.

use std::time::Duration;

use buzz_sdk::nip_oa;
use buzz_test_client::{BuzzTestClient, RelayMessage};
use nostr::{Alphabet, EventBuilder, EventId, Filter, Keys, Kind, SingleLetterTag, Tag, Timestamp};
use uuid::Uuid;

const KIND_STREAM_MESSAGE: u16 = 9;
const KIND_CANVAS: u16 = 40100;
const KIND_AGENT_RECEIPT: u16 = 46043;
const KIND_CONTACT_DECISION: u16 = 46044;
const KIND_CONTACT_CONTROL: u16 = 24210;
const KIND_CHANNEL_CREATE: u16 = 9007;
const CAPABILITY_TAG: &str = "crew-contact-fallback";

fn relay_url() -> String {
    std::env::var("BUZZ_TEST_RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string())
}

fn pg_url() -> String {
    std::env::var("BUZZ_POSTGRES_URL")
        .unwrap_or_else(|_| "postgres://buzz:buzz_dev@127.0.0.1:5432/buzz".to_string())
}

/// Create an open stream channel owned by `keys`; returns the channel UUID.
async fn create_channel(client: &mut BuzzTestClient, keys: &Keys, label: &str) -> Uuid {
    let channel_uuid = Uuid::new_v4();
    let event = EventBuilder::new(Kind::Custom(KIND_CHANNEL_CREATE), "")
        .tags(vec![
            Tag::parse(["h", &channel_uuid.to_string()]).unwrap(),
            Tag::parse(["name", &format!("contact-e2e-{label}")]).unwrap(),
            Tag::parse(["channel_type", "stream"]).unwrap(),
            Tag::parse(["visibility", "open"]).unwrap(),
        ])
        .sign_with_keys(keys)
        .unwrap();
    let ok = client.send_event(event).await.expect("create channel");
    assert!(ok.accepted, "channel create rejected: {}", ok.message);
    channel_uuid
}

/// Owner-signed canvas binding `contact` as the channel's fallback contact.
async fn set_contact_canvas(
    client: &mut BuzzTestClient,
    owner: &Keys,
    channel: Uuid,
    contact: &Keys,
) -> EventId {
    let content = format!("```crew\ncontact: {}\n```", contact.public_key().to_hex());
    let event = EventBuilder::new(Kind::Custom(KIND_CANVAS), content)
        .tags([Tag::parse(["h", &channel.to_string()]).unwrap()])
        .sign_with_keys(owner)
        .unwrap();
    let id = event.id;
    let ok = client.send_event(event).await.expect("canvas publish");
    assert!(ok.accepted, "canvas rejected: {}", ok.message);
    id
}

/// Mention-less kind:9 original carrying the routing capability tag.
fn capability_message(owner: &Keys, channel: Uuid, text: &str) -> nostr::Event {
    EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE), text)
        .tags([
            Tag::parse(["h", &channel.to_string()]).unwrap(),
            Tag::parse([CAPABILITY_TAG, "1"]).unwrap(),
        ])
        .custom_created_at(Timestamp::now())
        .sign_with_keys(owner)
        .unwrap()
}

/// Connect the contact agent with an owner-signed NIP-OA tag — this is the
/// real auth path that materializes `users.agent_owner_pubkey`.
async fn connect_agent(agent: &Keys, owner: &Keys) -> BuzzTestClient {
    let tag_json =
        nip_oa::compute_auth_tag(owner, &agent.public_key(), "").expect("compute NIP-OA auth tag");
    let auth_tag = nip_oa::parse_auth_tag(&tag_json).expect("parse NIP-OA auth tag");
    let mut client = BuzzTestClient::connect_unauthenticated(&relay_url())
        .await
        .expect("connect agent");
    client
        .authenticate_with_nip_oa(agent, &auth_tag)
        .await
        .expect("agent NIP-OA auth");
    client
}

/// Send a kind:24210 control verb; returns the relay's OK `(accepted, message)`.
async fn control(
    client: &mut BuzzTestClient,
    keys: &Keys,
    verb: &str,
    decision_hex: &str,
    channel: Uuid,
    generation: Option<i64>,
    ttl_secs: Option<i64>,
) -> (bool, String) {
    let mut tags = vec![
        Tag::parse(["verb", verb]).unwrap(),
        Tag::parse(["decision", decision_hex]).unwrap(),
        Tag::parse(["h", &channel.to_string()]).unwrap(),
    ];
    if let Some(g) = generation {
        tags.push(Tag::parse(["generation", &g.to_string()]).unwrap());
    }
    if let Some(t) = ttl_secs {
        tags.push(Tag::parse(["ttl", &t.to_string()]).unwrap());
    }
    let event = EventBuilder::new(Kind::Custom(KIND_CONTACT_CONTROL), "")
        .tags(tags)
        .sign_with_keys(keys)
        .unwrap();
    let ok = client.send_event(event).await.expect("control verb");
    (ok.accepted, ok.message)
}

/// Publish a kind:46043 receipt e-tagging the decision with the claim tag.
async fn publish_receipt(
    client: &mut BuzzTestClient,
    agent: &Keys,
    channel: Uuid,
    decision_id: &str,
    root_id: &str,
    generation: i64,
) -> (bool, String) {
    let content = serde_json::json!({
        "summary": "contact fallback handled",
        "verify": "observed",
        "lights": [{"label": "route", "status": "green"}],
        "engineering": {}
    })
    .to_string();
    let event = EventBuilder::new(Kind::Custom(KIND_AGENT_RECEIPT), &content)
        .tags([
            Tag::parse(["h", &channel.to_string()]).unwrap(),
            Tag::parse(["e", root_id, "", "root"]).unwrap(),
            Tag::parse(["e", decision_id, "", "reply"]).unwrap(),
            Tag::parse(["claim", &format!("{decision_id}:{generation}")]).unwrap(),
        ])
        .sign_with_keys(agent)
        .unwrap();
    let ok = client.send_event(event).await.expect("receipt publish");
    (ok.accepted, ok.message)
}

fn tag_value<'a>(event: &'a nostr::Event, name: &str) -> Option<&'a str> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(name))
        .and_then(|t| t.as_slice().get(1).map(|s| s.as_str()))
}

fn is_decision_for(event: &nostr::Event, original_hex: &str) -> bool {
    event.kind.as_u16() == KIND_CONTACT_DECISION
        && tag_value(event, "original") == Some(original_hex)
}

/// Subscribe to decision proofs p-tagged to `contact`; returns the sub id.
async fn subscribe_decisions(
    contact_client: &mut BuzzTestClient,
    contact: &Keys,
    channel: Uuid,
) -> String {
    let sub = format!("decisions-{}", Uuid::new_v4());
    // Channel-scoped subscription: fan-out only matches channel events to subs
    // whose scope covers the channel (global subs see no channel traffic).
    contact_client
        .subscribe(
            &sub,
            vec![Filter::new()
                .kind(Kind::Custom(KIND_CONTACT_DECISION))
                .custom_tags(
                    SingleLetterTag::lowercase(Alphabet::H),
                    [channel.to_string()],
                )
                .custom_tags(
                    SingleLetterTag::lowercase(Alphabet::P),
                    [contact.public_key()],
                )],
        )
        .await
        .expect("subscribe decisions");
    sub
}

/// Wait for a live event on `sub` matching `pred`; `None` on timeout.
async fn next_match<F: Fn(&nostr::Event) -> bool>(
    client: &mut BuzzTestClient,
    sub: &str,
    pred: F,
    secs: u64,
) -> Option<nostr::Event> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match client.recv_event(remaining).await {
            Ok(RelayMessage::Event {
                subscription_id,
                event,
            }) if subscription_id == sub && pred(&event) => return Some(*event),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

struct Fixture {
    owner: Keys,
    contact: Keys,
    owner_client: BuzzTestClient,
    contact_client: BuzzTestClient,
    channel: Uuid,
}

async fn setup() -> Fixture {
    let owner = Keys::generate();
    let contact = Keys::generate();
    let mut owner_client = BuzzTestClient::connect(&relay_url(), &owner)
        .await
        .expect("owner connect");
    // Contact authenticates first so NIP-OA backfill materializes the
    // agent→owner row before the canvas binds it.
    let contact_client = connect_agent(&contact, &owner).await;
    let channel = create_channel(&mut owner_client, &owner, "route").await;
    set_contact_canvas(&mut owner_client, &owner, channel, &contact).await;
    Fixture {
        owner,
        contact,
        owner_client,
        contact_client,
        channel,
    }
}

async fn pg_scalar(sql: &'static str, id: &[u8; 32]) -> i64 {
    let pool = sqlx::PgPool::connect(&pg_url()).await.expect("pg connect");
    sqlx::query_scalar::<_, i64>(sql)
        .bind(id.to_vec())
        .fetch_one(&pool)
        .await
        .expect("pg query")
}

#[tokio::test]
#[ignore]
async fn e2e_contact_fallback_routes_and_receipt_completes_claim() {
    let Fixture {
        owner,
        contact,
        mut owner_client,
        mut contact_client,
        channel,
    } = setup().await;

    let original = capability_message(&owner, channel, "anyone around?");
    let original_hex = original.id.to_hex();
    let sub = subscribe_decisions(&mut contact_client, &contact, channel).await;
    let ok = owner_client
        .send_event(original.clone())
        .await
        .expect("original publish");
    assert!(ok.accepted, "original rejected: {}", ok.message);

    let decision = next_match(
        &mut contact_client,
        &sub,
        |e| is_decision_for(e, &original_hex),
        5,
    )
    .await
    .expect("decision proof must be delivered to the configured contact");
    let content: serde_json::Value = serde_json::from_str(&decision.content).unwrap();
    assert_eq!(content["outcome"], "routed");
    assert_eq!(content["original"], original_hex);
    assert_eq!(content["contact"], contact.public_key().to_hex());
    let decision_hex = decision.id.to_hex();

    // Exactly one route row + one pending claim for this original.
    assert_eq!(
        pg_scalar(
            "SELECT count(*) FROM contact_routes WHERE original_id = $1",
            original.id.as_bytes()
        )
        .await,
        1
    );
    assert_eq!(
        pg_scalar(
            "SELECT count(*) FROM contact_claims WHERE decision_id = $1 AND state = 'pending'",
            decision.id.as_bytes()
        )
        .await,
        1
    );

    let (ok, msg) = control(
        &mut contact_client,
        &contact,
        "claim",
        &decision_hex,
        channel,
        None,
        Some(300),
    )
    .await;
    assert!(ok, "claim rejected: {msg}");
    let generation: i64 = msg
        .strip_prefix("claim:")
        .and_then(|g| g.parse().ok())
        .expect("claim generation");
    let (ok, msg) = control(
        &mut contact_client,
        &contact,
        "start",
        &decision_hex,
        channel,
        Some(generation),
        None,
    )
    .await;
    assert!(ok, "start rejected: {msg}");

    let (ok, msg) = publish_receipt(
        &mut contact_client,
        &contact,
        channel,
        &decision_hex,
        &original_hex,
        generation,
    )
    .await;
    assert!(ok, "receipt rejected: {msg}");

    // Receipt readback: one 46043 bound to this decision.
    let sub2 = format!("receipts-{}", Uuid::new_v4());
    contact_client
        .subscribe(
            &sub2,
            vec![Filter::new()
                .kind(Kind::Custom(KIND_AGENT_RECEIPT))
                .custom_tags(
                    SingleLetterTag::lowercase(Alphabet::E),
                    [decision_hex.clone()],
                )],
        )
        .await
        .unwrap();
    let receipts = contact_client
        .collect_until_eose(&sub2, Duration::from_secs(5))
        .await
        .expect("receipt readback");
    assert_eq!(receipts.len(), 1, "exactly one receipt for the decision");
    assert_eq!(receipts[0].pubkey, contact.public_key());

    assert_eq!(
        pg_scalar(
            "SELECT count(*) FROM contact_claims WHERE decision_id = $1 AND state = 'completed'",
            decision.id.as_bytes()
        )
        .await,
        1
    );
}

#[tokio::test]
#[ignore]
async fn e2e_contact_fallback_duplicate_ingest_replays_one_decision() {
    let Fixture {
        owner,
        contact,
        mut owner_client,
        mut contact_client,
        channel,
    } = setup().await;

    let original = capability_message(&owner, channel, "dup ingest probe");
    let ok = owner_client
        .send_event(original.clone())
        .await
        .expect("first publish");
    assert!(ok.accepted, "first original rejected: {}", ok.message);
    // Duplicate ingest over a second connection (replay path).
    let mut second = BuzzTestClient::connect(&relay_url(), &owner).await.unwrap();
    let ok2 = second
        .send_event(original.clone())
        .await
        .expect("duplicate publish");
    assert!(
        ok2.accepted,
        "duplicate ingest must stay client-clean: {}",
        ok2.message
    );

    let sub = subscribe_decisions(&mut contact_client, &contact, channel).await;
    let decisions = contact_client
        .collect_until_eose(&sub, Duration::from_secs(5))
        .await
        .expect("decision readback");
    let for_original: Vec<_> = decisions
        .iter()
        .filter(|e| is_decision_for(e, &original.id.to_hex()))
        .collect();
    assert_eq!(for_original.len(), 1, "one decision per original");
    assert_eq!(
        pg_scalar(
            "SELECT count(*) FROM contact_routes WHERE original_id = $1",
            original.id.as_bytes()
        )
        .await,
        1
    );
}

#[tokio::test]
#[ignore]
async fn e2e_contact_fallback_crash_before_receipt_no_loss() {
    let Fixture {
        owner,
        contact,
        mut owner_client,
        mut contact_client,
        channel,
    } = setup().await;

    let original = capability_message(&owner, channel, "crash probe");
    let sub = subscribe_decisions(&mut contact_client, &contact, channel).await;
    owner_client
        .send_event(original.clone())
        .await
        .expect("original publish");
    let decision = next_match(
        &mut contact_client,
        &sub,
        |e| is_decision_for(e, &original.id.to_hex()),
        5,
    )
    .await
    .expect("decision");
    let decision_hex = decision.id.to_hex();

    // Claim with a short lease, then "crash" — hard disconnect, no start.
    let (ok, msg) = control(
        &mut contact_client,
        &contact,
        "claim",
        &decision_hex,
        channel,
        None,
        Some(5),
    )
    .await;
    assert!(ok, "first claim rejected: {msg}");
    let gen1: i64 = msg.strip_prefix("claim:").unwrap().parse().unwrap();
    drop(contact_client); // crash mid-publish: no start, no receipt

    // Live lease keeps the claim idempotent for the same holder.
    let mut rejoined = connect_agent(&contact, &owner).await;
    let (ok, msg) = control(
        &mut rejoined,
        &contact,
        "claim",
        &decision_hex,
        channel,
        None,
        Some(60),
    )
    .await;
    assert!(ok, "re-claim by same holder must be idempotent: {msg}");
    let gen_again: i64 = msg.strip_prefix("claim:").unwrap().parse().unwrap();
    assert_eq!(
        gen_again, gen1,
        "idempotent re-claim returns same generation"
    );

    // A different key cannot steal the claim while it is held.
    let outsider = Keys::generate();
    let mut outsider_client = connect_agent(&outsider, &owner).await;
    let (ok, msg) = control(
        &mut outsider_client,
        &outsider,
        "claim",
        &decision_hex,
        channel,
        None,
        Some(60),
    )
    .await;
    assert!(!ok, "outsider claim must be denied, got: {msg}");

    // Let the lease lapse; the holder re-claims the fenced decision.
    tokio::time::sleep(Duration::from_secs(7)).await;
    let (ok, msg) = control(
        &mut rejoined,
        &contact,
        "claim",
        &decision_hex,
        channel,
        None,
        Some(60),
    )
    .await;
    assert!(ok, "post-expiry claim rejected: {msg}");
    let gen2: i64 = msg.strip_prefix("claim:").unwrap().parse().unwrap();
    assert!(gen2 > gen1, "lease expiry must bump the generation");

    let (ok, msg) = control(
        &mut rejoined,
        &contact,
        "start",
        &decision_hex,
        channel,
        Some(gen2),
        None,
    )
    .await;
    assert!(ok, "start after reclaim rejected: {msg}");
    let (ok, msg) = publish_receipt(
        &mut rejoined,
        &contact,
        channel,
        &decision_hex,
        &original.id.to_hex(),
        gen2,
    )
    .await;
    assert!(ok, "receipt after crash rejected: {msg}");

    // No duplicate, no loss: one route, one receipt, claim completed.
    assert_eq!(
        pg_scalar(
            "SELECT count(*) FROM contact_routes WHERE original_id = $1",
            original.id.as_bytes()
        )
        .await,
        1
    );
    assert_eq!(
        pg_scalar(
            "SELECT count(*) FROM contact_claims WHERE decision_id = $1 AND state = 'completed'",
            decision.id.as_bytes()
        )
        .await,
        1
    );
}

#[tokio::test]
#[ignore]
async fn e2e_contact_fallback_mention_and_plain_message_do_not_route() {
    let Fixture {
        owner,
        contact,
        mut owner_client,
        mut contact_client,
        channel,
    } = setup().await;

    // Explicit mention suppresses routing.
    let mentioned = EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE), "@contact ping")
        .tags([
            Tag::parse(["h", &channel.to_string()]).unwrap(),
            Tag::parse([CAPABILITY_TAG, "1"]).unwrap(),
            Tag::parse(["p", &contact.public_key().to_hex()]).unwrap(),
        ])
        .sign_with_keys(&owner)
        .unwrap();
    owner_client.send_event(mentioned.clone()).await.unwrap();
    // A capability-less message never enters the decision path at all.
    let plain = EventBuilder::new(Kind::Custom(KIND_STREAM_MESSAGE), "just chatter")
        .tags([Tag::parse(["h", &channel.to_string()]).unwrap()])
        .sign_with_keys(&owner)
        .unwrap();
    owner_client.send_event(plain.clone()).await.unwrap();

    let sub = subscribe_decisions(&mut contact_client, &contact, channel).await;
    let stray = next_match(
        &mut contact_client,
        &sub,
        |e| is_decision_for(e, &mentioned.id.to_hex()) || is_decision_for(e, &plain.id.to_hex()),
        5,
    )
    .await;
    assert!(
        stray.is_none(),
        "no decision may be produced for a mentioned or capability-less message"
    );
}
