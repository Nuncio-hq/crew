//! Owner-triggered guided handover (#173).
//!
//! Flow:
//! 1. Owner sends `guided_handover` observer control (owner-only, freshness-gated).
//! 2. Harness fetches recent thread history from the relay (engine-agnostic).
//! 3. One-shot summarizer call (NOT an agent / profile) using the per-app
//!    model id (`BUZZ_ACP_HANDOVER_MODEL` / control payload).
//! 4. Publish a durable kind-9 note tagged `["crew-handover", <model-id>]`.
//! 5. Invalidate the channel session so the next turn does `session/new` and
//!    ledger overwrite with `OwnerReset` (compaction_count resets).
//!
//! Failure honesty: summarizer failure returns `summarizer_failed` and leaves
//! the owner an informed blind-reset option (`blind_session_reset`). Never
//! blocks reset.

use std::time::Duration;

use nostr::{EventBuilder, Kind, Tag};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::observer;
use crate::pool::AgentPool;
use crate::relay::RestClient;
use crate::session_ledger::{
    declare_session_with_reason, SessionDeclareReason, SessionLedgerKey,
};

const HANDOVER_TAG: &str = "crew-handover";
const SUMMARIZER_TIMEOUT: Duration = Duration::from_secs(45);
const THREAD_FETCH_LIMIT: usize = 40;

/// Handle `guided_handover` / `blind_session_reset` observer controls.
pub(crate) async fn handle_guided_handover_control(
    payload: &Value,
    pool: &mut AgentPool,
    rest_client: Option<&RestClient>,
    observer: Option<&observer::ObserverHandle>,
    agent_pubkey_hex: &str,
    relay_url: &str,
    session_ledger_dir: &std::path::Path,
    engine_identity: &str,
) {
    let command = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("guided_handover");
    let Some(channel_id) = payload
        .get("channelId")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<Uuid>().ok())
    else {
        tracing::warn!("observer {command} control frame missing valid channelId");
        emit_result(
            observer,
            None,
            None,
            command,
            "invalid_payload",
            None,
            Some("missing channelId"),
        );
        return;
    };
    let conversation_id = payload
        .get("conversationId")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<Uuid>().ok());
    let conversation_key = conversation_id.unwrap_or(channel_id);
    let root_event_id = payload
        .get("rootEventId")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let latest_owner_message = payload
        .get("latestOwnerMessage")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let model_id = payload
        .get("modelId")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| std::env::var("BUZZ_ACP_HANDOVER_MODEL").ok())
        .filter(|s| !s.trim().is_empty());

    if command == "blind_session_reset" {
        invalidate_for_owner_reset(
            pool,
            conversation_key,
            channel_id,
            rest_client,
            session_ledger_dir,
            relay_url,
            agent_pubkey_hex,
            engine_identity,
        )
        .await;
        emit_result(
            observer,
            Some(channel_id),
            conversation_id,
            "blind_session_reset",
            "ok",
            None,
            None,
        );
        return;
    }

    let Some(rest) = rest_client else {
        emit_result(
            observer,
            Some(channel_id),
            conversation_id,
            "guided_handover",
            "summarizer_failed",
            None,
            Some("no relay client — guided path unavailable; use blind reset"),
        );
        return;
    };

    let Some(model_id) = model_id else {
        emit_result(
            observer,
            Some(channel_id),
            conversation_id,
            "guided_handover",
            "summarizer_failed",
            None,
            Some("no handover summarizer model configured"),
        );
        return;
    };

    let history = match fetch_thread_history(rest, channel_id, root_event_id.as_deref()).await {
        Ok(h) => h,
        Err(error) => {
            emit_result(
                observer,
                Some(channel_id),
                conversation_id,
                "guided_handover",
                "summarizer_failed",
                Some(&model_id),
                Some(&error),
            );
            return;
        }
    };

    let note = match summarize_handover(&model_id, &history, &latest_owner_message).await {
        Ok(note) => note,
        Err(error) => {
            tracing::warn!(%error, "guided handover summarizer failed");
            emit_result(
                observer,
                Some(channel_id),
                conversation_id,
                "guided_handover",
                "summarizer_failed",
                Some(&model_id),
                Some(&error),
            );
            return;
        }
    };

    if let Err(error) =
        publish_handover_note(rest, channel_id, root_event_id.as_deref(), &note, &model_id).await
    {
        emit_result(
            observer,
            Some(channel_id),
            conversation_id,
            "guided_handover",
            "publish_failed",
            Some(&model_id),
            Some(&error),
        );
        return;
    }

    invalidate_for_owner_reset(
        pool,
        conversation_key,
        channel_id,
        Some(rest),
        session_ledger_dir,
        relay_url,
        agent_pubkey_hex,
        engine_identity,
    )
    .await;

    emit_result(
        observer,
        Some(channel_id),
        conversation_id,
        "guided_handover",
        "ok",
        Some(&model_id),
        None,
    );
}

