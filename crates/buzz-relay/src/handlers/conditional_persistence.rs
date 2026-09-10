//! Conditional NIP-33 persistence shared by HTTP and WebSocket ingest.
use std::sync::Arc;

use buzz_core::{
    kind::{KIND_GIT_REPO_ANNOUNCEMENT, KIND_PROJECT},
    tenant::TenantContext,
    StoredEvent,
};
use buzz_db::replaceable::ParameterizedReplaceStatus;
use nostr::Event;
use uuid::Uuid;

use super::conditional_publication::{policy, PublicationPolicy, PROJECT_EXTENSION};
use super::ingest::IngestError;
use crate::state::AppState;

/// Preserve legacy Project/page writes; apply explicit conditional and immutable
/// Wiki rules inside the existing deletion-fenced event transaction.
pub(crate) async fn persist(
    state: &Arc<AppState>,
    tenant: &TenantContext,
    event: &Event,
    channel: Option<Uuid>,
) -> Result<(StoredEvent, bool), IngestError> {
    persist_with_db(
        &state.db,
        tenant,
        event,
        channel,
        state.config.crew_conditional_publication_v1,
        state.config.crew_project_channel_link_v1,
    )
    .await
}

// The same production transaction/dispatch boundary is exercised by owned DB fixtures.
async fn persist_with_db(
    db: &buzz_db::Db,
    tenant: &TenantContext,
    event: &Event,
    channel: Option<Uuid>,
    conditional_enabled: bool,
    project_enabled: bool,
) -> Result<(StoredEvent, bool), IngestError> {
    let d = buzz_db::event::extract_d_tag(event).unwrap_or_default();
    if d.len() > buzz_db::event::D_TAG_MAX_LEN {
        return Err(IngestError::Rejected(format!(
            "invalid: d tag too long ({} bytes, max {})",
            d.len(),
            buzz_db::event::D_TAG_MAX_LEN
        )));
    }
    let policy = policy(event, conditional_enabled)?;
    let Some(policy) = policy.filter(|policy| !matches!(policy, PublicationPolicy::Legacy)) else {
        return db
            .replace_parameterized_event(tenant.community(), event, &d, channel)
            .await
            .map_err(|error| IngestError::Internal(format!("error: {error}")));
    };
    let project = u32::from(event.kind.as_u16()) == KIND_PROJECT;
    if project && !project_enabled {
        return Err(IngestError::Rejected(format!(
            "unsupported: {PROJECT_EXTENSION}"
        )));
    }
    let mut tx = db
        .begin_event_write_transaction()
        .await
        .map_err(|_| IngestError::Internal("error: publication transaction unavailable".into()))?;
    buzz_deletion::store(db)
        .guard_transaction(&mut tx, tenant.community())
        .await
        .map_err(|_| IngestError::Rejected("restricted: community writes are fenced".into()))?;
    let immutable = event.kind.as_u16() == 30623
        && d.split_once('/')
            .is_some_and(|(_, slug)| slug.starts_with("p1-") || slug.starts_with("m1-"));
    let result = if project {
        db.replace_project_event_in_transaction(
            &mut tx,
            tenant.community(),
            event,
            &d,
            policy.precondition(),
        )
        .await
    } else if immutable {
        db.replace_wiki_immutable_event_in_transaction(
            &mut tx,
            tenant.community(),
            event,
            &d,
            channel,
            matches!(policy, PublicationPolicy::ReplayOnly),
        )
        .await
    } else {
        db.replace_parameterized_event_in_transaction(
            &mut tx,
            tenant.community(),
            event,
            &d,
            channel,
            policy.precondition(),
        )
        .await
    }
    .map_err(|error| match error {
        buzz_db::DbError::AccessDenied(_) if project => {
            IngestError::Rejected("restricted: project-channel-association".into())
        }
        buzz_db::DbError::InvalidData(_) if project => {
            IngestError::Rejected("invalid: project-channel-association".into())
        }
        buzz_db::DbError::WikiStorageQuota => {
            IngestError::Rejected("restricted: wiki-storage-quota".into())
        }
        _ => IngestError::Internal("error: publication storage unavailable".into()),
    })?;
    let inserted = match result.status {
        ParameterizedReplaceStatus::Inserted => true,
        ParameterizedReplaceStatus::Duplicate => false,
        ParameterizedReplaceStatus::Superseded | ParameterizedReplaceStatus::DuplicateNotLive
            if matches!(policy, PublicationPolicy::LegacyWikiHead) =>
        {
            false
        }
        _ => {
            return Err(IngestError::Rejected(
                "conflict: publication revision is not current".into(),
            ))
        }
    };
    tx.commit()
        .await
        .map_err(|_| IngestError::Internal("error: publication commit outcome unknown".into()))?;
    Ok((result.event, inserted))
}

/// Reconcile a conditional repository announcement on both insertion and exact
/// live replay. A committed event is its durable retry record if ensure fails.
/// The caller dispatches a newly inserted event before propagating that failure.
pub(crate) async fn ensure_repository(
    tenant: &TenantContext,
    event: &Event,
    state: &Arc<AppState>,
) -> Result<bool, IngestError> {
    if u32::from(event.kind.as_u16()) != KIND_GIT_REPO_ANNOUNCEMENT
        || !event.tags.iter().any(|tag| {
            tag.as_slice()
                .first()
                .is_some_and(|key| key == "expected-revision")
        })
    {
        return Ok(false);
    }
    super::side_effects::handle_side_effects(tenant, KIND_GIT_REPO_ANNOUNCEMENT, event, state)
        .await
        .map_err(|error| {
            tracing::error!(event_id = %event.id, "conditional repository ensure failed: {error}");
            IngestError::Internal("error: side-effect-pending".into())
        })?;
    Ok(true)
}

#[cfg(test)]
#[path = "conditional_project_tests.rs"]
mod project_tests;
