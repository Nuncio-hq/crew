//! Bounded, nonsecret recovery payload for one exact local instance.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    managed_agents::{BackendKind, ManagedAgentRecord},
    owner_operations::{Operation, OperationKind},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Instance {
    pub pubkey: String,
    pub generation: Uuid,
    pub backend_id: Option<String>,
    pub backend_agent_id: Option<String>,
    pub relay_url: String,
    pub persona_id: Option<String>,
    pub team_id: Option<String>,
}
impl Instance {
    pub fn capture(record: &ManagedAgentRecord) -> Result<Self, String> {
        if record.provider_policy_pending {
            return Err("Provider deployment is unresolved. Review this instance and its remote state before removal; Retry Start alone does not prove earlier remote work stopped".into());
        }
        let generation = record
            .instance_generation
            .filter(|id| !id.is_nil())
            .ok_or("The managed instance has no persisted incarnation")?;
        Ok(Self {
            pubkey: record.pubkey.clone(),
            generation,
            backend_id: match &record.backend {
                BackendKind::Local => None,
                BackendKind::Provider { id, .. } => Some(id.clone()),
            },
            backend_agent_id: record.backend_agent_id.clone(),
            relay_url: nonsecret_relay(&record.relay_url)?,
            persona_id: record.persona_id.clone(),
            team_id: record.team_id.clone(),
        })
    }
    pub fn matches(&self, record: &ManagedAgentRecord) -> bool {
        Self::capture(record).is_ok_and(|current| current == *self)
    }
}

