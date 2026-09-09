//! Typed recovery payload for one existing-channel link; no runtime/template work.

use nostr::{Event, PublicKey};
use serde::{Deserialize, Serialize};

use super::project_change_link::{
    validate_signed_attachment, validate_signed_link, validate_signed_unlink,
};
use crate::owner_operations::{Operation, OperationKind, OperationStatus};

/// Immutable signed intent, with bounded retry and worker ownership metadata.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectLinkRecord {
    pub(super) version: u32,
    pub(super) project_coordinate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) action: Option<ProjectMetadataAction>,
    pub(super) original_head: Event,
    pub(super) signed_patch: Event,
    pub(super) attempts: u8,
    pub(super) publication_attempted: bool,
    pub(super) reconcile_only: bool,
    pub(super) reconciliation: Option<ProjectLinkReconciliation>,
    pub(super) retry_at: i64,
    pub(super) last_error: Option<String>,
    pub(super) lease: Option<ProjectLinkLease>,
}

/// Version two explicitly identifies metadata-only repository attachment.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum ProjectMetadataAction {
    AttachRepository { repository_coordinate: String },
    UnlinkWorkspace { repository_coordinate: String },
}

/// A journal CAS claims a worker; expiry never permits a different signed event.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectLinkLease {
    pub(super) worker_id: String,
    pub(super) expires_at: i64,
}

impl ProjectLinkRecord {
    pub(super) fn validate_intent(&self, native_owner: PublicKey) -> Result<(), String> {
        match (self.version, self.channel_id.as_deref(), &self.action) {
            (1, Some(channel), None) => validate_signed_link(
                &self.original_head,
                &self.signed_patch,
                native_owner,
                channel,
            ),
            (
                2,
                None,
                Some(ProjectMetadataAction::AttachRepository {
                    repository_coordinate,
                }),
            ) => validate_signed_attachment(
                &self.original_head,
                &self.signed_patch,
                native_owner,
                repository_coordinate,
            ),
            (
                3,
                None,
                Some(ProjectMetadataAction::UnlinkWorkspace {
                    repository_coordinate,
                }),
            ) => validate_signed_unlink(
                &self.original_head,
                &self.signed_patch,
                native_owner,
                repository_coordinate,
            ),
            _ => Err("Invalid Project operation action or version.".into()),
        }
    }

    /// Parse renderer-readable metadata and independently revalidate its exact intent.
    /// Scope, live-head authority and channel membership are checked by the caller.
    pub(super) fn from_operation(
        operation: &Operation,
        native_owner: PublicKey,
    ) -> Result<Self, String> {
        if operation.kind != OperationKind::ProjectChange
            || operation.reconciled
            || operation.status == OperationStatus::Complete
        {
            return Err("Project operation is not available for dispatch.".into());
        }
        let record: Self = serde_json::from_value(operation.payload.clone())
            .map_err(|_| "Invalid Project channel-link recovery payload.".to_string())?;
        if record.attempts > 5 || record.retry_at < 0 || record.reconciliation.is_some() {
            return Err("Unsupported Project recovery version or attempt count.".into());
        }
        record.validate_intent(native_owner)?;
        // The SDK envelope validation above guarantees exactly one nonempty d tag.
        let identifier = record
            .original_head
            .tags
            .iter()
            .find_map(|tag| {
                let values = tag.as_slice();
                (values.first().is_some_and(|name| name == "d"))
                    .then(|| values.get(1))
                    .flatten()
            })
            .ok_or_else(|| "Project identifier is missing.".to_string())?;
        let kind = match &record.action {
            Some(ProjectMetadataAction::UnlinkWorkspace { .. }) => {
                buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT
            }
            _ => buzz_core_pkg::kind::KIND_PROJECT,
        };
        let coordinate = format!("{kind}:{}:{identifier}", native_owner.to_hex());
        if record.project_coordinate != coordinate || operation.resource_key != coordinate {
            return Err("Project recovery resource does not match its signed coordinate.".into());
        }
        if let Some(lease) = &record.lease {
            let worker = uuid::Uuid::parse_str(&lease.worker_id)
                .map_err(|_| "Invalid Project worker identity.".to_string())?;
            if worker.to_string() != lease.worker_id || lease.expires_at < 0 {
                return Err("Invalid Project worker lease.".into());
            }
        }
        Ok(record)
    }
}

/// Evidence recorded only after the native dispatcher has established it.
#[derive(Serialize, Deserialize)]
#[serde(tag = "proof", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum ProjectLinkReconciliation {
    Applied { event_id: String },
    UnattemptedConflict { current_head_id: String },
    ConditionalConflict { current_head_id: String },
}
