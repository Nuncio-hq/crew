//! Read-only observation of the contact classification foundation.
//!
//! This module deliberately stops at storage evidence. It does not read or
//! create route rows, resolve a canvas, authenticate a contact owner, validate
//! a wire proof kind, wake an agent, or authorize automatic contact routing.
//! Those are separate #355 protocol decisions and remain disabled until an
//! approved successor exists.

use buzz_core::kind::KIND_STREAM_MESSAGE;
use buzz_core::CommunityId;
use buzz_datastore_tracing::datastore_span;
use sqlx::{PgConnection, Row};

use crate::error::{DbError, Result};
use crate::Db;

/// Read-only result for one contact-original lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactOriginalObservation {
    /// No event with this ID exists in the requested community.
    Missing,
    /// The event exists but is not a kind-9 channel original.
    NotKind9,
    /// The kind-9 event predates contact classification.
    Legacy,
    /// The classified kind-9 event is explicitly suppressed (class zero).
    Suppressed,
}

/// Observe one original event's persisted contact classification.
///
/// The lookup always uses the writer pool because a future dispatch or
/// authorization consumer cannot safely make a decision from a lagging
/// replica. SQL errors and unsupported/corrupt nonzero classifications
/// propagate to the caller. A soft-deleted row is still observed so a future
/// dispatcher cannot mistake its absence from a live-only query for permission
/// to replay it. This method is read-only and has no routing or wake-up side
/// effect.
impl Db {
    /// Observe one original event's persisted contact classification.
    #[datastore_span(name = "observe_contact_original", system = "postgresql")]
    pub async fn observe_contact_original(
        &self,
        community_id: CommunityId,
        original_id: &[u8],
    ) -> Result<ContactOriginalObservation> {
        if original_id.len() != 32 {
            return Err(DbError::InvalidData(
                "contact original ID must contain 32 bytes".into(),
            ));
        }

        let mut connection = crate::observability::acquire_writer(
            &self.pool,
            crate::observability::WriterOperation::Authorization,
        )
        .await?;
        observe_contact_original_on(&mut connection, community_id, original_id).await
    }
}

async fn observe_contact_original_on(
    connection: &mut PgConnection,
    community_id: CommunityId,
    original_id: &[u8],
) -> Result<ContactOriginalObservation> {
    // Include tombstones: replay/cancellation consumers need to distinguish a
    // historical original from an event that never existed. LIMIT 2 lets us
    // detect the partitioned table's permitted duplicate raw ID instead of
    // silently choosing one timestamp.
    let rows = sqlx::query(
        "SELECT kind, channel_id IS NOT NULL AS channel_scoped, contact_class, \
                deleted_at IS NOT NULL AS deleted \
         FROM events \
         WHERE community_id = $1 AND id = $2 \
         ORDER BY created_at DESC \
         LIMIT 2",
    )
    .bind(community_id.as_uuid())
    .bind(original_id)
    .fetch_all(&mut *connection)
    .await?;

    let Some(row) = rows.first() else {
        return Ok(ContactOriginalObservation::Missing);
    };
    if rows.len() > 1 {
        return Err(DbError::InvalidData(
            "contact original ID is ambiguous in this community".into(),
        ));
    }

    let kind: i32 = row.try_get("kind")?;
    let channel_scoped: bool = row.try_get("channel_scoped")?;
    let class: Option<i16> = row.try_get("contact_class")?;
    // Read the tombstone column as part of the production query even though
    // classification is intentionally independent of liveness. This keeps
    // the historical-observation contract explicit and prevents a future edit
    // from accidentally adding `deleted_at IS NULL` to the SQL predicate.
    let _deleted: bool = row.try_get("deleted")?;

    classify_original(kind, channel_scoped, class)
}

fn classify_original(
    kind: i32,
    channel_scoped: bool,
    class: Option<i16>,
) -> Result<ContactOriginalObservation> {
    if kind != KIND_STREAM_MESSAGE as i32 {
        return if class.is_none() {
            Ok(ContactOriginalObservation::NotKind9)
        } else {
            Err(DbError::InvalidData(
                "non-kind-9 event carries contact classification".into(),
            ))
        };
    }
    if !channel_scoped {
        return match class {
            None | Some(0) => Ok(ContactOriginalObservation::NotKind9),
            Some(value) => Err(DbError::InvalidData(format!(
                "global kind-9 event carries unsupported contact classification {value}"
            ))),
        };
    }

    match class {
        None => Ok(ContactOriginalObservation::Legacy),
        Some(0) => Ok(ContactOriginalObservation::Suppressed),
        Some(value) => Err(DbError::InvalidData(format!(
            "unsupported contact classification {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_original, ContactOriginalObservation};
    use buzz_core::kind::KIND_STREAM_MESSAGE;

    #[test]
    fn classification_decoder_accepts_only_legacy_or_suppressed() {
        assert_eq!(
            classify_original(KIND_STREAM_MESSAGE as i32, true, None).unwrap(),
            ContactOriginalObservation::Legacy
        );
        assert_eq!(
            classify_original(KIND_STREAM_MESSAGE as i32, true, Some(0)).unwrap(),
            ContactOriginalObservation::Suppressed
        );
        assert_eq!(
            classify_original(1, true, None).unwrap(),
            ContactOriginalObservation::NotKind9
        );
        assert_eq!(
            classify_original(KIND_STREAM_MESSAGE as i32, false, None).unwrap(),
            ContactOriginalObservation::NotKind9
        );
        assert_eq!(
            classify_original(KIND_STREAM_MESSAGE as i32, false, Some(0)).unwrap(),
            ContactOriginalObservation::NotKind9
        );
        for class in 1..=12 {
            assert!(
                classify_original(KIND_STREAM_MESSAGE as i32, true, Some(class)).is_err(),
                "unapproved class {class} must fail closed"
            );
        }
    }
}
