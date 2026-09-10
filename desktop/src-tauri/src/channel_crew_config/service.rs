//! Scoped command orchestration and durable operation readback.
use super::{
    driver::{self, Backend},
    native::{now, NativeBackend},
    prepare::{self, Input, Preparation},
    record::{Payload, Progress},
    CrewSaveResult,
};
use crate::{
    app_state::{
        owner_scope::{assert_current, OwnerScopeToken},
        AppState,
    },
    commands::{
        load_owner_operation_for_dispatch, owner_operation_create, owner_operation_list,
        owner_operation_load,
    },
    owner_operations::{
        CreateResult, NewOperation, Operation, OperationKind, OperationStatus, OperationSummary,
    },
};
use buzz_core_pkg::crew_role::CrewConfigDraft;
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager};

pub(super) fn scoped(
    token: OwnerScopeToken,
    value: impl serde::Serialize,
) -> Result<Value, String> {
    Ok(
        json!({"token":token,"value":serde_json::to_value(value).map_err(|_|"invalid canvas response")?}),
    )
}

pub(crate) async fn save(
    app: AppHandle,
    expected: OwnerScopeToken,
    channel_id: String,
    expected_head: Option<String>,
    draft: CrewConfigDraft,
) -> Result<crate::commands::ScopedOperationResult<CrewSaveResult>, String> {
    super::worker::start(app.clone());
    let backend = NativeBackend::new(app.clone(), &expected).await?;
    let _guard = super::save_lock::acquire(&expected, &channel_id).await?;
    let relay_url = expected.scope.community.clone();
    let current = backend.head(&relay_url, &channel_id).await?;
    let requested: Vec<String> = draft
        .assignments
        .keys()
        .chain(draft.contact.iter())
        .map(|key| {
            nostr::PublicKey::parse(key.trim())
                .map(|key| key.to_hex())
                .map_err(|_| "invalid selected agent".to_string())
        })
        .collect::<Result<_, _>>()?;
    backend.check_scope().await?;
    let agents = tokio::time::timeout(
        Duration::from_secs(10),
        crate::commands::revalidate_relay_agents(
            requested,
            Some(channel_id.clone()),
            app.state::<AppState>(),
        ),
    )
    .await
    .map_err(|_| "agent validation timed out")??;
    backend.check_scope().await?;
    let known: BTreeSet<String> = agents
        .into_iter()
        .filter(|agent| agent.channel_ids.contains(&channel_id))
        .map(|agent| agent.pubkey)
        .collect();
    let prepared = prepare::prepare(
        Input {
            keys: &backend.captured.keys,
            channel_id: &channel_id,
            relay_url: &relay_url,
            expected_head: expected_head.as_deref(),
            current: current.as_ref(),
            known_members: &known,
            now: now()?,
        },
        &draft,
    )?;
    dispatch_prepared(&backend, prepared, uuid::Uuid::new_v4().to_string()).await
}

pub(super) async fn dispatch_prepared(
    backend: &NativeBackend,
    prepared: Preparation,
    id: String,
) -> Result<crate::commands::ScopedOperationResult<CrewSaveResult>, String> {
    let app = backend.app.clone();
    let expected = backend.captured.token.clone();
    match prepared {
        Preparation::Unchanged(current) => Ok(crate::commands::ScopedOperationResult {
            token: expected,
            value: CrewSaveResult::Unchanged {
                current_event_id: current,
            },
        }),
        Preparation::Conflict(current) => Ok(crate::commands::ScopedOperationResult {
            token: expected,
            value: CrewSaveResult::Conflict {
                current_event_id: current,
            },
        }),
        Preparation::ReviewRequired(current) => Ok(crate::commands::ScopedOperationResult {
            token: expected,
            value: CrewSaveResult::ReviewRequired {
                current_event_id: Some(current),
            },
        }),
        Preparation::Ready(payload) => {
            let created = owner_operation_create(
                app.clone(),
                expected.clone(),
                NewOperation {
                    id,
                    kind: OperationKind::ChannelCrewConfig,
                    resource_key: payload.channel_id.clone(),
                    payload: serde_json::to_value(&payload)
                        .map_err(|_| "invalid canvas recovery payload")?,
                },
            )
            .await?;
            match created.value {
                CreateResult::Existing(operation) => {
                    // OperationStore only returns an existing resource claim
                    // when its immutable creation digest matches. Keep this
                    // domain check beside the recovery hand-off as a second
                    // identity fence: a malformed or legacy record must
                    // never make a different draft appear recoverable.
                    let existing: Payload = serde_json::from_value(operation.payload)
                        .map_err(|_| "invalid canvas recovery payload")?;
                    if existing.channel_id != payload.channel_id
                        || existing.canvas.id != payload.canvas.id
                        || existing.announcement.id != payload.announcement.id
                    {
                        return Err(
                            "another canvas recovery is pending for this channel; review it before saving"
                                .into(),
                        );
                    }
                    Ok(crate::commands::ScopedOperationResult {
                        token: expected,
                        value: CrewSaveResult::RecoveryPending {
                            operation_id: operation.id,
                        },
                    })
                }
                CreateResult::Created(operation) => {
                    let id = operation.id.clone();
                    let result = driver::resume(backend, operation, false).await;
                    super::worker::wake(&app);
                    assert_current(app, &expected).await?;
                    match result {
                        Ok(progress) => Ok(crate::commands::ScopedOperationResult {
                            token: expected,
                            value: CrewSaveResult::Saved { progress },
                        }),
                        Err(_) => Ok(crate::commands::ScopedOperationResult {
                            token: expected,
                            value: CrewSaveResult::RecoveryPending { operation_id: id },
                        }),
                    }
                }
            }
        }
    }
}

