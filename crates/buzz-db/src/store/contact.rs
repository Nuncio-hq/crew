//! Crew contact-fallback routing (issue #412).
//!
//! A mention-less kind:9 message that opts in with a
//! `["crew-contact-fallback", "1"]` tag is routed to the channel's configured
//! contact: the registered agent named by the winning ` ```crew ` canvas
//! block's `contact:` field, provided the canvas was authored by that
//! contact's registered owner and the message's raw signer is that same owner.
//!
//! The whole decision — original event, routing class, relay-signed kind:46044
//! proof, `contact_routes` row, `contact_claims` row, and quota spend —
//! commits in ONE transaction. Deferred constraint triggers installed by
//! migration 0047 re-prove the invariant at commit time on every events leaf,
//! so the guards — not this code — are the atomicity contract.
//!
//! Exactly-once is keyed on `(community_id, original_event_id)`: a duplicate
//! ingest replays the stored `contact_class` without re-deciding, re-spending
//! quota, or emitting a second proof — including replays arriving after the
//! canvas changed.

use chrono::{DateTime, Utc};
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Timestamp};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

use buzz_core::kind::{event_kind_i32, KIND_CONTACT_DECISION};
use buzz_core::{CommunityId, StoredEvent};
use buzz_datastore_tracing::datastore_span;

use crate::error::{DbError, Result};
use crate::Db;

use super::event::{insert_event_with_thread_metadata_classified_tx, ThreadMetadataParams};

/// Capability tag a kind:9 author sets to opt in to contact-fallback routing.
/// Without it the message is suppressed-classified and never routed.
pub const CONTACT_CAPABILITY_TAG: &str = "crew-contact-fallback";

/// Transaction-local flag the decision path raises before writing a non-zero
/// `contact_class`; `contact_classify_original_v1` rejects non-zero classes
/// without it.
const DECISION_GUC: &str = "SET LOCAL buzz.contact_decision_v1 = 'on'";

/// INSERT variant carrying the decided `contact_class` as `$13`. Only the
/// reviewed decision path may pass a non-`None` class — the GUC flag above
/// plus the classify trigger enforce it at the row level.
pub(crate) const CLASSIFIED_INSERT: &str = r#"
INSERT INTO events (community_id, id, pubkey, created_at, kind, tags, content,
    sig, received_at, channel_id, d_tag, not_before, contact_class)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
ON CONFLICT DO NOTHING
"#;

/// Stored contact-routing class on a kind:9 original (`events.contact_class`).
///
/// `0` (suppressed) is the default for originals that never produced a
/// reviewed decision. `1` (routed) is the only class that carries a route
/// row. The rest are explicit no-route outcomes with distinct reasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i16)]
pub enum ContactClass {
    /// Decision path not taken (legacy rows and trigger-defaulted inserts).
    Suppressed = 0,
    /// Routed to the configured contact's owner; proof + route + claim exist.
    Routed = 1,
    /// The message carried an explicit `p` tag — routed nowhere.
    ExplicitMention = 2,
    /// The signer is a registered agent, or is not the contact's owner.
    SignerIneligible = 3,
    /// No `crew-contact-fallback` capability tag on the original.
    Disabled = 4,
    /// No usable contact: no canvas, no `contact:` field, foreign canvas
    /// author, unregistered contact, or contact not permitted in channel.
    NoContact = 5,
    /// Community quota stripe is exhausted; the original stored anyway.
    Quota = 10,
    /// Reserved: R4 fixture replay-floor counterexample class.
    #[cfg(test)]
    ReplayFloor = 12,
}

impl ContactClass {
    /// Decode a stored `contact_class` column value (`NULL` maps to
    /// `Suppressed` — pre-migration-46 rows were never classified).
    pub fn from_i16(value: Option<i16>) -> Self {
        match value.unwrap_or(0) {
            1 => Self::Routed,
            2 => Self::ExplicitMention,
            3 => Self::SignerIneligible,
            4 => Self::Disabled,
            5 => Self::NoContact,
            10 => Self::Quota,
            #[cfg(test)]
            12 => Self::ReplayFloor,
            _ => Self::Suppressed,
        }
    }
}

/// Result of the decision transaction for one kind:9 ingest.
#[derive(Debug)]
pub struct ContactRouteOutcome {
    /// The stored original (whether or not this call inserted it).
    pub stored: StoredEvent,
    /// Whether this call inserted the original (`false` = replay of a stored
    /// decision).
    pub was_inserted: bool,
    /// The committed (or replayed) routing class.
    pub class: ContactClass,
    /// The relay-signed kind:46044 decision proof when `class == Routed`
    /// and this call committed it. `None` on replays — the committed proof
    /// is re-fetched by id when needed.
    pub proof: Option<StoredEvent>,
    /// The configured contact pubkey that won the route, when routed.
    pub contact_pubkey: Option<Vec<u8>>,
}

