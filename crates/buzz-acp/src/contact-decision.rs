/// Crew contact-fallback consumption (issue #412, successor contract v4).
///
/// A kind:46044 decision proof is the only routing authority an agent may
/// act on for a mention-less message. `verified_contact_decision` mirrors
/// [`verified_workflow_owner`]: cryptographic relay signature (NIP-11 `self`
/// key), routed outcome, single canonical tags, and the content payload must
/// agree with the tags. Anything else returns `None` — the caller treats an
/// unverifiable event as no routing authority at all.
use nostr::{Event, EventId, JsonUtil};

/// Prompt tag carried on queued contact-fallback turns; mirrors
/// `CONTACT_CAPABILITY_TAG` in `buzz-db` (that crate is not a dependency here).
const CONTACT_PROMPT_TAG: &str = "crew-contact-fallback";

/// A cryptographically proven routed decision addressed to this agent.
pub(crate) struct VerifiedContactDecision {
    /// Decision event id (hex) — also the claim key.
    pub(crate) decision_id_hex: String,
    /// Channel the routed original lives in.
    pub(crate) channel_id: Uuid,
    /// Mention-less original event id (hex).
    pub(crate) original_id_hex: String,
}

fn single_tag<'a>(event: &'a Event, name: &str) -> Option<&'a String> {
    let tags: Vec<&[String]> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice())
        .filter(|values| values.first().map(String::as_str) == Some(name))
        .collect();
    let [tag] = tags.as_slice() else {
        return None;
    };
    tag.get(1)
}

/// Verify a kind:46044 event as a routed contact decision for this agent.
///
/// Returns `None` for any deviation: wrong kind, not relay-signed, unverifiable
/// signature, missing/extra canonical tags, tag/content disagreement, a
/// `no_route` outcome, or a `p` target that is not this agent. Verification
/// is fail-closed — relay identity must be known (fetched via NIP-11) for the
/// decision to count.
fn verified_contact_decision(
    event: &Event,
    relay_self: Option<&str>,
    agent_pubkey_hex: &str,
) -> Option<VerifiedContactDecision> {
    if event.kind.as_u16() as u32 != KIND_CONTACT_DECISION {
        return None;
    }
    let relay_self = PublicKey::from_hex(relay_self?).ok()?;
    if event.pubkey != relay_self || event.verify().is_err() {
        return None;
    }

    let content: serde_json::Value = serde_json::from_str(&event.content).ok()?;
    if content.get("v")?.as_u64()? != 1 {
        return None;
    }
    if content.get("outcome")?.as_str()? != "routed" {
        return None;
    }

    // Every canonical tag must appear exactly once and agree with the signed
    // content payload — a tag/content mismatch means a fabricated decision.
    let original_hex = single_tag(event, "original")?;
    let canvas_hex = single_tag(event, "canvas")?;
    let channel_text = single_tag(event, "h")?;
    let contact_hex = single_tag(event, "p")?;
    if single_tag(event, "phase").map(String::as_str) != Some("decision") {
        return None;
    }
    if content.get("original")?.as_str()? != original_hex
        || content.get("canvas")?.as_str()? != canvas_hex
        || content.get("channel")?.as_str()? != channel_text
        || content.get("contact")?.as_str()? != contact_hex
    {
        return None;
    }

    // The decision must be threaded as a reply to its original.
    let reply_targets: Vec<&[String]> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice())
        .filter(|values| values.first().map(String::as_str) == Some("e"))
        .filter(|values| values.get(3).map(String::as_str) == Some("reply"))
        .collect();
    let [reply] = reply_targets.as_slice() else {
        return None;
    };
    if reply.get(1)? != original_hex {
        return None;
    }

    // Only the configured contact may consume the decision (invariant #1).
    let agent_pubkey = PublicKey::from_hex(agent_pubkey_hex).ok()?.to_hex();
    if contact_hex.as_str() != agent_pubkey {
        return None;
    }

    let channel_id = Uuid::parse_str(channel_text).ok()?;
    Some(VerifiedContactDecision {
        decision_id_hex: event.id.to_hex(),
        channel_id,
        original_id_hex: original_hex.clone(),
    })
}

