//! Idempotent offboarding uses the existing atomic retention transaction.
use crate::managed_agents::{
    agent_events::build_agent_delete,
    persona_events::monotonic_created_at,
    retention::{
        delete_retained_event, get_retained_event, open_retention_db, retain_event,
        tombstone_retention_d_tag, RetainedEvent,
    },
};
use buzz_core_pkg::kind::{KIND_IA_ARCHIVE_REQUEST, KIND_MANAGED_AGENT};
use nostr::{Event, EventBuilder, JsonUtil, Keys, Tag};
use std::path::Path;

/// Nonsecret references; signed envelopes stay in the existing retention store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentOffboardingReceipt {
    pub tombstone_id: String,
    pub archive_id: String,
}

/// Resume the exact offboarding transaction for one durable delete intent.
/// A different/current head or an unresolved older pair requires review.
pub(crate) fn enqueue_agent_offboarding_at(
    path: &Path,
    keys: &Keys,
    agent: &str,
    operation_id: &str,
    expected_head: Option<&str>,
) -> Result<AgentOffboardingReceipt, String> {
    let id = uuid::Uuid::parse_str(operation_id).map_err(|_| "Invalid offboarding intent")?;
    if id.is_nil() || id.to_string() != operation_id {
        return Err("Invalid offboarding intent".into());
    }
    enqueue(path, keys, agent, Some(operation_id), expected_head)
}

fn event(row: &RetainedEvent) -> Result<Event, String> {
    let event =
        Event::from_json(&row.raw_event).map_err(|_| "Invalid retained offboarding event")?;
    event
        .verify()
        .map_err(|_| "Invalid retained offboarding signature")?;
    if event.kind.as_u16() as u32 != row.kind
        || event.pubkey.to_hex() != row.pubkey
        || event.content != row.content
        || event.created_at.as_secs() as i64 != row.created_at
    {
        return Err("Inconsistent retained offboarding event".into());
    }
    Ok(event)
}
fn belongs(event: &Event, id: &str) -> bool {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().map(String::as_str) == Some("crew-operation"))
        .collect();
    tags.len() == 1 && tags[0].as_slice() == ["crew-operation", id]
}
fn tagged(event: Event, keys: &Keys, id: Option<&str>) -> Result<Event, String> {
    let Some(id) = id else {
        return Ok(event);
    };
    let tag = Tag::parse(["crew-operation", id]).map_err(|_| "Invalid offboarding intent")?;
    EventBuilder::new(event.kind, event.content)
        .tags(event.tags)
        .tags([tag])
        .custom_created_at(event.created_at)
        .allow_self_tagging()
        .sign_with_keys(keys)
        .map_err(|_| "Could not sign offboarding intent".into())
}

pub(super) fn enqueue(
    path: &Path,
    keys: &Keys,
    agent: &str,
    operation_id: Option<&str>,
    expected_head: Option<&str>,
) -> Result<AgentOffboardingReceipt, String> {
    let owner = keys.public_key().to_hex();
    let conn = open_retention_db(path)?;
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|_| "Could not begin offboarding transaction")?;
    let result = (|| {
        let head = get_retained_event(&conn, KIND_MANAGED_AGENT, &owner, agent)?;
        let tombstone_key = tombstone_retention_d_tag(KIND_MANAGED_AGENT, agent);
        if let Some(id) = operation_id {
            let prior_delete = get_retained_event(&conn, 5, &owner, &tombstone_key)?;
            let prior_archive = get_retained_event(&conn, KIND_IA_ARCHIVE_REQUEST, &owner, agent)?;
            let delete = prior_delete.as_ref().map(event).transpose()?;
            let archive = prior_archive.as_ref().map(event).transpose()?;
            if let Some(event) = &delete {
                let coordinate = format!("{KIND_MANAGED_AGENT}:{owner}:{agent}");
                if !event
                    .tags
                    .iter()
                    .any(|tag| tag.as_slice() == ["a", coordinate.as_str()])
                {
                    return Err("Retained tombstone targets another instance".into());
                }
            }
            if let Some(event) = &archive {
                if !event.tags.iter().any(|tag| tag.as_slice() == ["p", agent]) {
                    return Err("Retained archive targets another instance".into());
                }
            }
            let ours_delete = delete.as_ref().is_some_and(|e| belongs(e, id));
            let ours_archive = archive.as_ref().is_some_and(|e| belongs(e, id));
            if ours_delete || ours_archive {
                if head.is_some() || !ours_delete || !ours_archive {
                    return Err("Offboarding references changed; review the retained state".into());
                }
                if let (Some(delete), Some(archive)) = (delete, archive) {
                    return Ok(AgentOffboardingReceipt {
                        tombstone_id: delete.id.to_hex(),
                        archive_id: archive.id.to_hex(),
                    });
                }
            }
            if prior_delete.as_ref().is_some_and(|r| r.pending_sync)
                || prior_archive.as_ref().is_some_and(|r| r.pending_sync)
            {
                return Err("An earlier offboarding request still needs reconciliation".into());
            }
            let actual_head = head.as_ref().map(event).transpose()?.map(|e| e.id.to_hex());
            if actual_head.as_deref() != expected_head {
                return Err(
                    "Managed agent head changed before offboarding; review required".into(),
                );
            }
        }
        let deletion = build_agent_delete(agent, &owner)?
            .custom_created_at(monotonic_created_at(head.as_ref().map(|r| r.created_at)))
            .sign_with_keys(keys)
            .map_err(|_| "Could not sign managed agent tombstone")?;
        let deletion = tagged(deletion, keys, operation_id)?;
        let persona = head
            .as_ref()
            .and_then(|r| super::persona_id_from_head(&r.content));
        let archive = tagged(
            super::build_agent_archive_request(keys, agent, persona.as_deref())?,
            keys,
            operation_id,
        )?;
        delete_retained_event(&conn, KIND_MANAGED_AGENT, &owner, agent)?;
        for (event, d_tag) in [(&deletion, tombstone_key), (&archive, agent.to_owned())] {
            retain_event(
                &conn,
                &RetainedEvent {
                    kind: event.kind.as_u16() as u32,
                    pubkey: owner.clone(),
                    d_tag,
                    content: event.content.clone(),
                    created_at: event.created_at.as_secs() as i64,
                    raw_event: event.as_json(),
                    pending_sync: true,
                },
            )?;
        }
        Ok(AgentOffboardingReceipt {
            tombstone_id: deletion.id.to_hex(),
            archive_id: archive.id.to_hex(),
        })
    })();
    match result {
        Ok(receipt) => {
            conn.execute_batch("COMMIT")
                .map_err(|_| "Could not commit offboarding transaction")?;
            Ok(receipt)
        }
        Err(error) => {
            conn.execute_batch("ROLLBACK")
                .map_err(|_| "Could not roll back offboarding transaction")?;
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "agents_pending_durable_tests.rs"]
mod tests;
