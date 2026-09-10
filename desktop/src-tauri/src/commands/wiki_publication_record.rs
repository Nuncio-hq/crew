//! Typed recovery payload for one coherent repository Wiki publication.
//!
//! The record deliberately owns the complete signed graph.  A retry may
//! inspect or resend these exact envelopes, but it cannot ask the renderer or
//! a newer generator to replace one of them after the operation is reserved.

use crew_wiki::snapshot_v1::verify_snapshot;
use nostr::{Event, PublicKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::owner_operations::{Operation, OperationKind};

const MAX_EVENT_BYTES: usize = 192 * 1024;
const MAX_PUBLICATION_BYTES: usize = 64 * 1024 * 1024;
const MAX_PAGES: usize = 256;

/// Which immutable dependency the writer has durably confirmed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum WikiPublicationProgress {
    /// The operation is reserved, but no dependency has been attempted.
    Preparing,
    /// Exact page IDs below `confirmed` have been read back.
    Pages { confirmed: usize },
    /// Every page is confirmed and the immutable manifest is next.
    Manifest,
    /// Pages and manifest are confirmed and the conditional head is next.
    Head,
}

/// Evidence recorded only after the native dispatcher establishes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "proof", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum WikiPublicationReconciliation {
    /// The exact head event is authoritative at the repository coordinate.
    Applied { head_id: String },
    /// A different head won, so the stored publication can never be submitted.
    Superseded {
        current_head_id: Option<String>,
        /// When regeneration retired an immutable dependency, preserve the
        /// typed proof on the reconciled predecessor as well.
        #[serde(default)]
        retired_dependency_id: Option<String>,
    },
    /// User canceled before the conditional head was attempted and native
    /// reconciliation proved the expected head is still live (or absent).
    CanceledBeforeHead { current_head_id: Option<String> },
    /// The relay permanently refused one exact immutable dependency because
    /// its reserved address is retired. This is evidence for the explicit
    /// successor flow only; it does not prove that the old TOC write landed.
    ImmutableDependencyRetired { dependency_id: String },
}

/// A journal lease identifies the current native worker without storing keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WikiPublicationLease {
    pub(super) worker_id: String,
    pub(super) expires_at: i64,
}

/// Complete immutable Wiki publication plus bounded recovery metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WikiPublicationRecord {
    pub(super) version: u32,
    /// Full NIP-34 repository coordinate, including the owner key.
    pub(super) coordinate: String,
    /// Snapshot UUID bound into the manifest and every page envelope.
    pub(super) snapshot_id: String,
    /// Exact captured source revision bound by the immutable graph.
    pub(super) source_revision: String,
    /// Conditional TOC precondition (`absent` or one event ID).
    pub(super) expected_revision: String,
    /// The only replaceable event in the graph.
    pub(super) head: Event,
    /// Immutable manifest referenced by `head`.
    pub(super) manifest: Event,
    /// Immutable pages in manifest traversal order.
    pub(super) pages: Vec<Event>,
    /// Head cadence retained as journal metadata and checked against the head.
    pub(super) cadence: String,
    pub(super) attempts: u8,
    pub(super) progress: WikiPublicationProgress,
    /// Set immediately before the conditional `_toc` publication is attempted.
    /// Immutable dependency publication may have happened while this remains
    /// false, so recovery must never infer head delivery from this field.
    #[serde(alias = "attempted")]
    pub(super) head_attempted: bool,
    /// User requested cancellation. Cancellation is read-only until the exact
    /// persisted graph has been reconciled.
    #[serde(default)]
    pub(super) cancel_requested: bool,
    /// Once set, this record may only perform read-only reconciliation.
    pub(super) reconcile_only: bool,
    pub(super) reconciliation: Option<WikiPublicationReconciliation>,
    /// Unix time at which an automatic retry may begin; zero means due now.
    pub(super) retry_at: i64,
    pub(super) last_error: Option<String>,
    pub(super) lease: Option<WikiPublicationLease>,
}

