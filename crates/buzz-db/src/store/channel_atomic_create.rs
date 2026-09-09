//! Atomic original create-command and channel-owner persistence.

use buzz_core::{CommunityId, StoredEvent};
use nostr::Event;
use uuid::Uuid;

use crate::channel::{ChannelRecord, ChannelType, ChannelVisibility};
use crate::{Db, DbError, Result};

impl Db {
    /// Commit a client UUID, its initial owner, and exact signed create event together.
    /// Existing UUIDs are never adopted, repaired by name, deleted, or replaced.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_channel_with_event(
        &self,
        community: CommunityId,
        channel_id: Uuid,
        name: &str,
        channel_type: ChannelType,
        visibility: ChannelVisibility,
        description: Option<&str>,
        ttl_seconds: Option<i32>,
        event: &Event,
        thread_meta: Option<crate::event::ThreadMetadataParams<'_>>,
    ) -> Result<(ChannelRecord, StoredEvent)> {
        let channel_tag = channel_id.to_string();
        if event.kind.as_u16() != 9007
            || !event
                .tags
                .iter()
                .any(|tag| tag.as_slice() == ["h", channel_tag.as_str()])
        {
            return Err(DbError::InvalidData(
                "atomic channel creation requires an exact 9007 UUID".into(),
            ));
        }
        let connection = crate::observability::acquire_writer(
            &self.pool,
            crate::observability::WriterOperation::EventWrite,
        )
        .await?;
        let mut tx = sqlx::Transaction::begin(connection, None).await?;
        sqlx::query("SELECT assert_community_write_allowed($1)")
            .bind(community.as_uuid())
            .execute(&mut *tx)
            .await?;
        let (channel, created) = crate::channel::create_channel_with_id_in_transaction(
            &mut tx,
            community,
            channel_id,
            name,
            channel_type,
            visibility,
            description,
            &event.pubkey.to_bytes(),
            ttl_seconds,
        )
        .await?;
        if !created {
            return Err(DbError::AtomicChannelConflict(
                "channel UUID exists without a verified exact replay".into(),
            ));
        }
        let (stored, inserted) = crate::event::insert_event_with_thread_metadata_tx(
            &mut tx,
            community,
            event,
            Some(channel_id),
            thread_meta,
        )
        .await?;
        if !inserted {
            return Err(DbError::AtomicChannelConflict(
                "original create event already exists".into(),
            ));
        }
        tx.commit().await?;
        Ok((channel, stored))
    }
}
