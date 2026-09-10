use buzz_core_pkg::crew_role::CrewConfigDraft;
use nostr::Event;
use serde::{Deserialize, Serialize};

#[cfg(all(test, unix))]
#[path = "review_tests.rs"]
mod review_tests;

/// Whole domain snapshot; signed events contain no private signing material.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Payload {
    pub version: u32,
    pub relay_url: String,
    pub channel_id: String,
    pub expected_head: Option<String>,
    pub canvas: Event,
    pub announcement: Event,
    pub canvas_attempted: bool,
    pub canvas_acknowledged: bool,
    pub announcement_attempted: bool,
    pub announcement_acknowledged: bool,
    pub failures: u8,
    pub next_retry_at: Option<i64>,
    pub lease: Option<Lease>,
    pub outcome: Outcome,
    /// The exact renderer form that produced the signed events. This is
    /// recovery metadata only; the signed canvas remains domain authority.
    #[serde(default)]
    pub draft: Option<CrewConfigDraft>,
    /// Canonical deletion intent supplied by the durable outer deletion journal.
    #[serde(default)]
    pub cleanup_members: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Lease {
    pub worker: String,
    pub expires_at: i64,
}

/// Partial commit is distinct from a fully applied configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    NotCommitted,
    CommitUncertain,
    CanvasCommittedAnnouncementPending,
    Applied,
    Superseded,
}

#[derive(Debug, Serialize)]
pub(crate) struct Progress {
    pub operation_id: String,
    pub outcome: Outcome,
    pub canvas_event_id: String,
    pub current_event_id: Option<String>,
    pub automatic_retry_at: Option<i64>,
    pub manual_retry_required: bool,
    /// Submitted form state retained across dialog close/reopen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<CrewConfigDraft>,
}

impl Payload {
    pub fn progress(&self, id: &str, current: Option<String>) -> Progress {
        Progress {
            operation_id: id.to_string(),
            outcome: self.outcome.clone(),
            canvas_event_id: self.canvas.id.to_hex(),
            current_event_id: current,
            automatic_retry_at: self.next_retry_at,
            manual_retry_required: self.failures >= 5,
            draft: self.draft.clone(),
        }
    }
}

pub(super) fn validate(
    operation: &crate::owner_operations::Operation,
    payload: &Payload,
) -> Result<(), String> {
    let invalid = || "invalid canvas recovery record".to_string();
    if operation.kind != crate::owner_operations::OperationKind::ChannelCrewConfig
        || payload.version != 1
        || operation.resource_key != payload.channel_id
        || uuid::Uuid::parse_str(&payload.channel_id)
            .map_err(|_| invalid())?
            .to_string()
            != payload.channel_id
        || payload.failures > 5
        || payload.next_retry_at.is_some_and(|time| time < 0)
        || serde_json::to_vec(operation).map_err(|_| invalid())?.len() > 1024 * 1024
    {
        return Err(invalid());
    }
    let relay = url::Url::parse(&payload.relay_url).map_err(|_| invalid())?;
    if !matches!(relay.scheme(), "http" | "https")
        || !relay.username().is_empty()
        || relay.password().is_some()
        || relay.query().is_some()
        || relay.fragment().is_some()
        || relay.origin().ascii_serialization() != operation.scope.community
        || payload.relay_url != operation.scope.community
    {
        return Err(invalid());
    }
    if let Some(head) = &payload.expected_head {
        if nostr::EventId::from_hex(head)
            .map_err(|_| invalid())?
            .to_hex()
            != *head
        {
            return Err(invalid());
        }
    }
    if let Some(members) = &payload.cleanup_members {
        if super::prepare::canonical_members(members)? != *members {
            return Err(invalid());
        }
    }
    for (event, kind) in [(&payload.canvas, 40100), (&payload.announcement, 9)] {
        if event.kind.as_u16() != kind
            || event.pubkey.to_hex() != operation.scope.owner
            || event.content.len() > 65536
            || event.tags.len() != 1
            || event
                .tags
                .iter()
                .next()
                .is_none_or(|tag| tag.as_slice() != ["h", payload.channel_id.as_str()])
        {
            return Err(invalid());
        }
        event.verify().map_err(|_| invalid())?;
    }
    if !payload
        .announcement
        .content
        .starts_with("AGENT-WORKING-AGREEMENT:")
        || !payload
            .announcement
            .content
            .contains(&format!("Canvas: {}", payload.canvas.id))
    {
        return Err(invalid());
    }
    if let Some(lease) = &payload.lease {
        if uuid::Uuid::parse_str(&lease.worker)
            .map_err(|_| invalid())?
            .to_string()
            != lease.worker
        {
            return Err(invalid());
        }
        if lease.expires_at < 0
            || lease.expires_at > operation.updated_at.checked_add(60).ok_or_else(invalid)?
        {
            return Err(invalid());
        }
    }
    use crate::owner_operations::OperationStatus;
    let phase_ok = match operation.status {
        OperationStatus::Preparing | OperationStatus::Pending => {
            !operation.reconciled
                && payload.lease.is_none()
                && payload.outcome == Outcome::NotCommitted
        }
        OperationStatus::Reconciling => {
            !operation.reconciled
                && payload.lease.is_some()
                && !matches!(payload.outcome, Outcome::Applied | Outcome::Superseded)
        }
        OperationStatus::Failed => {
            !operation.reconciled
                && payload.lease.is_none()
                && !matches!(payload.outcome, Outcome::Applied | Outcome::Superseded)
        }
        OperationStatus::Complete => {
            operation.reconciled
                && payload.outcome == Outcome::Applied
                && payload.canvas_acknowledged
                && payload.announcement_acknowledged
                && payload.lease.is_none()
                && payload.next_retry_at.is_none()
        }
        OperationStatus::Superseded => {
            payload.outcome == Outcome::Superseded
                && payload.lease.is_none()
                && payload.next_retry_at.is_none()
                && (!operation.reconciled
                    || ((!payload.canvas_attempted || payload.canvas_acknowledged)
                        && (!payload.announcement_attempted || payload.announcement_acknowledged)))
        }
        OperationStatus::Canceled => false,
    };
    let outcome_ok = match payload.outcome {
        Outcome::NotCommitted => !payload.canvas_attempted && !payload.canvas_acknowledged,
        Outcome::CommitUncertain => payload.canvas_attempted && !payload.canvas_acknowledged,
        Outcome::CanvasCommittedAnnouncementPending | Outcome::Applied => {
            payload.canvas_acknowledged
        }
        Outcome::Superseded => true,
    };
    if !phase_ok
        || !outcome_ok
        || ((payload.announcement_attempted || payload.announcement_acknowledged)
            && !payload.canvas_acknowledged)
    {
        return Err(invalid());
    }
    Ok(())
}