impl WikiPublicationRecord {
    /// Validate the immutable intent against the native repository owner.
    pub(super) fn validate_intent(&self, native_owner: PublicKey) -> Result<(), String> {
        if self.version != 1
            || self.attempts > 5
            || self.retry_at < 0
            || self.pages.is_empty()
            || self.pages.len() > MAX_PAGES
        {
            return Err("Invalid Wiki publication recovery payload.".into());
        }
        let (owner, repo_d) = coordinate_parts(&self.coordinate)?;
        if owner != native_owner.to_hex() {
            return Err("Wiki publication owner does not match the native signer.".into());
        }
        if self.expected_revision != "absent" && !is_hex(&self.expected_revision, 64) {
            return Err("Wiki publication expected revision is invalid.".into());
        }
        if !is_revision(&self.source_revision)
            || !is_uuid_v4(&self.snapshot_id)
            || buzz_core_pkg::wiki_page::WikiCadence::parse(&self.cadence).is_err()
        {
            return Err("Wiki publication metadata is invalid.".into());
        }
        for event in std::iter::once(&self.head)
            .chain(std::iter::once(&self.manifest))
            .chain(self.pages.iter())
        {
            validate_event(event, native_owner)?;
        }
        let head = event_value(&self.head)?;
        let manifest = event_value(&self.manifest)?;
        let pages: Vec<Value> = self
            .pages
            .iter()
            .map(event_value)
            .collect::<Result<_, _>>()?;
        let verified = verify_snapshot(&owner, &repo_d, &head, &manifest, &pages)
            .map_err(|error| format!("Invalid Wiki publication graph: {error}"))?;
        let manifest_tuple = verified.index().manifest();
        if manifest_tuple.1 != self.snapshot_id
            || manifest_tuple.2 != owner
            || manifest_tuple.3 != repo_d
            || manifest_tuple.4 != self.source_revision
            || expected_revision_from_head(&self.head)? != self.expected_revision
            || cadence_from_head(&self.head)? != self.cadence
        {
            return Err("Stored Wiki publication metadata does not match its signed graph.".into());
        }
        let expected_head_d = format!("{repo_d}/_toc");
        if !self
            .head
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["d", expected_head_d.as_str()])
        {
            return Err("Wiki publication head is at the wrong coordinate.".into());
        }
        validate_progress(&self.progress, self.pages.len())?;
        if self.head_attempted && matches!(self.progress, WikiPublicationProgress::Preparing) {
            return Err("Wiki publication head attempt has no recorded dependency phase.".into());
        }
        let retirement_id = match &self.reconciliation {
            Some(WikiPublicationReconciliation::ImmutableDependencyRetired { dependency_id }) => {
                Some(dependency_id)
            }
            Some(WikiPublicationReconciliation::Superseded {
                retired_dependency_id: Some(dependency_id),
                ..
            }) => Some(dependency_id),
            _ => None,
        };
        if let Some(dependency_id) = retirement_id {
            if !is_hex(dependency_id, 64)
                || !std::iter::once(&self.manifest)
                    .chain(self.pages.iter())
                    .any(|event| event.id.to_hex() == *dependency_id)
            {
                return Err("Wiki retirement evidence does not name a stored dependency.".into());
            }
        }
        if let Some(error) = &self.last_error {
            if error.is_empty() || error.len() > 512 || error.contains('\0') {
                return Err("Wiki publication failure metadata is invalid.".into());
            }
        }
        if let Some(lease) = &self.lease {
            let worker = uuid::Uuid::parse_str(&lease.worker_id)
                .map_err(|_| "Invalid Wiki publication worker identity.")?;
            if worker.to_string() != lease.worker_id || lease.expires_at < 0 {
                return Err("Invalid Wiki publication worker lease.".into());
            }
        }
        let total = std::iter::once(&self.head)
            .chain(std::iter::once(&self.manifest))
            .chain(self.pages.iter())
            .try_fold(0usize, |total, event| {
                total
                    .checked_add(serialized_size(event)?)
                    .ok_or_else(|| "Wiki publication size overflow.".to_string())
            })?;
        if total > MAX_PUBLICATION_BYTES {
            return Err("Wiki publication exceeds its 64 MiB bound.".into());
        }
        Ok(())
    }

    /// Validate only the bounded journal metadata needed for a status
    /// projection.  Full signature, graph-membership, and snapshot checks stay
    /// on the dispatch/reconciliation seam; repeatedly doing those checks for
    /// every status poll makes the UI cost scale with every retained page.
    pub(super) fn validate_projection(&self, native_owner: PublicKey) -> Result<(), String> {
        if self.version != 1
            || self.attempts > 5
            || self.retry_at < 0
            || self.pages.is_empty()
            || self.pages.len() > MAX_PAGES
        {
            return Err("Invalid Wiki publication recovery payload.".into());
        }
        let (owner, _) = coordinate_parts(&self.coordinate)?;
        if owner != native_owner.to_hex() {
            return Err("Wiki publication owner does not match the native signer.".into());
        }
        if self.expected_revision != "absent" && !is_hex(&self.expected_revision, 64) {
            return Err("Wiki publication expected revision is invalid.".into());
        }
        if !is_revision(&self.source_revision)
            || !is_uuid_v4(&self.snapshot_id)
            || buzz_core_pkg::wiki_page::WikiCadence::parse(&self.cadence).is_err()
        {
            return Err("Wiki publication metadata is invalid.".into());
        }
        for event in std::iter::once(&self.head)
            .chain(std::iter::once(&self.manifest))
            .chain(self.pages.iter())
        {
            validate_event_shape(event, native_owner)?;
        }
        if expected_revision_from_head(&self.head)? != self.expected_revision
            || cadence_from_head(&self.head)? != self.cadence
        {
            return Err("Stored Wiki publication metadata does not match its head.".into());
        }
        validate_progress(&self.progress, self.pages.len())?;
        if self.head_attempted && matches!(self.progress, WikiPublicationProgress::Preparing) {
            return Err("Wiki publication head attempt has no recorded dependency phase.".into());
        }
        validate_retirement_metadata(self)?;
        validate_failure_metadata(self)?;
        let total = std::iter::once(&self.head)
            .chain(std::iter::once(&self.manifest))
            .chain(self.pages.iter())
            .try_fold(0usize, |total, event| {
                total
                    .checked_add(serialized_size(event)?)
                    .ok_or_else(|| "Wiki publication size overflow.".to_string())
            })?;
        if total > MAX_PUBLICATION_BYTES {
            return Err("Wiki publication exceeds its 64 MiB bound.".into());
        }
        Ok(())
    }

    /// Parse and validate an opaque owner-operation row before dispatch.
    pub(super) fn from_operation(
        operation: &Operation,
        native_owner: PublicKey,
    ) -> Result<Self, String> {
        if operation.kind != OperationKind::WikiPublication || operation.reconciled {
            return Err("Wiki publication is not available for dispatch.".into());
        }
        let record: Self = serde_json::from_value(operation.payload.clone())
            .map_err(|_| "Invalid Wiki publication recovery payload.".to_string())?;
        record.validate_intent(native_owner)?;
        if operation.resource_key != record.coordinate {
            return Err("Wiki publication resource does not match its signed coordinate.".into());
        }
        if operation.scope.owner != native_owner.to_hex() {
            return Err("Wiki publication scope does not match its native owner.".into());
        }
        if let Some(lease) = &record.lease {
            if lease.expires_at > operation.updated_at.saturating_add(60) {
                return Err("Wiki publication lease exceeds its journal bound.".into());
            }
        }
        Ok(record)
    }
}