/// Consume a verified routed contact decision for this agent.
///
/// Binds the ACP side of the contract: fetch the routed original from the
/// relay, run the original author through the inbound gate (the decision is
/// relay-signed, so the human the prompt is attributed to is the original's
/// effective author), take the `claim` → `start` handshake over kind:24210,
/// then queue the *decision* event as the prompt trigger — with the original's
/// content as `edited_content`. The terminal receipt therefore e-tags the
/// decision, which lets the relay bind the `claim` tag and complete the claim
/// in the receipt's own transaction.
///
/// Returns `true` when the decision was claimed, started, and queued.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn consume_contact_decision(
    ctx: &pool::PromptContext,
    author_gate: &mut inbound_author_gate::InboundAuthorGate,
    buzz_event: &relay::BuzzEvent,
    respond_to: &RespondTo,
    allowlist: &HashSet<String>,
    owner_cache: &OwnerCache,
    publisher: &relay::RelayEventPublisher,
    queue: &mut EventQueue,
) -> bool {
    let agent_pubkey_hex = ctx.agent_keys.public_key().to_hex();
    let Some(decision) = verified_contact_decision(
        &buzz_event.event,
        author_gate.relay_identity(),
        &agent_pubkey_hex,
    ) else {
        tracing::debug!(
            event_id = %buzz_event.event.id,
            "dropping unverifiable contact decision"
        );
        return false;
    };

    // Idempotent intake: a decision already claimed by this process was queued
    // the first time (relay replays stored proofs — at-least-once delivery).
    if let Ok(claims) = ctx.contact_claims.lock() {
        if claims.contains_key(&decision.decision_id_hex) {
            tracing::debug!(
                decision = %decision.decision_id_hex,
                "contact decision replay — already claimed locally"
            );
            return false;
        }
    }

    // Fetch the routed original: the prompt body is authored content, so it
    // comes from the original — the decision proof carries no user text.
    let Ok(original_id) = EventId::from_hex(&decision.original_id_hex) else {
        return false;
    };
    let fetched = ctx
        .rest_client
        .query(&[nostr::Filter::new().id(original_id)])
        .await
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let mut original: Option<nostr::Event> = None;
    for value in fetched {
        let candidate = nostr::Event::from_json(value.to_string());
        if let Ok(candidate) = candidate {
            if candidate.id == original_id
                && candidate.kind.as_u16() as u32 == buzz_core::kind::KIND_STREAM_MESSAGE
                && candidate.verify().is_ok()
            {
                original = Some(candidate);
                break;
            }
        }
    }
    let Some(original) = original else {
        tracing::warn!(
            decision = %decision.decision_id_hex,
            original = %decision.original_id_hex,
            "contact decision routed an original we cannot fetch/verify"
        );
        return false;
    };

    // Authorize the original's author — the decision relays their prompt.
    let authorized = author_gate
        .authorize_listener_event(
            relay::BuzzEvent {
                connection_generation: buzz_event.connection_generation,
                channel_id: decision.channel_id,
                event: original.clone(),
            },
            respond_to,
            allowlist,
            owner_cache,
            &ctx.channel_info,
            &ctx.rest_client,
        )
        .await;
    let Some(authorized) = authorized else {
        return false;
    };

    // Claim → start handshake over kind:24210. Both verbs must be acked by the
    // relay before the prompt is queued; a silent drop must never look like a
    // granted claim.
    let claimed = publisher
        .contact_control(
            &ctx.agent_keys,
            "claim",
            &decision.decision_id_hex,
            decision.channel_id,
            None,
            Some(300),
        )
        .await;
    let generation = match claimed {
        Ok((true, message)) => message
            .strip_prefix("claim:")
            .and_then(|digits| digits.parse::<i64>().ok()),
        Ok((false, message)) => {
            tracing::debug!(
                decision = %decision.decision_id_hex,
                reason = %message,
                "contact claim denied — another holder or expired decision"
            );
            None
        }
        Err(error) => {
            tracing::debug!(
                decision = %decision.decision_id_hex,
                %error,
                "contact claim transport failure"
            );
            None
        }
    };
    let Some(generation) = generation else {
        return false;
    };
    let started = publisher
        .contact_control(
            &ctx.agent_keys,
            "start",
            &decision.decision_id_hex,
            decision.channel_id,
            Some(generation),
            None,
        )
        .await;
    let start_accepted = match started {
        Ok((accepted, message)) => {
            if !accepted {
                tracing::debug!(
                    decision = %decision.decision_id_hex,
                    reason = %message,
                    "contact claim start refused"
                );
            }
            accepted
        }
        Err(error) => {
            tracing::debug!(
                decision = %decision.decision_id_hex,
                %error,
                "contact claim start transport failure"
            );
            false
        }
    };
    if !start_accepted {
        // Best-effort release: otherwise the lease holds until expiry.
        let _ = publisher
            .contact_control(
                &ctx.agent_keys,
                "cancel",
                &decision.decision_id_hex,
                decision.channel_id,
                Some(generation),
                None,
            )
            .await;
        return false;
    }

    let conversation_id = conversation::id_for_event(
        decision.channel_id,
        &buzz_event.event,
        authorized.channel_is_dm(),
    );
    let decision_id_hex = decision.decision_id_hex;
    let accepted = queue.push(QueuedEvent {
        channel_id: conversation_id,
        event: buzz_event.event.clone(),
        received_at: std::time::Instant::now(),
        prompt_tag: CONTACT_PROMPT_TAG.to_string(),
        edited_content: Some(original.content.clone()),
        hold_exempt: false,
    });
    if !accepted {
        // Queue refused (dedup/drop) — release the claim so the decision can
        // be taken by a later replay rather than dying on this lease.
        let _ = publisher
            .contact_control(
                &ctx.agent_keys,
                "cancel",
                &decision_id_hex,
                decision.channel_id,
                Some(generation),
                None,
            )
            .await;
        return false;
    }
    if let Ok(mut claims) = ctx.contact_claims.lock() {
        claims.insert(decision_id_hex, generation);
    }
    let rest = ctx.rest_client.clone();
    let seen_id = decision.original_id_hex.clone();
    tokio::spawn(async move {
        pool::reaction_add(&rest, &seen_id, "\u{1f440}").await;
    });
    true
}