/// What the claim control handler did with a `claim` verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactClaimResult {
    /// Granted — the claim row moved to `claimed` at this generation.
    Granted {
        /// Fencing generation the holder must echo on start/cancel/receipt.
        generation: i64,
    },
    /// The decision is already held by someone else, terminal, or the caller
    /// is not the configured contact.
    Denied(&'static str),
    /// No route row exists for that decision id.
    Missing,
}

/// What the control handler did with a `start`/`cancel` verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactControlResult {
    /// Transition committed.
    Applied,
    /// Wrong generation/holder/state — the fenced request lost.
    Denied(&'static str),
    /// No claim row for that decision id.
    Missing,
}

/// Parsed `["claim", "<decision_hex>:<generation>"]` reference carried by a
/// kind:46043 receipt that completes a contact claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactClaimRef {
    /// The kind:46044 decision event id (32 bytes).
    pub decision_id: [u8; 32],
    /// The fenced generation the completing holder owns.
    pub generation: i64,
}

/// Whether the event carries any `p` tag (explicit mention).
fn has_p_tag(event: &Event) -> bool {
    event
        .tags
        .iter()
        .any(|t| t.as_slice().first().map(String::as_str) == Some("p"))
}

/// Whether the event opted in via `["crew-contact-fallback", "1"]`.
fn has_capability_tag(event: &Event) -> bool {
    event.tags.iter().any(|t| {
        let s = t.as_slice();
        s.first().map(String::as_str) == Some(CONTACT_CAPABILITY_TAG)
            && s.get(1).map(String::as_str) == Some("1")
    })
}

/// Parse a `["claim", "<decision_hex>:<generation>"]` tag on a receipt.
///
/// Returns `Ok(None)` when no claim tag is present (ordinary receipt), and
/// `Err` when a claim tag exists but is malformed — malformed tags are
/// rejected rather than ignored so a missing generation never silently
/// completes the wrong fence.
pub fn parse_claim_tag(event: &Event) -> Result<Option<ContactClaimRef>> {
    let mut found = None;
    for tag in event.tags.iter() {
        let s = tag.as_slice();
        if s.first().map(String::as_str) != Some("claim") {
            continue;
        }
        if found.is_some() {
            return Err(DbError::InvalidData(
                "receipt carries more than one claim tag".into(),
            ));
        }
        let Some(value) = s.get(1) else {
            return Err(DbError::InvalidData("claim tag lacks a value".into()));
        };
        let (decision_hex, generation_text) = value.split_once(':').ok_or_else(|| {
            DbError::InvalidData("claim tag must be <decision>:<generation>".into())
        })?;
        let decision_bytes = hex::decode(decision_hex)
            .map_err(|e| DbError::InvalidData(format!("claim decision id is not hex: {e}")))?;
        let decision_id: [u8; 32] = decision_bytes
            .try_into()
            .map_err(|_| DbError::InvalidData("claim decision id must be 32 bytes".into()))?;
        let generation: i64 = generation_text
            .parse()
            .map_err(|_| DbError::InvalidData("claim generation is not an integer".into()))?;
        found = Some(ContactClaimRef {
            decision_id,
            generation,
        });
    }
    Ok(found)
}

/// The winning canvas pinned at decision time.
struct CanvasCandidate {
    id: Vec<u8>,
    contact_pubkey: Vec<u8>,
}

/// Everything the routed branch needs to write the proof/route/claim.
struct ContactCandidate {
    canvas: CanvasCandidate,
    /// Registered owner of the configured contact (`users.agent_owner_pubkey`).
    owner_pubkey: Vec<u8>,
}

