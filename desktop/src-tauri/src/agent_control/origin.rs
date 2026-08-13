//! Two-layer origin policy: subject origin unrestricted; others need allowlist
//! or owner elicitation (Allow once / Allow domain / Deny).

use std::collections::{HashMap, HashSet};

use super::protocol::ControlError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginDecision {
    AllowOnce,
    AllowDomain,
    Deny,
}

#[derive(Debug, Clone, Default)]
pub struct OriginPolicy {
    /// Per-channel extra origins (from canvas `tooling.browserAllowlist`).
    allowlist: HashMap<String, HashSet<String>>,
    /// Session-only (Allow once) grants.
    once: HashMap<String, HashSet<String>>,
}

impl OriginPolicy {
    #[allow(dead_code)]
    pub fn set_allowlist(&mut self, channel_id: &str, origins: impl IntoIterator<Item = String>) {
        self.allowlist.insert(
            channel_id.to_string(),
            origins.into_iter().map(|o| normalize_origin(&o)).collect(),
        );
    }

    #[allow(dead_code)]
    pub fn allowlist(&self, channel_id: &str) -> Vec<String> {
        self.allowlist
            .get(channel_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn grant_once(&mut self, channel_id: &str, origin: &str) {
        self.once
            .entry(channel_id.to_string())
            .or_default()
            .insert(normalize_origin(origin));
    }

    pub fn grant_domain(&mut self, channel_id: &str, origin: &str) {
        self.allowlist
            .entry(channel_id.to_string())
            .or_default()
            .insert(normalize_origin(origin));
    }

    pub fn is_allowed(&self, channel_id: &str, subject_origin: &str, target: &str) -> bool {
        let subject = normalize_origin(subject_origin);
        let target = normalize_origin(target);
        if target.is_empty() {
            return false;
        }
        if !subject.is_empty() && origins_match(&subject, &target) {
            return true;
        }
        if self
            .allowlist
            .get(channel_id)
            .is_some_and(|set| set.iter().any(|allowed| origins_match(allowed, &target)))
        {
            return true;
        }
        self.once
            .get(channel_id)
            .is_some_and(|set| set.iter().any(|allowed| origins_match(allowed, &target)))
    }

    pub fn check(
        &self,
        channel_id: &str,
        subject_origin: &str,
        target: &str,
    ) -> Result<(), ControlError> {
        if self.is_allowed(channel_id, subject_origin, target) {
            Ok(())
        } else {
            Err(ControlError::origin_blocked(&normalize_origin(target)))
        }
    }
}

pub fn origin_of_url(url: &str) -> Result<String, ControlError> {
    let parsed = url::Url::parse(url).map_err(|e| {
        ControlError::origin_blocked(url)
            .with_data(serde_json::json!({ "parse_error": e.to_string() }))
    })?;
    let origin = parsed.origin().ascii_serialization();
    Ok(normalize_origin(&origin))
}

pub fn normalize_origin(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        return url.origin().ascii_serialization();
    }
    if let Ok(url) = url::Url::parse(&format!("https://{trimmed}")) {
        return url.origin().ascii_serialization();
    }
    trimmed.to_string()
}

fn origins_match(allowed: &str, target: &str) -> bool {
    allowed.eq_ignore_ascii_case(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_origin_is_unrestricted() {
        let policy = OriginPolicy::default();
        assert!(policy.is_allowed("ch", "http://127.0.0.1:5173", "http://127.0.0.1:5173/app"));
        assert!(!policy.is_allowed("ch", "http://127.0.0.1:5173", "https://api.stripe.com"));
    }

    #[test]
    fn allowlist_and_once() {
        let mut policy = OriginPolicy::default();
        policy.grant_once("ch", "https://example.com");
        assert!(policy.is_allowed("ch", "http://127.0.0.1:5173", "https://example.com/docs"));
        policy.set_allowlist("ch", ["https://api.stripe.com".into()]);
        assert!(policy.is_allowed("ch", "http://127.0.0.1:5173", "https://api.stripe.com/v1"));
        assert!(policy.allowlist("ch").iter().any(|o| o.contains("stripe")));
    }
}
