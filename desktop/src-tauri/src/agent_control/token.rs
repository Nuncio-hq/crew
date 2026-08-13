//! Bearer token for `POST /agent-control`.

use getrandom::getrandom;

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    if getrandom(&mut bytes).is_err() {
        // Fall back to uuid if the OS RNG is unavailable.
        return uuid::Uuid::new_v4().to_string().replace('-', "");
    }
    hex::encode(bytes)
}

pub fn bearer_matches(header: Option<&str>, token: &str) -> bool {
    let Some(header) = header else {
        return false;
    };
    let rest = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .unwrap_or("");
    !token.is_empty() && rest == token
}