pub(crate) async fn retry(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
) -> Result<Value, String> {
    super::worker::start(app.clone());
    let loaded = owner_operation_load(app.clone(), expected.clone(), id.clone(), None)
        .await?
        .value;
    let progress = resume_loaded(app.clone(), expected.clone(), loaded, true).await?;
    super::worker::wake(&app);
    scoped(expected, progress)
}

pub(super) async fn resume_loaded(
    app: AppHandle,
    expected: OwnerScopeToken,
    loaded: Operation,
    manual: bool,
) -> Result<Progress, String> {
    let (captured, operation) =
        load_owner_operation_for_dispatch(app.clone(), expected, loaded.id, loaded.revision)
            .await?;
    let backend = NativeBackend {
        app: app.clone(),
        captured,
        dispatch: Mutex::new(None),
    };
    driver::resume(&backend, operation, manual).await
}

pub(crate) async fn status(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
) -> Result<Value, String> {
    let operation = owner_operation_load(app.clone(), expected.clone(), id, None)
        .await?
        .value;
    let mut payload: Payload = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "invalid canvas recovery record")?;
    super::record::validate(&operation, &payload)?;
    let backend = NativeBackend::new(app.clone(), &expected).await?;
    let current = backend.latest(&payload).await?;
    if operation.reconciled && current.as_deref() != Some(&payload.canvas.id.to_hex()) {
        payload.outcome = super::record::Outcome::Superseded;
    }
    let progress = payload.progress(&operation.id, current);
    assert_current(app, &expected).await?;
    scoped(expected, progress)
}

pub(super) async fn operations(
    app: AppHandle,
    expected: OwnerScopeToken,
) -> Result<Vec<Operation>, String> {
    let mut after = None;
    let mut result = Vec::new();
    for _ in 0..10 {
        let page = owner_operation_list(app.clone(), expected.clone(), after, 100)
            .await?
            .value;
        let done = page.len() < 100;
        after = page.last().map(|item| item.id.clone());
        for summary in page {
            if is_recovery_visible(&summary) {
                result.push(
                    owner_operation_load(app.clone(), expected.clone(), summary.id, None)
                        .await?
                        .value,
                );
            }
        }
        if done {
            return Ok(result);
        }
    }
    Err("recovery list exceeded its bounded scan".into())
}

pub(crate) async fn list(
    app: AppHandle,
    expected: OwnerScopeToken,
    channel_id: String,
) -> Result<Value, String> {
    super::worker::start(app.clone());
    let operations = operations(app.clone(), expected.clone()).await?;
    let mut progress = Vec::<Progress>::new();
    for operation in operations
        .into_iter()
        .filter(|operation| operation.resource_key == channel_id)
    {
        let payload: Payload = serde_json::from_value(operation.payload.clone())
            .map_err(|_| "invalid canvas recovery record; manual review required")?;
        super::record::validate(&operation, &payload)?;
        progress.push(payload.progress(&operation.id, None));
    }
    assert_current(app, &expected).await?;
    scoped(expected, progress)
}

fn is_recovery_visible(summary: &OperationSummary) -> bool {
    summary.kind == OperationKind::ChannelCrewConfig
        && (!summary.reconciled || summary.status == OperationStatus::Superseded)
}

#[cfg(test)]
mod visibility_tests {
    use super::*;

    fn summary(status: OperationStatus, reconciled: bool) -> OperationSummary {
        OperationSummary {
            id: "00000000-0000-0000-0000-000000000001".into(),
            kind: OperationKind::ChannelCrewConfig,
            resource_key: "channel".into(),
            revision: 1,
            status,
            reconciled,
            updated_at: 1,
        }
    }

    #[test]
    fn superseded_channel_recovery_remains_visible_after_reconciliation() {
        assert!(is_recovery_visible(&summary(
            OperationStatus::Superseded,
            true
        )));
    }

    #[test]
    fn completed_channel_recovery_is_not_visible() {
        assert!(!is_recovery_visible(&summary(
            OperationStatus::Complete,
            true
        )));
    }
}