/// Read and evaluate the winning canvas under the advisory lock.
///
/// Returns `Ok(None)` for every `NoContact` reason: no canvas, a crew block
/// that is absent/malformed/without `contact:`, a contact that is not a
/// registered agent, a canvas not authored by the contact's registered
/// owner, or a contact not permitted in the channel.
async fn eval_contact_candidate(
    tx: &mut PgConnection,
    community_id: CommunityId,
    channel_id: Uuid,
) -> Result<Option<ContactCandidate>> {
    let canvas_row = sqlx::query(
        "SELECT id, pubkey, content FROM events \
         WHERE community_id = $1 AND channel_id = $2 AND kind = 40100 \
           AND deleted_at IS NULL \
         ORDER BY created_at DESC, id ASC LIMIT 1",
    )
    .bind(community_id.as_uuid())
    .bind(channel_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(canvas_row) = canvas_row else {
        return Ok(None);
    };
    let content: String = canvas_row.try_get("content")?;
    let contact_hex = match buzz_core::crew_role::parse_canvas_assignments(&content) {
        Ok(Some(block)) => block.contact_pubkey,
        Ok(None) => None,
        Err(e) => {
            // A malformed crew block is a stable configuration fault — record
            // an explicit no-route rather than a retryable failure.
            tracing::warn!(channel = %channel_id, "contact canvas crew block invalid: {e}");
            None
        }
    };
    let Some(contact_hex) = contact_hex else {
        return Ok(None);
    };
    let contact_bytes = hex::decode(&contact_hex)
        .map_err(|e| DbError::InvalidData(format!("contact pubkey is not hex: {e}")))?;
    if contact_bytes.len() != 32 {
        return Ok(None);
    }

    let owner_row = sqlx::query(
        "SELECT agent_owner_pubkey FROM users \
         WHERE community_id = $1 AND pubkey = $2",
    )
    .bind(community_id.as_uuid())
    .bind(&contact_bytes)
    .fetch_optional(&mut *tx)
    .await?;
    let owner_pubkey = match owner_row {
        Some(row) => row.try_get::<Option<Vec<u8>>, _>("agent_owner_pubkey")?,
        None => None,
    };
    let Some(owner_pubkey) = owner_pubkey else {
        // Contact is not a registered agent (no row or no owner link).
        return Ok(None);
    };

    let canvas_pubkey: Vec<u8> = canvas_row.try_get("pubkey")?;
    if canvas_pubkey != owner_pubkey {
        // Foreign canvas — only the contact's owner may configure routing.
        return Ok(None);
    }

    // The contact must be permitted in the channel: member, or the channel is
    // open-visibility (non-member writers are permitted there anyway).
    let is_member = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM channel_members \
         WHERE community_id = $1 AND channel_id = $2 AND pubkey = $3 \
           AND removed_at IS NULL)",
    )
    .bind(community_id.as_uuid())
    .bind(channel_id)
    .bind(&contact_bytes)
    .fetch_one(&mut *tx)
    .await?;
    if !is_member {
        let open = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM channels \
             WHERE community_id = $1 AND id = $2 AND visibility = 'open')",
        )
        .bind(community_id.as_uuid())
        .bind(channel_id)
        .fetch_one(&mut *tx)
        .await?;
        if !open {
            return Ok(None);
        }
    }

    Ok(Some(ContactCandidate {
        canvas: CanvasCandidate {
            id: canvas_row.try_get("id")?,
            contact_pubkey: contact_bytes,
        },
        owner_pubkey,
    }))
}

/// Reserve one quota unit for `(community, stripe)` inside the decision tx.
///
/// Returns `false` when the stripe exists and is at the 8192 ceiling.
/// Cross-channel stripe contention serializes on the row/unique key; the
/// channel advisory lock already serializes same-channel writes.
async fn reserve_contact_quota(
    tx: &mut PgConnection,
    community_id: CommunityId,
    stripe: i16,
) -> Result<bool> {
    for _ in 0..3 {
        let bumped = sqlx::query(
            "UPDATE contact_quota SET used = used + 1 \
             WHERE community_id = $1 AND stripe = $2 AND used < 8192 \
             RETURNING used",
        )
        .bind(community_id.as_uuid())
        .bind(stripe)
        .fetch_optional(&mut *tx)
        .await?;
        if bumped.is_some() {
            return Ok(true);
        }
        let created = sqlx::query(
            "INSERT INTO contact_quota (community_id, stripe, used) \
             VALUES ($1, $2, 1) ON CONFLICT DO NOTHING RETURNING used",
        )
        .bind(community_id.as_uuid())
        .bind(stripe)
        .fetch_optional(&mut *tx)
        .await?;
        match created {
            Some(_) => return Ok(true),
            // Row appeared between the UPDATE and the INSERT — retry UPDATE.
            None => continue,
        }
    }
    let used: Option<i32> = sqlx::query_scalar(
        "SELECT used FROM contact_quota WHERE community_id = $1 AND stripe = $2",
    )
    .bind(community_id.as_uuid())
    .bind(stripe)
    .fetch_optional(&mut *tx)
    .await?;
    match used {
        Some(u) if u >= 8192 => Ok(false),
        _ => Err(DbError::InvalidData(
            "contact quota reservation raced; retry the decision".into(),
        )),
    }
}

