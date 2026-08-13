//! Crew repo wiki ingest (kind [`buzz_core::kind::KIND_REPO_WIKI_PAGE`]).

use buzz_core::kind::{KIND_ORG_ROSTER, KIND_REPO_WIKI_PAGE};
use buzz_core::tenant::TenantContext;
use buzz_core::wiki_page::validate_wiki_page_envelope;
use nostr::Event;

use super::ingest::IngestError;
use crate::state::AppState;

/// Pre-storage envelope check. LWW replace is NIP-33.
pub(crate) fn validate_wiki_page_ingest(event: &Event) -> Result<(), IngestError> {
    let tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice().iter().map(|s| s.to_string()).collect())
        .collect();
    validate_wiki_page_envelope(event.kind.as_u16() as u32, &tags)
        .map_err(|e| IngestError::Rejected(format!("invalid: {e}")))?;
    Ok(())
}

/// Crew addressable envelopes that must stay out of `ingest.rs` (D-022).
pub(crate) async fn validate_roster_or_wiki(
    kind: u32,
    tenant: &TenantContext,
    event: &Event,
    state: &AppState,
) -> Result<(), IngestError> {
    if kind == KIND_ORG_ROSTER {
        return super::org_roster::validate_org_roster_ingest(tenant, event, state).await;
    }
    if kind == KIND_REPO_WIKI_PAGE {
        return validate_wiki_page_ingest(event);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_core::kind::KIND_REPO_WIKI_PAGE;
    use nostr::{EventBuilder, Kind, Tag};

    fn owner() -> String {
        "ab".repeat(32)
    }

    #[tokio::test]
    async fn accepts_page_with_matching_a_and_commit() {
        let keys = nostr::Keys::generate();
        let a = format!("30617:{}:crew", owner());
        let event = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "# Overview")
            .tags([
                Tag::parse(["d", "crew/overview"]).expect("d"),
                Tag::parse(["a", &a]).expect("a"),
                Tag::parse(["commit", "abc123"]).expect("commit"),
            ])
            .sign_with_keys(&keys)
            .expect("sign");
        assert!(validate_wiki_page_ingest(&event).is_ok());
    }

    #[tokio::test]
    async fn rejects_missing_a_tag() {
        let keys = nostr::Keys::generate();
        let event = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "# Overview")
            .tags([
                Tag::parse(["d", "crew/overview"]).expect("d"),
                Tag::parse(["commit", "abc123"]).expect("commit"),
            ])
            .sign_with_keys(&keys)
            .expect("sign");
        assert!(validate_wiki_page_ingest(&event).is_err());
    }
}
