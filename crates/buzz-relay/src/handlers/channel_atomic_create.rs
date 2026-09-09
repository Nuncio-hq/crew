//! Opt-in atomic channel creation and exact-event discovery recovery.

use crate::state::AppState;
use buzz_core::{tenant::TenantContext, StoredEvent};
use nostr::Event;
use std::sync::Arc;
use uuid::Uuid;

/// NIP-11 extension enabled only after all relay writers are upgraded or quiesced.
pub const EXTENSION: &str = "crew-atomic-channel-create";

/// Add the advertised guarantee only after the operator activates it.
pub fn advertise(extensions: &mut Option<Vec<String>>, enabled: bool) {
    if enabled {
        extensions
            .get_or_insert_with(Vec::new)
            .push(EXTENSION.to_string());
    }
}

/// Validate the signed opt-in and refuse unsupported semantics instead of falling back.
pub fn requested(event: &Event, enabled: bool) -> Result<bool, String> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| {
            tag.as_slice()
                .first()
                .is_some_and(|value| value == "crew-atomic-create")
        })
        .collect();
    if tags.is_empty() {
        return Ok(false);
    }
    if tags.len() != 1
        || tags[0].as_slice() != ["crew-atomic-create", "1"]
        || event.kind.as_u16() != 9007
    {
        return Err(
            "invalid: atomic channel creation requires one crew-atomic-create=1 tag on kind 9007"
                .into(),
        );
    }
    if !enabled {
        return Err("unsupported: crew-atomic-channel-create is not enabled on this relay".into());
    }
    let channels: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|value| value == "h"))
        .collect();
    if channels.len() != 1
        || channels[0].as_slice().len() != 2
        || channels[0]
            .content()
            .and_then(|value| Uuid::parse_str(value).ok())
            .is_none_or(|id| id.is_nil())
    {
        return Err("invalid: atomic create requires one non-nil channel UUID h tag".into());
    }
    Ok(true)
}

fn map_db_error(error: buzz_db::DbError) -> super::ingest::IngestError {
    use super::ingest::IngestError;
    match error {
        buzz_db::DbError::AtomicChannelConflict(message) => {
            IngestError::Rejected(format!("conflict: {message}"))
        }
        buzz_db::DbError::ChannelReplayDenied => IngestError::AuthFailed(
            "restricted: create replay requires original creator and current owner".into(),
        ),
        buzz_db::DbError::ChannelNotFound(_) => {
            IngestError::Rejected("conflict: channel no longer exists".into())
        }
        error => IngestError::Internal(format!("error: atomic-create-pending: {error}")),
    }
}

async fn reconcile(
    tenant: &TenantContext,
    state: &Arc<AppState>,
    event: &Event,
    channel_id: Uuid,
    original: StoredEvent,
) -> Result<(StoredEvent, bool), super::ingest::IngestError> {
    super::channel_discovery::emit(tenant, state, channel_id, Some(event))
        .await
        .map_err(|error| match error.downcast::<buzz_db::DbError>() {
            Ok(error) => map_db_error(error),
            Err(error) => {
                super::ingest::IngestError::Internal(format!("error: side-effect-pending: {error}"))
            }
        })?;
    Ok((original, false))
}