fn validate_progress(progress: &WikiPublicationProgress, pages: usize) -> Result<(), String> {
    let valid = match progress {
        WikiPublicationProgress::Preparing => true,
        WikiPublicationProgress::Pages { confirmed } => *confirmed <= pages,
        WikiPublicationProgress::Manifest | WikiPublicationProgress::Head => true,
    };
    valid
        .then_some(())
        .ok_or_else(|| "Wiki publication progress exceeds its page set.".into())
}

fn validate_event(event: &Event, owner: PublicKey) -> Result<(), String> {
    validate_event_shape(event, owner)?;
    if event.verify().is_err() {
        return Err("Wiki publication contains an invalid signed event.".into());
    }
    Ok(())
}

fn validate_event_shape(event: &Event, owner: PublicKey) -> Result<(), String> {
    if event.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_REPO_WIKI_PAGE
        || event.pubkey != owner
    {
        return Err("Wiki publication contains an invalid signed event.".into());
    }
    if serialized_size(event)? > MAX_EVENT_BYTES {
        return Err("Wiki publication event exceeds its 192 KiB bound.".into());
    }
    Ok(())
}

fn validate_retirement_metadata(record: &WikiPublicationRecord) -> Result<(), String> {
    let retirement_id = match &record.reconciliation {
        Some(WikiPublicationReconciliation::ImmutableDependencyRetired { dependency_id }) => {
            Some(dependency_id)
        }
        Some(WikiPublicationReconciliation::Superseded {
            retired_dependency_id: Some(dependency_id),
            ..
        }) => Some(dependency_id),
        _ => None,
    };
    if let Some(dependency_id) = retirement_id {
        if !is_hex(dependency_id, 64)
            || !std::iter::once(&record.manifest)
                .chain(record.pages.iter())
                .any(|event| event.id.to_hex() == *dependency_id)
        {
            return Err("Wiki retirement evidence does not name a stored dependency.".into());
        }
    }
    Ok(())
}

