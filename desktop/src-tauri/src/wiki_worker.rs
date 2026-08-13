//! Desktop-governed Crew Wiki generate worker.
//!
//! Caller-agnostic engine lives in `crew-wiki`. This command is the default
//! face: one generate per repo (lock), heuristic unless `CREW_WIKI_API_KEY`
//! is set on a build with the crate `llm` feature. Founder signing stays in JS
//! (D-028).

use crew_wiki::cadence::GenerateLock;
use crew_wiki::cluster::plan_pages;
use crew_wiki::generate::{generate_page, HeuristicGenerator};
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::{page_event_tags, toc_content, toc_event_tags, PageDraft, TocManifest};
use crew_wiki::steering::load_steering;
use serde::Serialize;
use std::path::{Path, PathBuf};
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
    /// True when the path is not a git repo or has no files.
    pub empty_repo: bool,
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

    let root = repo_path
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let cost_note = if std::env::var("CREW_WIKI_API_KEY").is_ok() {
        "OpenAI-compatible generator (CREW_WIKI_API_KEY)".to_string()
    } else {
        "Heuristic generator · no API key billed".to_string()
    };

    let snapshot = match RepoSnapshot::from_git(&root) {
        Ok(snapshot) => hydrate_contents(snapshot, &root),
        Err(_) => {
            return Ok(empty_outcome(&owner, &repo_d, cost_note));
        }
    };
    if snapshot.files.is_empty() {
        return Ok(empty_outcome(&owner, &repo_d, cost_note));
    }

    let steering = load_steering(&root);
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

fn hydrate_contents(mut snapshot: RepoSnapshot, root: &Path) -> RepoSnapshot {
    let paths = snapshot.files.clone();
    for path in paths.iter().take(80) {
        let body = snapshot.read(path, Some(root));
        if !body.is_empty() {
            snapshot.contents.insert(path.clone(), body);
        }
    }
    snapshot
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
        cost_note,
    }
}
