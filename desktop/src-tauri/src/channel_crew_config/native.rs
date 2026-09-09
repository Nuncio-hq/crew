//! Native scope/transport binding for the tested preparation and retry driver.
use std::{collections::BTreeSet, sync::Mutex, time::Duration};

use nostr::Event;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::{
    app_state::{
        owner_scope::{assert_current, capture, CapturedOwnerScope, OwnerScopeToken},
        AppState,
    },
    commands::{owner_operation_load, owner_operation_update},
    owner_operations::{Operation, OperationStatus, OperationUpdate},
};

use super::{driver::Backend, record::Payload};

#[cfg(all(test, unix))]
#[path = "native_tests.rs"]
mod tests;

fn changed_members(old: &str, new: &str) -> Result<Vec<String>, String> {
    let before = buzz_core_pkg::crew_role::read_canvas_crew_metadata(Some(old), None, "");
    let after = buzz_core_pkg::crew_role::read_canvas_crew_metadata(Some(new), None, "");
    if before.crew_parse_state == "invalid" || after.crew_parse_state == "invalid" {
        return Err("invalid canvas assignments before dispatch".into());
    }
    let mut changed = BTreeSet::new();
    for (raw, label) in &after.stored_assignments {
        if before.stored_assignments.get(raw) == Some(label) {
            continue;
        }
        match nostr::PublicKey::parse(raw.trim()) {
            Ok(key) => {
                changed.insert(key.to_hex());
            }
            Err(_) if before.stored_assignments.contains_key(raw) => {}
            Err(_) => return Err("new assignment contains an invalid agent".into()),
        }
    }
    if after.contact_pubkey != before.contact_pubkey {
        if let Some(contact) = after.contact_pubkey {
            changed.insert(contact);
        }
    }
    Ok(changed.into_iter().collect())
}

pub(super) struct NativeBackend {
    pub app: AppHandle,
    pub captured: CapturedOwnerScope,
    pub(super) dispatch: Mutex<Option<super::guard::DispatchStamp>>,
}

impl NativeBackend {
    pub async fn new(app: AppHandle, expected: &OwnerScopeToken) -> Result<Self, String> {
        let captured = capture(app.clone()).await?;
        if &captured.token != expected {
            return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
        }
        Ok(Self {
            app,
            captured,
            dispatch: Mutex::new(None),
        })
    }

    async fn before_send(&self) -> Result<(), String> {
        self.check_scope().await?;
        let stamp = self
            .dispatch
            .lock()
            .map_err(|_| "canvas dispatch guard unavailable")?
            .clone();
        if let Some(stamp) = stamp {
            let load = async {
                owner_operation_load(
                    self.app.clone(),
                    self.captured.token.clone(),
                    stamp.id.clone(),
                    Some(stamp.revision),
                )
                .await
                .map(|result| result.value)
            };
            super::guard::reload_and_verify(load, &stamp, now).await?;
        }
        self.check_scope().await
    }

    async fn before_canvas_send(&self, payload: &Payload) -> Result<(), String> {
        self.before_send().await?;
        let current = self.head(&payload.relay_url, &payload.channel_id).await?;
        if current.as_ref().map(|event| event.id.to_hex()) != payload.expected_head {
            return Err("canvas changed before dispatch; reconcile the saved operation".into());
        }
        if let Some(members) = &payload.cleanup_members {
            let expected = buzz_core_pkg::crew_role::remove_canvas_crew_members(
                current
                    .as_ref()
                    .map(|event| event.content.as_str())
                    .unwrap_or(""),
                members,
                65536,
            )
            .map_err(|error| error.to_string())?;
            if expected != payload.canvas.content {
                return Err("cleanup snapshot does not match its recorded member intent".into());
            }
        }
        let requested = changed_members(
            current
                .as_ref()
                .map(|event| event.content.as_str())
                .unwrap_or(""),
            &payload.canvas.content,
        )?;
        if !requested.is_empty() {
            let agents = tokio::time::timeout(
                Duration::from_secs(10),
                crate::commands::revalidate_relay_agents(
                    requested.clone(),
                    Some(payload.channel_id.clone()),
                    self.app.state::<AppState>(),
                ),
            )
            .await
            .map_err(|_| "agent validation timed out")??;
            let known: BTreeSet<_> = agents
                .into_iter()
                .filter(|agent| agent.channel_ids.contains(&payload.channel_id))
                .map(|agent| agent.pubkey)
                .collect();
            if requested.iter().any(|key| !known.contains(key)) {
                return Err("selected agent is no longer a current channel member".into());
            }
        }
        self.before_send().await
    }

