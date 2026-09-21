//! Which parts of one verified snapshot a question may be answered from.
//!
//! The question decides. A private Ask does not read the Wiki top to bottom
//! and call that grounding: retrieval scores every page of the *verified*
//! snapshot against the question's own terms, carries only the pages that
//! matched, reads only those pages' source references through the viewer's
//! grant, and records in a manifest what was taken and what was left. Nothing
//! here is a fallback: a question that matches nothing is a "not enough
//! source" outcome with the manifest to show for it, never the first page
//! pretending to be relevant.
//!
//! Everything in the manifest is evidence, not authority: the prompt assembler
//! may still trim it to fit the input bound (and moves what it drops into the
//! omitted lists, so the manifest always describes the prompt that was
//! actually built), and the citation fence still checks the answer's own
//! citations against the included sources.
//!
//! This module is pure. The `read` closure is the only way source bytes arrive
//! — the caller supplies the viewer-granted, hash-verified reader — so every
//! branch is exercisable without a folder, a repository or a relay.

use super::GroundedSource;
use crew_wiki::snapshot_v1::{VerifiedSnapshot, VerifiedSnapshotPage};
use crew_wiki::source_access::VerifiedSourceFile;
use crew_wiki::source_snapshot::SourceReference;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// How many question terms retrieval may carry. A question longer than the
/// bound is refused upstream; a question full of words still gets a bounded
/// working set so scoring stays proportional to the question.
const MAX_TERMS: usize = 24;

/// Pages one prompt may consult. Retrieval answers the asked question, so a
/// prompt carrying a quarter of the Wiki would be a read-all in disguise.
const MAX_PAGES: usize = 6;

/// One page's bytes inside the prompt. Larger pages are excerpted to their
/// matching windows instead of being carried whole.
const PAGE_BYTES: usize = 16 * 1024;

/// All page bytes combined. Retrieval's own bound, independent of the source
/// read budget, so two large matching pages cannot starve the sources that
/// evidence them.
const PAGES_BYTES: usize = 48 * 1024;

/// Source references read for one question. A page can name dozens of files;
/// this keeps the manifest-sized work proportional to a question, not to how
/// many files a page happened to reference.
const MAX_SOURCES: usize = 40;

/// Entries in each manifest list. The manifest is evidence shown to a viewer
/// and retained in history; it is bounded so a huge snapshot cannot write a
/// huge manifest.
const MANIFEST_LIMIT: usize = 64;

/// Bytes of context kept either side of a term hit when a page is excerpted.
const EXCERPT_RADIUS: usize = 1500;

/// Words that carry no retrieval signal. They are the connective tissue of a
/// question, not evidence about which page answers it.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "do", "does", "did",
    "doing", "what", "how", "why", "when", "where", "which", "who", "whom", "whose", "in", "on",
    "at", "to", "of", "for", "and", "or", "nor", "but", "if", "then", "else", "it", "its", "this",
    "that", "these", "those", "there", "here", "can", "could", "should", "would", "will", "shall",
    "may", "might", "must", "with", "without", "from", "by", "as", "about", "into", "through",
    "over", "under", "again", "further", "once", "any", "some", "no", "not", "only", "own", "same",
    "so", "than", "too", "very", "just", "now", "i", "you", "he", "she", "we", "they", "me", "him",
    "her", "us", "them", "my", "your", "his", "our", "their", "am",
];

/// Lowercase alphanumeric terms of at least two characters, minus stopwords.
///
/// Two characters is the floor: `go`, `os` and `fn` are real identifiers in
/// this codebase and a one-character term would score against everything.
/// Terms are matched by substring rather than word boundary so `answer` finds
/// `answers` and `answerer` — the question's own vocabulary stays in charge.
fn terms(question: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut terms = Vec::new();
    for raw in question.split(|c: char| !c.is_alphanumeric()) {
        let term = raw.to_lowercase();
        if term.len() < 2 || STOPWORDS.contains(&term.as_str()) {
            continue;
        }
        if seen.insert(term.clone()) {
            terms.push(term);
        }
        if terms.len() >= MAX_TERMS {
            break;
        }
    }
    terms
}

