//! The captured native read path for Wiki publication.
//!
//! This is the *only* implementation of the guarded read fences. It is a
//! borrowed context rather than a second copy: `NativeWikiPublication`
//! delegates `guard`, `query` and `query_head` here, so the identity capture,
//! durable revision/payload comparison, lease decode and transport dispatch a
//! test exercises are byte-for-byte the ones production runs.
//!
//! Two details are deliberately injected rather than looked up:
//!
//! * the journal path, resolved **lazily** and only when a read is fenced on
//!   an operation — a read-only query (`operation = None`) must not touch the
//!   app-data directory at all, exactly as before this extraction;
//! * the clock, so a lease can be aged without sleeping.
//!
//! Production supplies the real `journal_path` and `now`; a test supplies an
//! owned temporary journal and a controlled counter. Nothing else differs.

use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::{AtomicI64, Ordering};
#[cfg(test)]
use std::sync::Arc;

use nostr::{Event, PublicKey};
use serde_json::{json, Value};
use tauri::{AppHandle, Runtime};

use super::owner_operation_transport::OwnerOperationTransport;
use super::owner_operations::{journal_path, load_owner_operation_for_dispatch_at_path};
use super::wiki_publication_record::WikiPublicationLease;
use super::wiki_publication_runtime::{now, validate_coordinate, HeadRetirementReads, WIKI_KIND};
use crate::app_state::owner_scope::{assert_current, capture, OwnerScopeToken};
use crate::owner_operations::Operation;

/// How this context reaches the durable recovery journal.
#[derive(Clone)]
pub(super) enum NativeJournal {
    /// Production: the trusted platform app-data anchor, resolved on use.
    FromApp,
    /// An owned journal file supplied by a test fixture. Injected data only;
    /// it never exists in a shipped binary, so the shipped `resolve` has a
    /// single arm and carries no unreachable branch.
    #[cfg(test)]
    Path(PathBuf),
}

impl NativeJournal {
    pub(super) fn resolve<R: Runtime>(&self, app: &AppHandle<R>) -> Result<PathBuf, String> {
        match self {
            Self::FromApp => journal_path(app),
            #[cfg(test)]
            Self::Path(path) => Ok(path.clone()),
        }
    }
}

/// The clock the lease fence compares against.
#[derive(Clone)]
pub(super) enum NativeClock {
    /// Production: the system clock through the existing `now` helper.
    System,
    /// A controlled counter a test can advance past a lease. Injected data
    /// only; absent from a shipped binary.
    #[cfg(test)]
    Fixed(Arc<AtomicI64>),
}

impl NativeClock {
    pub(super) fn now(&self) -> Result<i64, String> {
        match self {
            Self::System => now(),
            #[cfg(test)]
            Self::Fixed(value) => Ok(value.load(Ordering::SeqCst)),
        }
    }
}

/// Everything a guarded native read borrows for the duration of one call.
pub(super) struct NativeReadContext<'a, R: Runtime> {
    pub(super) app: &'a AppHandle<R>,
    pub(super) expected: &'a OwnerScopeToken,
    pub(super) owner: PublicKey,
    pub(super) repo_d: &'a str,
    pub(super) transport: &'a OwnerOperationTransport,
    pub(super) journal: &'a NativeJournal,
    pub(super) clock: &'a NativeClock,
}

impl<R: Runtime> NativeReadContext<'_, R> {
    /// Re-capture the native scope and, when this read is fenced on an
    /// operation, re-read that exact durable row.
    ///
    /// Order is load-bearing and unchanged: capture the active
    /// owner/community/generation first and refuse a moved scope before any
    /// journal work; only then resolve the journal path and compare the
    /// durable payload/status/reconciled state at the caller's revision;
    /// decode only the mutable lease metadata; finish with `assert_current`
    /// so a scope that moved during the load is still caught.
    pub(super) async fn guard(&self, operation: Option<&Operation>) -> Result<(), String> {
        let captured = capture(self.app.clone()).await?;
        if captured.token != *self.expected || captured.keys.public_key() != self.owner {
            return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
        }
        if let Some(operation) = operation {
            let path = self.journal.resolve(self.app)?;
            let (_, current) = load_owner_operation_for_dispatch_at_path(
                self.app.clone(),
                path,
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
            let current_time = self.clock.now()?;
            if lease.is_none_or(|lease| lease.expires_at <= current_time) {
                return Err("Wiki publication worker lease expired.".into());
            }
        }
        assert_current(self.app.clone(), self.expected).await
    }

    /// One scoped relay query, fenced immediately before the transport await
    /// and again on its result.
    pub(super) async fn query(
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

    /// The coordinate's current `_toc` head, validated against this exact
    /// owner and repository.
    pub(super) async fn query_head(
        &self,
        operation: Option<&Operation>,
    ) -> Result<Option<Event>, String> {
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
                    self.repo_d,
                    &format!("{}/_toc", self.repo_d),
                )?;
                Ok(Some(event.clone()))
            }
            _ => Err("Wiki head query returned duplicate events.".into()),
        }
    }
}

impl<R: Runtime> HeadRetirementReads for NativeReadContext<'_, R> {
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