    pub async fn query(&self, url: &str, filters: &[Value]) -> Result<Vec<Event>, String> {
        self.check_scope().await?;
        let state = self.app.state::<AppState>();
        if filters.len() != 1 {
            return Err("canvas readback requires one bounded filter".into());
        }
        let result = crate::commands::channel_crew_transport_query(
            &state,
            url.to_string(),
            self.captured.keys.clone(),
            filters[0].clone(),
            self.before_send(),
        )
        .await?;
        self.check_scope().await?;
        Ok(result)
    }

    pub async fn head(&self, url: &str, channel: &str) -> Result<Option<Event>, String> {
        let events = self
            .query(url, &[json!({"kinds":[40100],"#h":[channel],"limit":1})])
            .await?;
        validated_head(events, channel)
    }
}

/// Preserve the relay's canonical created_at DESC, id ASC ordering.
fn validated_head(events: Vec<Event>, channel: &str) -> Result<Option<Event>, String> {
    let Some(event) = events.into_iter().next() else {
        return Ok(None);
    };
    event.verify().map_err(|_| "invalid canvas readback")?;
    if event.kind.as_u16() != 40100
        || !event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["h", channel])
    {
        return Err("canvas readback returned another channel".into());
    }
    Ok(Some(event))
}

impl Backend for NativeBackend {
    fn now(&self) -> Result<i64, String> {
        now()
    }
    async fn check_scope(&self) -> Result<(), String> {
        assert_current(self.app.clone(), &self.captured.token).await
    }
    async fn persist(
        &self,
        operation: &Operation,
        payload: &Payload,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String> {
        let result = owner_operation_update(
            self.app.clone(),
            self.captured.token.clone(),
            operation.id.clone(),
            operation.revision,
            OperationUpdate {
                status,
                reconciled,
                payload: serde_json::to_value(payload)
                    .map_err(|_| "invalid canvas recovery payload")?,
            },
        )
        .await?;
        *self
            .dispatch
            .lock()
            .map_err(|_| "canvas dispatch guard unavailable")? =
            payload
                .lease
                .as_ref()
                .map(|lease| super::guard::DispatchStamp {
                    id: result.value.id.clone(),
                    revision: result.value.revision,
                    worker: lease.worker.clone(),
                });
        Ok(result.value)
    }
    async fn latest(&self, payload: &Payload) -> Result<Option<String>, String> {
        Ok(self
            .head(&payload.relay_url, &payload.channel_id)
            .await?
            .map(|event| event.id.to_hex()))
    }
    async fn contains(&self, payload: &Payload, event: &Event) -> Result<bool, String> {
        let events = self
            .query(
                &payload.relay_url,
                &[json!({"ids":[event.id.to_hex()],"kinds":[event.kind.as_u16()],"limit":1})],
            )
            .await?;
        let Some(seen) = events.first() else {
            return Ok(false);
        };
        seen.verify().map_err(|_| "invalid event readback")?;
        if seen.id != event.id {
            return Err("event readback returned another event".into());
        }
        Ok(true)
    }
    async fn publish(&self, payload: &Payload, event: &Event) -> Result<(), String> {
        self.check_scope().await?;
        let state = self.app.state::<AppState>();
        let guard = async {
            if event.kind.as_u16() == 40100 {
                self.before_canvas_send(payload).await
            } else {
                self.before_send().await
            }
        };
        let result = crate::commands::channel_crew_transport_publish(
            &state,
            payload.relay_url.clone(),
            self.captured.keys.clone(),
            event,
            guard,
        )
        .await?;
        self.check_scope().await?;
        if result.event_id != event.id.to_hex() {
            return Err("publication acknowledged another event".into());
        }
        if !result.accepted {
            return Err("relay rejected the canvas operation".into());
        }
        Ok(())
    }
}

pub(super) fn now() -> Result<i64, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock unavailable")?
        .as_secs();
    i64::try_from(seconds).map_err(|_| "system clock unavailable".into())
}
