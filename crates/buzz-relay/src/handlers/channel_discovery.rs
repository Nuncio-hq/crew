//! Canonical channel discovery stored under one lifecycle-fenced snapshot.

use crate::state::AppState;
use buzz_core::tenant::TenantContext;
use nostr::{Event, EventBuilder, Kind, Tag, Timestamp};
use std::sync::Arc;
use uuid::Uuid;

/// Publish current metadata, admins and membership, optionally repairing an exact create.
pub async fn emit(
    tenant: &TenantContext,
    state: &Arc<AppState>,
    channel_id: Uuid,
    replay: Option<&Event>,
) -> anyhow::Result<()> {
    let relay_pubkey = state.relay_keypair.public_key().to_bytes();
    let mut snapshot = state
        .db
        .lock_discovery_snapshot(tenant.community(), channel_id, &relay_pubkey, replay)
        .await?;
    let channel = snapshot.channel.clone();
    let members = snapshot.members.clone();
    let group_id = channel_id.to_string();
    let mut tags: Vec<Tag> = vec![Tag::parse(["d", &group_id])?];
    tags.push(Tag::parse(["name", &channel.name])?);
    if let Some(ref desc) = channel.description {
        if !desc.is_empty() {
            tags.push(Tag::parse(["about", desc])?);
        }
    }
    if channel.visibility == "private" {
        tags.push(Tag::parse(["private"])?);
    } else {
        // Explicit "public" tag complements NIP-29's absence-of-"private" convention,
        // making channel visibility self-describing for clients.
        tags.push(Tag::parse(["public"])?);
    }
    // NIP-29 hidden tag: hint to clients not to show DMs in public group lists.
    // Not a security boundary — access control is handled by channel-scoped storage.
    if channel.channel_type == "dm" {
        tags.push(Tag::parse(["hidden"])?);
        // Include participant pubkeys in kind:39000 for DMs so clients can
        // resolve display names without a separate kind:39002 fetch.
        for m in &members {
            let pubkey_hex = hex::encode(&m.pubkey);
            tags.push(Tag::parse(["p", &pubkey_hex])?);
        }
    }
    // Buzz channels always require explicit membership
    tags.push(Tag::parse(["closed"])?);
    // Channel type tag so clients can distinguish stream/forum/dm without inference
    tags.push(Tag::parse(["t", &channel.channel_type])?);
    // Optional topic / purpose for richer client UX
    if let Some(ref topic) = channel.topic {
        if !topic.is_empty() {
            tags.push(Tag::parse(["topic", topic])?);
        }
    }
    if let Some(ref purpose) = channel.purpose {
        if !purpose.is_empty() {
            tags.push(Tag::parse(["purpose", purpose])?);
        }
    }
    // Archived state — clients use this to hide channels from the sidebar.
    if channel.archived_at.is_some() {
        tags.push(Tag::parse(["archived", "true"])?);
    }
    // Ephemeral channel TTL — clients use this to show countdown timers.
    if let Some(ttl) = channel.ttl_seconds {
        tags.push(Tag::parse(["ttl", &ttl.to_string()])?);
    }
    if let Some(ref deadline) = channel.ttl_deadline {
        tags.push(Tag::parse(["ttl_deadline", &deadline.to_rfc3339()])?);
    }
    let mut admins = vec![Tag::parse(["d", &group_id])?];
    for member in members
        .iter()
        .filter(|member| member.role == "owner" || member.role == "admin")
    {
        admins.push(Tag::parse([
            "p",
            &hex::encode(&member.pubkey),
            &member.role,
        ])?);
    }
    let roster = super::side_effects::group_members_tags(&group_id, &members)?;
    let mut stored = Vec::with_capacity(3);
    for (kind, tags) in [(39000, tags), (39001, admins), (39002, roster)] {
        let now = Timestamp::now().as_secs();
        let timestamp = snapshot
            .latest_timestamp(kind)
            .await?
            .map(|value| value.saturating_add(1))
            .unwrap_or(now)
            .max(now);
        let event = EventBuilder::new(Kind::Custom(kind as u16), "")
            .tags(tags)
            .allow_self_tagging()
            .custom_created_at(Timestamp::from(timestamp))
            .sign_with_keys(&state.relay_keypair)?;
        let (event, inserted) = snapshot.store(&event).await?;
        if !inserted {
            anyhow::bail!("canonical discovery head was not stored");
        }
        stored.push(event);
    }
    snapshot.commit().await?;
    let relay_hex = hex::encode(relay_pubkey);
    for event in stored {
        let kind = buzz_core::kind::event_kind_u32(&event.event);
        super::event::dispatch_persistent_event(tenant, state, &event, kind, &relay_hex, None)
            .await;
    }
    Ok(())
}
