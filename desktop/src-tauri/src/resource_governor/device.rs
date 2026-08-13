//! Deterministic `crew-<channel-id-prefix>` device names (D-010 / D-058).
//! Find-or-create by name; no parallel registry.

/// First eight hex digits of a channel UUID, used in `crew-<prefix>`.
pub fn channel_id_prefix(channel_id: &str) -> String {
    channel_id
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Deterministic CoreSimulator device name for a channel.
pub fn crew_device_name(channel_id: &str) -> String {
    format!("crew-{}", channel_id_prefix(channel_id))
}

pub fn is_crew_device_name(name: &str) -> bool {
    name.starts_with("crew-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_uuid_without_hyphens() {
        assert_eq!(
            crew_device_name("9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50"),
            "crew-9a1657ac"
        );
    }

    #[test]
    fn rejects_foreign_names() {
        assert!(!is_crew_device_name("iPhone 16 Pro"));
        assert!(is_crew_device_name("crew-abcd1234"));
    }
}
