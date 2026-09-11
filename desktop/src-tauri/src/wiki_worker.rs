//! Desktop-governed Crew Wiki generate worker.
//!
//! Caller-agnostic engine lives in `crew-wiki`. This command is the default
//! face: one generate per repo (lock), heuristic unless `CREW_WIKI_API_KEY`
//! is set on a build with the crate `llm` feature. The legacy preview command
//! returns unsigned drafts; durable publication signs and journals the complete
//! snapshot in the native Wiki publication command.

use crew_wiki::cadence::GenerateLock;
use crew_wiki::cluster::plan_pages;
use crew_wiki::generate::{generate_page, HeuristicGenerator};
use crew_wiki::generate_root::{
    classify_from_git_failure, resolve_wiki_generate_root, WikiGenerateRoot, WikiLocalSnapshotError,
};
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::{page_event_tags, toc_content, toc_event_tags, PageDraft, TocManifest};
use crew_wiki::source_folder::capture_folder;
use crew_wiki::steering::load_captured_steering;
use crew_wiki::types::WikiPlan;
use serde::Serialize;
use std::sync::OnceLock;

pub(crate) fn generate_lock() -> &'static GenerateLock {
    static LOCK: OnceLock<GenerateLock> = OnceLock::new();
    LOCK.get_or_init(GenerateLock::default)
}

/// One unsigned page the renderer signs and publishes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiDraftDto {
    /// Page slug.
    pub slug: String,
    /// Title.
    pub title: String,
    /// Section id.
    pub section: String,
    /// Source files.
    pub source_files: Vec<String>,
    /// Commit.
    pub commit: String,
    /// Language.
    pub language: String,
    /// Markdown body.
    pub content: String,
    /// Event tags ready to sign.
    pub tags: Vec<Vec<String>>,
}

/// Outcome returned to the renderer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiGenerateOutcome {
    /// Governance accepted the job.
    pub accepted: bool,
    /// Planned page count.
    pub pages: usize,
    /// Snapshot commit, if a git tree was readable.
    pub commit: String,
    /// Default branch.
    pub branch: String,
    /// TOC JSON.
    pub toc_content: String,
    /// TOC tags.
    pub toc_tags: Vec<Vec<String>>,
    /// Pages to sign (empty when the tree has no source files).
    pub drafts: Vec<WikiDraftDto>,
    /// True when a bound local git tree has no files / no HEAD.
    pub empty_repo: bool,
    /// True when `repo_path` is unset, gone, or not a git worktree.
    pub missing_local_path: bool,
    /// Cost note shown in the library (no silent generation).
    pub cost_note: String,
}

/// Captured source, plan, and generated drafts used by native publication.
/// The snapshot is retained in memory only until the signed graph is built;
/// all events are then persisted in the owner-operation journal.
pub(crate) struct WikiGeneration {
    pub(crate) snapshot: RepoSnapshot,
    pub(crate) plan: WikiPlan,
    pub(crate) drafts: Vec<PageDraft>,
}

/// Closed source admission outcomes used by both preview and native prepare.
#[derive(Debug)]
pub(crate) enum WikiGenerationError {
    MissingLocalPath,
    EmptyTree,
    Failed(String),
}

impl std::fmt::Display for WikiGenerationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLocalPath => {
                formatter.write_str("Source workspace is missing or not a directory.")
            }
            Self::EmptyTree => formatter.write_str("Source workspace is an empty repository."),
            Self::Failed(error) => formatter.write_str(error),
        }
    }
}

