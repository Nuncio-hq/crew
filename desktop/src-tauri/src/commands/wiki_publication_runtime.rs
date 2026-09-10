//! Captured desktop runtime for the Wiki publication driver.

use super::owner_operation_transport::{OperationTransportError, OwnerOperationTransport};
use super::owner_operations::owner_operation_update;
use super::wiki_publication_driver::{
    WikiDependencyState, WikiHead, WikiHeadRetirementProof, WikiPublicationRuntime,
    WikiPublishError,
};
use super::wiki_publication_native_reads::NativeReadContext;
use super::wiki_publication_record::{WikiHeadRetirement, WikiPublicationRecord};
use crate::app_state::owner_scope::{capture, OwnerScopeToken};
use crate::owner_operations::{Operation, OperationStatus, OperationUpdate};
use nostr::{Event, Keys, PublicKey};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager};

pub(super) const WIKI_KIND: u16 = 30623;
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

    /// Borrow this publication's captured identity, transport and coordinate
    /// as the shared guarded-read context. The journal path stays lazily
    /// resolved and the clock stays the system clock: production behaviour is
    /// unchanged, and a test can drive the very same checks with an owned
    /// journal and a controlled clock.
    fn reads(&self) -> NativeReadContext<'_, tauri::Wry> {
        NativeReadContext {
            app: &self.app,
            expected: &self.expected,
            owner: self.owner,
            repo_d: &self.repo_d,
            transport: &self.transport,
            journal: &super::wiki_publication_native_reads::FROM_APP,
            clock: &super::wiki_publication_native_reads::SYSTEM_CLOCK,
        }
    }

    async fn guard(&self, operation: Option<&Operation>) -> Result<(), String> {
        self.reads().guard(operation).await
    }

    async fn query(
        &self,
        operation: Option<&Operation>,
        filter: Value,
    ) -> Result<Vec<Event>, String> {
        self.reads().query(operation, filter).await
    }

    async fn query_head(&self, operation: Option<&Operation>) -> Result<Option<Event>, String> {
        self.reads().query_head(operation).await
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

    async fn prove_retired_head(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        event: &Event,
        error: &OperationTransportError,
    ) -> Result<Option<WikiHeadRetirementProof>, WikiPublishError> {
        validate_head_retirement_refusal(self, operation, record, event, error).await
    }
}

/// The two scoped reads the head-retirement decision depends on.
///
/// Both implementations go through the captured runtime's guarded query path,
/// which re-checks the owner/community/generation, the operation revision and
/// the worker lease immediately before *and* after each transport await. The
/// trait exists so the decision below can be exercised directly without an
/// `AppHandle`; it is not a second decision, and it adds no public API.
pub(super) trait HeadRetirementReads {
    /// Scoped read of one exact event ID at this repository's `_toc` address.
    async fn read_exact_toc_event(
        &self,
        operation: &Operation,
        event_id: &str,
    ) -> Result<Vec<Event>, String>;
    /// Scoped read of the coordinate's current `_toc` head.
    async fn read_current_toc(&self, operation: &Operation) -> Result<Option<Event>, String>;
}

impl HeadRetirementReads for NativeWikiPublication {
    async fn read_exact_toc_event(
        &self,
        operation: &Operation,
        event_id: &str,
    ) -> Result<Vec<Event>, String> {
        let head_d = format!("{}/_toc", self.repo_d);
        self.query(
            Some(operation),
            json!({
                "kinds":[WIKI_KIND],
                "authors":[self.owner.to_hex()],
                "ids":[event_id],
                "#d":[head_d],
                "limit":2
            }),
        )
        .await
    }

    async fn read_current_toc(&self, operation: &Operation) -> Result<Option<Event>, String> {
        self.query_head(Some(operation)).await
    }
}

