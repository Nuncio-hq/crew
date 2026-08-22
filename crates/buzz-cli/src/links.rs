//! Canonical `buzz://` deep links for Buzz-hosted git entities.
//!
//! Buzz Desktop renders these links as rich preview cards in chat and
//! navigates in-app when they are clicked. The desktop parser lives in
//! `desktop/src/shared/lib/entityLink.ts` — the two implementations must
//! stay format-compatible (see `golden_format_matches_desktop` below and
//! the mirror test in `entityLink.test.mjs`).
//!
//! Callers are expected to validate inputs first (`validate_hex64`,
//! `validate_repo_id`); the identifier charsets need no URL encoding.

/// Build a `buzz://repo` link for a repository announcement (kind 30617).
pub fn repo_link(owner: &str, repo_id: &str) -> String {
    format!("buzz://repo?owner={owner}&d={repo_id}")
}

/// Build a `buzz://pr` link for a pull request event (kind 1618).
pub fn pull_request_link(event_id: &str, owner: &str, repo_id: &str) -> String {
    format!("buzz://pr?id={event_id}&owner={owner}&d={repo_id}")
}

/// Build a `buzz://issue` link for an issue event (kind 1621).
pub fn issue_link(event_id: &str, owner: &str, repo_id: &str) -> String {
    format!("buzz://issue?id={event_id}&owner={owner}&d={repo_id}")
}

/// Build a `buzz://file` link for a repository path and line range.
#[allow(dead_code)] // format sibling of repo/pr/issue; desktop is the click target
pub fn file_link(owner: &str, repo_id: &str, path: &str, lines: &str) -> String {
    format!("buzz://file?owner={owner}&d={repo_id}&path={path}&lines={lines}")
}

/// A parsed `buzz://message?channel=<uuid>&id=<hex>[&thread=<hex>]` link.
///
/// Mirrors `parseMessageLink` in
/// `desktop/src/features/messages/lib/messageLink.ts` — Desktop's "Copy link"
/// produces exactly these links, and agents paste them into the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageLink {
    pub channel_id: String,
    pub event_id: String,
    /// Thread root, when the linked message is a reply. Informational for the
    /// CLI: `messages thread` resolves the thread from `event_id` alone.
    pub thread_root_id: Option<String>,
}

const MESSAGE_LINK_PREFIX: &str = "buzz://message?";

/// Cheap pre-check: does `input` look like a `buzz://message` link?
pub fn is_message_link(input: &str) -> bool {
    input.starts_with(MESSAGE_LINK_PREFIX) || input == "buzz://message"
}

/// Parse a `buzz://message` link. Returns `None` for a different scheme or
/// host, or when `channel`/`id` is missing or empty.
pub fn parse_message_link(input: &str) -> Option<MessageLink> {
    let query = input.strip_prefix(MESSAGE_LINK_PREFIX)?;

    let mut channel_id = None;
    let mut event_id = None;
    let mut thread_root_id = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = percent_decode(value);
        if value.is_empty() {
            continue;
        }
        match key {
            "channel" => channel_id = Some(value),
            "id" => event_id = Some(value),
            "thread" => thread_root_id = Some(value),
            _ => {}
        }
    }

    Some(MessageLink {
        channel_id: channel_id?,
        event_id: event_id?,
        thread_root_id,
    })
}

/// Decode `%XX` escapes in a query parameter value. Bytes that do not form a
/// valid escape are kept verbatim, and invalid UTF-8 falls back to the raw
/// input — link identifiers are ASCII, so this only has to be lossless.
fn percent_decode(value: &str) -> String {
    if !value.contains('%') {
        return value.to_string();
    }
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "71d67180ba17e749ee825fc8819c9c6ee7003617e1c126504f9b658070ab9224";
    const EVENT_ID: &str = "c3b589fa5713ba25bad6dc095e2de00a4ac8f50050fdea00fc6444e603be1dd1";
    const CHANNEL: &str = "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50";

    #[test]
    fn message_link_is_recognized_by_prefix() {
        assert!(is_message_link(&format!(
            "buzz://message?channel={CHANNEL}&id={EVENT_ID}"
        )));
        assert!(!is_message_link(EVENT_ID));
        assert!(!is_message_link("buzz://pr?id=abc&owner=x&d=y"));
        assert!(!is_message_link("https://example.com/message?id=abc"));
    }

    #[test]
    fn parse_message_link_extracts_channel_and_event() {
        let parsed = parse_message_link(&format!("buzz://message?channel={CHANNEL}&id={EVENT_ID}"))
            .expect("link parses");
        assert_eq!(parsed.channel_id, CHANNEL);
        assert_eq!(parsed.event_id, EVENT_ID);
        assert_eq!(parsed.thread_root_id, None);
    }

    #[test]
    fn parse_message_link_keeps_thread_root_and_ignores_param_order() {
        let parsed = parse_message_link(&format!(
            "buzz://message?thread={OWNER}&id={EVENT_ID}&channel={CHANNEL}"
        ))
        .expect("link parses");
        assert_eq!(parsed.channel_id, CHANNEL);
        assert_eq!(parsed.event_id, EVENT_ID);
        assert_eq!(parsed.thread_root_id.as_deref(), Some(OWNER));
    }

    #[test]
    fn parse_message_link_rejects_malformed_links() {
        // Missing id, missing channel, empty values, wrong host, wrong scheme.
        assert!(parse_message_link(&format!("buzz://message?channel={CHANNEL}")).is_none());
        assert!(parse_message_link(&format!("buzz://message?id={EVENT_ID}")).is_none());
        assert!(parse_message_link(&format!("buzz://message?channel=&id={EVENT_ID}")).is_none());
        assert!(parse_message_link(&format!("buzz://message?channel={CHANNEL}&id=")).is_none());
        assert!(
            parse_message_link(&format!("buzz://thread?channel={CHANNEL}&id={EVENT_ID}")).is_none()
        );
        assert!(
            parse_message_link(&format!("https://message?channel={CHANNEL}&id={EVENT_ID}"))
                .is_none()
        );
        assert!(parse_message_link("buzz://message").is_none());
    }

    #[test]
    fn parse_message_link_percent_decodes_values() {
        // Desktop builds links with URLSearchParams, which percent-encodes.
        let parsed = parse_message_link(&format!(
            "buzz://message?channel={CHANNEL}&id=mock%2Dgeneral%2Dwelcome"
        ))
        .expect("link parses");
        assert_eq!(parsed.event_id, "mock-general-welcome");
    }

    // Golden strings shared with desktop/src/shared/lib/entityLink.test.mjs
    // ("builders emit the canonical cross-language link format").
    #[test]
    fn golden_format_matches_desktop() {
        assert_eq!(
            pull_request_link(EVENT_ID, OWNER, "buzz-world"),
            format!("buzz://pr?id={EVENT_ID}&owner={OWNER}&d=buzz-world")
        );
        assert_eq!(
            issue_link(EVENT_ID, OWNER, "buzz-world"),
            format!("buzz://issue?id={EVENT_ID}&owner={OWNER}&d=buzz-world")
        );
        assert_eq!(
            repo_link(OWNER, "buzz-world"),
            format!("buzz://repo?owner={OWNER}&d=buzz-world")
        );
        assert_eq!(
            file_link(OWNER, "buzz-world", "README.md", "1-12"),
            format!("buzz://file?owner={OWNER}&d=buzz-world&path=README.md&lines=1-12")
        );
    }
}