/// Capture one explicitly selected workspace and generate a deterministic
/// page set. This is shared by the legacy preview command and native writer so
/// publication cannot silently use a second generation implementation.
pub(crate) fn generate_wiki_pages(
    owner: &str,
    repo_d: &str,
    repo_path: Option<&str>,
    workspace_mode: Option<&str>,
) -> Result<WikiGeneration, WikiGenerationError> {
    let root = match resolve_wiki_generate_root(repo_path) {
        WikiGenerateRoot::MissingLocalPath => return Err(WikiGenerationError::MissingLocalPath),
        WikiGenerateRoot::Ready(root) => root,
    };
    let coordinate = format!("30617:{owner}:{repo_d}");
    let snapshot = match workspace_mode {
        Some("folder") => capture_folder(&root, &coordinate)
            .map_err(|error| WikiGenerationError::Failed(error.to_string()))?,
        Some("git") | None => RepoSnapshot::from_git(&root).map_err(|error| {
            match classify_from_git_failure(&root, &error) {
                WikiLocalSnapshotError::MissingLocalPath => WikiGenerationError::MissingLocalPath,
                WikiLocalSnapshotError::EmptyTree => WikiGenerationError::EmptyTree,
                WikiLocalSnapshotError::CaptureFailed => {
                    WikiGenerationError::Failed(error.to_string())
                }
            }
        })?,
        Some(_) => {
            return Err(WikiGenerationError::Failed(
                "Unsupported Wiki workspace mode.".into(),
            ))
        }
    };
    if snapshot.is_empty_tree() {
        return Err(WikiGenerationError::EmptyTree);
    }
    if snapshot.files.is_empty() {
        return Err(WikiGenerationError::Failed(format!(
            "Source coverage unavailable: no supported source files ({} omitted paths)",
            snapshot.omissions.len()
        )));
    }
    let steering = load_captured_steering(&snapshot)
        .map_err(|error| WikiGenerationError::Failed(error.to_string()))?;
    let plan = plan_pages(&snapshot, steering.as_ref())
        .map_err(|error| WikiGenerationError::Failed(error.to_string()))?;
    let generator = HeuristicGenerator;
    let mut drafts = Vec::new();
    for section in &plan.sections {
        for page in &section.pages {
            drafts.push(
                generate_page(&generator, page, &snapshot, &plan.language)
                    .map_err(|error| WikiGenerationError::Failed(error.to_string()))?,
            );
        }
    }
    if drafts.is_empty() {
        return Err(WikiGenerationError::Failed(
            "Source coverage unavailable: no Wiki pages were generated.".into(),
        ));
    }
    Ok(WikiGeneration {
        snapshot,
        plan,
        drafts,
    })
}

