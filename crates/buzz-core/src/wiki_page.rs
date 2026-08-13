//! Crew repo wiki pages and TOC manifests (kind [`crate::kind::KIND_REPO_WIKI_PAGE`]).
//!
//! Addressable events. `d` is `<repo-d>/<page-slug>` for a page or
//! `<repo-d>/_toc` for the per-repo table of contents. An `a` tag points at the
//! NIP-34 repository coordinate `30617:<owner-hex>:<repo-d>`.

use thiserror::Error;

use crate::kind::{KIND_GIT_REPO_ANNOUNCEMENT, KIND_REPO_WIKI_PAGE};

/// TOC page slug — one manifest event per repository wiki.
pub const WIKI_TOC_SLUG: &str = "_toc";
/// Company-wiki proposal `d` prefix on kind 30023 (`_proposal/<slug>`).
pub const COMPANY_WIKI_PROPOSAL_D_PREFIX: &str = "_proposal/";
/// Tag marking an agent-drafted company wiki proposal (owner reviews).
pub const CREW_WIKI_PROPOSAL_TAG: &str = "crew-wiki-proposal";
/// Tag carrying proposal status (`pending` / `accepted` / `rejected`).
pub const CREW_WIKI_PROPOSAL_STATUS_TAG: &str = "crew-wiki-status";
/// Optional tag linking a proposal to an engram slug.
pub const CREW_WIKI_ENGRAM_SLUG_TAG: &str = "crew-engram-slug";

/// Refresh cadence stored on the TOC event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WikiCadence {
    /// Generate only when asked.
    Manual,
    /// After a default-branch kind 30618, debounced.
    OnPush,
    /// Once per calendar day (local).
    Daily,
    /// Once per calendar week (local).
    Weekly,
}

impl WikiCadence {
    /// Wire string used as the `cadence` tag value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::OnPush => "on-push",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    /// Parse a cadence tag. Unknown values are an error.
    pub fn parse(raw: &str) -> Result<Self, WikiPageError> {
        match raw {
            "manual" => Ok(Self::Manual),
            "on-push" => Ok(Self::OnPush),
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            _ => Err(WikiPageError::InvalidCadence),
        }
    }
}

/// Parsed `d` tag for a repo wiki event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiDTag {
    /// Repository `d` (NIP-34).
    pub repo_d: String,
    /// Page slug, or [`WIKI_TOC_SLUG`].
    pub slug: String,
}

impl WikiDTag {
    /// True when this addresses the TOC manifest.
    pub fn is_toc(&self) -> bool {
        self.slug == WIKI_TOC_SLUG
    }

    /// Format as a NIP-33 `d` tag value.
    pub fn wire(&self) -> String {
        format!("{}/{}", self.repo_d, self.slug)
    }
}

/// Parse `d = <repo-d>/<slug>`.
pub fn parse_wiki_d_tag(d: &str) -> Result<WikiDTag, WikiPageError> {
    let (repo_d, slug) = d.rsplit_once('/').ok_or(WikiPageError::InvalidDTag)?;
    if repo_d.is_empty() || slug.is_empty() {
        return Err(WikiPageError::InvalidDTag);
    }
    validate_repo_d(repo_d)?;
    if slug == WIKI_TOC_SLUG {
        return Ok(WikiDTag {
            repo_d: repo_d.to_string(),
            slug: slug.to_string(),
        });
    }
    validate_page_slug(slug)?;
    Ok(WikiDTag {
        repo_d: repo_d.to_string(),
        slug: slug.to_string(),
    })
}

/// Build `d` for a page or TOC.
pub fn wiki_d_tag(repo_d: &str, slug: &str) -> Result<String, WikiPageError> {
    validate_repo_d(repo_d)?;
    if slug != WIKI_TOC_SLUG {
        validate_page_slug(slug)?;
    }
    Ok(format!("{repo_d}/{slug}"))
}

/// `a` tag value `30617:<owner>:<repo-d>`.
pub fn wiki_repo_a_tag(owner_hex: &str, repo_d: &str) -> Result<String, WikiPageError> {
    if !is_hex64(owner_hex) {
        return Err(WikiPageError::InvalidOwner);
    }
    validate_repo_d(repo_d)?;
    Ok(format!(
        "{KIND_GIT_REPO_ANNOUNCEMENT}:{}:{repo_d}",
        owner_hex.to_ascii_lowercase()
    ))
}

/// Parse `30617:<owner>:<repo-d>`.
pub fn parse_wiki_repo_a_tag(a: &str) -> Result<(String, String), WikiPageError> {
    let mut parts = a.splitn(3, ':');
    let kind = parts.next().ok_or(WikiPageError::InvalidATag)?;
    let owner = parts.next().ok_or(WikiPageError::InvalidATag)?;
    let repo_d = parts.next().ok_or(WikiPageError::InvalidATag)?;
    if kind != KIND_GIT_REPO_ANNOUNCEMENT.to_string() {
        return Err(WikiPageError::InvalidATag);
    }
    if !is_hex64(owner) {
        return Err(WikiPageError::InvalidOwner);
    }
    validate_repo_d(repo_d)?;
    Ok((owner.to_ascii_lowercase(), repo_d.to_string()))
}

