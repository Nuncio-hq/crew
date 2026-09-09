//! Captured native implementation of the Project channel-link driver.
use super::owner_operation_transport::OwnerOperationTransport;
use super::owner_operations::{load_owner_operation_for_dispatch, owner_operation_update};
use super::project_change_link_driver::{LinkHead, ProjectLinkRuntime};
use super::project_change_link_record::{ProjectLinkRecord, ProjectMetadataAction};
use super::project_git_workflow::project_owner_identity;
use crate::app_state::owner_scope::{assert_current, capture, OwnerScopeToken};
use crate::owner_operations::{Operation, OperationStatus, OperationUpdate};
use nostr::{Event, PublicKey};
use tauri::{AppHandle, Manager};

pub(super) struct NativeProjectLink {
    pub(super) app: AppHandle,
    pub(super) expected: OwnerScopeToken,
    pub(super) owner: PublicKey,
    pub(super) identifier: String,
    pub(super) kind: u32,
    pub(super) keys: nostr::Keys,
    auth_tag: Option<String>,
    transport: OwnerOperationTransport,
}

pub(super) fn now() -> Result<i64, String> {
    let value = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "System clock unavailable")?
        .as_secs();
    i64::try_from(value).map_err(|_| "System clock overflow".into())
}

impl NativeProjectLink {
    pub(super) async fn new(
        app: AppHandle,
        expected: OwnerScopeToken,
        coordinate: &str,
    ) -> Result<Self, String> {
        let mut parts = coordinate.splitn(3, ':');
        let kind = match parts.next() {
            Some("30621") => buzz_core_pkg::kind::KIND_PROJECT,
            Some("30617") => buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT,
            _ => return Err("A Project or repository coordinate is required.".into()),
        };
        let owner_hex = parts.next().ok_or("Project owner is missing")?;
        let owner = PublicKey::from_hex(owner_hex).map_err(|_| "Project owner is invalid")?;
        if owner.to_hex() != owner_hex {
            return Err("Project owner must be canonical lowercase hex.".into());
        }
        let identifier = parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or("Project identifier is missing")?
            .to_string();
        let captured = capture(app.clone()).await?;
        if captured.token != expected {
            return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
        }
        let lookup_app = app.clone();
        let identity = tokio::task::spawn_blocking(move || {
            project_owner_identity(
                &lookup_app,
                &lookup_app.state::<crate::AppState>(),
                &owner.to_hex(),
            )
        })
        .await
        .map_err(|_| "Project signing identity lookup failed")??;
        assert_current(app.clone(), &expected).await?;
        let transport = OwnerOperationTransport::captured(
            &app.state::<crate::AppState>(),
            expected.scope.community.clone(),
            identity.keys.clone(),
            identity.auth_tag.clone(),
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            app,
            expected,
            owner,
            identifier,
            kind,
            keys: identity.keys,
            auth_tag: identity.auth_tag,
            transport,
        })
    }

    /// Recheck both the active scope and managed-owner authorization after admission.
    pub(super) async fn guard(&self, operation: Option<&Operation>) -> Result<(), String> {
        assert_current(self.app.clone(), &self.expected).await?;
        let lookup_app = self.app.clone();
        let owner = self.owner;
        let identity = tokio::task::spawn_blocking(move || {
            project_owner_identity(
                &lookup_app,
                &lookup_app.state::<crate::AppState>(),
                &owner.to_hex(),
            )
        })
        .await
        .map_err(|_| "Project signing identity lookup failed")??;
        if identity.keys.public_key() != self.owner || identity.auth_tag != self.auth_tag {
            return Err("Project signing authorization changed.".into());
        }
        if let Some(operation) = operation {
            let (_, current) = load_owner_operation_for_dispatch(
                self.app.clone(),
                self.expected.clone(),
                operation.id.clone(),
                operation.revision,
            )
            .await?;
            if current.payload != operation.payload
                || current.status != operation.status
                || current.reconciled
            {
                return Err("Project operation changed before dispatch.".into());
            }
            let record = ProjectLinkRecord::from_operation(&current, self.owner)?;
            let current_time = now()?;
            if record
                .lease
                .as_ref()
                .is_none_or(|lease| lease.expires_at <= current_time)
            {
                return Err("Project operation worker lease expired.".into());
            }
        }
        assert_current(self.app.clone(), &self.expected).await
    }

    async fn extensions(&self, operation: Option<&Operation>) -> Result<Vec<String>, String> {
        let result = self
            .transport
            .relay_information(self.guard(operation))
            .await;
        self.guard(operation).await?;
        Ok(result
            .map_err(|error| error.to_string())?
            .supported_extensions
            .unwrap_or_default())
    }

    pub(super) async fn has_capability(
        &self,
        operation: Option<&Operation>,
    ) -> Result<bool, String> {
        let extensions = self.extensions(operation).await?;
        let required: &[&str] = if self.kind == buzz_core_pkg::kind::KIND_PROJECT {
            &[
                "crew-conditional-publication-v1",
                "crew-project-channel-link-v1",
            ]
        } else {
            &["crew-conditional-publication-v1"]
        };
        Ok(required
            .iter()
            .all(|needed| extensions.iter().any(|extension| extension == needed)))
    }

    pub(super) async fn head(&self, operation: Option<&Operation>) -> Result<Event, String> {
        let result = self
            .transport
            .query(serde_json::json!({"kinds":[self.kind],"authors":[self.owner.to_hex()],"#d":[self.identifier],"limit":2}), self.guard(operation))
            .await;
        self.guard(operation).await?;
        let events = result.map_err(|error| error.to_string())?;
        if events.len() != 1 {
            return Err("Could not establish one authoritative Project head.".into());
        }
        let event = events.into_iter().next().ok_or("Project head missing")?;
        if event.pubkey != self.owner
            || event.kind.as_u16() as u32 != self.kind
            || event.verify().is_err()
        {
            return Err("Invalid Project head returned by relay.".into());
        }
        if self.kind == buzz_core_pkg::kind::KIND_PROJECT {
            buzz_sdk_pkg::builders::validate_project_envelope(
                event.tags.as_slice(),
                &event.content,
            )
            .map_err(|error| error.to_string())?;
        }
        if !event.tags.iter().any(|tag| {
            tag.as_slice().first().is_some_and(|name| name == "d")
                && tag
                    .as_slice()
                    .get(1)
                    .is_some_and(|value| value == &self.identifier)
        }) {
            return Err("Relay returned a different Project coordinate.".into());
        }
        Ok(event)
    }

    /// Establish an accessible signed repository at the exact selected coordinate.
    pub(super) async fn repository_eligible(
        &self,
        operation: Option<&Operation>,
        coordinate: &str,
    ) -> Result<(), String> {
        buzz_sdk_pkg::builders::ProjectMemberCoord::parse_full(coordinate)
            .map_err(|error| error.to_string())?;
        let mut parts = coordinate.splitn(3, ':');
        let _kind = parts.next();
        let owner = parts.next().ok_or("Repository owner is missing")?;
        let identifier = parts.next().ok_or("Repository identifier is missing")?;
        let result = self
            .transport
            .query(
                serde_json::json!({"kinds":[30617],"authors":[owner],"#d":[identifier],"limit":2}),
                self.guard(operation),
            )
            .await;
        self.guard(operation).await?;
        let events = result.map_err(|error| error.to_string())?;
        if events.len() != 1 {
            return Err("Could not establish one accessible repository head.".into());
        }
        let event = events.first().ok_or("Repository head missing")?;
        let identifiers: Vec<_> = event
            .tags
            .iter()
            .filter(|tag| tag.as_slice().first().map(String::as_str) == Some("d"))
            .collect();
        if event.kind.as_u16() != 30617
            || event.pubkey.to_hex() != owner
            || event.verify().is_err()
            || identifiers.len() != 1
            || identifiers[0].as_slice() != ["d", identifier]
        {
            return Err("Relay returned an invalid or different repository coordinate.".into());
        }
        Ok(())
    }

    pub(super) async fn eligible(
        &self,
        operation: Option<&Operation>,
        channel_id: &str,
    ) -> Result<(), String> {
        let result = self
            .transport
            .query(
                serde_json::json!({"kinds":[39000,39002],"#d":[channel_id],"limit":3}),
                self.guard(operation),
            )
            .await;
        self.guard(operation).await?;
        let events = result.map_err(|error| error.to_string())?;
        validate_channel_eligibility(&events, channel_id, self.owner)
    }
}

