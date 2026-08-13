//! Notify the desktop control endpoint when a turn ends or is cancelled (#197).
//! Fire-and-forget; never touches worktree leases.

use serde_json::json;

pub fn notify_lease_release(channel_id: Option<String>, reason: &str) {
    let Some(channel_id) = channel_id.filter(|s| !s.is_empty()) else {
        return;
    };
    let Ok(url) = std::env::var("BUZZ_DESKTOP_CONTROL_URL") else {
        return;
    };
    let Ok(token) = std::env::var("BUZZ_DESKTOP_CONTROL_TOKEN") else {
        return;
    };
    if url.is_empty() || token.is_empty() {
        return;
    }
    let channel = channel_id.to_string();
    let reason = reason.to_string();
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let body = json!({
            "v": 1,
            "method": "lease.release",
            "channel_id": channel,
            "params": { "reason": reason },
        });
        let _ = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&body)
            .send()
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_env_is_a_no_op() {
        // Must not panic or acquire any filesystem/worktree resource.
        notify_lease_release(Some("ch".into()), "turn_end");
        notify_lease_release(None, "cancel");
    }
}