fn validate_failure_metadata(record: &WikiPublicationRecord) -> Result<(), String> {
    if let Some(error) = &record.last_error {
        if error.is_empty() || error.len() > 512 || error.contains('\0') {
            return Err("Wiki publication failure metadata is invalid.".into());
        }
    }
    if let Some(lease) = &record.lease {
        let worker = uuid::Uuid::parse_str(&lease.worker_id)
            .map_err(|_| "Invalid Wiki publication worker identity.")?;
        if worker.to_string() != lease.worker_id || lease.expires_at < 0 {
            return Err("Invalid Wiki publication worker lease.".into());
        }
    }
    Ok(())
}

fn serialized_size(event: &Event) -> Result<usize, String> {
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("event exceeds bound"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut budget = Budget(MAX_EVENT_BYTES.max(MAX_PUBLICATION_BYTES));
    serde_json::to_writer(&mut budget, event)
        .map_err(|_| "Wiki publication event is not serializable.".to_string())?;
    Ok(MAX_EVENT_BYTES.max(MAX_PUBLICATION_BYTES) - budget.0)
}

fn event_value(event: &Event) -> Result<Value, String> {
    serde_json::to_value(event).map_err(|_| "Wiki publication event is not serializable.".into())
}

fn expected_revision_from_head(head: &Event) -> Result<String, String> {
    exact_tag(head, "expected-revision")
}

fn cadence_from_head(head: &Event) -> Result<String, String> {
    exact_tag(head, "cadence")
}

fn exact_tag(event: &Event, name: &str) -> Result<String, String> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| {
            tag.as_slice()
                .first()
                .is_some_and(|candidate| candidate == name)
        })
        .collect();
    if tags.len() != 1 || tags[0].as_slice().len() != 2 {
        return Err(format!("Wiki head has invalid {name} metadata."));
    }
    Ok(tags[0].as_slice()[1].clone())
}

fn coordinate_parts(coordinate: &str) -> Result<(String, String), String> {
    let mut parts = coordinate.splitn(3, ':');
    if parts.next() != Some("30617") {
        return Err("Wiki publication requires a repository coordinate.".into());
    }
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if !is_hex(owner, 64)
        || repo.is_empty()
        || repo.len() > 64
        || repo.starts_with('.')
        || repo.contains("..")
        || !repo
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || parts.next().is_some()
    {
        return Err("Wiki publication repository coordinate is invalid.".into());
    }
    Ok((owner.to_owned(), repo.to_owned()))
}

fn is_hex(value: &str, width: usize) -> bool {
    value.len() == width
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index)
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(byte)
        })
}

fn is_revision(value: &str) -> bool {
    value
        .strip_prefix("git:")
        .is_some_and(|hash| is_hex(hash, 40) || is_hex(hash, 64))
        || value
            .strip_prefix("folder:")
            .is_some_and(|hash| is_hex(hash, 64))
}