/// Canonical relay discovery is authority; validate its signatures, exact IDs and membership.
pub(super) fn validate_channel_eligibility(
    events: &[Event],
    channel_id: &str,
    owner: PublicKey,
) -> Result<(), String> {
    if events.len() != 2
        || events.iter().any(|event| {
            event.verify().is_err()
                || !event
                    .tags
                    .iter()
                    .any(|tag| tag.as_slice() == ["d", channel_id])
        })
    {
        return Err("Channel discovery is missing or invalid.".into());
    }
    let metadata = events
        .iter()
        .find(|event| event.kind.as_u16() == 39000)
        .ok_or("Channel metadata is missing")?;
    let members = events
        .iter()
        .find(|event| event.kind.as_u16() == 39002)
        .ok_or("Channel membership is missing")?;
    if metadata.pubkey != members.pubkey {
        return Err("Channel discovery signers disagree.".into());
    }
    let details = crate::nostr_convert::channel_detail_from_event(metadata)?;
    let roster = crate::nostr_convert::channel_members_from_event(members)?;
    if details.channel_type != "stream"
        || details.archived_at.is_some()
        || !roster
            .members
            .iter()
            .any(|member| member.pubkey == owner.to_hex())
    {
        return Err(
            "Choose an active stream channel that the Project signing owner has joined.".into(),
        );
    }
    Ok(())
}