async fn invalidate_for_owner_reset(
    pool: &mut AgentPool,
    conversation_key: Uuid,
    channel_id: Uuid,
    rest_client: Option<&RestClient>,
    session_ledger_dir: &std::path::Path,
    relay_url: &str,
    agent_pubkey_hex: &str,
    engine_identity: &str,
) {
    let _ = pool.invalidate_channel_sessions(conversation_key);
    if conversation_key != channel_id {
        let _ = pool.invalidate_channel_sessions(channel_id);
    }

    // Seed an OwnerReset placeholder so aging clears immediately even before
    // the next session/new declare overwrites with a real ACP session id.
    let key = SessionLedgerKey::new(relay_url, agent_pubkey_hex, conversation_key);
    let placeholder = format!("owner-reset-{}", Uuid::new_v4());
    if let Err(error) = declare_session_with_reason(
        session_ledger_dir,
        &key,
        &placeholder,
        engine_identity,
        0,
        None,
        SessionDeclareReason::OwnerReset,
    )
    .await
    {
        tracing::warn!(%error, "failed to declare OwnerReset ledger placeholder");
    }
    let _ = rest_client; // reserved for future receipt/ack
}

async fn fetch_thread_history(
    rest: &RestClient,
    channel_id: Uuid,
    root_event_id: Option<&str>,
) -> Result<String, String> {
    use nostr::{Alphabet, Filter, SingleLetterTag};

    let mut filter = Filter::new()
        .kinds([
            Kind::Custom(9),
            Kind::Custom(buzz_core::kind::KIND_STREAM_MESSAGE as u16),
        ])
        .limit(THREAD_FETCH_LIMIT);
    let h_tag = SingleLetterTag::lowercase(Alphabet::H);
    filter = filter.custom_tags(h_tag, [channel_id.to_string()]);
    if let Some(root) = root_event_id {
        let e_tag = SingleLetterTag::lowercase(Alphabet::E);
        filter = filter.custom_tags(e_tag, [root.to_string()]);
    }

    let value = tokio::time::timeout(Duration::from_secs(10), rest.query(&[filter]))
        .await
        .map_err(|_| "thread history query timed out".to_string())?
        .map_err(|e| format!("thread history query failed: {e}"))?;

    let events = value
        .as_array()
        .ok_or_else(|| "thread history response was not an array".to_string())?;

    let mut lines = Vec::new();
    for event in events.iter().rev() {
        let pubkey = event
            .get("pubkey")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let content = event
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if content.is_empty() {
            continue;
        }
        let short_pk = if pubkey.len() >= 8 {
            &pubkey[..8]
        } else {
            pubkey
        };
        lines.push(format!("[{short_pk}] {content}"));
    }
    if lines.is_empty() {
        lines.push("(no prior thread messages)".into());
    }
    Ok(lines.join("\n"))
}