/// How strongly one page matches the question's terms.
///
/// Every term counts once per field — title, body and source paths — so a page
/// that repeats one term a thousand times does not outscore a page that hits
/// many of the question's terms.
fn score(terms: &[String], page: &VerifiedSnapshotPage) -> u32 {
    let title = page.title().to_lowercase();
    let body = page.content().to_lowercase();
    let paths = page
        .source_references()
        .iter()
        .map(|reference| reference.0.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    terms
        .iter()
        .map(|term| {
            // Title hits are the strongest signal — the page was named for
            // this; a source path hit means the page stands on the file the
            // question named; a body hit is the baseline.
            (title.contains(term) as u32) * 4
                + (paths.contains(term) as u32) * 2
                + body.contains(term) as u32
        })
        .sum()
}

/// The windows of `content` that hold term hits, merged and bounded.
///
/// `lower` is `content` lowercased, already computed by the scorer. Windows
/// are snapped to character boundaries: a multibyte character is never split
/// for the same reason a prompt delimiter must never be — the excerpt stays
/// text. When the windows cannot cover the question within the bound, the
/// earliest windows win; the manifest records the page as excerpted either
/// way.
fn excerpted(terms: &[String], lower: &str, content: &str) -> String {
    let mut windows: Vec<(usize, usize)> = Vec::new();
    for term in terms {
        let mut from = 0;
        while let Some(offset) = lower[from..].find(term.as_str()) {
            let start = from + offset;
            let end = start + term.len();
            windows.push((
                start.saturating_sub(EXCERPT_RADIUS),
                end.saturating_add(EXCERPT_RADIUS).min(content.len()),
            ));
            from = end;
            // Enough windows to cover the bound is enough searching.
            if windows.len() >= 64 {
                break;
            }
        }
    }
    windows.sort_unstable();
    // Merge overlapping windows, then keep the earliest until the page bound.
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in windows {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    let mut excerpt = String::new();
    let mut covered = 0;
    for (start, end) in merged {
        let room = PAGE_BYTES.saturating_sub(covered);
        if room == 0 {
            break;
        }
        let mut from = start;
        while from < end && !content.is_char_boundary(from) {
            from += 1;
        }
        let mut to = end.min(from + room);
        while to > from && !content.is_char_boundary(to) {
            to -= 1;
        }
        if to > from {
            excerpt.push_str(&content[from..to]);
            covered += to - from;
            if to < end {
                break;
            }
        }
    }
    excerpt
}

/// One page carried into the prompt: whole body, or the excerpted windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetrievedPage {
    pub(crate) slug: String,
    pub(crate) title: String,
    /// The page body, or its matching windows when the body exceeded the bound.
    pub(crate) content: String,
    /// True when `content` is an excerpt rather than the complete body — the
    /// prompt labels it so the model does not treat a window as the page.
    pub(crate) excerpted: bool,
}

/// One page in the manifest: which page, and how strongly the question hit it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ManifestPage {
    pub(crate) slug: String,
    pub(crate) title: String,
    /// The score the question's terms produced. Kept so a viewer can see why a
    /// page was taken and so a zero cannot masquerade as a reason.
    pub(crate) score: u32,
}

/// One source range in the manifest: where it lives, nothing more.
///
/// The bytes themselves are grounding or omitted evidence, not manifest
/// content — the manifest names the coverage decision, it does not copy the
/// source into the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ManifestSource {
    pub(crate) path: String,
    pub(crate) start_line: u64,
    pub(crate) end_line: u64,
}

impl From<&SourceReference> for ManifestSource {
    fn from(reference: &SourceReference) -> Self {
        Self {
            path: reference.0.clone(),
            start_line: reference.3,
            end_line: reference.4,
        }
    }
}

impl From<&GroundedSource> for ManifestSource {
    fn from(source: &GroundedSource) -> Self {
        Self {
            path: source.path().to_owned(),
            start_line: source.start_line(),
            end_line: source.end_line(),
        }
    }
}

/// The retrieval record: what the prompt consulted and what it did not.
///
/// `included` is what reached the request (after the prompt bound's own trim
/// moves its casualties into `omitted`); `omitted` is what the question hit
/// but the bounds dropped. Both lists are bounded. `source_grant` records
/// whether a viewer source grant existed at all — without one there are no
/// included sources to speak of, which is a statement about the grant, not
/// about the snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RetrievalManifest {
    pub(crate) included_pages: Vec<ManifestPage>,
    pub(crate) omitted_pages: Vec<ManifestPage>,
    pub(crate) included_sources: Vec<ManifestSource>,
    pub(crate) omitted_sources: Vec<ManifestSource>,
    /// Whether source reads were attempted under a viewer grant.
    pub(crate) source_grant: bool,
}

/// What one question retrieved from one verified snapshot.
///
/// `grounding` is the citeable source — every entry already byte-verified by
/// [`GroundedSource::from_verified_snapshot`]. `pages` is the matching page
/// context. `manifest` is the coverage record that travels with the answer and
/// into history, so a viewer can see the "not enough source" boundary as well
/// as the evidence.
pub(crate) struct Retrieval {
    pub(crate) pages: Vec<RetrievedPage>,
    pub(crate) grounding: Vec<GroundedSource>,
    pub(crate) manifest: RetrievalManifest,
}

impl Retrieval {
    /// Whether the question found anything to answer from.
    ///
    /// A question with no matching page and no grounded source is the honest
    /// "not enough source" outcome: an answer without either would be the
    /// model inventing, which is exactly what a grounded Ask exists to
    /// prevent. The manifest still records the coverage so the outcome can be
    /// inspected.
    pub(crate) fn insufficient(&self) -> bool {
        self.pages.is_empty() && self.grounding.is_empty()
    }
}

