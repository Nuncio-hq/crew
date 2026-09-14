//! Desktop-governed Crew Wiki generate worker.
//!
//! Caller-agnostic engine lives in `crew-wiki`. The legacy preview command
//! retains its deterministic heuristic generator for compatibility; native
//! callers can pass an explicit [`WikiRuntimeSelection`] to use a bounded
//! installed runtime. Selecting a runtime never falls back to the heuristic.

use crate::managed_agents::wiki_runtime::{WikiRuntimeGenerator, WikiRuntimeSelection};
use crew_wiki::cadence::GenerateLock;
use crew_wiki::cluster::plan_pages;
use crew_wiki::generate::{generate_page, Generator, HeuristicGenerator};
use crew_wiki::generate_root::{
    classify_from_git_failure, resolve_wiki_generate_root, WikiGenerateRoot, WikiLocalSnapshotError,
};
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::{page_event_tags, toc_content, toc_event_tags, PageDraft, TocManifest};
use crew_wiki::snapshot_v1::SnapshotManifest;
use crew_wiki::snapshot_v1_build::SnapshotPublication;
use crew_wiki::source_folder::capture_folder;
use crew_wiki::source_snapshot::{source_hash, SourceReference};
use crew_wiki::steering::load_captured_steering;
use crew_wiki::types::WikiPlan;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Maximum wall-clock time for one complete native Wiki generation job.
/// Individual runtime requests remain bounded by the adapter's 180-second
/// process deadline; this outer budget prevents a large page plan from
/// extending a job indefinitely.
pub(crate) const WIKI_RUNTIME_JOB_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub(crate) fn generate_lock() -> &'static GenerateLock {
    static LOCK: OnceLock<GenerateLock> = OnceLock::new();
    LOCK.get_or_init(GenerateLock::default)
}

fn generation_cancels() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register one owner/community/repository generation for explicit cancel.
pub(crate) fn begin_generation_cancel(key: &str) -> Result<Arc<AtomicBool>, String> {
    let mut cancels = generation_cancels()
        .lock()
        .map_err(|_| "Wiki generation cancellation registry unavailable.".to_string())?;
    if cancels.contains_key(key) {
        return Err("Wiki generation is already running for this repository.".to_string());
    }
    let token = Arc::new(AtomicBool::new(false));
    cancels.insert(key.to_owned(), token.clone());
    Ok(token)
}

/// Request cancellation of the currently running generation for `key`.
/// Returns false when no foreground generation owns that key.
pub(crate) fn cancel_generation(key: &str) -> bool {
    generation_cancels()
        .lock()
        .ok()
        .and_then(|cancels| cancels.get(key).cloned())
        .is_some_and(|token| {
            token.store(true, Ordering::Release);
            true
        })
}

/// True only while this process owns the exact generation registration.
pub(crate) fn generation_is_active(key: &str) -> Result<bool, String> {
    generation_cancels()
        .lock()
        .map(|cancels| cancels.contains_key(key))
        .map_err(|_| "Wiki generation cancellation registry unavailable.".to_string())
}

/// Remove a completed generation without clearing a newer replacement.
pub(crate) fn finish_generation_cancel(key: &str, token: &Arc<AtomicBool>) {
    if let Ok(mut cancels) = generation_cancels().lock() {
        if cancels
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            cancels.remove(key);
        }
    }
}

pub(crate) fn generation_cancel_key(
    community: &str,
    coordinate: &str,
    operation_id: &str,
    revision: u64,
) -> String {
    format!("{community}\u{0}{coordinate}\u{0}{operation_id}\u{0}{revision}")
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
    generate_wiki_pages_with_runtime(owner, repo_d, repo_path, workspace_mode, None)
}

