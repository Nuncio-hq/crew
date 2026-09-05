//! Event builders and publish idempotency.

use crate::types::PlannedSection;
use buzz_core::kind::KIND_REPO_WIKI_PAGE;
use buzz_core::wiki_page::{wiki_d_tag, wiki_repo_a_tag, WIKI_TOC_SLUG};
use serde::{Deserialize, Serialize};

/// One generated (or previously published) page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageDraft {
    /// Page slug.
    pub slug: String,
    /// Title.
    pub title: String,
    /// Section id.
    pub section: String,
    /// Source files.
    pub source_files: Vec<String>,
    /// Commit this draft was generated from.
    pub commit: String,
    /// Language.
    pub language: String,
    /// Markdown body.
    pub content: String,
}

/// TOC JSON stored as event content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocManifest {
    /// Two-level tree.
    pub sections: Vec<PlannedSection>,
    /// Commit.
    pub commit: String,
    /// Branch (default branch).
    pub branch: String,
    /// Cadence wire value.
    pub cadence: String,
    /// Unix timestamp.
    pub generated_at: i64,
}

/// Unsigned event payload ready to sign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishBatch {
    /// Kind (always wiki page).
    pub kind: u32,
    /// TOC plus pages that actually changed.
    pub events: Vec<UnsignedWikiEvent>,
}

/// One unsigned wiki event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsignedWikiEvent {
    /// Markdown or TOC JSON.
    pub content: String,
    /// Tags.
    pub tags: Vec<Vec<String>>,
}

/// Pages whose content+commit differ from the previously published set.
/// Re-run without changes publishes nothing.
pub fn pages_to_publish(new_pages: &[PageDraft], existing: &[PageDraft]) -> Vec<PageDraft> {
    new_pages
        .iter()
        .filter(|page| {
            !existing.iter().any(|old| {
                old.slug == page.slug
                    && old.commit == page.commit
                    && old.content == page.content
                    && old.source_files == page.source_files
            })
        })
        .cloned()
        .collect()
}

/// TOC event content (JSON).
pub fn toc_content(manifest: &TocManifest) -> String {
    serde_json::to_string(manifest).unwrap_or_else(|_| "{}".into())
}

/// Tags for a page event.
pub fn page_event_tags(
    owner_hex: &str,
    repo_d: &str,
    page: &PageDraft,
) -> Result<Vec<Vec<String>>, String> {
    let d = wiki_d_tag(repo_d, &page.slug).map_err(|e| e.to_string())?;
    let a = wiki_repo_a_tag(owner_hex, repo_d).map_err(|e| e.to_string())?;
    let mut tags = vec![
        vec!["d".into(), d],
        vec!["a".into(), a],
        vec!["title".into(), page.title.clone()],
        vec!["commit".into(), page.commit.clone()],
        vec!["section".into(), page.section.clone()],
        vec!["language".into(), page.language.clone()],
    ];
    for file in &page.source_files {
        tags.push(vec!["source".into(), file.clone()]);
    }
    let _ = KIND_REPO_WIKI_PAGE;
    Ok(tags)
}

/// Tags for the TOC manifest event.
pub fn toc_event_tags(
    owner_hex: &str,
    repo_d: &str,
    manifest: &TocManifest,
) -> Result<Vec<Vec<String>>, String> {
    let d = wiki_d_tag(repo_d, WIKI_TOC_SLUG).map_err(|e| e.to_string())?;
    let a = wiki_repo_a_tag(owner_hex, repo_d).map_err(|e| e.to_string())?;
    Ok(vec![
        vec!["d".into(), d],
        vec!["a".into(), a],
        vec!["commit".into(), manifest.commit.clone()],
        vec!["branch".into(), manifest.branch.clone()],
        vec!["cadence".into(), manifest.cadence.clone()],
        vec!["title".into(), "Wiki".into()],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(commit: &str, content: &str) -> PageDraft {
        PageDraft {
            slug: "overview".into(),
            title: "Overview".into(),
            section: "overview".into(),
            source_files: vec!["README.md".into()],
            commit: commit.into(),
            language: "en".into(),
            content: content.into(),
        }
    }

    #[test]
    fn idempotent_when_unchanged() {
        let page = draft("abc", "hello");
        let publish = pages_to_publish(std::slice::from_ref(&page), std::slice::from_ref(&page));
        assert!(publish.is_empty());
    }

    #[test]
    fn publishes_when_commit_changes() {
        let old = draft("aaa", "hello");
        let new = draft("bbb", "hello");
        let publish = pages_to_publish(&[new], &[old]);
        assert_eq!(publish.len(), 1);
    }
}