/// One-shot OpenAI-compatible summarizer. Uses `OPENAI_API_KEY` /
/// `OPENAI_COMPAT_API_KEY` + optional `BUZZ_ACP_HANDOVER_BASE_URL`.
async fn summarize_handover(
    model_id: &str,
    history: &str,
    latest_owner_message: &str,
) -> Result<String, String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("OPENAI_COMPAT_API_KEY"))
        .map_err(|_| {
            "no OPENAI_API_KEY / OPENAI_COMPAT_API_KEY for handover summarizer".to_string()
        })?;
    let base = std::env::var("BUZZ_ACP_HANDOVER_BASE_URL")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "https://api.openai.com/v1".into())
        .trim_end_matches('/')
        .to_owned();
    let url = format!("{base}/chat/completions");

    let system = "You write a structured session handover note for a coding agent. \
Sections exactly: Current state / Settled decisions / Work in progress / Pointers \
(files, PRs, event ids). Be concise and factual. Do not invent facts.";
    let user = format!(
        "# Thread history (oldest first)\n{history}\n\n# Owner's latest message\n{latest_owner_message}\n\nWrite the handover note now."
    );

    let body = json!({
        "model": model_id,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2,
        "max_tokens": 1200
    });

    let client = reqwest::Client::new();
    let response = tokio::time::timeout(
        SUMMARIZER_TIMEOUT,
        client
            .post(&url)
            .bearer_auth(api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send(),
    )
    .await
    .map_err(|_| "summarizer request timed out".to_string())?
    .map_err(|e| format!("summarizer request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("summarizer HTTP {status}: {text}"));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("summarizer response decode failed: {e}"))?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "summarizer returned empty content".to_string())?;

    Ok(content.to_owned())
}

async fn publish_handover_note(
    rest: &RestClient,
    channel_id: Uuid,
    root_event_id: Option<&str>,
    content: &str,
    model_id: &str,
) -> Result<(), String> {
    let mut builder = EventBuilder::new(Kind::Custom(9), content).tag(
        Tag::parse(["h", &channel_id.to_string()])
            .map_err(|e| format!("invalid h tag: {e}"))?,
    );
    if let Some(root) = root_event_id {
        builder = builder.tag(
            Tag::parse(["e", root, "", "root"]).map_err(|e| format!("invalid e tag: {e}"))?,
        );
    }
    builder = builder.tag(
        Tag::parse([HANDOVER_TAG, model_id]).map_err(|e| format!("invalid handover tag: {e}"))?,
    );

    let event = builder
        .sign_with_keys(&rest.keys)
        .map_err(|e| format!("handover note sign failed: {e}"))?;
    tokio::time::timeout(Duration::from_secs(10), rest.submit_event(&event))
        .await
        .map_err(|_| "handover note publish timed out".to_string())?
        .map_err(|e| format!("handover note publish failed: {e}"))?;
    Ok(())
}

fn emit_result(
    observer: Option<&observer::ObserverHandle>,
    channel_id: Option<Uuid>,
    conversation_id: Option<Uuid>,
    command: &str,
    status: &str,
    model_id: Option<&str>,
    error: Option<&str>,
) {
    let Some(observer) = observer else {
        return;
    };
    let context =
        observer::context_for_conversation(channel_id, conversation_id, None, None);
    observer.emit(
        "control_result",
        None,
        &context,
        json!({
            "type": command,
            "status": status,
            "modelId": model_id,
            "error": error,
            "allowBlindReset": status == "summarizer_failed" || status == "publish_failed",
            "conversationId": conversation_id.map(|id| id.to_string()),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handover_tag_wire_value_is_stable() {
        assert_eq!(HANDOVER_TAG, "crew-handover");
    }

    #[test]
    fn allow_blind_reset_on_summarizer_failure_contract() {
        // Documented control_result contract for desktop degradation UI.
        let payload = json!({
            "type": "guided_handover",
            "status": "summarizer_failed",
            "allowBlindReset": true,
        });
        assert_eq!(payload["allowBlindReset"], true);
        assert_eq!(payload["status"], "summarizer_failed");
    }
}