// Empty means the managed record inherits its relay. Explicit values must
// be nonsecret origins; never copy embedded credentials into recovery data.
fn nonsecret_relay(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    let url = url::Url::parse(value).map_err(|_| "Managed relay needs review")?;
    if !matches!(url.scheme(), "ws" | "wss" | "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err("Managed relay must be a nonsecret origin before removal".into());
    }
    Ok(crate::relay::relay_http_base_url(url.as_str()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Inventory,
    Quiesce,
    Channels,
    LocalRemoval,
    Offboarding,
    Complete,
    ReviewRequired,
    Canceled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ChannelStep {
    pub channel_id: String,
    pub expected_head: Option<String>,
    pub cleanup_id: String,
    pub inspected: bool,
    pub cleanup_needed: bool,
    pub membership_present: bool,
    pub canvas_done: bool,
    pub canvas_attempted: bool,
    pub canvas_settled: bool,
    pub membership: Option<nostr::Event>,
    pub membership_attempted: bool,
    pub membership_done: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct RetainedPair {
    pub expected_agent_head: Option<String>,
    /// Exact signed envelopes remain in the existing retention transaction.
    pub tombstone_id: String,
    pub archive_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Lease {
    pub worker: String,
    pub expires_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Payload {
    pub version: u32,
    pub instance: Instance,
    pub force_remote_delete: bool,
    pub retention_scope_id: String,
    pub expected_agent_head: Option<String>,
    pub last_error: Option<String>,
    pub effects_started: bool,
    pub cancel_requested: bool,
    pub phase: Phase,
    /// None is unknown, distinct from a complete empty authorized/known view.
    pub channels: Option<Vec<ChannelStep>>,
    pub retained_pair: Option<RetainedPair>,
    pub local_removed: bool,
    pub key_removed: bool,
    pub lease: Option<Lease>,
    pub failures: u8,
    pub next_retry_at: Option<i64>,
}

pub(super) fn validate(operation: &Operation, payload: &Payload) -> Result<(), String> {
    if operation.kind != OperationKind::ManagedAgentDelete
        || payload.version != 1
        || operation.resource_key != payload.instance.pubkey
        || payload.instance.generation.is_nil()
        || payload.failures > 5
        || payload
            .channels
            .as_ref()
            .is_some_and(|channels| channels.len() > 64)
    {
        return Err("Invalid managed instance removal record".into());
    }
    let key = nostr::PublicKey::parse(&payload.instance.pubkey)
        .map_err(|_| "Invalid instance identity")?;
    if key.to_hex() != payload.instance.pubkey {
        return Err("Noncanonical instance identity".into());
    }
    if nonsecret_relay(&payload.instance.relay_url)? != payload.instance.relay_url {
        return Err("Noncanonical managed relay".into());
    }
    validate_event_id(&payload.retention_scope_id)?;
    if let Some(head) = &payload.expected_agent_head {
        validate_event_id(head)?;
    }
    if payload.key_removed && !payload.local_removed {
        return Err("Invalid instance key removal phase".into());
    }
    if payload.local_removed
        && payload.channels.as_ref().is_none_or(|channels| {
            channels
                .iter()
                .any(|step| !step.canvas_done || !step.membership_done)
        })
    {
        return Err("Local removal precedes channel reconciliation".into());
    }
    if payload.phase == Phase::Complete
        && (!payload.local_removed || !payload.key_removed || payload.retained_pair.is_none())
    {
        return Err("Incomplete instance removal cannot be complete".into());
    }
    if payload.phase == Phase::Canceled
        && payload.channels.as_ref().is_some_and(|channels| {
            channels.iter().any(|step| {
                (step.canvas_attempted && !step.canvas_settled)
                    || (step.membership_attempted && !step.membership_done)
            })
        })
    {
        return Err("Cancellation has unresolved channel effects".into());
    }
    if payload.phase == Phase::Canceled && payload.local_removed {
        return Err("Committed instance removal cannot be canceled".into());
    }
    if payload.next_retry_at.is_some_and(|time| time < 0) {
        return Err("Invalid instance retry time".into());
    }
    if let Some(lease) = &payload.lease {
        let id = Uuid::parse_str(&lease.worker).map_err(|_| "Invalid removal worker")?;
        if id.is_nil() || id.to_string() != lease.worker || lease.expires_at < 0 {
            return Err("Invalid removal worker lease".into());
        }
    }
    if let Some(pair) = &payload.retained_pair {
        validate_event_id(&pair.tombstone_id)?;
        validate_event_id(&pair.archive_id)?;
        if pair.tombstone_id == pair.archive_id {
            return Err("Ambiguous retained offboarding events".into());
        }
        if let Some(head) = &pair.expected_agent_head {
            validate_event_id(head)?;
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut cleanup_ids = std::collections::BTreeSet::new();
    for step in payload.channels.iter().flatten() {
        if let Some(head) = &step.expected_head {
            validate_event_id(head)?;
        }
        if !step.inspected
            && (step.canvas_done || step.membership_done || step.membership_attempted)
        {
            return Err("Uninspected channel cannot have completed effects".into());
        }
        if step.membership_attempted && step.membership.is_none() {
            return Err("Membership attempt has no retained signed request".into());
        }
        let channel = Uuid::parse_str(&step.channel_id).map_err(|_| "Invalid cleanup channel")?;
        let cleanup = Uuid::parse_str(&step.cleanup_id).map_err(|_| "Invalid cleanup operation")?;
        if channel.to_string() != step.channel_id
            || cleanup.to_string() != step.cleanup_id
            || channel.is_nil()
            || cleanup.is_nil()
            || cleanup.to_string() == operation.id
            || !cleanup_ids.insert(&step.cleanup_id)
            || !seen.insert(&step.channel_id)
        {
            return Err("Ambiguous cleanup channel or operation".into());
        }
        if let Some(event) = &step.membership {
            event
                .verify()
                .map_err(|_| "Invalid retained membership removal")?;
            if event.pubkey.to_hex() != operation.scope.owner
                || event.kind.as_u16() != 9001
                || !event
                    .tags
                    .iter()
                    .any(|tag| tag.as_slice() == ["h", step.channel_id.as_str()])
                || !event
                    .tags
                    .iter()
                    .any(|tag| tag.as_slice() == ["p", payload.instance.pubkey.as_str()])
            {
                return Err("Membership removal does not match this instance and channel".into());
            }
        }
    }
    Ok(())
}

fn validate_event_id(value: &str) -> Result<(), String> {
    let id = nostr::EventId::from_hex(value).map_err(|_| "Invalid retained event reference")?;
    if id.to_hex() != value {
        return Err("Noncanonical retained event reference".into());
    }
    Ok(())
}

/// Verify native possession AND the captured owner's signed delegation. A
/// renderer pubkey, same-name record, or legacy missing proof is insufficient.
pub(super) fn owned_keys(record: &ManagedAgentRecord, owner: &str) -> Result<nostr::Keys, String> {
    let keys = nostr::Keys::parse(&record.private_key_nsec)
        .map_err(|_| "The managed instance key is unavailable")?;
    if keys.public_key().to_hex() != record.pubkey {
        return Err("The managed instance key does not match".into());
    }
    let auth = record
        .auth_tag
        .as_deref()
        .ok_or("The managed instance ownership proof needs review")?;
    let agent =
        nostr::PublicKey::from_hex(&record.pubkey).map_err(|_| "Invalid managed instance")?;
    let delegated_owner = buzz_sdk_pkg::nip_oa::verify_auth_tag(auth, &agent)
        .map_err(|_| "The managed instance ownership proof needs review")?;
    if delegated_owner.to_hex() != owner {
        return Err("The managed instance belongs to another owner".into());
    }
    Ok(keys)
}