/// Run `crew-wiki generate` for a repository coordinate.
#[tauri::command]
pub async fn wiki_generate(
    owner: String,
    repo_d: String,
    repo_path: Option<String>,
) -> Result<WikiGenerateOutcome, String> {
    let key = format!("{owner}:{repo_d}");
    let _guard = generate_lock()
        .acquire(&key)
        .map_err(|err| err.to_string())?;

    let cost_note = if std::env::var("CREW_WIKI_API_KEY").is_ok() {
        "OpenAI-compatible generator (CREW_WIKI_API_KEY)".to_string()
    } else {
        "Heuristic generator · no API key billed".to_string()
    };

    let generation = match generate_wiki_pages(&owner, &repo_d, repo_path.as_deref(), Some("git")) {
        Ok(generation) => generation,
        Err(WikiGenerationError::MissingLocalPath) => {
            return Ok(missing_local_outcome(&owner, &repo_d, cost_note));
        }
        Err(WikiGenerationError::EmptyTree) => {
            return Ok(empty_outcome(&owner, &repo_d, cost_note));
        }
        Err(WikiGenerationError::Failed(error)) => return Err(error),
    };
    let snapshot = generation.snapshot;
    let plan = generation.plan;
    let drafts = generation
        .drafts
        .into_iter()
        .map(|draft| {
            let tags = page_event_tags(&owner, &repo_d, &draft)?;
            Ok(dto_from_draft(draft, tags))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let manifest = TocManifest {
        sections: plan.sections.clone(),
        commit: snapshot.commit.clone(),
        branch: snapshot.branch.clone(),
        cadence: "manual".into(),
        generated_at: 0,
    };
    let toc_tags = toc_event_tags(&owner, &repo_d, &manifest)?;
    Ok(WikiGenerateOutcome {
        accepted: true,
        pages: drafts.len(),
        commit: snapshot.commit,
        branch: snapshot.branch,
        toc_content: toc_content(&manifest),
        toc_tags,
        drafts,
        empty_repo: false,
        missing_local_path: false,
        cost_note,
    })
}

fn dto_from_draft(draft: PageDraft, tags: Vec<Vec<String>>) -> WikiDraftDto {
    WikiDraftDto {
        slug: draft.slug,
        title: draft.title,
        section: draft.section,
        source_files: draft.source_files,
        commit: draft.commit,
        language: draft.language,
        content: draft.content,
        tags,
    }
}

fn empty_outcome(owner: &str, repo_d: &str, cost_note: String) -> WikiGenerateOutcome {
    let manifest = TocManifest {
        sections: Vec::new(),
        commit: String::new(),
        branch: "main".into(),
        cadence: "manual".into(),
        generated_at: 0,
    };
    let toc_tags = toc_event_tags(owner, repo_d, &manifest).unwrap_or_default();
    WikiGenerateOutcome {
        accepted: true,
        pages: 0,
        commit: String::new(),
        branch: "main".into(),
        toc_content: toc_content(&manifest),
        toc_tags,
        drafts: Vec::new(),
        empty_repo: true,
        missing_local_path: false,
        cost_note,
    }
}

fn missing_local_outcome(owner: &str, repo_d: &str, cost_note: String) -> WikiGenerateOutcome {
    let mut outcome = empty_outcome(owner, repo_d, cost_note);
    outcome.empty_repo = false;
    outcome.missing_local_path = true;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[tokio::test]
    async fn wiki_generate_reports_a_bound_non_git_directory_as_missing_local() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let outcome = wiki_generate(
            "ab".repeat(32),
            "crew".to_owned(),
            Some(directory.path().display().to_string()),
        )
        .await
        .expect("typed missing-local outcome");

        assert!(outcome.missing_local_path);
        assert!(!outcome.empty_repo);
        assert_eq!(outcome.pages, 0);
    }

    #[tokio::test]
    async fn wiki_generate_reports_an_empty_git_tree_as_empty_repo() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let status = Command::new("git")
            .args(["init"])
            .current_dir(directory.path())
            .status()
            .expect("git init");
        assert!(status.success());

        let outcome = wiki_generate(
            "cd".repeat(32),
            "empty".to_owned(),
            Some(directory.path().display().to_string()),
        )
        .await
        .expect("typed empty-repo outcome");

        assert!(outcome.empty_repo);
        assert!(!outcome.missing_local_path);
        assert_eq!(outcome.pages, 0);
    }

    #[tokio::test]
    async fn wiki_generate_propagates_invalid_committed_steering() {
        let directory = tempfile::tempdir().expect("temporary directory");
        for args in [
            &["init"][..],
            &["config", "user.email", "crew-wiki-tests@example.invalid"][..],
            &["config", "user.name", "Crew Wiki Tests"][..],
        ] {
            let status = Command::new("git")
                .args(args)
                .current_dir(directory.path())
                .status()
                .expect("git command");
            assert!(status.success(), "git command failed: {args:?}");
        }
        std::fs::create_dir_all(directory.path().join(".crew")).expect("steering directory");
        std::fs::write(directory.path().join(".crew/wiki.json"), "{ invalid")
            .expect("invalid steering");
        for args in [
            &["add", "."][..],
            &["commit", "--quiet", "-m", "invalid steering"][..],
        ] {
            let status = Command::new("git")
                .args(args)
                .current_dir(directory.path())
                .status()
                .expect("git command");
            assert!(status.success(), "git command failed: {args:?}");
        }

        let error = wiki_generate(
            "ef".repeat(32),
            "invalid-steering".to_owned(),
            Some(directory.path().display().to_string()),
        )
        .await
        .expect_err("invalid committed steering must not become an empty success");
        assert!(
            error.contains("invalid steering file"),
            "unexpected worker error: {error}"
        );
    }
}
