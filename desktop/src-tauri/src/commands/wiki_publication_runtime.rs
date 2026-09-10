//! Captured desktop runtime for the Wiki publication driver.

use super::owner_operation_transport::{OperationTransportError, OwnerOperationTransport};
use super::owner_operations::{load_owner_operation_for_dispatch, owner_operation_update};
use super::wiki_publication_driver::{
    WikiDependencyState, WikiHead, WikiPublicationRuntime, WikiPublishError,
};
use super::wiki_publication_record::{WikiPublicationLease, WikiPublicationRecord};
use crate::app_state::owner_scope::{assert_current, capture, OwnerScopeToken};
use crate::owner_operations::{Operation, OperationStatus, OperationUpdate};
use nostr::{Event, Keys, PublicKey};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager};

const WIKI_KIND: u16 = 30623;
// Four maximum-size events stay below the transport's 1 MiB response cap.
const MAX_QUERY_BATCH: usize = 4;

pub(super) struct NativeWikiPublication {
    app: AppHandle,
    expected: OwnerScopeToken,
    pub(super) owner: PublicKey,
    pub(super) repo_d: String,
    keys: Keys,
    transport: OwnerOperationTransport,
}

pub(super) fn now() -> Result<i64, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "System clock unavailable")?
        .as_secs();
    i64::try_from(seconds).map_err(|_| "System clock overflow".into())
}