/// Retrieve the relevant slice of one verified snapshot for one question.
///
/// `workspace_mode` is `Some(mode)` only when the viewer has a live source
/// grant for this repository; without it no source bytes are read at all, and
/// the manifest says so. `deadline` bounds the whole source-read walk — the
/// same bound the renderer-facing read uses — and `read` is the verified
/// reader. This function never fails: every individual miss lands in
/// `omitted`, and the shape of the result is what `insufficient` reads.
pub(crate) fn retrieve<E>(
    question: &str,
    snapshot: &VerifiedSnapshot,
    workspace_mode: Option<&str>,
    deadline: Instant,
    read: impl Fn(&str, &SourceReference) -> Result<VerifiedSourceFile, E>,
) -> Retrieval {
    let terms = terms(question);
    let mut manifest = RetrievalManifest {
        source_grant: workspace_mode.is_some(),
        ..RetrievalManifest::default()
    };
    if terms.is_empty() {
        return Retrieval {
            pages: Vec::new(),
            grounding: Vec::new(),
            manifest,
        };
    }

    // Score every page, take the matching prefix in score order. Snapshot order
    // is the tie-break, so two equal questions see the same cut.
    let mut scored: Vec<(u32, usize)> = snapshot
        .pages()
        .iter()
        .enumerate()
        .map(|(index, page)| (score(&terms, page), index))
        .filter(|(score, _)| *score > 0)
        .collect();
    scored.sort_by_key(|(score, index)| (std::cmp::Reverse(*score), *index));

    let mut pages = Vec::new();
    let mut included_indices: Vec<usize> = Vec::new();
    let mut page_bytes = 0usize;
    for &(page_score, index) in &scored {
        let page = &snapshot.pages()[index];
        let slug = page.slug().to_owned();
        let title = page.title().to_owned();
        let entry = |score| ManifestPage {
            slug: slug.clone(),
            title: title.clone(),
            score,
        };
        if pages.len() >= MAX_PAGES {
            push_bounded(&mut manifest.omitted_pages, entry(page_score));
            continue;
        }
        let body = page.content();
        let (content, excerpted) = if body.len() <= PAGE_BYTES {
            (body.to_owned(), false)
        } else {
            (excerpted(&terms, &body.to_lowercase(), body), true)
        };
        if content.is_empty()
            || page_bytes
                .checked_add(content.len())
                .is_none_or(|total| total > PAGES_BYTES)
        {
            push_bounded(&mut manifest.omitted_pages, entry(page_score));
            continue;
        }
        page_bytes += content.len();
        included_indices.push(index);
        manifest.included_pages.push(entry(page_score));
        pages.push(RetrievedPage {
            slug,
            title,
            content,
            excerpted,
        });
    }

    // Sources come only from included pages, in the same order, and only under
    // a grant whose checkout mode matches the snapshot's revision. A grant
    // anchored to a different mode reads bytes the snapshot never signed for.
    let mut budget = super::PRIVATE_ASK_INPUT_LIMIT;
    let mut reads = 0usize;
    let mut grounding = Vec::new();
    let grant = workspace_mode.is_some_and(|mode| {
        snapshot
            .index()
            .source_revision()
            .starts_with(&format!("{mode}:"))
    });
    let revision = snapshot.index().source_revision();
    for index in included_indices {
        let page = &snapshot.pages()[index];
        for reference in page.source_references() {
            // An identical range on a second page is still one source to read
            // and one citation target, so the reference sets are deduplicated.
            if grounding.iter().any(|source: &GroundedSource| {
                source.path() == reference.0
                    && source.start_line() == reference.3
                    && source.end_line() == reference.4
            }) || manifest.omitted_sources.iter().any(|source| {
                source.path == reference.0
                    && source.start_line == reference.3
                    && source.end_line == reference.4
            }) {
                continue;
            }
            // The reference carries the byte count the hash was taken over —
            // the read budget can be decided before any bytes move.
            if !grant
                || reads >= MAX_SOURCES
                || reference.2 as usize > budget
                || Instant::now() >= deadline
            {
                push_bounded(&mut manifest.omitted_sources, reference.into());
                continue;
            }
            reads += 1;
            let Ok(file) = read(revision, reference) else {
                push_bounded(&mut manifest.omitted_sources, reference.into());
                continue;
            };
            let Ok(source) =
                GroundedSource::from_verified_snapshot(snapshot, page, reference, &file)
            else {
                push_bounded(&mut manifest.omitted_sources, reference.into());
                continue;
            };
            budget = budget.saturating_sub(source.content_len());
            manifest.included_sources.push((&source).into());
            grounding.push(source);
        }
    }

    Retrieval {
        pages,
        grounding,
        manifest,
    }
}

/// Push one manifest entry inside its bound. Entries past the bound are the
/// ones nobody could act on anyway — the manifest stays a summary a viewer can
/// actually read.
fn push_bounded<T>(list: &mut Vec<T>, entry: T) {
    if list.len() < MANIFEST_LIMIT {
        list.push(entry);
    }
}

#[cfg(test)]
#[path = "retrieval_tests.rs"]
mod tests;