/// Sign the relay-authored kind:46044 decision proof for a routed original.
fn build_decision_proof(
    relay_keys: &Keys,
    community_id: CommunityId,
    channel_id: Uuid,
    original: &Event,
    candidate: &ContactCandidate,
    root_event_id: Option<Vec<u8>>,
) -> Result<Event> {
    let original_hex = hex::encode(original.id.as_bytes());
    let contact_hex = hex::encode(&candidate.canvas.contact_pubkey);
    let canvas_hex = hex::encode(&candidate.canvas.id);
    let mut tag_strings: Vec<Vec<String>> = vec![
        vec!["h".into(), channel_id.to_string()],
        vec!["p".into(), contact_hex.clone()],
        vec!["original".into(), original_hex.clone()],
        vec!["canvas".into(), canvas_hex.clone()],
        vec!["phase".into(), "decision".into()],
        vec![
            "e".into(),
            original_hex.clone(),
            String::new(),
            "reply".into(),
        ],
    ];
    if let Some(root_id) = root_event_id {
        if root_id.as_slice() != original.id.as_bytes() {
            tag_strings.push(vec![
                "e".into(),
                hex::encode(root_id),
                String::new(),
                "root".into(),
            ]);
        }
    }
    let mut parsed = Vec::with_capacity(tag_strings.len());
    for tag in tag_strings {
        parsed.push(
            Tag::parse(tag.iter().map(String::as_str).collect::<Vec<_>>())
                .map_err(|e| DbError::InvalidData(format!("decision proof tag invalid: {e}")))?,
        );
    }
    let content = serde_json::json!({
        "v": 1,
        "outcome": "routed",
        "community": community_id.as_uuid().to_string(),
        "channel": channel_id.to_string(),
        "original": original_hex,
        "canvas": canvas_hex,
        "contact": contact_hex,
    });
    let created_at = Timestamp::from(original.created_at.as_secs());
    EventBuilder::new(
        Kind::Custom(KIND_CONTACT_DECISION as u16),
        content.to_string(),
    )
    .tags(parsed)
    .custom_created_at(created_at)
    .sign_with_keys(relay_keys)
    .map_err(|e| DbError::InvalidData(format!("decision proof signing failed: {e}")))
}