impl NativeWikiPublication {
    pub(super) async fn new(
        app: AppHandle,
        expected: OwnerScopeToken,
        coordinate: &str,
    ) -> Result<Self, String> {
        let (owner, repo_d) = coordinate_parts(coordinate)?;
        let owner_key = PublicKey::from_hex(owner).map_err(|_| "Wiki owner is invalid")?;
        let captured = capture(app.clone()).await?;
        if captured.token != expected {
            return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
        }
        if captured.token.scope.owner != owner {
            return Err("Wiki publication owner does not match the active signer.".into());
        }
        let keys = captured.keys;
        let transport = OwnerOperationTransport::captured(
            &app.state::<crate::AppState>(),
            expected.scope.community.clone(),
            keys.clone(),
            None,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            app,
            expected,
            owner: owner_key,
            repo_d: repo_d.to_owned(),
            keys,
            transport,
        })
    }

    async fn guard(&self, operation: Option<&Operation>) -> Result<(), String> {
        let captured = capture(self.app.clone()).await?;
        if captured.token != self.expected || captured.keys.public_key() != self.owner {
            return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
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
                return Err("Wiki publication changed before dispatch.".into());
            }
            // `drive` validates the complete signed graph before entering the
            // runtime. The exact payload equality above is the immutable
            // revision fence, so re-running full graph verification on every
            // transport pre/post fence would make a large publication scale
            // with every page for every query. Decode only the mutable lease
            // metadata needed for this fence; the next CAS still validates the
            // complete record before persisting it.
            let lease = current
                .payload
                .get("lease")
                .cloned()
                .map(serde_json::from_value::<WikiPublicationLease>)
                .transpose()
                .map_err(|_| "Invalid Wiki publication lease metadata.".to_string())?;
            let current_time = now()?;
            if lease.is_none_or(|lease| lease.expires_at <= current_time) {
                return Err("Wiki publication worker lease expired.".into());
            }
        }
        assert_current(self.app.clone(), &self.expected).await
    }

    async fn query(
        &self,
        operation: Option<&Operation>,
        filter: Value,
    ) -> Result<Vec<Event>, String> {
        let result = self
            .transport
            .query(filter, self.guard(operation))
            .await
            .map_err(|error| error.to_string());
        self.guard(operation).await?;
        result
    }

    async fn query_head(&self, operation: Option<&Operation>) -> Result<Option<Event>, String> {
        let d = format!("{}/_toc", self.repo_d);
        let events = self
            .query(
                operation,
                json!({"kinds":[WIKI_KIND],"authors":[self.owner.to_hex()],"#d":[d],"limit":2}),
            )
            .await?;
        match events.as_slice() {
            [] => Ok(None),
            [event] => {
                validate_coordinate(
                    event,
                    self.owner,
                    &self.repo_d,
                    &format!("{}/_toc", self.repo_d),
                )?;
                Ok(Some(event.clone()))
            }
            _ => Err("Wiki head query returned duplicate events.".into()),
        }
    }

    pub(super) async fn current_head(&self) -> Result<Option<Event>, String> {
        self.query_head(None).await
    }

    /// Verify the repository announcement at the exact owner/d coordinate.
    /// The NIP-34 repository contract does not require an association tag.
    pub(super) async fn repository_head(&self) -> Result<Event, String> {
        let events = self
            .query(
                None,
                json!({
                    "kinds":[buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT],
                    "authors":[self.owner.to_hex()],
                    "#d":[self.repo_d],
                    "limit":2
                }),
            )
            .await?;
        let [event] = events.as_slice() else {
            return Err("Repository source anchor is unavailable.".into());
        };
        let ds: Vec<_> = event
            .tags
            .iter()
            .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
            .collect();
        if event.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_GIT_REPO_ANNOUNCEMENT
            || event.pubkey != self.owner
            || event.verify().is_err()
            || ds.len() != 1
            || ds[0].as_slice() != ["d", self.repo_d.as_str()]
        {
            return Err("Repository source anchor is invalid.".into());
        }
        Ok(event.clone())
    }

    /// Read and fully verify the current v1 graph for cadence-only updates.
    pub(super) async fn current_publication(
        &self,
    ) -> Result<Option<crew_wiki::snapshot_v1_build::SnapshotPublication>, String> {
        let Some(head) = self.current_head().await? else {
            return Ok(None);
        };
        let Some((manifest, pages)) = self.load_graph(None, &head).await? else {
            return Err("Current Wiki is legacy or incomplete; cadence update requires a verified v1 snapshot.".into());
        };
        let final_head = self.current_head().await?;
        if final_head.as_ref().map(|event| event.id) != Some(head.id) {
            return Err("Wiki head changed while reading its immutable snapshot.".into());
        }
        let head_value = serde_json::to_value(&head)
            .map_err(|_| "Wiki head is not serializable.".to_string())?;
        let manifest_value = serde_json::to_value(&manifest)
            .map_err(|_| "Wiki manifest is not serializable.".to_string())?;
        let index = crew_wiki::snapshot_v1::verify_snapshot_index(
            &self.owner.to_hex(),
            &self.repo_d,
            &head_value,
            &manifest_value,
        )
        .map_err(|error| error.to_string())?;
        let expected_revision = tag_value(&head, "expected-revision")
            .ok_or_else(|| "Current Wiki head has no expected revision.".to_string())?;
        Ok(Some(crew_wiki::snapshot_v1_build::SnapshotPublication {
            head,
            manifest,
            pages,
            snapshot_id: index.manifest().1.clone(),
            source_revision: index.manifest().4.clone(),
            expected_revision,
        }))
    }

    pub(super) fn keys(&self) -> &Keys {
        &self.keys
    }

    async fn renew_lease(
        &self,
        operation: &mut Operation,
        record: &mut WikiPublicationRecord,
        worker: &str,
    ) -> Result<(), String> {
        self.guard(Some(operation)).await?;
        let expires_at = self
            .now()?
            .checked_add(60)
            .ok_or("Wiki publication lease clock overflow")?;
        record.lease = Some(super::wiki_publication_record::WikiPublicationLease {
            worker_id: worker.to_owned(),
            expires_at,
        });
        record.validate_intent(self.owner)?;
        let payload = serde_json::to_value(record)
            .map_err(|_| "Wiki publication recovery serialization failed.".to_string())?;
        let result = owner_operation_update(
            self.app.clone(),
            self.expected.clone(),
            operation.id.clone(),
            operation.revision,
            OperationUpdate {
                status: OperationStatus::Reconciling,
                reconciled: false,
                payload,
            },
        )
        .await?;
        *operation = result.value;
        Ok(())
    }

    async fn load_graph(
        &self,
        operation: Option<&Operation>,
        head: &Event,
    ) -> Result<Option<(Event, Vec<Event>)>, String> {
        let Some(manifest_tag) = exact_tag(head, "wiki-manifest", 3) else {
            return Ok(None);
        };
        let manifest_id = manifest_tag[1].clone();
        let manifest_hash = manifest_tag[2].clone();
        if !is_lower_hex(&manifest_id, 64) || !is_lower_hex(&manifest_hash, 64) {
            return Err("Current Wiki manifest reference is invalid.".into());
        }
        let manifest_d = format!("{}/m1-{manifest_hash}", self.repo_d);
        let manifests = self
            .query(
                operation,
                json!({
                    "kinds":[WIKI_KIND],
                    "authors":[self.owner.to_hex()],
                    "ids":[manifest_id],
                    "#d":[manifest_d],
                    "limit":2
                }),
            )
            .await?;
        let Some(manifest) = manifests.first().filter(|event| {
            event.id.to_hex() == manifest_tag[1]
                && event.kind.as_u16() == WIKI_KIND
                && event.pubkey == self.owner
                && event.verify().is_ok()
                && event
                    .tags
                    .iter()
                    .any(|tag| tag.as_slice() == ["d", manifest_d.as_str()])
        }) else {
            return Ok(None);
        };
        let manifest = manifest.clone();
        let head_value =
            serde_json::to_value(head).map_err(|_| "Wiki head is not serializable.".to_string())?;
        let manifest_value = serde_json::to_value(&manifest)
            .map_err(|_| "Wiki manifest is not serializable.".to_string())?;
        let index = crew_wiki::snapshot_v1::verify_snapshot_index(
            &self.owner.to_hex(),
            &self.repo_d,
            &head_value,
            &manifest_value,
        )
        .map_err(|error| error.to_string())?;
        let mut pages = Vec::new();
        for chunk in index.manifest().7.chunks(MAX_QUERY_BATCH) {
            let ids: Vec<_> = chunk.iter().map(|reference| reference.2.clone()).collect();
            let ds: Vec<_> = chunk
                .iter()
                .map(|reference| format!("{}/{}", self.repo_d, reference.1))
                .collect();
            let events = self
                .query(
                    operation,
                    json!({
                        "kinds":[WIKI_KIND],
                        "authors":[self.owner.to_hex()],
                        "ids":ids,
                        "#d":ds,
                        "limit":MAX_QUERY_BATCH
                    }),
                )
                .await?;
            if events.len() != chunk.len() {
                return Ok(None);
            }
            let mut by_id = BTreeMap::new();
            for event in events {
                by_id.insert(event.id.to_hex(), event);
            }
            for reference in chunk {
                let Some(event) = by_id.remove(&reference.2) else {
                    return Ok(None);
                };
                pages.push(event);
            }
        }
        let values = pages
            .iter()
            .map(|event| {
                serde_json::to_value(event)
                    .map_err(|_| "Wiki page is not serializable.".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        crew_wiki::snapshot_v1::verify_snapshot(
            &self.owner.to_hex(),
            &self.repo_d,
            &head_value,
            &manifest_value,
            &values,
        )
        .map_err(|error| error.to_string())?;
        Ok(Some((manifest, pages)))
    }

    async fn dependencies_state(
        &self,
        mut operation: Option<&mut Operation>,
        record: &mut WikiPublicationRecord,
        worker: Option<&str>,
    ) -> Result<WikiDependencyState, String> {
        let manifest_d = manifest_d_tag(&record.manifest)?;
        if let (Some(operation), Some(worker)) = (operation.as_deref_mut(), worker) {
            self.renew_lease(operation, record, worker).await?;
        }
        let manifests = self
            .query(
                operation.as_deref(),
                json!({
                    "kinds":[WIKI_KIND],
                    "authors":[self.owner.to_hex()],
                    "ids":[record.manifest.id.to_hex()],
                    "#d":[manifest_d],
                    "limit":2
                }),
            )
            .await?;
        let Some(manifest) = exact_event(&manifests, &record.manifest, &manifest_d) else {
            return Ok(WikiDependencyState::Missing);
        };
        let mut by_id = BTreeMap::new();
        let page_count = record.pages.len();
        for start in (0..page_count).step_by(MAX_QUERY_BATCH) {
            if let (Some(operation), Some(worker)) = (operation.as_deref_mut(), worker) {
                self.renew_lease(operation, record, worker).await?;
            }
            let end = (start + MAX_QUERY_BATCH).min(page_count);
            let chunk = &record.pages[start..end];
            let ids: Vec<_> = chunk.iter().map(|event| event.id.to_hex()).collect();
            let ds: Vec<_> = chunk.iter().map(page_d_tag).collect::<Result<_, _>>()?;
            let events = self
                .query(
                    operation.as_deref(),
                        json!({"kinds":[WIKI_KIND],"authors":[self.owner.to_hex()],"ids":ids,"#d":ds,"limit":MAX_QUERY_BATCH}),
                )
                .await?;
            if events.len() != chunk.len() {
                return Ok(WikiDependencyState::Missing);
            }
            for event in events {
                if event.pubkey != self.owner || event.verify().is_err() {
                    return Ok(WikiDependencyState::Missing);
                }
                by_id.insert(event.id.to_hex(), event);
            }
        }
        if by_id.len() != record.pages.len()
            || record
                .pages
                .iter()
                .any(|event| by_id.get(&event.id.to_hex()).is_none())
        {
            return Ok(WikiDependencyState::Missing);
        }
        let values = record
            .pages
            .iter()
            .map(|event| {
                by_id
                    .get(&event.id.to_hex())
                    .ok_or_else(|| "Wiki page disappeared after readback.".to_string())
                    .and_then(|event| {
                        serde_json::to_value(event)
                            .map_err(|_| "Wiki page is not serializable.".to_string())
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let head = serde_json::to_value(&record.head)
            .map_err(|_| "Wiki head is not serializable.".to_string())?;
        let manifest_value = serde_json::to_value(&manifest)
            .map_err(|_| "Wiki manifest is not serializable.".to_string())?;
        crew_wiki::snapshot_v1::verify_snapshot(
            &self.owner.to_hex(),
            &self.repo_d,
            &head,
            &manifest_value,
            &values,
        )
        .map_err(|error| error.to_string())?;
        Ok(WikiDependencyState::Verified)
    }

    async fn extensions(&self, operation: Option<&Operation>) -> Result<Vec<String>, String> {
        let result = self
            .transport
            .relay_information(self.guard(operation))
            .await
            .map_err(|error| error.to_string());
        self.guard(operation).await?;
        Ok(result?.supported_extensions.unwrap_or_default())
    }

    async fn prove_retired_dependency(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        event: &Event,
        error: &OperationTransportError,
    ) -> Result<Option<String>, WikiPublishError> {
        let OperationTransportError::RelayResponse {
            status: 400,
            reason,
        } = error
        else {
            return Ok(None);
        };
        let Some(machine_id) = parse_retired_dependency_reason(reason) else {
            return Ok(None);
        };
        let event_id = event.id.to_hex();
        if machine_id != event_id
            || (!record
                .pages
                .iter()
                .any(|candidate| candidate.id == event.id)
                && record.manifest.id != event.id)
        {
            return Ok(None);
        }
        let d = if event.id == record.manifest.id {
            manifest_d_tag(event).map_err(WikiPublishError::Unknown)?
        } else {
            page_d_tag(event).map_err(WikiPublishError::Unknown)?
        };
        let events = self
            .query(
                Some(operation),
                json!({"kinds":[WIKI_KIND],"authors":[self.owner.to_hex()],"ids":[event_id],"#d":[d],"limit":2}),
            )
            .await
            .map_err(|query_error| WikiPublishError::Unknown(query_error.to_string()))?;
        if events.is_empty() {
            Ok(Some(machine_id))
        } else {
            // A machine refusal paired with a live exact event is not a
            // permanent proof. Preserve it as an ambiguous relay outcome.
            Ok(None)
        }
    }
}

impl WikiPublicationRuntime for NativeWikiPublication {
    fn now(&self) -> Result<i64, String> {
        now()
    }

    async fn checkpoint(&self, operation: &Operation) -> Result<(), String> {
        self.guard(Some(operation)).await
    }

    async fn save(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String> {
        self.guard(None).await?;
        record.validate_intent(self.owner)?;
        let payload = serde_json::to_value(record)
            .map_err(|_| "Wiki publication recovery serialization failed.".to_string())?;
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
        Ok(self
            .extensions(Some(operation))
            .await?
            .iter()
            .any(|extension| extension == "crew-conditional-publication-v1"))
    }

    async fn inspect(
        &self,
        operation: &mut Operation,
        record: &mut WikiPublicationRecord,
        worker: &str,
    ) -> Result<WikiHead, String> {
        self.renew_lease(operation, record, worker).await?;
        let Some(head) = self.query_head(Some(operation)).await? else {
            return Ok(WikiHead::Missing);
        };
        if head.id == record.head.id {
            let state = self
                .dependencies_state(Some(operation), record, Some(worker))
                .await?;
            let Some(rechecked) = self.query_head(Some(operation)).await? else {
                return Err("Wiki head disappeared while verifying immutable dependencies.".into());
            };
            if rechecked.id != head.id {
                return Err("Wiki head changed while verifying immutable dependencies.".into());
            }
            return match state {
                WikiDependencyState::Verified => Ok(WikiHead::Applied),
                // A desired head with a missing immutable dependency can be
                // repaired by the normal exact-replay path.  Treat it as a
                // precondition that still needs publication; read-only
                // reconciliation will keep it unresolved instead.
                WikiDependencyState::Missing => Ok(WikiHead::Original),
            };
        }
        if record.expected_revision != "absent" && head.id.to_hex() == record.expected_revision {
            Ok(WikiHead::Original)
        } else {
            Ok(WikiHead::Conflict(head.id.to_hex()))
        }
    }

    async fn dependencies(
        &self,
        operation: &mut Operation,
        record: &mut WikiPublicationRecord,
        worker: &str,
    ) -> Result<WikiDependencyState, String> {
        self.renew_lease(operation, record, worker).await?;
        self.dependencies_state(Some(operation), record, Some(worker))
            .await
    }

    async fn publish(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        event: &Event,
    ) -> Result<(), WikiPublishError> {
        record
            .validate_intent(self.owner)
            .map_err(WikiPublishError::Unknown)?;
        let published = self
            .transport
            .publish(event, self.guard(Some(operation)))
            .await;
        let ack = match published {
            Ok(ack) => ack,
            Err(error) => {
                if let Some(event_id) = self
                    .prove_retired_dependency(operation, record, event, &error)
                    .await?
                {
                    return Err(WikiPublishError::ImmutableDependencyRetired { event_id });
                }
                return Err(WikiPublishError::Unknown(error.to_string()));
            }
        };
        self.guard(Some(operation))
            .await
            .map_err(WikiPublishError::Unknown)?;
        if !ack.accepted {
            return Err(WikiPublishError::Unknown(ack.message));
        }
        let d = if event.id == record.head.id {
            format!("{}/_toc", self.repo_d)
        } else if event.id == record.manifest.id {
            manifest_d_tag(&record.manifest).map_err(WikiPublishError::Unknown)?
        } else {
            page_d_tag(event).map_err(WikiPublishError::Unknown)?
        };
        let events = self
            .query(
                Some(operation),
                json!({"kinds":[WIKI_KIND],"authors":[self.owner.to_hex()],"ids":[event.id.to_hex()],"#d":[d],"limit":2}),
            )
            .await
            .map_err(|error| WikiPublishError::Unknown(error.to_string()))?;
        if exact_event(&events, event, &d).is_none() {
            return Err(WikiPublishError::Unknown(
                "Relay did not retain the exact Wiki event after ACK.".into(),
            ));
        }
        Ok(())
    }
}

pub(super) fn coordinate_parts(coordinate: &str) -> Result<(&str, &str), String> {
    let mut parts = coordinate.splitn(3, ':');
    let kind = parts.next();
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if kind != Some("30617")
        || owner.len() != 64
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || repo.is_empty()
        || repo.len() > 64
        || repo.starts_with('.')
        || repo.contains("..")
        || !repo
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("Wiki repository coordinate is invalid.".into());
    }
    Ok((owner, repo))
}

fn validate_coordinate(
    event: &Event,
    owner: PublicKey,
    repo: &str,
    expected_d: &str,
) -> Result<(), String> {
    if event.kind.as_u16() != WIKI_KIND || event.pubkey != owner || event.verify().is_err() {
        return Err("Relay returned an invalid signed Wiki head.".into());
    }
    let ds: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    if ds.len() != 1 || ds[0].as_slice() != ["d", expected_d] {
        return Err("Relay returned a Wiki event at a different coordinate.".into());
    }
    let coordinate = format!("30617:{owner}:{repo}");
    let as_: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "a"))
        .collect();
    if as_.len() == 1 && as_[0].as_slice() == ["a", coordinate.as_str()] {
        Ok(())
    } else {
        Err("Relay returned a Wiki event with an invalid repository association.".into())
    }
}

fn exact_event<'a>(events: &'a [Event], expected: &Event, d: &str) -> Option<&'a Event> {
    let [event] = events else { return None };
    (event.id == expected.id
        && event.kind.as_u16() == WIKI_KIND
        && event.pubkey == expected.pubkey
        && event.verify().is_ok()
        && event.tags.iter().any(|tag| tag.as_slice() == ["d", d]))
    .then_some(event)
}

fn manifest_d_tag(event: &Event) -> Result<String, String> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    let [tag] = tags.as_slice() else {
        return Err("Wiki manifest d tag is invalid.".into());
    };
    if tag.as_slice().len() != 2 || !tag.as_slice()[1].contains("/m1-") {
        return Err("Wiki manifest d tag is invalid.".into());
    }
    Ok(tag.as_slice()[1].clone())
}

fn page_d_tag(event: &Event) -> Result<String, String> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    let [tag] = tags.as_slice() else {
        return Err("Wiki page d tag is invalid.".into());
    };
    if tag.as_slice().len() != 2 || !tag.as_slice()[1].contains("/p1-") {
        return Err("Wiki page d tag is invalid.".into());
    }
    Ok(tag.as_slice()[1].clone())
}

fn exact_tag<'a>(event: &'a Event, name: &str, width: usize) -> Option<&'a [String]> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|value| value == name))
        .map(|tag| tag.as_slice())
        .collect();
    let [tag] = tags.as_slice() else { return None };
    (tag.len() == width).then_some(*tag)
}

fn tag_value(event: &Event, name: &str) -> Option<String> {
    exact_tag(event, name, 2).map(|tag| tag[1].clone())
}

fn is_lower_hex(value: &str, width: usize) -> bool {
    value.len() == width
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_retired_dependency_reason(reason: &str) -> Option<String> {
    let prefix = "conflict: wiki-immutable-retired:";
    let value = reason.strip_prefix(prefix)?;
    is_lower_hex(value, 64).then(|| value.to_owned())
}
