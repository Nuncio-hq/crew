use super::CrewSaveResult;
use super::{
    native::{self, NativeBackend},
    prepare::{self, Input},
    record::Payload,
};
use crate::commands::ScopedOperationResult;
use crate::{
    app_state::owner_scope::{assert_current, OwnerScopeToken},
    commands::owner_operation_load,
    owner_operations::StoreError,
};
use std::collections::BTreeSet;
use tauri::AppHandle;

/// The deletion coordinator must persist its channel-to-operation UUID before
/// calling this helper, and must release managed-store/process locks first.
/// Only applied or unchanged completes cleanup; partial/superseded stays durable.
/// Entry point for the durable managed-agent deletion coordinator.
pub(crate) async fn save_channel_crew_member_cleanup(
    app: AppHandle,
    expected: OwnerScopeToken,
    operation_id: String,
    channel_id: String,
    expected_head: Option<String>,
    members: Vec<String>,
    manual: bool,
) -> Result<ScopedOperationResult<CrewSaveResult>, String> {
    let id = uuid::Uuid::parse_str(&operation_id).map_err(|_| "invalid cleanup operation ID")?;
    if id.to_string() != operation_id {
        return Err("cleanup operation ID must be canonical".into());
    }
    let members = prepare::canonical_members(&members)?;
    super::worker::start(app.clone());
    let existing =
        match owner_operation_load(app.clone(), expected.clone(), operation_id.clone(), None).await
        {
            Ok(result) => Some(result.value),
            Err(error) if error == StoreError::Missing.to_string() => None,
            Err(error) => return Err(error),
        };
    if let Some(operation) = existing {
        let payload: Payload = serde_json::from_value(operation.payload.clone())
            .map_err(|_| "invalid cleanup recovery record")?;
        super::record::validate(&operation, &payload)?;
        if payload.channel_id != channel_id || payload.cleanup_members.as_ref() != Some(&members) {
            return Err("cleanup operation belongs to another deletion intent".into());
        }
        let progress =
            super::service::resume_loaded(app.clone(), expected.clone(), operation, manual).await?;
        assert_current(app, &expected).await?;
        return Ok(ScopedOperationResult {
            token: expected,
            value: CrewSaveResult::Saved { progress },
        });
    }
    let backend = NativeBackend::new(app, &expected).await?;
    let _guard = super::save_lock::acquire(&expected, &channel_id).await?;
    let current = backend.head(&expected.scope.community, &channel_id).await?;
    // A deletion coordinator may not have a canvas head at journal creation
    // time. Bind a wildcard cleanup to the head read under the channel lock;
    // passing `None` through to `prepare_cleanup` would always conflict when
    // a canvas exists because its optimistic head check is exact.
    let expected_head = expected_head.or_else(|| current.as_ref().map(|event| event.id.to_hex()));
    let known = BTreeSet::new();
    let prepared = prepare::prepare_cleanup(
        Input {
            keys: &backend.captured.keys,
            channel_id: &channel_id,
            relay_url: &expected.scope.community,
            expected_head: expected_head.as_deref(),
            current: current.as_ref(),
            known_members: &known,
            now: native::now()?,
        },
        &members,
    )?;
    super::service::dispatch_prepared(&backend, prepared, operation_id).await
}