#[cfg(test)]
mod contact_decision_tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn decision_event(
        relay_keys: &Keys,
        agent_pubkey_hex: &str,
        community: Uuid,
        channel: Uuid,
        original: &nostr::EventId,
        canvas: &nostr::EventId,
    ) -> Event {
        let content = serde_json::json!({
            "v": 1,
            "outcome": "routed",
            "community": community.to_string(),
            "channel": channel.to_string(),
            "original": original.to_hex(),
            "canvas": canvas.to_hex(),
            "contact": agent_pubkey_hex,
        })
        .to_string();
        EventBuilder::new(Kind::Custom(KIND_CONTACT_DECISION as u16), content)
            .tags(vec![
                Tag::parse(["h", &channel.to_string()]).unwrap(),
                Tag::parse(["p", agent_pubkey_hex]).unwrap(),
                Tag::parse(["original", &original.to_hex()]).unwrap(),
                Tag::parse(["canvas", &canvas.to_hex()]).unwrap(),
                Tag::parse(["phase", "decision"]).unwrap(),
                Tag::parse(["e", &original.to_hex(), "", "reply"]).unwrap(),
            ])
            .sign_with_keys(relay_keys)
            .unwrap()
    }

    #[test]
    fn valid_routed_decision_verifies() {
        let relay = Keys::generate();
        let agent = Keys::generate();
        let original_id = nostr::EventId::from_byte_array([7u8; 32]);
        let canvas_id = nostr::EventId::from_byte_array([9u8; 32]);
        let channel = Uuid::new_v4();
        let event = decision_event(
            &relay,
            &agent.public_key().to_hex(),
            Uuid::new_v4(),
            channel,
            &original_id,
            &canvas_id,
        );
        let verified = verified_contact_decision(
            &event,
            Some(&relay.public_key().to_hex()),
            &agent.public_key().to_hex(),
        )
        .expect("valid decision");
        assert_eq!(verified.decision_id_hex, event.id.to_hex());
        assert_eq!(verified.channel_id, channel);
        assert_eq!(verified.original_id_hex, original_id.to_hex());
    }

    #[test]
    fn rejects_wrong_kind() {
        let relay = Keys::generate();
        let agent = Keys::generate();
        let event = EventBuilder::text_note("x").sign_with_keys(&relay).unwrap();
        assert!(verified_contact_decision(
            &event,
            Some(&relay.public_key().to_hex()),
            &agent.public_key().to_hex()
        )
        .is_none());
    }

    #[test]
    fn rejects_non_relay_signer() {
        let relay = Keys::generate();
        let impostor = Keys::generate();
        let agent = Keys::generate();
        let event = decision_event(
            &impostor,
            &agent.public_key().to_hex(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            &nostr::EventId::from_byte_array([7u8; 32]),
            &nostr::EventId::from_byte_array([9u8; 32]),
        );
        assert!(verified_contact_decision(
            &event,
            Some(&relay.public_key().to_hex()),
            &agent.public_key().to_hex()
        )
        .is_none());
    }

    #[test]
    fn rejects_unknown_relay_identity() {
        let relay = Keys::generate();
        let agent = Keys::generate();
        let event = decision_event(
            &relay,
            &agent.public_key().to_hex(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            &nostr::EventId::from_byte_array([7u8; 32]),
            &nostr::EventId::from_byte_array([9u8; 32]),
        );
        assert!(verified_contact_decision(&event, None, &agent.public_key().to_hex()).is_none());
    }

    #[test]
    fn rejects_no_route_outcome() {
        let relay = Keys::generate();
        let agent = Keys::generate();
        let agent_hex = agent.public_key().to_hex();
        let original = nostr::EventId::from_byte_array([7u8; 32]);
        let canvas = nostr::EventId::from_byte_array([9u8; 32]);
        let channel = Uuid::new_v4();
        let community = Uuid::new_v4();
        let content = serde_json::json!({
            "v": 1, "outcome": "no_route", "reason": "quota_exhausted",
            "community": community.to_string(), "channel": channel.to_string(),
            "original": original.to_hex(), "canvas": canvas.to_hex(),
            "contact": agent_hex,
        })
        .to_string();
        let event = EventBuilder::new(Kind::Custom(KIND_CONTACT_DECISION as u16), content)
            .tags(vec![
                Tag::parse(["h", &channel.to_string()]).unwrap(),
                Tag::parse(["p", &agent_hex]).unwrap(),
                Tag::parse(["original", &original.to_hex()]).unwrap(),
                Tag::parse(["canvas", &canvas.to_hex()]).unwrap(),
                Tag::parse(["phase", "decision"]).unwrap(),
                Tag::parse(["e", &original.to_hex(), "", "reply"]).unwrap(),
            ])
            .sign_with_keys(&relay)
            .unwrap();
        assert!(verified_contact_decision(
            &event,
            Some(&relay.public_key().to_hex()),
            &agent_hex
        )
        .is_none());
    }

    #[test]
    fn rejects_decision_for_another_contact() {
        let relay = Keys::generate();
        let agent = Keys::generate();
        let other = Keys::generate();
        let event = decision_event(
            &relay,
            &other.public_key().to_hex(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            &nostr::EventId::from_byte_array([7u8; 32]),
            &nostr::EventId::from_byte_array([9u8; 32]),
        );
        assert!(verified_contact_decision(
            &event,
            Some(&relay.public_key().to_hex()),
            &agent.public_key().to_hex()
        )
        .is_none());
    }

    #[test]
    fn rejects_tag_content_mismatch() {
        // Signature is valid — the signed payload itself disagrees with a tag.
        let relay = Keys::generate();
        let agent = Keys::generate();
        let agent_hex = agent.public_key().to_hex();
        let original = nostr::EventId::from_byte_array([7u8; 32]);
        let canvas = nostr::EventId::from_byte_array([9u8; 32]);
        let channel = Uuid::new_v4();
        let community = Uuid::new_v4();
        let content = serde_json::json!({
            "v": 1, "outcome": "routed",
            "community": community.to_string(), "channel": channel.to_string(),
            "original": original.to_hex(),
            "canvas": canvas.to_hex(),
            "contact": agent_hex,
        })
        .to_string();
        let forged = nostr::EventId::from_byte_array([8u8; 32]);
        let event = EventBuilder::new(Kind::Custom(KIND_CONTACT_DECISION as u16), content)
            .tags(vec![
                Tag::parse(["h", &channel.to_string()]).unwrap(),
                Tag::parse(["p", &agent_hex]).unwrap(),
                Tag::parse(["original", &forged.to_hex()]).unwrap(),
                Tag::parse(["canvas", &canvas.to_hex()]).unwrap(),
                Tag::parse(["phase", "decision"]).unwrap(),
                Tag::parse(["e", &forged.to_hex(), "", "reply"]).unwrap(),
            ])
            .sign_with_keys(&relay)
            .unwrap();
        assert!(verified_contact_decision(
            &event,
            Some(&relay.public_key().to_hex()),
            &agent_hex
        )
        .is_none());
    }

    #[test]
    fn rejects_duplicate_original_tags() {
        let relay = Keys::generate();
        let agent = Keys::generate();
        let agent_hex = agent.public_key().to_hex();
        let original = nostr::EventId::from_byte_array([7u8; 32]);
        let canvas = nostr::EventId::from_byte_array([9u8; 32]);
        let channel = Uuid::new_v4();
        let community = Uuid::new_v4();
        let content = serde_json::json!({
            "v": 1, "outcome": "routed",
            "community": community.to_string(), "channel": channel.to_string(),
            "original": original.to_hex(), "canvas": canvas.to_hex(),
            "contact": agent_hex,
        })
        .to_string();
        let event = EventBuilder::new(Kind::Custom(KIND_CONTACT_DECISION as u16), content)
            .tags(vec![
                Tag::parse(["h", &channel.to_string()]).unwrap(),
                Tag::parse(["p", &agent_hex]).unwrap(),
                Tag::parse(["original", &original.to_hex()]).unwrap(),
                Tag::parse(["original", &original.to_hex()]).unwrap(),
                Tag::parse(["canvas", &canvas.to_hex()]).unwrap(),
                Tag::parse(["phase", "decision"]).unwrap(),
                Tag::parse(["e", &original.to_hex(), "", "reply"]).unwrap(),
            ])
            .sign_with_keys(&relay)
            .unwrap();
        assert!(verified_contact_decision(
            &event,
            Some(&relay.public_key().to_hex()),
            &agent_hex
        )
        .is_none());
    }
}