/// Decide and record contact routing for one kind:9 channel original.
///
/// One transaction: existence replay check, channel-scoped advisory lock,
/// candidate eval against the winning canvas, catalog verification, quota
/// reservation, original insert, then — only for `routed` — proof insert,
/// route row, and pending claim row.
///
/// `thread_meta` is the relay's already-resolved NIP-10 ancestry for the
/// original; the proof attaches under the original as a `reply` marker.
pub async fn decide_contact_route(
    pool: &PgPool,
    community_id: CommunityId,
    event: &Event,
    channel_id: Uuid,
    thread_meta: Option<ThreadMetadataParams<'_>>,
    relay_keys: &Keys,
) -> Result<ContactRouteOutcome> {
    debug_assert_eq!(event_kind_i32(event), 9);
    let id_bytes = *event.id.as_bytes();
    let original_created_at = DateTime::from_timestamp(event.created_at.as_secs() as i64, 0)
        .ok_or(DbError::InvalidTimestamp(event.created_at.as_secs() as i64))?;

    // Prescan classes need no reads: explicit mention suppresses routing, no
    // capability tag means the author never opted in.
    let prescanned = if has_p_tag(event) {
        Some(ContactClass::ExplicitMention)
    } else if !has_capability_tag(event) {
        Some(ContactClass::Disabled)
    } else {
        None
    };

    // Capture the original's ancestry before `thread_meta` is moved into the
    // insert call — the proof inherits it.
    let root_event_id: Option<Vec<u8>> = thread_meta
        .as_ref()
        .and_then(|m| m.root_event_id.map(|r| r.to_vec()))
        .or_else(|| Some(id_bytes.to_vec()));
    let root_event_created_at = thread_meta
        .as_ref()
        .and_then(|m| m.root_event_created_at)
        .unwrap_or(original_created_at);
    let depth = thread_meta.as_ref().map_or(0, |m| m.depth);

    let connection = crate::observability::acquire_writer(
        pool,
        crate::observability::WriterOperation::EventWrite,
    )
    .await?;
    let mut tx = sqlx::Transaction::begin(connection, None).await?;
    sqlx::query(DECISION_GUC).execute(&mut *tx).await?;

    // Exactly-once: a committed decision for this original replays verbatim —
    // no re-decision, no quota spend, no second proof — even after the canvas
    // moved on.
    let stored_class: Option<i16> = sqlx::query_scalar::<_, Option<i16>>(
        "SELECT contact_class FROM events \
         WHERE community_id = $1 AND id = $2 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(community_id.as_uuid())
    .bind(id_bytes.as_slice())
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if stored_class.is_some() {
        let class = ContactClass::from_i16(stored_class);
        tx.rollback().await?;
        return Ok(ContactRouteOutcome {
            stored: StoredEvent::with_received_at(
                event.clone(),
                Utc::now(),
                Some(channel_id),
                true,
            ),
            was_inserted: false,
            class,
            proof: None,
            contact_pubkey: None,
        });
    }

    let mut candidate = None;
    let mut class = prescanned.unwrap_or(ContactClass::Suppressed);
    if prescanned.is_none() {
        let signer_bytes = event.pubkey.to_bytes();
        let signer_is_agent = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM users WHERE community_id = $1 \
             AND pubkey = $2 AND agent_owner_pubkey IS NOT NULL)",
        )
        .bind(community_id.as_uuid())
        .bind(signer_bytes.as_slice())
        .fetch_one(&mut *tx)
        .await?;
        if signer_is_agent {
            class = ContactClass::SignerIneligible;
        } else {
            // Serialize per (community, channel) so concurrent originals
            // evaluate the same "latest canvas" boundary.
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(format!(
                    "crew_contact:{}:{}",
                    community_id.as_uuid(),
                    channel_id
                ))
                .execute(&mut *tx)
                .await?;

            match eval_contact_candidate(&mut *tx, community_id, channel_id).await? {
                None => class = ContactClass::NoContact,
                Some(found) => {
                    if signer_bytes.as_slice() != found.owner_pubkey.as_slice() {
                        class = ContactClass::SignerIneligible;
                    } else {
                        candidate = Some(found);
                    }
                }
            }
        }
    }

    if candidate.is_some() {
        // Fail closed on catalog drift before spending quota or writing rows.
        sqlx::query("SELECT contact_verify_catalog_v1()")
            .execute(&mut *tx)
            .await?;
        let stripe = i16::from(id_bytes[0] & 0x0f);
        if !reserve_contact_quota(&mut *tx, community_id, stripe).await? {
            candidate = None;
            class = ContactClass::Quota;
        }
    }

    let final_class = if candidate.is_some() {
        ContactClass::Routed
    } else {
        class
    };

    // Insert the original with its decided class. A concurrent ingest that
    // committed between the existence check and now surfaces as
    // `was_inserted=false`; the whole decision tx rolls back and replays the
    // stored class — the winner's proof/route/claim stand untouched.
    let (stored, was_inserted) = insert_event_with_thread_metadata_classified_tx(
        &mut tx,
        community_id,
        event,
        Some(channel_id),
        thread_meta,
        Some(final_class),
    )
    .await?;
    if !was_inserted {
        tx.rollback().await?;
        let stored_class: Option<i16> = sqlx::query_scalar::<_, Option<i16>>(
            "SELECT contact_class FROM events \
             WHERE community_id = $1 AND id = $2 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(community_id.as_uuid())
        .bind(id_bytes.as_slice())
        .fetch_optional(pool)
        .await?
        .flatten();
        return Ok(ContactRouteOutcome {
            stored,
            was_inserted: false,
            class: ContactClass::from_i16(stored_class),
            proof: None,
            contact_pubkey: None,
        });
    }

    let mut proof = None;
    if let Some(candidate) = &candidate {
        let proof_event = build_decision_proof(
            relay_keys,
            community_id,
            channel_id,
            event,
            &candidate,
            root_event_id.clone(),
        )?;
        let proof_created_at = DateTime::from_timestamp(proof_event.created_at.as_secs() as i64, 0)
            .ok_or(DbError::InvalidTimestamp(
                proof_event.created_at.as_secs() as i64
            ))?;
        let proof_meta = ThreadMetadataParams {
            event_id: proof_event.id.as_bytes(),
            event_created_at: proof_created_at,
            channel_id,
            parent_event_id: Some(id_bytes.as_slice()),
            parent_event_created_at: Some(original_created_at),
            root_event_id: root_event_id.as_deref(),
            root_event_created_at: Some(root_event_created_at),
            depth: depth + 1,
            broadcast: false,
        };
        let (proof_stored, proof_inserted) = insert_event_with_thread_metadata_classified_tx(
            &mut tx,
            community_id,
            &proof_event,
            Some(channel_id),
            Some(proof_meta),
            None,
        )
        .await?;
        if !proof_inserted {
            // A proof id is content-addressed; an existing row means a prior
            // decision wrote this exact proof — which the deferred guards
            // would have rejected as proof-without-route already.
            return Err(DbError::InvalidData(
                "decision proof id already exists without a route".into(),
            ));
        }
        // The `#p` historical-query path joins `event_mentions`, so the proof's
        // recipient index must commit inside this same decision transaction.
        crate::insert_mentions_in_transaction(
            &mut tx,
            community_id,
            &proof_event,
            Some(channel_id),
        )
        .await?;

        let stripe = i16::from(id_bytes[0] & 0x0f);
        sqlx::query(
            "INSERT INTO contact_routes \
             (community_id, original_id, original_created_at, channel_id, \
              contact_pubkey, relay_pubkey, decision_id, decision_created_at, \
              stripe, canvas_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(community_id.as_uuid())
        .bind(id_bytes.as_slice())
        .bind(original_created_at)
        .bind(channel_id)
        .bind(&candidate.canvas.contact_pubkey)
        .bind(relay_keys.public_key().to_bytes().as_slice())
        .bind(proof_event.id.as_bytes().as_slice())
        .bind(proof_created_at)
        .bind(stripe)
        .bind(&candidate.canvas.id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO contact_claims \
             (community_id, decision_id, original_id, channel_id, contact_pubkey, \
              state, generation) \
             VALUES ($1, $2, $3, $4, $5, 'pending', 0)",
        )
        .bind(community_id.as_uuid())
        .bind(proof_event.id.as_bytes().as_slice())
        .bind(id_bytes.as_slice())
        .bind(channel_id)
        .bind(&candidate.canvas.contact_pubkey)
        .execute(&mut *tx)
        .await?;

        proof = Some(proof_stored);
    }

    tx.commit().await?;

    Ok(ContactRouteOutcome {
        stored,
        was_inserted: true,
        class: final_class,
        proof,
        contact_pubkey: candidate.as_ref().map(|c| c.canvas.contact_pubkey.clone()),
    })
}

/// Fenced `claim` transition for a kind:46044 decision.
///
/// `holder` must equal the route's `contact_pubkey` — only the configured
/// contact may claim. Pending rows claim directly; expired `claimed`/`started`
/// leases are fenced to `unknown` first and then reclaimable. Terminal rows
/// (`completed`, `cancelled`) never reopen.
pub async fn claim_contact_decision(
    pool: &PgPool,
    community_id: CommunityId,
    decision_id: &[u8],
    channel_id: Uuid,
    holder: &[u8],
    ttl_secs: i64,
) -> Result<ContactClaimResult> {
    let ttl = ttl_secs.clamp(5, 3600);
    let connection = crate::observability::acquire_writer(
        pool,
        crate::observability::WriterOperation::EventWrite,
    )
    .await?;
    let mut tx = sqlx::Transaction::begin(connection, None).await?;

    // Only the route's configured contact may hold its claim.
    let contact: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT contact_pubkey FROM contact_routes \
         WHERE community_id = $1 AND decision_id = $2",
    )
    .bind(community_id.as_uuid())
    .bind(decision_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(contact) = contact else {
        tx.rollback().await?;
        return Ok(ContactClaimResult::Missing);
    };
    if contact.as_slice() != holder {
        tx.rollback().await?;
        return Ok(ContactClaimResult::Denied(
            "only the configured contact may claim",
        ));
    }

    // Fence an expired lease to `unknown` before evaluating the claim.
    sqlx::query(
        "UPDATE contact_claims \
         SET state = 'unknown', holder = NULL, lease_expires_at = NULL \
         WHERE community_id = $1 AND decision_id = $2 \
           AND state IN ('claimed', 'started') \
           AND lease_expires_at IS NOT NULL \
           AND lease_expires_at < clock_timestamp()",
    )
    .bind(community_id.as_uuid())
    .bind(decision_id)
    .execute(&mut *tx)
    .await?;

    let granted: Option<i64> = sqlx::query_scalar(
        "UPDATE contact_claims \
         SET state = 'claimed', generation = generation + 1, holder = $4, \
             claimed_at = clock_timestamp(), \
             lease_expires_at = clock_timestamp() + make_interval(secs => $5) \
         WHERE community_id = $1 AND decision_id = $2 AND channel_id = $3 \
           AND state IN ('pending', 'unknown') \
         RETURNING generation",
    )
    .bind(community_id.as_uuid())
    .bind(decision_id)
    .bind(channel_id)
    .bind(holder)
    .bind(ttl)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(generation) = granted {
        tx.commit().await?;
        return Ok(ContactClaimResult::Granted { generation });
    }

    // Idempotent re-claim by the live holder returns its current generation —
    // retries after a lost OK must not bump the fence.
    let current = sqlx::query(
        "SELECT state, holder, generation FROM contact_claims \
         WHERE community_id = $1 AND decision_id = $2",
    )
    .bind(community_id.as_uuid())
    .bind(decision_id)
    .fetch_optional(&mut *tx)
    .await?;
    let outcome = match current {
        None => ContactClaimResult::Missing,
        Some(row) => {
            let state: String = row.try_get("state")?;
            let holder_row: Option<Vec<u8>> = row.try_get("holder")?;
            let generation: i64 = row.try_get("generation")?;
            match state.as_str() {
                "claimed" | "started" if holder_row.as_deref() == Some(holder) => {
                    ContactClaimResult::Granted { generation }
                }
                "claimed" | "started" => ContactClaimResult::Denied("decision already claimed"),
                "completed" => ContactClaimResult::Denied("decision already completed"),
                "cancelled" => ContactClaimResult::Denied("decision cancelled"),
                _ => ContactClaimResult::Denied("decision is not claimable"),
            }
        }
    };
    tx.commit().await?;
    Ok(outcome)
}

/// Fenced `start` transition: the holder marks its granted claim as started.
/// Wrong generation or holder fences the request out.
pub async fn start_contact_claim(
    pool: &PgPool,
    community_id: CommunityId,
    decision_id: &[u8],
    channel_id: Uuid,
    holder: &[u8],
    generation: i64,
) -> Result<ContactControlResult> {
    let rows = sqlx::query(
        "UPDATE contact_claims \
         SET state = 'started', started_at = clock_timestamp() \
         WHERE community_id = $1 AND decision_id = $2 AND channel_id = $3 \
           AND state = 'claimed' AND holder = $4 AND generation = $5 \
           AND (lease_expires_at IS NULL OR lease_expires_at > clock_timestamp())",
    )
    .bind(community_id.as_uuid())
    .bind(decision_id)
    .bind(channel_id)
    .bind(holder)
    .bind(generation)
    .execute(pool)
    .await?
    .rows_affected();
    if rows == 1 {
        return Ok(ContactControlResult::Applied);
    }
    contact_control_denial(pool, community_id, decision_id).await
}

/// Fenced `cancel` transition: the holder abandons a granted/started claim.
/// Cancellation is terminal — a cancelled decision is never re-claimable.
pub async fn cancel_contact_claim(
    pool: &PgPool,
    community_id: CommunityId,
    decision_id: &[u8],
    channel_id: Uuid,
    holder: &[u8],
    generation: i64,
) -> Result<ContactControlResult> {
    let rows = sqlx::query(
        "UPDATE contact_claims SET state = 'cancelled' \
         WHERE community_id = $1 AND decision_id = $2 AND channel_id = $3 \
           AND state IN ('claimed', 'started') AND holder = $4 AND generation = $5",
    )
    .bind(community_id.as_uuid())
    .bind(decision_id)
    .bind(channel_id)
    .bind(holder)
    .bind(generation)
    .execute(pool)
    .await?
    .rows_affected();
    if rows == 1 {
        return Ok(ContactControlResult::Applied);
    }
    contact_control_denial(pool, community_id, decision_id).await
}

async fn contact_control_denial(
    pool: &PgPool,
    community_id: CommunityId,
    decision_id: &[u8],
) -> Result<ContactControlResult> {
    let row = sqlx::query(
        "SELECT state FROM contact_claims \
         WHERE community_id = $1 AND decision_id = $2",
    )
    .bind(community_id.as_uuid())
    .bind(decision_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        None => ContactControlResult::Missing,
        Some(row) => {
            let state: String = row.try_get("state")?;
            match state.as_str() {
                "completed" => ContactControlResult::Denied("decision already completed"),
                "cancelled" => ContactControlResult::Denied("decision cancelled"),
                "pending" => ContactControlResult::Denied("decision not claimed"),
                _ => ContactControlResult::Denied("claim fence rejected the transition"),
            }
        }
    })
}