/// Validate a relay head/precondition retirement refusal for the exact head
/// this operation is submitting (D-079; accepted, pending acceptance).
///
/// This is the single production decision `NativeWikiPublication::publish`
/// uses. Every one of these must hold or the outcome stays `Unknown` with a
/// retryable claim: the captured relay answered HTTP 400, the machine reason
/// parses exactly, its IDs bind this exact signed head (and, for the
/// precondition form, this exact non-absent `expected_revision`), the
/// owner/community/generation/revision/lease fences still pass on both reads,
/// the exact allegedly retired event really reads back absent, and the current
/// `_toc` read succeeds without contradicting the claim.
pub(super) async fn validate_head_retirement_refusal<R: HeadRetirementReads>(
    reads: &R,
    operation: &Operation,
    record: &WikiPublicationRecord,
    event: &Event,
    error: &OperationTransportError,
) -> Result<Option<WikiHeadRetirementProof>, WikiPublishError> {
    let OperationTransportError::RelayResponse {
        status: 400,
        reason,
    } = error
    else {
        return Ok(None);
    };
    // Only the head write can carry a head-retirement proof. A page or
    // manifest refusal is a different fact and is never reinterpreted here.
    if event.id != record.head.id {
        return Ok(None);
    }
    let head_id = record.head.id.to_hex();
    let Some(parsed) = parse_retired_head_reason(reason) else {
        return Ok(None);
    };
    let (retired_event_id, retirement) = match parsed {
        ParsedHeadRetirement::Head { head } => {
            if head != head_id {
                return Ok(None);
            }
            (head.clone(), WikiHeadRetirement::Head { head_id: head })
        }
        ParsedHeadRetirement::ExpectedHead { head, expected } => {
            if head != head_id
                || record.expected_revision == "absent"
                || expected != record.expected_revision
            {
                return Ok(None);
            }
            (
                expected.clone(),
                WikiHeadRetirement::ExpectedHead {
                    head_id: head,
                    expected_revision: expected,
                },
            )
        }
    };

    // The allegedly retired event must actually read back absent at this exact
    // coordinate. A failed query is Unknown, never proof.
    let retired = reads
        .read_exact_toc_event(operation, &retired_event_id)
        .await
        .map_err(WikiPublishError::Unknown)?;
    if !retired.is_empty() {
        return Ok(None);
    }

    // Read the current head as well. Two different facts contradict the claim
    // and both must be rejected: a live desired head H means the attempt did
    // land, and a live copy of the allegedly retired event — H for the head
    // form, E for the precondition form — means it was never retired at all.
    // Either way this stays Unknown with the claim intact. Any *other* current
    // head is legitimate and is retained as Superseded metadata.
    let current = reads
        .read_current_toc(operation)
        .await
        .map_err(WikiPublishError::Unknown)?;
    if let Some(current) = current.as_ref() {
        let current_id = current.id.to_hex();
        if current_id == head_id || current_id == retired_event_id {
            return Ok(None);
        }
    }
    Ok(Some(WikiHeadRetirementProof {
        retirement,
        current_head_id: current.map(|head| head.id.to_hex()),
    }))
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
                if let Some(proof) = self
                    .prove_retired_head(operation, record, event, &error)
                    .await?
                {
                    return Err(WikiPublishError::HeadRetired(Box::new(proof)));
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

pub(super) fn validate_coordinate(
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

/// Machine reason shapes for D-079 head/precondition retirement.
pub(super) enum ParsedHeadRetirement {
    Head { head: String },
    ExpectedHead { head: String, expected: String },
}

/// Parse the exact machine reasons the relay emits for a retired conditional
/// head or a retired exact precondition.
///
/// The syntax is strict and total: any other text, a generic conflict, an
/// older relay's phrasing, or a malformed ID yields `None` and the outcome
/// stays Unknown. The longer prefix is tested first so it cannot be shadowed.
pub(super) fn parse_retired_head_reason(reason: &str) -> Option<ParsedHeadRetirement> {
    if let Some(value) = reason.strip_prefix("conflict: wiki-expected-head-retired:") {
        let (head, expected) = value.split_once(':')?;
        return (is_lower_hex(head, 64) && is_lower_hex(expected, 64)).then(|| {
            ParsedHeadRetirement::ExpectedHead {
                head: head.to_owned(),
                expected: expected.to_owned(),
            }
        });
    }
    let value = reason.strip_prefix("conflict: wiki-head-retired:")?;
    is_lower_hex(value, 64).then(|| ParsedHeadRetirement::Head {
        head: value.to_owned(),
    })
}

fn parse_retired_dependency_reason(reason: &str) -> Option<String> {
    let prefix = "conflict: wiki-immutable-retired:";
    let value = reason.strip_prefix(prefix)?;
    is_lower_hex(value, 64).then(|| value.to_owned())
}

#[cfg(test)]
mod head_retirement_reason_tests {
    use super::{parse_retired_head_reason, ParsedHeadRetirement};

    const H: &str = "aa11bb22cc33dd44ee55ff6677889900aa11bb22cc33dd44ee55ff6677889900";
    const E: &str = "bb11cc22dd33ee44ff5500667788990011223344556677889900aabbccddeeff";

    /// Only the exact machine syntax is proof. Everything else — a generic
    /// conflict, an older relay's phrasing, a truncated or upper-case ID, a
    /// missing field — must stay unproven so the driver keeps a retryable
    /// claim instead of settling on a guess.
    #[test]
    fn only_exact_head_retirement_machine_reasons_parse() {
        match parse_retired_head_reason(&format!("conflict: wiki-head-retired:{H}")) {
            Some(ParsedHeadRetirement::Head { head }) => assert_eq!(head, H),
            other => panic!("exact head reason must parse: {}", other.is_some()),
        }
        match parse_retired_head_reason(&format!("conflict: wiki-expected-head-retired:{H}:{E}")) {
            Some(ParsedHeadRetirement::ExpectedHead { head, expected }) => {
                assert_eq!(head, H);
                assert_eq!(expected, E);
            }
            other => panic!("exact precondition reason must parse: {}", other.is_some()),
        }

        for unproven in [
            String::from("conflict: conditional publication revision changed"),
            String::from("conflict: conditional publication is no longer the live head"),
            format!("conflict: wiki-immutable-retired:{H}"),
            format!("conflict: wiki-head-retired:{}", H.to_uppercase()),
            format!("conflict: wiki-head-retired:{}", &H[..63]),
            format!("conflict: wiki-head-retired:{H}:{E}"),
            format!("conflict: wiki-expected-head-retired:{H}"),
            format!("conflict: wiki-expected-head-retired:{H}:"),
            format!("conflict: wiki-expected-head-retired:{H}:{}", &E[..10]),
            format!("  conflict: wiki-head-retired:{H}"),
            format!("restricted: wiki-head-retired:{H}"),
        ] {
            assert!(
                parse_retired_head_reason(&unproven).is_none(),
                "must not parse as proof: {unproven}"
            );
        }
    }
}
