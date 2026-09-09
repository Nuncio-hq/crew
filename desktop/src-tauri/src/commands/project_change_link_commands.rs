//! IPC takes intent at prepare, then only an opaque record ID/revision at dispatch.
use super::owner_operations::{
    load_owner_operation_for_dispatch, owner_operation_create, ScopedOperationResult,
};
use super::project_change_link::{
    attach_repository_tags, link_channel_tags, link_repository_tags, unlink_repository_tags,
};
use super::project_change_link_driver::drive;
use super::project_change_link_record::{ProjectLinkRecord, ProjectMetadataAction};
use super::project_change_link_runtime::{now, NativeProjectLink};
use crate::app_state::owner_scope::{assert_current, OwnerScopeToken};
use crate::owner_operations::{CreateResult, NewOperation, Operation, OperationKind};
use nostr::{EventBuilder, Kind, Timestamp};
use tauri::AppHandle;

/// Prepare one existing-channel link without any channel/template/runtime mutation.
#[tauri::command]
pub(crate) async fn project_change_link_prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    project_coordinate: String,
    channel_id: String,
) -> Result<ScopedOperationResult<CreateResult>, String> {
    prepare_metadata(app, expected, project_coordinate, Some(channel_id), None).await
}

/// Prepare an exact repository attachment in the shared Project journal.
#[tauri::command]
pub(crate) async fn project_change_attach_repository_prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    project_coordinate: String,
    repository_coordinate: String,
) -> Result<ScopedOperationResult<CreateResult>, String> {
    prepare_metadata(
        app,
        expected,
        project_coordinate,
        None,
        Some(ProjectMetadataAction::AttachRepository {
            repository_coordinate,
        }),
    )
    .await
}

/// Prepare a durable exact workspace link on one repository announcement.
#[tauri::command]
pub(crate) async fn project_change_link_workspace_prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    repository_coordinate: String,
    channel_id: String,
    local_path: String,
) -> Result<ScopedOperationResult<CreateResult>, String> {
    prepare_metadata(
        app,
        expected,
        repository_coordinate.clone(),
        None,
        Some(ProjectMetadataAction::LinkWorkspace {
            repository_coordinate,
            channel_id,
            local_path,
        }),
    )
    .await
}

