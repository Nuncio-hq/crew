//! Crew repo wiki ingest (kind [`buzz_core::kind::KIND_REPO_WIKI_PAGE`]).

use buzz_core::kind::{KIND_ORG_ROSTER, KIND_REPO_WIKI_PAGE};
use buzz_core::tenant::TenantContext;
use buzz_core::wiki_page::validate_wiki_page_envelope;
use nostr::Event;

use super::ingest::IngestError;
use crate::state::AppState;

/// Pre-storage envelope check. LWW replace is NIP-33.
pub(crate) fn validate_wiki_page_ingest(
    event: &Event,
    conditional_publication_enabled: bool,
) -> Result<(), IngestError> {
    let tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .map(|tag| tag.as_slice().iter().map(|s| s.to_string()).collect())
        .collect();
    validate_wiki_page_envelope(event.kind.as_u16() as u32, &tags)
        .map_err(|e| IngestError::Rejected(format!("invalid: {e}")))?;
    super::source_publication::validate(event, conditional_publication_enabled)?;
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
        return validate_wiki_page_ingest(event, state.config.crew_conditional_publication_v1);
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
        assert!(validate_wiki_page_ingest(&event, false).is_ok());
    }

    #[tokio::test]
    async fn accepts_folder_snapshot_commit_at_ingest_seam() {
        let keys = nostr::Keys::generate();
        let a = format!("30617:{}:crew", owner());
        let event = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "# Overview")
            .tags([
                Tag::parse(["d", "crew/overview"]).expect("d"),
                Tag::parse(["a", &a]).expect("a"),
                Tag::parse(["commit", &format!("folder:{}", "ab".repeat(32))])
                    .expect("folder commit"),
            ])
            .sign_with_keys(&keys)
            .expect("sign");
        assert!(validate_wiki_page_ingest(&event, false).is_ok());
    }

    #[tokio::test]
    async fn accepts_v1_folder_snapshot_at_enabled_ingest_seam() {
        let keys = nostr::Keys::generate();
        let owner = keys.public_key().to_hex();
        let a = format!("30617:{owner}:crew");
        let snapshot = uuid::Uuid::new_v4().to_string();
        let event = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "# Overview")
            .tags([
                Tag::parse(["d", "crew/overview"]).expect("d"),
                Tag::parse(["a", &a]).expect("a"),
                Tag::parse(["wiki-version", "1"]).expect("version"),
                Tag::parse(["wiki-snapshot", &snapshot]).expect("snapshot"),
                Tag::parse(["source-kind", "folder"]).expect("source kind"),
                Tag::parse(["commit", &format!("folder:{}", "ab".repeat(32))])
                    .expect("folder commit"),
                Tag::parse(["wiki-slug", "overview"]).expect("slug"),
                Tag::parse(["title", "Overview"]).expect("title"),
                Tag::parse(["section", "guide"]).expect("section"),
                Tag::parse(["language", "en"]).expect("language"),
                Tag::parse(["wiki-source-files", "[\"README.md\"]"]).expect("source files"),
            ])
            .sign_with_keys(&keys)
            .expect("sign");

        assert!(validate_wiki_page_ingest(&event, true).is_ok());
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
        assert!(validate_wiki_page_ingest(&event, false).is_err());
    }

    #[tokio::test]
    async fn rejects_source_bound_page_at_the_ingest_validation_seam() {
        let keys = nostr::Keys::generate();
        let event = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "# Overview")
            .tags([
                Tag::parse(["d", "crew/overview"]).expect("d"),
                Tag::parse(["a", &format!("30617:{}:crew", owner())]).expect("a"),
                Tag::parse(["commit", "abc123"]).expect("commit"),
                Tag::parse(["wiki-source-files", "1"]).expect("source marker"),
            ])
            .sign_with_keys(&keys)
            .expect("sign");

        assert!(matches!(
            validate_wiki_page_ingest(&event, false),
            Err(IngestError::Rejected(reason))
                if reason.starts_with("unsupported: crew-conditional-publication-v1")
        ));
    }
}
