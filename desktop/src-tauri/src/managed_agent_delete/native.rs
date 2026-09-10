//! Captured native authority and short journal transactions for deletion.
use super::{
    admission::now,
    inventory, journal,
    record::{self, Payload},
};
use crate::{
    app_state::{
        owner_scope::{assert_current, capture, OwnerScopeToken},
        AppState,
    },
    commands::OwnerOperationTransport,
    managed_agents,
    owner_operations::{Limits, Operation, OperationKind, OperationStatus, OperationStore},
};
use nostr::{Event, Keys};
use serde_json::json;
use tauri::{AppHandle, Manager};

pub(super) struct Native {
    pub app: AppHandle,
    pub token: OwnerScopeToken,
    pub keys: Keys,
    pub id: String,
    pub worker: String,
}
impl Native {
    pub async fn new(
        app: AppHandle,
        token: OwnerScopeToken,
        id: String,
        worker: String,
    ) -> Result<Self, String> {
        let captured = capture(app.clone()).await?;
        if captured.token != token {
            return Err("Removal scope changed".into());
        }
        Ok(Self {
            app,
            token,
            keys: captured.keys,
            id,
            worker,
        })
    }
    pub async fn load(&self) -> Result<Operation, String> {
        let app = self.app.clone();
        let token = self.token.clone();
        let id = self.id.clone();
        tokio::task::spawn_blocking(move || {
            let store =
                OperationStore::open(&crate::commands::journal_path(&app)?, Limits::default())
                    .map_err(|e| e.to_string())?;
            let operation = store.load(&token.scope, &id).map_err(|e| e.to_string())?;
            journal::decode(&operation)?;
            Ok(operation)
        })
        .await
        .map_err(|_| "Removal load failed")?
    }
    pub async fn acquire(&self, manual: bool) -> Result<Operation, String> {
        assert_current(self.app.clone(), &self.token).await?;
        let app = self.app.clone();
        let token = self.token.clone();
        let id = self.id.clone();
        let worker = self.worker.clone();
        tokio::task::spawn_blocking(move || {
            let mut store =
                OperationStore::open(&crate::commands::journal_path(&app)?, Limits::default())
                    .map_err(|e| e.to_string())?;
            let operation = store.load(&token.scope, &id).map_err(|e| e.to_string())?;
            journal::acquire(&mut store, &operation, &worker, manual, now()?)
        })
        .await
        .map_err(|_| "Removal worker admission failed")?
    }
    pub async fn guard(&self) -> Result<(), String> {
        assert_current(self.app.clone(), &self.token).await?;
        let operation = self.load().await?;
        let payload = journal::decode(&operation)?;
        journal::assert_worker(&payload, &self.worker, now()?)?;
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let _store = state
                .managed_agents_store_lock
                .lock()
                .map_err(|_| "Instance store unavailable")?;
            let records = managed_agents::load_managed_agents(&app)?;
            if let Some(record) = records.iter().find(|r| r.pubkey == payload.instance.pubkey) {
                if payload.local_removed || !payload.instance.matches(record) {
                    return Err("Managed instance changed; review removal".into());
                }
            } else if !payload.local_removed && payload.phase != record::Phase::LocalRemoval {
                return Err("Managed instance disappeared before removal commit".into());
            }
            Ok(())
        })
        .await
        .map_err(|_| "Instance guard failed")?
    }
    pub async fn commit(
        &self,
        operation: &Operation,
        mut payload: Payload,
        complete: bool,
    ) -> Result<Operation, String> {
        assert_current(self.app.clone(), &self.token).await?;
        journal::assert_worker(&journal::decode(operation)?, &self.worker, now()?)?;
        payload.lease = if complete {
            None
        } else {
            Some(record::Lease {
                worker: self.worker.clone(),
                expires_at: now()?.saturating_add(90),
            })
        };
        let app = self.app.clone();
        let operation = operation.clone();
        tokio::task::spawn_blocking(move || {
            let mut store =
                OperationStore::open(&crate::commands::journal_path(&app)?, Limits::default())
                    .map_err(|e| e.to_string())?;
            let status = if complete && payload.phase == record::Phase::Canceled {
                OperationStatus::Canceled
            } else if complete {
                OperationStatus::Complete
            } else {
                OperationStatus::Reconciling
            };
            journal::save(&mut store, &operation, payload, status, complete, now()?)
        })
        .await
        .map_err(|_| "Removal progress save failed")?
    }
    pub async fn failure(&self, reason: &str, review: bool) -> Result<Operation, String> {
        let app = self.app.clone();
        let token = self.token.clone();
        let id = self.id.clone();
        let worker = self.worker.clone();
        let reason = reason.to_owned();
        tokio::task::spawn_blocking(move || {
            let mut store =
                OperationStore::open(&crate::commands::journal_path(&app)?, Limits::default())
                    .map_err(|e| e.to_string())?;
            let operation = store.load(&token.scope, &id).map_err(|e| e.to_string())?;
            journal::release_failure(&mut store, &operation, &worker, &reason, review, now()?)
        })
        .await
        .map_err(|_| "Removal failure save failed")?
    }
    async fn query_with(
        &self,
        keys: Keys,
        auth: Option<String>,
        filter: serde_json::Value,
    ) -> Result<Vec<Event>, String> {
        self.guard().await?;
        let state = self.app.state::<AppState>();
        let transport = OwnerOperationTransport::captured(
            &state,
            self.token.scope.community.clone(),
            keys,
            auth,
        )
        .map_err(|e| e.to_string())?;
        let result = transport
            .query(filter, self.guard())
            .await
            .map_err(|e| e.to_string())?;
        self.guard().await?;
        Ok(result)
    }
    pub async fn query(&self, filter: serde_json::Value) -> Result<Vec<Event>, String> {
        self.query_with(self.keys.clone(), None, filter).await
    }
    async fn agent_credentials(&self, payload: &Payload) -> Result<(Keys, Option<String>), String> {
        self.guard().await?;
        let app = self.app.clone();
        let expected = payload.instance.clone();
        let owner = self.token.scope.owner.clone();
        tokio::task::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let _store = state
                .managed_agents_store_lock
                .lock()
                .map_err(|_| "Instance store unavailable")?;
            let records = managed_agents::load_managed_agents(&app)?;
            let record = records
                .iter()
                .find(|r| r.pubkey == expected.pubkey)
                .ok_or("Managed instance unavailable")?;
            if !expected.matches(record) {
                return Err("Managed instance changed".into());
            }
            Ok((record::owned_keys(record, &owner)?, record.auth_tag.clone()))
        })
        .await
        .map_err(|_| "Instance credentials unavailable")?
    }
    pub async fn known_channels(&self) -> Result<Vec<String>, String> {
        let app = self.app.clone();
        let token = self.token.clone();
        tokio::task::spawn_blocking(move || {
            let store =
                OperationStore::open(&crate::commands::journal_path(&app)?, Limits::default())
                    .map_err(|e| e.to_string())?;
            let mut after = None;
            let mut channels = std::collections::BTreeSet::new();
            for _ in 0..5 {
                let page = store
                    .list(&token.scope, after.as_deref(), 100)
                    .map_err(|e| e.to_string())?;
                for item in &page {
                    if item.kind == OperationKind::ChannelCrewConfig {
                        channels.insert(item.resource_key.clone());
                    }
                }
                if channels.len() > inventory::CHANNEL_LIMIT {
                    return Err("Known channel coverage exceeds removal bounds".into());
                }
                if page.len() < 100 {
                    return Ok(channels.into_iter().collect());
                }
                after = page.last().map(|item| item.id.clone());
            }
            Err("Known channel coverage is incomplete; review required".into())
        })
        .await
        .map_err(|_| "Known channel coverage unavailable")?
    }
    pub async fn covered_channels(&self, payload: &Payload) -> Result<Vec<String>, String> {
        let owner = self.query(json!({"kinds":[39000],"limit":65})).await?;
        let (keys, auth) = self.agent_credentials(payload).await?;
        let membership = self
            .query_with(
                keys,
                auth,
                json!({"kinds":[39002],"#p":[payload.instance.pubkey],"limit":65}),
            )
            .await?;
        let retained = self.known_channels().await?;
        Ok(
            inventory::covered_channels(&owner, &membership, &retained, &payload.instance.pubkey)?
                .into_iter()
                .collect(),
        )
    }
    pub async fn inspect(
        &self,
        channel: &str,
        agent: &str,
    ) -> Result<inventory::ChannelCoverage, String> {
        let roster = self
            .query(json!({"kinds":[39002],"#d":[channel],"limit":1}))
            .await?;
        let [roster] = roster.as_slice() else {
            return Err("Known channel membership is unavailable; review required".into());
        };
        let canvas = self
            .query(json!({"kinds":[40100],"#h":[channel],"limit":1}))
            .await?;
        if canvas.len() > 1 {
            return Err("Ambiguous channel canvas".into());
        }
        inventory::inspect_channel(
            channel,
            roster,
            canvas.first(),
            &self.token.scope.owner,
            agent,
        )
    }
}
