//! Desktop-governed Crew Wiki generate worker.
//!
//! Caller-agnostic engine lives in `crew-wiki`. This command is the default
//! face: one generate per repo (lock), heuristic unless `CREW_WIKI_API_KEY`
//! is set on a build with the crate `llm` feature. Founder signing stays in JS
//! (D-028).

use crew_wiki::cadence::GenerateLock;
use crew_wiki::cluster::plan_pages;
use crew_wiki::generate::{generate_page, HeuristicGenerator};
use crew_wiki::generate_root::{
    classify_from_git_failure, resolve_wiki_generate_root, WikiGenerateRoot, WikiLocalSnapshotError,
};
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::{page_event_tags, toc_content, toc_event_tags, PageDraft, TocManifest};
use crew_wiki::steering::load_captured_steering;
use serde::Serialize;
use std::sync::OnceLock;

fn generate_lock() -> &'static GenerateLock {
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

    let root = match resolve_wiki_generate_root(repo_path.as_deref()) {
        WikiGenerateRoot::MissingLocalPath => {
            return Ok(missing_local_outcome(&owner, &repo_d, cost_note));
        }
        WikiGenerateRoot::Ready(root) => root,
    };

    let snapshot = match RepoSnapshot::from_git(&root) {
        Ok(snapshot) if snapshot.is_empty_tree() => {
            return Ok(empty_outcome(&owner, &repo_d, cost_note));
        }
        Ok(snapshot) if snapshot.files.is_empty() => {
            return Err(format!(
                "Source coverage unavailable: no supported source files ({} omitted paths)",
                snapshot.omissions.len()
            ));
        }
        Ok(snapshot) => snapshot,
        Err(err) => {
            return Ok(
                match classify_from_git_failure(&root, &err.to_string(), true) {
                    WikiLocalSnapshotError::MissingLocalPath => {
                        missing_local_outcome(&owner, &repo_d, cost_note)
                    }
                    WikiLocalSnapshotError::EmptyTree => empty_outcome(&owner, &repo_d, cost_note),
                },
            );
        }
    };

    let steering = load_captured_steering(&snapshot).map_err(|err| err.to_string())?;
    let plan = plan_pages(&snapshot, steering.as_ref()).map_err(|err| err.to_string())?;
    let generator = HeuristicGenerator;
    let mut drafts = Vec::new();
    for section in &plan.sections {
        for page in &section.pages {
            let draft = generate_page(&generator, page, &snapshot, &plan.language)
                .map_err(|err| err.to_string())?;
            let tags = page_event_tags(&owner, &repo_d, &draft)?;
            drafts.push(dto_from_draft(draft, tags));
        }
    }
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
}
