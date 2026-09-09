//! Strict canonical primitives shared by native Wiki snapshot verification.
use crate::source_snapshot::{source_hash, valid_source_path, SourceReference};
use crate::WikiError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const MAX_EVENT_BYTES: usize = 192 * 1024;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SignedEvent {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u32,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

pub(super) fn invalid() -> WikiError {
    WikiError::Publish("invalid signed Wiki snapshot".into())
}

pub(super) fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn metadata(value: &str) -> bool {
    !value.contains('\0')
        && value
            .chars()
            .any(|c| c != ' ' && !('\u{0009}'..='\u{000d}').contains(&c))
}

pub(super) fn valid_revision(value: &str) -> bool {
    if let Some(hash) = value.strip_prefix("git:") {
        hex(hash, 40) || hex(hash, 64)
    } else {
        value
            .strip_prefix("folder:")
            .is_some_and(|hash| hex(hash, 64))
    }
}

pub(super) fn valid_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|&i| bytes[i] == b'-')
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(i, b)| {
            [8, 13, 18, 23].contains(&i) || b.is_ascii_digit() || (b'a'..=b'f').contains(b)
        })
}

pub(super) fn valid_repo(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('.')
        && !value.contains("..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

pub(super) fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub(super) fn valid_sources(sources: &[SourceReference]) -> bool {
    let mut previous: Option<&SourceReference> = None;
    for source in sources {
        let (path, hash, bytes, start, end) = source;
        if !valid_source_path(path)
            || !hex(hash, 64)
            || *bytes == 0
            || *bytes > 1024 * 1024
            || *start == 0
            || end < start
            || end > bytes
        {
            return false;
        }
        if let Some(prior) = previous {
            if prior.0 > *path
                || (prior.0 == *path
                    && (prior.1 != *hash
                        || prior.2 != *bytes
                        || prior.3 > *start
                        || (prior.3 == *start && prior.4 >= *end)))
            {
                return false;
            }
        }
        previous = Some(source);
    }
    true
}

pub(super) fn canonical<T: Serialize + ?Sized>(value: &T) -> Result<String, WikiError> {
    serde_json::to_string(value).map_err(|_| invalid())
}

pub(super) fn digest<T: Serialize + ?Sized>(value: &T) -> Result<String, WikiError> {
    Ok(source_hash(canonical(value)?.as_bytes()))
}

pub(super) fn tag<'a>(
    event: &'a SignedEvent,
    name: &str,
    width: usize,
) -> Result<&'a [String], WikiError> {
    let mut matches = event
        .tags
        .iter()
        .filter(|t| t.first().is_some_and(|v| v == name));
    let value = matches.next().ok_or_else(invalid)?;
    if value.len() != width || matches.next().is_some() {
        return Err(invalid());
    }
    Ok(value)
}

pub(super) fn verify_event(raw: &Value) -> Result<SignedEvent, WikiError> {
    // Count only the seven signed fields without copying arbitrary foreign fields
    // or allocating an oversized signed payload before its bound is established.
    bound_signed_fields(raw)?;
    // Deserialize exactly seven owned signed fields; ignore foreign cache flags.
    let event = SignedEvent::deserialize(raw).map_err(|_| invalid())?;
    if !hex(&event.id, 64)
        || !hex(&event.pubkey, 64)
        || !hex(&event.sig, 128)
        || event.kind != 30623
        || event.created_at > MAX_SAFE_INTEGER
        || event.content.contains('\0')
        || event.tags.iter().flatten().any(|v| v.contains('\0'))
    {
        return Err(invalid());
    }
    let encoded = canonical(&event)?;
    if encoded.len() > MAX_EVENT_BYTES {
        return Err(invalid());
    }
    let signed: nostr::Event = serde_json::from_str(&encoded).map_err(|_| invalid())?;
    if !signed.verify_id() || !signed.verify_signature() {
        return Err(invalid());
    }
    Ok(event)
}

fn bound_signed_fields(raw: &Value) -> Result<(), WikiError> {
    use serde::ser::SerializeMap;
    struct Fields<'a>(&'a Value);
    impl Serialize for Fields<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut fields = serializer.serialize_map(Some(7))?;
            for name in [
                "id",
                "pubkey",
                "created_at",
                "kind",
                "tags",
                "content",
                "sig",
            ] {
                fields.serialize_entry(name, &self.0[name])?;
            }
            fields.end()
        }
    }
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_EVENT_BYTES)
                .ok_or_else(|| std::io::Error::other("signed Wiki event exceeds limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), &Fields(raw)).map_err(|_| invalid())
}
