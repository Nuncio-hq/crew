//! Canonical discovery snapshots serialized with channel lifecycle changes.

use buzz_core::{CommunityId, StoredEvent};
use chrono::{DateTime, Utc};
use nostr::Event;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::channel::{ChannelRecord, MemberRecord};
use crate::{Db, DbError, Result};

/// Channel and roster captured under one writer transaction until publication.
pub struct LockedDiscoverySnapshot {
    /// Current metadata; never reconstructed from an old create command.
    pub channel: ChannelRecord,
    /// Current active members, including their authoritative roles.
    pub members: Vec<MemberRecord>,
    community: CommunityId,
    relay_pubkey: Vec<u8>,
    tx: Transaction<'static, Postgres>,
}

impl LockedDiscoverySnapshot {
    /// Read a canonical head's timestamp without checking out another connection.
    pub async fn latest_timestamp(&mut self, kind: i32) -> Result<Option<u64>> {
        self.validate_kind(kind)?;
        let timestamp: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT created_at FROM events WHERE community_id=$1 AND channel_id=$2 AND pubkey=$3 AND kind=$4 AND deleted_at IS NULL ORDER BY created_at DESC,id ASC LIMIT 1",
        ).bind(self.community.as_uuid()).bind(self.channel.id)
            .bind(&self.relay_pubkey).bind(kind).fetch_optional(&mut *self.tx).await?;
        Ok(timestamp.map(|value| value.timestamp() as u64))
    }

    fn validate_kind(&self, kind: i32) -> Result<()> {
        if !matches!(kind, 39000..=39002) {
            return Err(DbError::InvalidData(
                "discovery snapshot requires kind 39000, 39001 or 39002".into(),
            ));
        }
        Ok(())
    }

    /// Store one head while retaining the lifecycle and membership fences.
    pub async fn store(&mut self, event: &Event) -> Result<(StoredEvent, bool)> {
        self.validate_kind(buzz_core::kind::event_kind_i32(event))?;
        if event.pubkey.to_bytes().as_slice() != self.relay_pubkey
            || crate::event::extract_d_tag(event).as_deref()
                != Some(self.channel.id.to_string().as_str())
        {
            return Err(DbError::InvalidData(
                "discovery event does not match its captured coordinate".into(),
            ));
        }
        crate::replaceable::replace_addressable_event_in_transaction(
            &mut self.tx,
            self.community,
            event,
            Some(self.channel.id),
        )
        .await
    }

    /// Commit all discovery heads before any network fan-out.
    pub async fn commit(self) -> Result<()> {
        self.tx.commit().await?;
        Ok(())
    }
}

impl Db {
    /// Capture canonical discovery and optionally authorize an exact create replay.
    /// All advisory locks precede the channel tuple lock to avoid TTL lock upgrades
    /// deadlocking with another discovery transaction.
    pub async fn lock_discovery_snapshot(
        &self,
        community: CommunityId,
        channel_id: Uuid,
        relay_pubkey: &[u8],
        replay: Option<&Event>,
    ) -> Result<LockedDiscoverySnapshot> {
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
        sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended($1,0))")
            .bind(format!(
                "buzz_channel_ttl:{}:{channel_id}",
                community.as_uuid()
            ))
            .execute(&mut *tx)
            .await?;
        for kind in [39000, 39001, 39002] {
            let key = crate::replaceable::event_replacement_lock_key(
                community,
                kind,
                relay_pubkey,
                Some(channel_id.as_bytes()),
            );
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }
        crate::channel_members::acquire_channel_membership_lock(&mut tx, community, channel_id)
            .await?;
        let row = sqlx::query(
            "SELECT id,name,channel_type::text AS channel_type,visibility::text AS visibility,description,canvas,created_by,created_at,updated_at,archived_at,deleted_at,nip29_group_id,topic_required,max_members,topic,topic_set_by,topic_set_at,purpose,purpose_set_by,purpose_set_at,ttl_seconds,ttl_deadline FROM channels WHERE community_id=$1 AND id=$2 AND deleted_at IS NULL FOR SHARE",
        ).bind(community.as_uuid()).bind(channel_id).fetch_optional(&mut *tx).await?
            .ok_or(DbError::ChannelNotFound(channel_id))?;
        let channel = crate::channel::row_to_channel_record(row)?;
        let members = sqlx::query(
            "SELECT channel_id,pubkey,role::text AS role,joined_at,invited_by,removed_at FROM channel_members WHERE community_id=$1 AND channel_id=$2 AND removed_at IS NULL ORDER BY joined_at,pubkey",
        ).bind(community.as_uuid()).bind(channel_id).fetch_all(&mut *tx).await?
            .into_iter().map(crate::channel_members::row_to_member_record).collect::<Result<Vec<_>>>()?;
        if let Some(event) = replay {
            let actor = event.pubkey.to_bytes();
            let matches_channel = event
                .tags
                .iter()
                .any(|tag| tag.as_slice() == ["h", channel_id.to_string().as_str()]);
            if event.kind.as_u16() != 9007
                || !matches_channel
                || channel.created_by != actor
                || !members
                    .iter()
                    .any(|member| member.pubkey == actor && member.role == "owner")
            {
                return Err(DbError::ChannelReplayDenied);
            }
            let original: Option<Vec<u8>> = sqlx::query_scalar(
                "SELECT id FROM events WHERE community_id=$1 AND id=$2 AND channel_id=$3 AND kind=9007 AND pubkey=$4 AND deleted_at IS NULL FOR SHARE",
            ).bind(community.as_uuid()).bind(event.id.as_bytes().as_slice())
                .bind(channel_id).bind(actor.as_slice()).fetch_optional(&mut *tx).await?;
            if original.is_none() {
                return Err(DbError::AtomicChannelConflict(
                    "create replay has no exact live original event".into(),
                ));
            }
        }
        Ok(LockedDiscoverySnapshot {
            channel,
            members,
            community,
            relay_pubkey: relay_pubkey.to_vec(),
            tx,
        })
    }
}
