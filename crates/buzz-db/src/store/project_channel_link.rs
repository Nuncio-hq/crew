//! Conditional Project associations extend the existing NIP-33 transaction.
use crate::replaceable::{
    ParameterizedReplacePrecondition, ParameterizedReplaceResult, ParameterizedReplaceStatus,
};
use crate::{Db, DbError, Result};
use buzz_core::{kind::KIND_PROJECT, CommunityId};
use chrono::{DateTime, Utc};
use nostr::Event;
use sqlx::{Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

// Existing Project convention: one home plus at most 64 related channels.
const MAX_RELATED_CHANNELS: usize = 64;

fn association_values<'a>(
    tags: impl Iterator<Item = &'a [String]>,
    check_home: bool,
) -> Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    let mut homes = 0;
    for tag in tags {
        let Some(name) = tag.first() else { continue };
        if name == "buzz-channel" {
            homes += 1;
        }
        if name != "buzz-channel" && name != "buzz-related-channel" {
            continue;
        }
        let value = tag
            .get(1)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                DbError::InvalidData("Project channel association is missing its value".into())
            })?;
        result.insert(value.clone());
    }
    if check_home && homes > 1 {
        return Err(DbError::InvalidData(
            "Project must have at most one home channel".into(),
        ));
    }
    Ok(result)
}

fn newly_associated(old_tags: &[Vec<String>], event: &Event) -> Result<Vec<Uuid>> {
    let old = association_values(old_tags.iter().map(Vec::as_slice), false)?;
    let new = association_values(event.tags.iter().map(|tag| tag.as_slice()), true)?;
    let added: Vec<_> = new.difference(&old).collect();
    // Legacy oversized associations can be retained by unrelated metadata edits;
    // growing the set requires the current bounded contract, never truncation.
    let related: BTreeSet<_> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice())
        .filter(|tag| {
            tag.first()
                .is_some_and(|name| name == "buzz-related-channel")
        })
        .filter_map(|tag| tag.get(1))
        .collect();
    if !added.is_empty() && related.len() > MAX_RELATED_CHANNELS {
        return Err(DbError::InvalidData(
            "Project has too many distinct channel associations".into(),
        ));
    }
    let mut channels = Vec::with_capacity(added.len());
    for value in added {
        let id = Uuid::parse_str(value)
            .map_err(|_| DbError::InvalidData("Project channel must be a canonical UUID".into()))?;
        if id.to_string() != *value {
            return Err(DbError::InvalidData(
                "Project channel must be a canonical UUID".into(),
            ));
        }
        channels.push(id);
    }
    channels.sort_unstable();
    Ok(channels)
}

impl Db {
    /// Conditionally replace a global Project while validating newly associated
    /// home/related channels in the same caller-owned transaction.
    ///
    /// Caller must hold the community deletion fence and commit only on success.
    /// Exact-current replay bypasses mutable membership/archive eligibility.
    pub async fn replace_project_event_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        community: CommunityId,
        event: &Event,
        d_tag: &str,
        precondition: ParameterizedReplacePrecondition<'_>,
    ) -> Result<ParameterizedReplaceResult> {
        if u32::from(event.kind.as_u16()) != KIND_PROJECT {
            return Err(DbError::InvalidData(
                "Project transaction requires kind 30621".into(),
            ));
        }
        let lock = crate::replaceable::event_replacement_lock_key(
            community,
            KIND_PROJECT as i32,
            event.pubkey.as_bytes(),
            Some(d_tag.as_bytes()),
        );
        crate::observability::observe_advisory_lock(
            crate::observability::LockType::Replacement,
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(lock)
                .execute(&mut **tx),
        )
        .await?;
        // Same ordering as replaceable.rs. Row lock also fences a NIP-09 delete,
        // which need not acquire the Project coordinate advisory lock.
        let old: Option<(Vec<u8>, serde_json::Value)> = sqlx::query_as(
            "SELECT id,tags FROM events WHERE community_id=$1 AND kind=$2 AND pubkey=$3 AND d_tag=$4 AND deleted_at IS NULL ORDER BY created_at DESC,id ASC LIMIT 1 FOR SHARE"
        ).bind(community.as_uuid()).bind(KIND_PROJECT as i32).bind(event.pubkey.as_bytes().as_slice()).bind(d_tag).fetch_optional(&mut **tx).await?;
        let result = self
            .replace_parameterized_event_in_transaction(
                tx,
                community,
                event,
                d_tag,
                None,
                precondition,
            )
            .await?;
        if result.status != ParameterizedReplaceStatus::Inserted {
            return Ok(result);
        }
        let old_tags: Vec<Vec<String>> = match old {
            Some((_, tags)) => serde_json::from_value(tags)
                .map_err(|_| DbError::InvalidData("Stored Project tags are invalid".into()))?,
            None => Vec::new(),
        };
        let channels = newly_associated(&old_tags, event)?;
        for channel in &channels {
            crate::channel_members::acquire_channel_membership_lock(tx, community, *channel)
                .await?;
        }
        // Row locks also cover relay-admin kick, which bypasses the advisory lock.
        for channel in &channels {
            let member: Option<Vec<u8>> = sqlx::query_scalar(
                "SELECT pubkey FROM channel_members WHERE community_id=$1 AND channel_id=$2 AND pubkey=$3 AND removed_at IS NULL FOR SHARE"
            ).bind(community.as_uuid()).bind(channel).bind(event.pubkey.as_bytes().as_slice()).fetch_optional(&mut **tx).await?;
            if member.is_none() {
                return Err(DbError::AccessDenied(
                    "Project owner must be an active member of a newly linked channel".into(),
                ));
            }
        }
        for channel in &channels {
            let row: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
                "SELECT channel_type::text,archived_at FROM channels WHERE community_id=$1 AND id=$2 AND deleted_at IS NULL FOR SHARE"
            ).bind(community.as_uuid()).bind(channel).fetch_optional(&mut **tx).await?;
            if !row.is_some_and(|(kind, archived)| kind == "stream" && archived.is_none()) {
                return Err(DbError::AccessDenied(
                    "New Project associations require active stream channels".into(),
                ));
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "project_channel_link_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "project_channel_link_db_tests.rs"]
mod db_postgres_tests;
