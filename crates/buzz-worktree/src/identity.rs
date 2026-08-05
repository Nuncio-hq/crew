//! Full thread-root identity validation.

use crate::error::{LeaseError, RecordError};

/// Length of a Nostr event id hex string.
pub const ROOT_EVENT_ID_LEN: usize = 64;

/// Normalize and validate a full root event id (64 lowercase hex).
pub fn normalize_root_event_id(root_event_id: &str) -> Result<String, String> {
    let trimmed = root_event_id.trim();
    if trimmed.len() != ROOT_EVENT_ID_LEN {
        return Err(format!(
            "expected {ROOT_EVENT_ID_LEN} hex characters, got {}",
            trimmed.len()
        ));
    }
    if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("root event id must be hexadecimal".to_string());
    }
    Ok(trimmed.to_ascii_lowercase())
}

/// Validate a root id for lease APIs.
pub fn validate_root_event_id(root_event_id: &str) -> Result<String, LeaseError> {
    normalize_root_event_id(root_event_id).map_err(LeaseError::InvalidIdentity)
}

/// Validate a root id for record APIs.
pub(crate) fn validate_root_for_record(root_event_id: &str) -> Result<String, RecordError> {
    normalize_root_event_id(root_event_id).map_err(RecordError::InvalidIdentity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_full_hex_root() {
        let root = "a".repeat(64);
        assert_eq!(normalize_root_event_id(&root).unwrap(), root);
    }

    #[test]
    fn rejects_short_and_prefix_collision_aliases() {
        assert!(normalize_root_event_id(&"a".repeat(12)).is_err());
        assert!(normalize_root_event_id(&format!("{}ZZ", "a".repeat(62))).is_err());
    }
}
