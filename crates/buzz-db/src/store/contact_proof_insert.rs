//! Disabled #355 storage proof inputs. No relay path constructs these yet.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i16)]
pub(super) enum ContactClass {
    Routed = 1,
    ExplicitMention = 2,
    Disabled = 4,
    Quota = 10,
    ReplayFloor = 12,
}

pub(super) const CLASSIFIED_INSERT: &str = r#"
INSERT INTO events (community_id, id, pubkey, created_at, kind, tags, content,
    sig, received_at, channel_id, d_tag, not_before, contact_class)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
ON CONFLICT DO NOTHING
"#;