/// Insert a kind:46043 receipt and, in the same transaction, complete the
/// contact claim it names. The claim must be `started`, held by the receipt
/// signer, and at the named generation — otherwise the whole insert rolls
/// back and the receipt is rejected.
///
/// The deferred `contact_check_claim_complete_v1` trigger re-proves at commit
/// that the completing receipt row exists, is a 46043 signed by the contact,
/// and e-tags the decision.
pub async fn insert_contact_receipt(
    pool: &PgPool,
    community_id: CommunityId,
    event: &Event,
    channel_id: Option<Uuid>,
    thread_meta: Option<ThreadMetadataParams<'_>>,
    claim: ContactClaimRef,
) -> Result<(StoredEvent, bool)> {
    let signer = event.pubkey.to_bytes();
    let receipt_id = event.id.as_bytes().to_vec();
    let connection = crate::observability::acquire_writer(
        pool,
        crate::observability::WriterOperation::EventWrite,
    )
    .await?;
    let mut tx = sqlx::Transaction::begin(connection, None).await?;
    let (stored, was_inserted) = insert_event_with_thread_metadata_classified_tx(
        &mut tx,
        community_id,
        event,
        channel_id,
        thread_meta,
        None,
    )
    .await?;
    if was_inserted {
        let completed = sqlx::query(
            "UPDATE contact_claims \
             SET state = 'completed', receipt_id = $4 \
             WHERE community_id = $1 AND decision_id = $2 AND generation = $3 \
               AND state = 'started' AND holder = $5",
        )
        .bind(community_id.as_uuid())
        .bind(claim.decision_id.as_slice())
        .bind(claim.generation)
        .bind(&receipt_id)
        .bind(signer.as_slice())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if completed == 0 {
            tx.rollback().await?;
            return Err(DbError::AccessDenied(
                "contact claim is not started/held by this signer at that generation".into(),
            ));
        }
    }
    tx.commit().await?;
    Ok((stored, was_inserted))
}

