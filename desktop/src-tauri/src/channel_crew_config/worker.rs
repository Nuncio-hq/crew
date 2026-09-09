//! One bounded recovery worker per desktop app; no renderer timer owns effects.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use super::{
    driver,
    native::{self, NativeBackend},
    record::Payload,
};
use crate::{
    app_state::owner_scope::capture,
    owner_operations::{OperationStatus, OperationUpdate},
};
use tauri::{AppHandle, Manager};

#[derive(Default)]
struct Worker {
    running: AtomicBool,
    wake: tokio::sync::Notify,
}

pub(crate) fn start(app: AppHandle) {
    if app.try_state::<Arc<Worker>>().is_none() {
        app.manage(Arc::new(Worker::default()));
    }
    let worker = app.state::<Arc<Worker>>().inner().clone();
    if worker.running.swap(true, Ordering::AcqRel) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut failures = 0_u8;
        let mut last_scope = None;
        loop {
            let scope = capture(app.clone()).await.map(|captured| captured.token);
            if let Ok(scope) = &scope {
                if last_scope.as_ref() != Some(scope) {
                    failures = 0;
                    last_scope = Some(scope.clone());
                }
            }
            if failures >= 5 {
                // Terminal automatic storage/scope failure. Only user work or
                // an actual native scope change re-enables external retries.
                tokio::select! {_=worker.wake.notified()=>failures=0,_=tokio::time::sleep(Duration::from_secs(60))=>{}}
                continue;
            }
            let result = match scope {
                Ok(scope) => run_due(app.clone(), scope).await,
                Err(error) => Err(error),
            };
            failures = if result.is_ok() {
                0
            } else {
                failures.saturating_add(1)
            };
            let delay = (5_u64 << failures.min(5)).min(300);
            tokio::select! {_=worker.wake.notified()=>failures=0,_=tokio::time::sleep(Duration::from_secs(delay))=>{}}
        }
    });
}

pub(super) fn wake(app: &AppHandle) {
    if let Some(worker) = app.try_state::<Arc<Worker>>() {
        worker.wake.notify_one();
    }
}

async fn run_due(
    app: AppHandle,
    scope: crate::app_state::owner_scope::OwnerScopeToken,
) -> Result<(), String> {
    let operations = super::service::operations(app.clone(), scope.clone()).await?;
    let mut processed = 0;
    for operation in operations {
        let payload = serde_json::from_value::<Payload>(operation.payload.clone());
        let payload = match payload {
            Ok(payload) => payload,
            Err(_) => {
                if operation.status != OperationStatus::Failed {
                    crate::commands::owner_operation_update(
                        app.clone(),
                        scope.clone(),
                        operation.id.clone(),
                        operation.revision,
                        OperationUpdate {
                            status: OperationStatus::Failed,
                            reconciled: false,
                            payload: operation.payload,
                        },
                    )
                    .await?;
                }
                continue;
            }
        };
        let now = native::now()?;
        if payload.failures >= 5
            || payload.next_retry_at.is_some_and(|due| due > now)
            || payload
                .lease
                .as_ref()
                .is_some_and(|lease| lease.expires_at > now)
        {
            continue;
        }
        if super::record::validate(&operation, &payload).is_err() {
            if operation.status != OperationStatus::Failed {
                crate::commands::owner_operation_update(
                    app.clone(),
                    scope.clone(),
                    operation.id.clone(),
                    operation.revision,
                    OperationUpdate {
                        status: OperationStatus::Failed,
                        reconciled: false,
                        payload: operation.payload,
                    },
                )
                .await?;
            }
            continue;
        }
        let backend = NativeBackend::new(app.clone(), &scope).await?;
        driver::resume(&backend, operation, false).await?;
        processed += 1;
        if processed >= 5 {
            break;
        }
    }
    Ok(())
}