/// Envelope checks used by relay ingest (no I/O).
pub fn validate_wiki_page_envelope(
    kind: u32,
    tags: &[Vec<String>],
) -> Result<WikiDTag, WikiPageError> {
    if kind != KIND_REPO_WIKI_PAGE {
        return Err(WikiPageError::WrongKind);
    }
    let d = first_tag(tags, "d").ok_or(WikiPageError::MissingDTag)?;
    let parsed = parse_wiki_d_tag(d)?;
    let a = first_tag(tags, "a").ok_or(WikiPageError::MissingATag)?;
    let (_owner, repo_d) = parse_wiki_repo_a_tag(a)?;
    if repo_d != parsed.repo_d {
        return Err(WikiPageError::ATagRepoMismatch);
    }
    if !parsed.is_toc() {
        let commit = first_tag(tags, "commit").ok_or(WikiPageError::MissingCommit)?;
        if commit.is_empty() || commit.len() > 64 {
            return Err(WikiPageError::InvalidCommit);
        }
    }
    Ok(parsed)
}

/// Validation failures for wiki page / TOC events.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WikiPageError {
    /// Kind is not [`KIND_REPO_WIKI_PAGE`].
    #[error("wrong kind")]
    WrongKind,
    /// Missing or malformed `d` tag.
    #[error("invalid d tag")]
    InvalidDTag,
    /// Missing `d` tag.
    #[error("missing d tag")]
    MissingDTag,
    /// Missing `a` tag.
    #[error("missing a tag")]
    MissingATag,
    /// `a` tag is not a 30617 coordinate.
    #[error("invalid a tag")]
    InvalidATag,
    /// `a` repo-d does not match `d`.
    #[error("a tag repo does not match d")]
    ATagRepoMismatch,
    /// Owner pubkey is not 64 hex chars.
    #[error("invalid owner pubkey")]
    InvalidOwner,
    /// Page events need a commit hash tag.
    #[error("missing commit tag")]
    MissingCommit,
    /// Commit tag empty or too long.
    #[error("invalid commit tag")]
    InvalidCommit,
    /// Cadence tag is not one of the four values.
    #[error("invalid cadence")]
    InvalidCadence,
}

fn first_tag<'a>(tags: &'a [Vec<String>], name: &str) -> Option<&'a str> {
    tags.iter()
        .find(|t| t.first().map(String::as_str) == Some(name))
        .and_then(|t| t.get(1))
        .map(String::as_str)
}

fn validate_repo_d(repo_d: &str) -> Result<(), WikiPageError> {
    if repo_d.is_empty() || repo_d.len() > 64 {
        return Err(WikiPageError::InvalidDTag);
    }
    if repo_d.starts_with('.') || repo_d.contains("..") {
        return Err(WikiPageError::InvalidDTag);
    }
    if !repo_d
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(WikiPageError::InvalidDTag);
    }
    Ok(())
}

fn validate_page_slug(slug: &str) -> Result<(), WikiPageError> {
    if slug.is_empty() || slug.len() > 80 {
        return Err(WikiPageError::InvalidDTag);
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(WikiPageError::InvalidDTag);
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(WikiPageError::InvalidDTag);
    }
    Ok(())
}

fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_page_and_toc_d_tags() {
        let page = parse_wiki_d_tag("crew/overview").expect("page");
        assert_eq!(page.repo_d, "crew");
        assert_eq!(page.slug, "overview");
        assert!(!page.is_toc());
        let toc = parse_wiki_d_tag("crew/_toc").expect("toc");
        assert!(toc.is_toc());
    }

    #[test]
    fn envelope_requires_matching_a_tag() {
        let owner = "ab".repeat(32);
        let a = wiki_repo_a_tag(&owner, "crew").expect("a");
        let tags = vec![
            vec!["d".into(), "crew/overview".into()],
            vec!["a".into(), a],
            vec!["commit".into(), "abc123".into()],
        ];
        validate_wiki_page_envelope(KIND_REPO_WIKI_PAGE, &tags).expect("ok");
    }

    #[test]
    fn envelope_rejects_a_mismatch() {
        let owner = "ab".repeat(32);
        let a = wiki_repo_a_tag(&owner, "other").expect("a");
        let tags = vec![
            vec!["d".into(), "crew/overview".into()],
            vec!["a".into(), a],
            vec!["commit".into(), "abc123".into()],
        ];
        assert_eq!(
            validate_wiki_page_envelope(KIND_REPO_WIKI_PAGE, &tags),
            Err(WikiPageError::ATagRepoMismatch)
        );
    }
}
