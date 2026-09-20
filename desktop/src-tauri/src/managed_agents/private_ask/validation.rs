use super::super::hermes_profile;

const MAX_SCOPE_VALUE_BYTES: usize = 256;

pub(super) fn valid_scope_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SCOPE_VALUE_BYTES
        && value == value.trim()
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SCOPE_VALUE_BYTES
        && value == value.trim()
        && !value.starts_with('-')
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_profile(value: &str) -> bool {
    hermes_profile::validate_hermes_profile_name(value).is_ok()
}

/// Match the immutable revision grammar emitted by `crew-wiki` source
/// snapshots. A free-form revision could relabel grounding with an
/// unresolvable or mutable checkout name.
pub(super) fn valid_source_revision(value: &str) -> bool {
    if let Some(hash) = value.strip_prefix("git:") {
        (hash.len() == 40 || hash.len() == 64) && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    } else if let Some(hash) = value.strip_prefix("folder:") {
        hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    } else {
        false
    }
}

pub(super) fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