/// Store the exact new command, or reconcile only discovery for an authorized replay.
/// The bool is false only for a fully reconciled replay.
pub async fn store_or_replay(
    tenant: &TenantContext,
    state: &Arc<AppState>,
    event: &Event,
    channel_id: Uuid,
    thread_meta: Option<buzz_db::event::ThreadMetadataParams<'_>>,
) -> Result<(StoredEvent, bool), super::ingest::IngestError> {
    if let Some(original) = state
        .db
        .get_event_by_id_for_event_write(tenant.community(), event.id.as_bytes())
        .await
        .map_err(map_db_error)?
    {
        return reconcile(tenant, state, event, channel_id, original).await;
    }
    let tag = |name: &str| {
        event.tags.iter().find_map(|tag| {
            let values = tag.as_slice();
            (values.first().is_some_and(|value| value == name))
                .then(|| values.get(1).map(String::as_str))
                .flatten()
        })
    };
    let invalid = |message| super::ingest::IngestError::Rejected(format!("invalid: {message}"));
    let name = tag("name").ok_or_else(|| invalid("missing channel name"))?;
    let channel_type = tag("channel_type")
        .unwrap_or("stream")
        .parse()
        .map_err(|_| invalid("channel type"))?;
    let visibility = tag("visibility")
        .unwrap_or("open")
        .parse()
        .map_err(|_| invalid("visibility"))?;
    match state
        .db
        .create_channel_with_event(
            tenant.community(),
            channel_id,
            name,
            channel_type,
            visibility,
            tag("about"),
            super::resolve_ttl(event, state.config.ephemeral_ttl_override),
            event,
            thread_meta,
        )
        .await
    {
        Ok((_, stored)) => Ok((stored, true)),
        Err(error @ buzz_db::DbError::AtomicChannelConflict(_)) => {
            // Another identical request can commit while this request waits on
            // the UUID insert. Re-read the exact event, never infer by UUID/name.
            if let Some(original) = state
                .db
                .get_event_by_id_for_event_write(tenant.community(), event.id.as_bytes())
                .await
                .map_err(map_db_error)?
            {
                reconcile(tenant, state, event, channel_id, original).await
            } else {
                Err(map_db_error(error))
            }
        }
        Err(error) => Err(map_db_error(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    #[test]
    fn atomic_channel_error_classes_keep_conflict_and_denial_out_of_pending() {
        use super::super::ingest::IngestError;
        assert!(
            matches!(map_db_error(buzz_db::DbError::AtomicChannelConflict("UUID collision".into())), IngestError::Rejected(message) if message.starts_with("conflict:"))
        );
        assert!(
            matches!(map_db_error(buzz_db::DbError::ChannelReplayDenied), IngestError::AuthFailed(message) if message.starts_with("restricted:"))
        );
        assert!(
            matches!(map_db_error(buzz_db::DbError::ChannelNotFound(Uuid::new_v4())), IngestError::Rejected(message) if message.starts_with("conflict:"))
        );
    }

    fn event(extra: &[&[&str]]) -> Event {
        EventBuilder::new(Kind::Custom(9007), "")
            .tags(
                extra
                    .iter()
                    .map(|values| Tag::parse(values.iter().copied()).expect("tag")),
            )
            .sign_with_keys(&Keys::generate())
            .expect("sign")
    }

    #[test]
    fn atomic_channel_create_off_refuses_optin_without_fallback() {
        let event = event(&[
            &["crew-atomic-create", "1"],
            &["h", "11111111-1111-4111-8111-111111111111"],
        ]);
        assert!(requested(&event, false)
            .expect_err("disabled")
            .starts_with("unsupported:"));
        assert!(requested(&event, true).expect("enabled"));
        let mut extensions = Some(vec!["nip-er".to_string()]);
        advertise(&mut extensions, false);
        assert_eq!(extensions, Some(vec!["nip-er".to_string()]));
        advertise(&mut extensions, true);
        assert!(extensions
            .expect("extensions")
            .iter()
            .any(|value| value == EXTENSION));
    }

    #[test]
    fn atomic_channel_create_rejects_missing_uuid_duplicate_optin_and_unknown_version() {
        for tags in [
            vec![&["crew-atomic-create", "1"][..]],
            vec![&["crew-atomic-create", "2"][..]],
            vec![
                &["crew-atomic-create", "1"][..],
                &["crew-atomic-create", "1"][..],
            ],
            vec![&["crew-atomic-create", "1"][..], &["h", "not-a-uuid"][..]],
        ] {
            assert!(requested(&event(&tags), true).is_err());
        }
        assert!(!requested(&event(&[&["name", "legacy"]]), false).expect("legacy unchanged"));
    }
}