/// Fetch the committed kind:46044 proof for an already-routed original, if
/// one exists. Used to re-deliver a stored decision when a duplicate ingest
/// arrives after the original commit (at-least-once proof delivery — the
/// post-commit fanout is not part of the decision transaction).
pub async fn contact_proof_for_original(
    pool: &PgPool,
    community_id: CommunityId,
    original_id: &[u8],
) -> Result<Option<StoredEvent>> {
    let decision_id: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT decision_id FROM contact_routes          WHERE community_id = $1 AND original_id = $2",
    )
    .bind(community_id.as_uuid())
    .bind(original_id)
    .fetch_optional(pool)
    .await?;
    let Some(decision_id) = decision_id else {
        return Ok(None);
    };
    super::event::get_event_by_id(pool, community_id, &decision_id).await
}

/// Telemetry-spanning `Db` wrappers for the relay seams.
impl Db {
    /// See [`decide_contact_route`].
    #[datastore_span(name = "decide_contact_route", system = "postgresql")]
    pub async fn decide_contact_route(
        &self,
        community_id: CommunityId,
        event: &Event,
        channel_id: Uuid,
        thread_meta: Option<ThreadMetadataParams<'_>>,
        relay_keys: &Keys,
    ) -> Result<ContactRouteOutcome> {
        decide_contact_route(
            &self.pool,
            community_id,
            event,
            channel_id,
            thread_meta,
            relay_keys,
        )
        .await
    }