async fn prepare_metadata(
    app: AppHandle,
    expected: OwnerScopeToken,
    project_coordinate: String,
    channel_id: Option<String>,
    action: Option<ProjectMetadataAction>,
) -> Result<ScopedOperationResult<CreateResult>, String> {
    let runtime =
        NativeProjectLink::new(app.clone(), expected.clone(), &project_coordinate).await?;
    if !runtime.has_capability(None).await? {
        return Err("Relay Project channel-link publication capability is not confirmed.".into());
    }
    let original_head = runtime.head(None).await?;
    let tags = match (channel_id.as_deref(), &action) {
        (Some(channel), None) => {
            let tags = link_channel_tags(&original_head, runtime.owner, channel)?
                .ok_or("That channel is already linked to this Project.")?;
            runtime.eligible(None, channel).await?;
            tags
        }
        (
            None,
            Some(ProjectMetadataAction::AttachRepository {
                repository_coordinate,
            }),
        ) => {
            let tags =
                attach_repository_tags(&original_head, runtime.owner, repository_coordinate)?
                    .ok_or("That repository is already attached to this Project.")?;
            runtime
                .repository_eligible(None, repository_coordinate)
                .await?;
            tags
        }
        (
            None,
            Some(ProjectMetadataAction::LinkWorkspace {
                repository_coordinate,
                channel_id,
                local_path,
            }),
        ) => {
            let tags = link_repository_tags(
                &original_head,
                runtime.owner,
                repository_coordinate,
                channel_id,
                local_path,
            )?
            .ok_or("That repository workspace is already linked.")?;
            runtime
                .repository_eligible(None, repository_coordinate)
                .await?;
            runtime.eligible(None, channel_id).await?;
            tags
        }
        (
            None,
            Some(ProjectMetadataAction::UnlinkWorkspace {
                repository_coordinate,
            }),
        ) => unlink_repository_tags(&original_head, runtime.owner, repository_coordinate)?
            .ok_or("Repository has no local workspace metadata to remove.")?,
        _ => return Err("Invalid Project operation intent.".into()),
    };
    runtime.guard(None).await?;
    let current_time = u64::try_from(now()?).map_err(|_| "System clock is before the epoch")?;
    if original_head.created_at.as_secs() > current_time.saturating_add(30) {
        return Err(
            "Project timestamp is ahead of this device; correct its clock before retrying.".into(),
        );
    }
    let timestamp = original_head
        .created_at
        .as_secs()
        .checked_add(1)
        .ok_or("Project timestamp overflow")?
        .max(current_time);
    let signed_patch = if matches!(
        &action,
        Some(ProjectMetadataAction::UnlinkWorkspace { .. })
            | Some(ProjectMetadataAction::LinkWorkspace { .. })
    ) {
        EventBuilder::new(
            Kind::Custom(buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT as u16),
            &original_head.content,
        )
        .tags(tags)
        .custom_created_at(Timestamp::from_secs(timestamp))
        .sign_with_keys(&runtime.keys)
        .map_err(|error| error.to_string())?
    } else {
        buzz_sdk_pkg::builders::build_project_with_tags(&original_head.content, tags)
            .map_err(|error| error.to_string())?
            .custom_created_at(Timestamp::from_secs(timestamp))
            .sign_with_keys(&runtime.keys)
            .map_err(|error| error.to_string())?
    };
    runtime.guard(None).await?;
    let record = ProjectLinkRecord {
        version: match &action {
            None => 1,
            Some(ProjectMetadataAction::AttachRepository { .. }) => 2,
            Some(ProjectMetadataAction::UnlinkWorkspace { .. }) => 3,
            Some(ProjectMetadataAction::LinkWorkspace { .. }) => 4,
        },
        project_coordinate: project_coordinate.clone(),
        channel_id,
        action,
        original_head,
        signed_patch,
        attempts: 0,
        publication_attempted: false,
        reconcile_only: false,
        reconciliation: None,
        retry_at: 0,
        last_error: None,
        lease: None,
    };
    // Validate the exact signed bytes before they enter the durable journal;
    // a malformed payload must never become a retryable operation.
    record.validate_intent(runtime.owner)?;
    let result = owner_operation_create(
        app.clone(),
        expected.clone(),
        NewOperation {
            id: uuid::Uuid::new_v4().to_string(),
            kind: OperationKind::ProjectChange,
            resource_key: project_coordinate,
            payload: serde_json::to_value(record)
                .map_err(|_| "Project recovery serialization failed")?,
        },
    )
    .await?;
    assert_current(app, &expected).await?;
    Ok(result)
}

/// Prepare a metadata-only unlink of one exact 30617 repository workspace.
#[tauri::command]
pub(crate) async fn project_change_unlink_workspace_prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    repository_coordinate: String,
) -> Result<ScopedOperationResult<CreateResult>, String> {
    prepare_metadata(
        app,
        expected,
        repository_coordinate.clone(),
        None,
        Some(ProjectMetadataAction::UnlinkWorkspace {
            repository_coordinate,
        }),
    )
    .await
}

/// Perform one bounded attempt; retries reuse the persisted signed publication.
#[tauri::command]
pub(crate) async fn project_change_link_dispatch(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    revision: u64,
    explicit_retry: bool,
) -> Result<ScopedOperationResult<Operation>, String> {
    let (_, operation) =
        load_owner_operation_for_dispatch(app.clone(), expected.clone(), id, revision).await?;
    let runtime =
        NativeProjectLink::new(app.clone(), expected.clone(), &operation.resource_key).await?;
    let value = drive(&runtime, operation, runtime.owner, explicit_retry).await?;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value,
    })
}