/// Capture and generate using an explicitly selected installed runtime.
///
/// `None` is reserved for the unsigned legacy preview surface. A native
/// publication caller that supplies a selection receives a hard failure when
/// the executable, profile, output or process contract is unavailable.
pub(crate) fn generate_wiki_pages_with_runtime(
    owner: &str,
    repo_d: &str,
    repo_path: Option<&str>,
    workspace_mode: Option<&str>,
    runtime_selection: Option<WikiRuntimeSelection>,
) -> Result<WikiGeneration, WikiGenerationError> {
    generate_wiki_pages_with_runtime_and_cancel(
        owner,
        repo_d,
        repo_path,
        workspace_mode,
        runtime_selection,
        None,
    )
}

/// Capture and generate with an explicit cancellation flag owned by the
/// foreground native job. Legacy preview callers use the wrapper above.
pub(crate) fn generate_wiki_pages_with_runtime_and_cancel(
    owner: &str,
    repo_d: &str,
    repo_path: Option<&str>,
    workspace_mode: Option<&str>,
    runtime_selection: Option<WikiRuntimeSelection>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<WikiGeneration, WikiGenerationError> {
    generate_wiki_pages_with_runtime_and_cancel_and_previous(
        owner,
        repo_d,
        repo_path,
        workspace_mode,
        runtime_selection,
        cancel,
        None,
    )
}

/// Capture and generate while reusing pages from one fully verified previous
/// publication. The previous graph is only a source of immutable content; the
/// new snapshot and plan still bind every event that the caller eventually
/// signs.
pub(crate) fn generate_wiki_pages_with_runtime_and_cancel_and_previous(
    owner: &str,
    repo_d: &str,
    repo_path: Option<&str>,
    workspace_mode: Option<&str>,
    runtime_selection: Option<WikiRuntimeSelection>,
    cancel: Option<Arc<AtomicBool>>,
    previous: Option<&SnapshotPublication>,
) -> Result<WikiGeneration, WikiGenerationError> {
    let started = Instant::now();
    check_generation_state(started, cancel.as_ref())?;
    let (snapshot, plan) = capture_wiki_source(
        owner,
        repo_d,
        repo_path,
        workspace_mode,
        cancel.as_ref(),
        started,
    )?;
    let runtime_cancel = cancel.clone();
    let drafts = generate_planned_pages(
        &snapshot,
        &plan,
        previous,
        cancel.as_ref(),
        started,
        move || {
            let generator: Box<dyn Generator> = match runtime_selection {
                Some(selection) => Box::new(
                    WikiRuntimeGenerator::installed_with_cancel(
                        selection,
                        runtime_cancel
                            .clone()
                            .unwrap_or_else(|| Arc::new(AtomicBool::new(false))),
                    )
                    .map_err(|error| WikiGenerationError::Failed(error.to_string()))?,
                ),
                None => Box::new(HeuristicGenerator),
            };
            Ok(generator)
        },
    )?;
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

fn check_generation_state(
    started: Instant,
    cancel: Option<&Arc<AtomicBool>>,
) -> Result<(), WikiGenerationError> {
    if cancel.is_some_and(|token| token.load(Ordering::Acquire)) {
        return Err(WikiGenerationError::Failed(
            "Wiki generation was canceled.".into(),
        ));
    }
    if started.elapsed() >= WIKI_RUNTIME_JOB_TIMEOUT {
        return Err(WikiGenerationError::Failed(
            "Wiki generation exceeded its job time limit.".into(),
        ));
    }
    Ok(())
}

fn capture_wiki_source(
    owner: &str,
    repo_d: &str,
    repo_path: Option<&str>,
    workspace_mode: Option<&str>,
    cancel: Option<&Arc<AtomicBool>>,
    started: Instant,
) -> Result<(RepoSnapshot, WikiPlan), WikiGenerationError> {
    check_generation_state(started, cancel)?;
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
    check_generation_state(started, cancel)?;
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
    check_generation_state(started, cancel)?;
    Ok((snapshot, plan))
}

/// Generate the pages for one captured plan, constructing the runtime lazily.
/// This is the production counting seam for incremental generation: unchanged
/// pages are copied only after their verified source and metadata identities
/// match, so a zero-call run never constructs or invokes the selected runtime.
fn generate_planned_pages<F>(
    snapshot: &RepoSnapshot,
    plan: &WikiPlan,
    previous: Option<&SnapshotPublication>,
    cancel: Option<&Arc<AtomicBool>>,
    started: Instant,
    make_generator: F,
) -> Result<Vec<PageDraft>, WikiGenerationError>
where
    F: FnOnce() -> Result<Box<dyn Generator>, WikiGenerationError>,
{
    let reusable = previous
        .map(|publication| reusable_drafts(snapshot, plan, publication))
        .unwrap_or_default();
    let mut drafts = Vec::new();
    let mut generator: Option<Box<dyn Generator>> = None;
    let mut make_generator = Some(make_generator);
    for section in &plan.sections {
        for page in &section.pages {
            check_generation_state(started, cancel)?;
            if let Some(draft) = reusable.get(&page.slug) {
                drafts.push(draft.clone());
                continue;
            }
            if generator.is_none() {
                let factory = make_generator.take().ok_or_else(|| {
                    WikiGenerationError::Failed("Wiki generator unavailable.".into())
                })?;
                generator = Some(factory()?);
            }
            let generator = generator
                .as_deref()
                .ok_or_else(|| WikiGenerationError::Failed("Wiki generator unavailable.".into()))?;
            drafts.push(
                generate_page(generator, page, snapshot, &plan.language)
                    .map_err(|error| WikiGenerationError::Failed(error.to_string()))?,
            );
        }
    }
    Ok(drafts)
}

/// Reconstruct drafts only from a publication that has already passed the
/// native complete-graph verifier. A mismatch is simply non-reusable and is
/// sent through the selected generator; it never becomes an unverified cache.
fn reusable_drafts(
    snapshot: &RepoSnapshot,
    plan: &WikiPlan,
    previous: &SnapshotPublication,
) -> BTreeMap<String, PageDraft> {
    let Ok(manifest) = serde_json::from_str::<SnapshotManifest>(&previous.manifest.content) else {
        return BTreeMap::new();
    };
    if !steering_identity_matches(snapshot, previous) {
        return BTreeMap::new();
    }
    let source_kind = if snapshot.source_revision.starts_with("folder:") {
        "folder"
    } else {
        "git"
    };
    let mut reusable = BTreeMap::new();
    for section in &plan.sections {
        let section_matches = manifest
            .6
            .iter()
            .find(|candidate| candidate.0 == section.id)
            .is_some_and(|candidate| candidate.1 == section.title);
        for page in &section.pages {
            let Some(reference) = manifest.7.iter().find(|candidate| candidate.0 == page.slug)
            else {
                continue;
            };
            let Some(event) = previous
                .pages
                .iter()
                .find(|candidate| candidate.id.to_hex() == reference.2)
            else {
                continue;
            };
            if !section_matches
                || reference.4 != page.title
                || reference.5 != page.section
                || reference.6 != plan.language
                || !source_membership_matches(&page.source_files, &reference.7, snapshot)
                || event_tag(event, "source-kind") != Some(source_kind)
                || (source_kind == "git"
                    && event_tag(event, "branch") != Some(snapshot.branch.as_str()))
                || (source_kind == "folder" && event_tag(event, "branch").is_some())
                || event.content.trim().is_empty()
            {
                continue;
            }
            reusable.insert(
                page.slug.clone(),
                PageDraft {
                    slug: page.slug.clone(),
                    title: page.title.clone(),
                    section: page.section.clone(),
                    source_files: page.source_files.clone(),
                    // The signed graph is rebuilt against the new immutable
                    // source revision. The body is the only reused value.
                    commit: snapshot.commit.clone(),
                    language: plan.language.clone(),
                    content: event.content.clone(),
                },
            );
        }
    }
    reusable
}

/// Whether every page in a newly captured plan can be reused from a verified
/// publication. This keeps the no-op decision on the same production reuse
/// predicate as partial generation.
pub(crate) fn all_pages_reused(
    snapshot: &RepoSnapshot,
    plan: &WikiPlan,
    previous: &SnapshotPublication,
) -> bool {
    let page_count: usize = plan
        .sections
        .iter()
        .map(|section| section.pages.len())
        .sum();
    reusable_drafts(snapshot, plan, previous).len() == page_count
}

fn source_membership_matches(
    planned: &[String],
    references: &[SourceReference],
    snapshot: &RepoSnapshot,
) -> bool {
    if planned.len() != references.len() {
        return false;
    }
    let mut planned_paths = BTreeSet::new();
    if planned
        .iter()
        .any(|path| !planned_paths.insert(path.as_str()))
    {
        return false;
    }
    let mut reference_paths = BTreeSet::new();
    for reference in references {
        if !reference_paths.insert(reference.0.as_str()) {
            return false;
        }
        let Some(content) = snapshot.contents.get(&reference.0) else {
            return false;
        };
        if source_hash(content.as_bytes()) != reference.1 || content.len() as u64 != reference.2 {
            return false;
        }
    }
    planned_paths == reference_paths
}

fn steering_identity_matches(snapshot: &RepoSnapshot, previous: &SnapshotPublication) -> bool {
    let current = snapshot
        .contents
        .get(".crew/wiki.json")
        .map(|content| source_hash(content.as_bytes()));
    let Some(previous) = event_tag(&previous.head, "wiki-steering-hash") else {
        // Publications written before the signed steering identity existed
        // cannot prove that page bodies were produced under the current
        // steering notes and must take the safe full-generation path.
        return false;
    };
    match current {
        Some(current) => previous == current,
        None => previous == "absent",
    }
}

fn event_tag<'a>(event: &'a nostr::Event, name: &str) -> Option<&'a str> {
    let mut found = None;
    for tag in event.tags.iter() {
        let values = tag.as_slice();
        if values.first().map(String::as_str) != Some(name) {
            continue;
        }
        if values.len() != 2 || found.is_some() {
            return None;
        }
        found = values.get(1).map(String::as_str);
    }
    found
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

    // This unsigned preview command intentionally remains deterministic while
    // the native publication path requires an explicit installed runtime.
    // Do not infer a provider or billing claim from an environment variable.
    let cost_note =
        "Deterministic preview generator · select an installed runtime to publish".to_string();

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

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    #[test]
    fn generation_cancel_key_is_bound_to_the_exact_operation_revision() {
        let first = generation_cancel_key("https://relay.example", "30617:owner:repo", "one", 4);
        assert_ne!(
            first,
            generation_cancel_key("https://relay.example", "30617:owner:repo", "two", 4)
        );
        assert_ne!(
            first,
            generation_cancel_key("https://relay.example", "30617:owner:repo", "one", 5)
        );

        let token = begin_generation_cancel(&first).expect("generation token");
        let stale = generation_cancel_key("https://relay.example", "30617:owner:repo", "one", 5);
        assert!(
            !cancel_generation(&stale),
            "a stale revision must not cancel the live generation"
        );
        assert!(!token.load(Ordering::Acquire));
        assert!(cancel_generation(&first));
        assert!(token.load(Ordering::Acquire));
        finish_generation_cancel(&first, &token);
    }

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

    #[test]
    fn wiki_generate_reports_an_empty_git_tree_as_empty_repo() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        block_on(async {
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
        });
    }

    #[test]
    fn wiki_generate_propagates_invalid_committed_steering() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        block_on(async {
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
        });
    }
}

#[cfg(test)]
#[path = "wiki_incremental_tests.rs"]
mod incremental_tests;