impl ProjectLinkRuntime for NativeProjectLink {
    fn now(&self) -> Result<i64, String> {
        now()
    }
    async fn checkpoint(&self, operation: &Operation) -> Result<(), String> {
        self.guard(Some(operation)).await
    }
    async fn save(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String> {
        assert_current(self.app.clone(), &self.expected).await?;
        let payload =
            serde_json::to_value(record).map_err(|_| "Project recovery serialization failed")?;
        let result = owner_operation_update(
            self.app.clone(),
            self.expected.clone(),
            operation.id.clone(),
            operation.revision,
            OperationUpdate {
                status,
                reconciled,
                payload,
            },
        )
        .await?;
        Ok(result.value)
    }
    async fn capability(&self, operation: &Operation) -> Result<bool, String> {
        self.has_capability(Some(operation)).await
    }
    async fn eligible(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<(), String> {
        match (&record.action, record.channel_id.as_deref()) {
            (None, Some(channel)) => {
                NativeProjectLink::eligible(self, Some(operation), channel).await
            }
            (
                Some(ProjectMetadataAction::AttachRepository {
                    repository_coordinate,
                }),
                None,
            ) => {
                self.repository_eligible(Some(operation), repository_coordinate)
                    .await
            }
            (
                Some(ProjectMetadataAction::LinkWorkspace {
                    repository_coordinate,
                    channel_id,
                    ..
                }),
                None,
            ) => {
                self.repository_eligible(Some(operation), repository_coordinate)
                    .await?;
                NativeProjectLink::eligible(self, Some(operation), channel_id).await
            }
            (Some(ProjectMetadataAction::UnlinkWorkspace { .. }), None) => Ok(()),
            _ => Err("Invalid Project operation action.".into()),
        }
    }
    async fn inspect(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<LinkHead, String> {
        let head = self.head(Some(operation)).await?;
        classify_link_head(&head, record)
    }
    async fn prove_superseded(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<Option<LinkHead>, String> {
        if !self
            .extensions(Some(operation))
            .await?
            .iter()
            .any(|extension| extension == "crew-conditional-publication-v1")
        {
            return Ok(None);
        }
        let head = self.head(Some(operation)).await?;
        Ok(Some(classify_link_head(&head, record)?))
    }
    async fn publish(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<(), String> {
        record.validate_intent(self.owner)?;
        let result = self
            .transport
            .publish(&record.signed_patch, self.guard(Some(operation)))
            .await;
        self.guard(Some(operation)).await?;
        let ack = result.map_err(|error| error.to_string())?;
        if !ack.accepted {
            return Err(ack.message);
        }
        Ok(())
    }
}

fn classify_link_head(head: &Event, record: &ProjectLinkRecord) -> Result<LinkHead, String> {
    if head.kind != record.original_head.kind
        || head.pubkey != record.original_head.pubkey
        || head.verify().is_err()
    {
        return Err("Invalid Project reconciliation head.".into());
    }
    if head.kind.as_u16() as u32 == buzz_core_pkg::kind::KIND_PROJECT {
        buzz_sdk_pkg::builders::validate_project_envelope(head.tags.as_slice(), &head.content)
            .map_err(|error| error.to_string())?;
    }
    let identifier = |event: &Event| {
        event
            .tags
            .iter()
            .find(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
            .and_then(|tag| tag.as_slice().get(1))
            .cloned()
    };
    if identifier(head) != identifier(&record.original_head) {
        return Err("Reconciliation returned a different Project coordinate.".into());
    }
    Ok(if head.id == record.signed_patch.id {
        LinkHead::Applied
    } else if head.id == record.original_head.id {
        LinkHead::Original
    } else {
        LinkHead::Conflict(head.id.to_hex())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_link_native_inspector_applied_head_survives_membership_removal() {
        let keys = nostr::Keys::generate();
        let head = buzz_sdk_pkg::builders::build_project_with_tags(
            "",
            vec![nostr::Tag::parse(["d", "project"]).unwrap()],
        )
        .unwrap()
        .custom_created_at(nostr::Timestamp::from_secs(1))
        .sign_with_keys(&keys)
        .unwrap();
        let channel = "12345678-1234-4234-8234-123456789abc";
        let tags =
            super::super::project_change_link::link_channel_tags(&head, keys.public_key(), channel)
                .unwrap()
                .unwrap();
        let patch = buzz_sdk_pkg::builders::build_project_with_tags("", tags)
            .unwrap()
            .custom_created_at(nostr::Timestamp::from_secs(2))
            .sign_with_keys(&keys)
            .unwrap();
        let record = ProjectLinkRecord {
            version: 1,
            project_coordinate: format!("30621:{}:project", keys.public_key().to_hex()),
            channel_id: Some(channel.into()),
            action: None,
            original_head: head,
            signed_patch: patch,
            attempts: 1,
            publication_attempted: true,
            reconcile_only: false,
            reconciliation: None,
            retry_at: 0,
            last_error: None,
            lease: None,
        };
        assert!(
            matches!(
                classify_link_head(&record.signed_patch, &record),
                Ok(LinkHead::Applied)
            ),
            "exact applied native readback must not require new-write eligibility"
        );
    }
}