    /// See [`contact_proof_for_original`].
    #[datastore_span(name = "contact_proof_for_original", system = "postgresql")]
    pub async fn contact_proof_for_original(
        &self,
        community_id: CommunityId,
        original_id: &[u8],
    ) -> Result<Option<StoredEvent>> {
        contact_proof_for_original(&self.pool, community_id, original_id).await
    }

    /// See [`claim_contact_decision`].
    #[datastore_span(name = "claim_contact_decision", system = "postgresql")]
    pub async fn claim_contact_decision(
        &self,
        community_id: CommunityId,
        decision_id: &[u8],
        channel_id: Uuid,
        holder: &[u8],
        ttl_secs: i64,
    ) -> Result<ContactClaimResult> {
        claim_contact_decision(
            &self.pool,
            community_id,
            decision_id,
            channel_id,
            holder,
            ttl_secs,
        )
        .await
    }

    /// See [`start_contact_claim`].
    #[datastore_span(name = "start_contact_claim", system = "postgresql")]
    pub async fn start_contact_claim(
        &self,
        community_id: CommunityId,
        decision_id: &[u8],
        channel_id: Uuid,
        holder: &[u8],
        generation: i64,
    ) -> Result<ContactControlResult> {
        start_contact_claim(
            &self.pool,
            community_id,
            decision_id,
            channel_id,
            holder,
            generation,
        )
        .await
    }

    /// See [`cancel_contact_claim`].
    #[datastore_span(name = "cancel_contact_claim", system = "postgresql")]
    pub async fn cancel_contact_claim(
        &self,
        community_id: CommunityId,
        decision_id: &[u8],
        channel_id: Uuid,
        holder: &[u8],
        generation: i64,
    ) -> Result<ContactControlResult> {
        cancel_contact_claim(
            &self.pool,
            community_id,
            decision_id,
            channel_id,
            holder,
            generation,
        )
        .await
    }

    /// See [`insert_contact_receipt`].
    #[datastore_span(name = "insert_contact_receipt", system = "postgresql")]
    pub async fn insert_contact_receipt(
        &self,
        community_id: CommunityId,
        event: &Event,
        channel_id: Option<Uuid>,
        thread_meta: Option<ThreadMetadataParams<'_>>,
        claim: ContactClaimRef,
    ) -> Result<(StoredEvent, bool)> {
        insert_contact_receipt(
            &self.pool,
            community_id,
            event,
            channel_id,
            thread_meta,
            claim,
        )
        .await
    }
}

#[cfg(test)]
#[path = "contact_decision_postgres_tests.rs"]
mod contact_decision_postgres_tests;
