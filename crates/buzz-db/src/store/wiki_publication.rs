//! Owner-serialized admission for immutable Wiki events on NIP-33 coordinates.
use buzz_core::CommunityId;
use nostr::Event;
use sha2::{Digest, Sha256};
use sqlx::{Acquire, Postgres, Transaction};
use uuid::Uuid;

use super::replaceable::{
    ParameterizedReplacePrecondition, ParameterizedReplaceResult, ParameterizedReplaceStatus,
};
use crate::{Db, DbError, Result};

const MAX_LIVE_BYTES: i64 = 512 * 1024 * 1024;
const MAX_LIVE_EVENTS: i64 = 4096;

fn reserved_coordinate(d: &str) -> bool {
    let Some((repo, slug)) = d.split_once('/') else {
        return false;
    };
    !repo.is_empty()
        && slug.len() == 67
        && (slug.starts_with("p1-") || slug.starts_with("m1-"))
        && slug[3..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

struct ImmutableWrite<'a> {
    event: &'a Event,
    d: &'a str,
    channel: Option<Uuid>,
    replay_only: bool,
}

async fn replace_with_limits(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    community: CommunityId,
    write: ImmutableWrite<'_>,
    limits: (i64, i64),
) -> Result<ParameterizedReplaceResult> {
    let ImmutableWrite {
        event,
        d,
        channel,
        replay_only,
    } = write;
    if event.kind.as_u16() != 30623
        || !reserved_coordinate(d)
        || crate::event::extract_d_tag(event).as_deref() != Some(d)
    {
        return Err(DbError::InvalidData(
            "invalid immutable Wiki coordinate".into(),
        ));
    }
    let owner = event.pubkey.to_bytes();
    let mut hash = Sha256::new();
    hash.update(b"crew-wiki-publication-owner-v1");
    hash.update(community.as_uuid().as_bytes());
    hash.update(owner);
    let digest = hash.finalize();
    let first = i32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    let second = i32::from_be_bytes([digest[4], digest[5], digest[6], digest[7]]);
    // PostgreSQL's two-int key space does not overlap the NIP-33 single-int64
    // coordinate key space. Always acquire owner before coordinate.
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(first)
        .bind(second)
        .execute(&mut **tx)
        .await?;
    let mut admission = tx.begin().await?;
    let precondition = if replay_only {
        ParameterizedReplacePrecondition::ExactReplayOnly
    } else {
        ParameterizedReplacePrecondition::ExpectedMissing
    };
    let result = db
        .replace_parameterized_event_in_transaction(
            &mut admission,
            community,
            event,
            d,
            channel,
            precondition,
        )
        .await?;
    if result.status == ParameterizedReplaceStatus::Inserted {
        // Count a deterministic serialization of all seven signed event fields.
        // PostgreSQL JSON spacing makes this conservative vs compact wire JSON;
        // content escaping is included, unlike raw content/column-size estimates.
        let (count, bytes): (i64, i64) = sqlx::query_as(
            "SELECT count(*), coalesce(sum(octet_length(jsonb_build_object(\
             'id',encode(id,'hex'),'pubkey',encode(pubkey,'hex'),\
             'created_at',extract(epoch from created_at)::bigint,'kind',kind,\
             'tags',tags,'content',content,'sig',encode(sig,'hex'))::text)),0)::bigint \
             FROM events WHERE community_id=$1 AND pubkey=$2 AND kind=30623 \
             AND deleted_at IS NULL AND d_tag ~ '/[pm]1-[0-9a-f]{64}$'",
        )
        .bind(community.as_uuid())
        .bind(owner.as_slice())
        .fetch_one(&mut *admission)
        .await?;
        if count > limits.0 || bytes > limits.1 {
            // Even a caller that commits its outer transaction cannot retain the
            // rejected row. No trim/delete is allowed to manufacture headroom.
            admission.rollback().await?;
            return Err(DbError::WikiStorageQuota);
        }
    }
    admission.commit().await?;
    Ok(result)
}

impl Db {
    /// Insert or exactly replay an immutable Wiki event under owner and coordinate
    /// locks, with a live 4096-event/512-MiB quota. Quota rejection rolls back the
    /// entire attempted admission even if the caller commits its outer transaction.
    /// The caller retains the community deletion fence and commit responsibility.
    pub async fn replace_wiki_immutable_event_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        community: CommunityId,
        event: &Event,
        d: &str,
        channel: Option<Uuid>,
        replay_only: bool,
    ) -> Result<ParameterizedReplaceResult> {
        replace_with_limits(
            self,
            tx,
            community,
            ImmutableWrite {
                event,
                d,
                channel,
                replay_only,
            },
            (MAX_LIVE_EVENTS, MAX_LIVE_BYTES),
        )
        .await
    }
}

#[cfg(test)]
#[path = "wiki_publication_tests.rs"]
mod tests;
