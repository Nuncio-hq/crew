use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

mod base;
mod binding;
mod checkout;
mod folder;

pub(crate) use base::BaseSource;
use base::{resolve_workspace_base, resolve_workspace_base_ref, WorkspaceBase};
pub(crate) use binding::WorkspaceBindingSpec;
use checkout::{
    attach_existing_branch, current_branch, existing_branch_worktree_path,
    find_worktree_for_branch, list_worktrees, uncommitted_count,
};

const CONTEXT_URL_PREFIX: &str = "buzz://project-workspace?";
const ROOT_CLAIM_DIRECTORY: &str = "buzz-thread-workspace-roots";
const ROOT_CLAIM_READ_ATTEMPTS: usize = 10;
const IN_PROGRESS_MARKERS: [&str; 7] = [
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "REBASE_HEAD",
    "BISECT_LOG",
    "rebase-merge",
    "rebase-apply",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckoutKind {
    IsolatedWorktree,
    MainCheckout,
    SharedBranch,
    Folder,
}

impl CheckoutKind {
    pub(crate) fn skips_lifecycle_record(self) -> bool {
        matches!(self, Self::MainCheckout | Self::Folder)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    Git,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorkspace {
    pub repo_address: String,
    pub local_path: PathBuf,
    pub binding: WorkspaceBindingSpec,
    pub mode: WorkspaceMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadWorkspace {
    pub(crate) root_event_id: String,
    pub(crate) repository_path: PathBuf,
    pub(crate) worktree_path: PathBuf,
    pub(crate) worktree_name: String,
    pub(crate) branch: String,
    pub(crate) base_revision: String,
    pub(crate) base_source: BaseSource,
    pub(crate) remote_default_branch: Option<String>,
    pub(crate) commits_behind_remote: Option<u64>,
    /// Canonical common Git directory (`git rev-parse --git-common-dir`),
    /// used to key `buzz-worktree` leases and lifecycle records.
    pub(crate) common_git: PathBuf,
    pub(crate) checkout_kind: CheckoutKind,
    pub(crate) requested_base: Option<String>,
    pub(crate) uncommitted_count: u64,
}

/// How `ensure_planned_thread_worktree` obtained a verified checkout.
///
/// Callers use this to force ACP session invalidation after create/reattach
/// even when path/branch/eviction-generation strings still match (issue #59
/// BLOCKER 5 — generation write can fail after eviction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnsureKind {
    /// Registered checkout already verified; no create/reattach mutation.
    AlreadyPresent,
    /// Fresh `git worktree add -b` succeeded under the lease.
    Created,
    /// Existing branch was reattached or recovered via checkout under the lease.
    Reattached,
    /// `git worktree add` without `-b` attached an existing named branch.
    AttachedExisting,
}

impl EnsureKind {
    /// True when ensure mutated the checkout (create or reattach).
    pub(crate) fn mutated_checkout(self) -> bool {
        !matches!(self, Self::AlreadyPresent)
    }
}

/// Discovery-only plan for a thread worktree: identity paths and base revision
/// without creating, reattaching, or claiming the checkout.
///
/// Acquire the shared active-turn lease against `common_git` + `root_event_id`
/// *before* calling [`ensure_planned_thread_worktree`].
#[derive(Debug, Clone)]
pub(crate) struct ThreadWorkspacePlan {
    pub(crate) root_event_id: String,
    pub(crate) repository_path: PathBuf,
    pub(crate) worktree_path: PathBuf,
    pub(crate) branch: String,
    pub(crate) common_git: PathBuf,
    pub(crate) workspace_base: WorkspaceBase,
    pub(crate) checkout_kind: CheckoutKind,
    pub(crate) claim_exclusive_root: bool,
}

#[derive(Debug)]
pub(crate) struct ThreadWorkspaceBranchConflict {
    worktree_path: PathBuf,
    current_branch: String,
    detached_head: Option<String>,
    automatic_recovery_safe: bool,
    expected_branch: String,
    git_errors: Vec<String>,
}

#[derive(Debug)]
pub(crate) enum ThreadWorkspaceRootVerificationError {
    Unavailable {
        root_event_id: String,
    },
    IdMismatch {
        requested_root_event_id: String,
        fetched_root_event_id: String,
    },
    UnexpectedContext {
        root_event_id: String,
    },
    NoMessages {
        root_event_id: String,
    },
    AuthorMismatch {
        root_event_id: String,
        expected_owner: String,
        actual_author: String,
    },
}

impl std::fmt::Display for ThreadWorkspaceRootVerificationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable { root_event_id } => write!(
                formatter,
                "Thread root '{root_event_id}' could not be verified. Check that the root is available from the relay, then retry."
            ),
            Self::IdMismatch {
                requested_root_event_id,
                fetched_root_event_id,
            } => write!(
                formatter,
                "Thread root '{requested_root_event_id}' could not be verified because the relay returned '{fetched_root_event_id}' instead. Check relay consistency and retry."
            ),
            Self::UnexpectedContext { root_event_id } => write!(
                formatter,
                "Thread root '{root_event_id}' could not be verified because the relay returned unexpected context. Check the root event and retry."
            ),
            Self::NoMessages { root_event_id } => write!(
                formatter,
                "Thread root '{root_event_id}' could not be verified because the relay returned no messages. Restore or republish the root event, then retry."
            ),
            Self::AuthorMismatch {
                root_event_id,
                expected_owner,
                actual_author,
            } => write!(
                formatter,
                "Thread root '{root_event_id}' is authored by '{actual_author}', not the Project owner '{expected_owner}'. Verify the Project owner or use the correct thread root, then retry."
            ),
        }
    }
}

impl std::error::Error for ThreadWorkspaceRootVerificationError {}

impl ThreadWorkspaceBranchConflict {
    fn new(worktree_path: &Path, current_branch: String, expected_branch: &str) -> Self {
        Self {
            worktree_path: worktree_path.to_path_buf(),
            current_branch,
            detached_head: None,
            automatic_recovery_safe: true,
            expected_branch: expected_branch.to_string(),
            git_errors: Vec::new(),
        }
    }

    fn detached(
        worktree_path: &Path,
        commit: String,
        expected_branch: &str,
        reachable_from_named_ref: bool,
    ) -> Self {
        Self {
            worktree_path: worktree_path.to_path_buf(),
            current_branch: format!("detached at {commit}"),
            detached_head: Some(commit),
            automatic_recovery_safe: reachable_from_named_ref,
            expected_branch: expected_branch.to_string(),
            git_errors: Vec::new(),
        }
    }

    fn automatic_recovery_is_safe(&self) -> bool {
        self.automatic_recovery_safe
    }

    fn record_git_error(&mut self, step: &str, stderr: &[u8]) {
        let stderr = String::from_utf8_lossy(stderr);
        let stderr = stderr.trim();
        if !stderr.is_empty() {
            self.git_errors.push(format!("{step}: {stderr}"));
        }
    }

    fn recovery_command(&self) -> String {
        format!(
            "git -C {} checkout {}",
            shell_quote(self.worktree_path.to_string_lossy().as_ref()),
            shell_quote(&self.expected_branch)
        )
    }
}

impl std::fmt::Display for ThreadWorkspaceBranchConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(commit) = self.detached_head.as_deref() {
            write!(
                formatter,
                "Worktree {} has a detached HEAD at '{}' instead of expected branch '{}'.",
                self.worktree_path.display(),
                commit,
                self.expected_branch
            )?;
            if !self.automatic_recovery_safe {
                write!(
                    formatter,
                    " Commit '{commit}' is not reachable from a named branch, remote, or tag; preserve it with a branch before recovery."
                )?;
            }
            write!(
                formatter,
                " Preserve or finish any in-progress work, then run `{}`.",
                self.recovery_command()
            )?;
        } else {
            write!(
                formatter,
                "Worktree {} is on branch '{}' instead of expected branch '{}'. Preserve or finish any in-progress work, then run `{}`.",
                self.worktree_path.display(),
                self.current_branch,
                self.expected_branch,
                self.recovery_command()
            )?;
        }
        if !self.git_errors.is_empty() {
            write!(formatter, " Git reported: {}", self.git_errors.join("; "))?;
        }
        Ok(())
    }
}

impl std::error::Error for ThreadWorkspaceBranchConflict {}

pub fn parse_project_workspace(content: &str) -> Result<Option<ProjectWorkspace>> {
    let Some(start) = content.find(CONTEXT_URL_PREFIX) else {
        return Ok(None);
    };
    let suffix = &content[start..];
    let end = suffix
        .find(['>', ' ', '\n', '\r', '\t'])
        .unwrap_or(suffix.len());
    let url = url::Url::parse(&suffix[..end]).context("invalid Project workspace URL")?;
    let mut repo_address = None;
    let mut local_path = None;
    let mut ws = None;
    let mut base = None;
    let mut mode = WorkspaceMode::Git;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "repo" => repo_address = Some(value.into_owned()),
            "path" => local_path = Some(PathBuf::from(value.into_owned())),
            "ws" => ws = Some(value.into_owned()),
            "base" => base = Some(value.into_owned()),
            "mode" if value.as_ref() == "folder" => mode = WorkspaceMode::Folder,
            _ => {}
        }
    }
    let repo_address = repo_address.context("Project workspace URL is missing repo")?;
    let local_path = local_path.context("Project workspace URL is missing path")?;
    if repo_address.trim().is_empty() || !local_path.is_absolute() {
        bail!("Project workspace metadata is invalid");
    }
    let binding = WorkspaceBindingSpec::from_params(ws.as_deref(), base.as_deref())?;
    Ok(Some(ProjectWorkspace {
        repo_address,
        local_path,
        binding,
        mode,
    }))
}

/// Resolve repository identity and deterministic worktree paths without
/// creating or reattaching a checkout.
pub async fn plan_thread_worktree(
    workspace: &ProjectWorkspace,
    root_event_id: &str,
) -> Result<ThreadWorkspacePlan> {
    validate_root_event_id(root_event_id)?;
    if workspace.mode == WorkspaceMode::Folder {
        return folder::plan_folder_workspace(workspace, root_event_id);
    }
    let selected_path = fs::canonicalize(&workspace.local_path).with_context(|| {
        format!(
            "Project workspace does not exist: {}",
            workspace.local_path.display()
        )
    })?;
    let repo_root = git_output(&selected_path, ["rev-parse", "--show-toplevel"]).await?;
    let repo_root =
        fs::canonicalize(repo_root.trim()).context("could not canonicalize git repository root")?;
    let common_git = git_output(&repo_root, ["rev-parse", "--git-common-dir"]).await?;
    let common_git = canonical_git_path(&repo_root, common_git.trim())
        .context("could not canonicalize git common directory")?;

    let short_root = &root_event_id[..12];
    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let parent = repo_root
        .parent()
        .context("git repository has no parent directory")?
        .join(".buzz-worktrees");

    match &workspace.binding {
        WorkspaceBindingSpec::NewWorktree { base } => {
            let worktree_path = parent.join(format!("{repo_name}-{short_root}"));
            let branch = format!("buzz/{short_root}");
            let workspace_base = resolve_workspace_base_ref(&repo_root, base.as_deref()).await?;
            Ok(ThreadWorkspacePlan {
                root_event_id: root_event_id.to_string(),
                repository_path: repo_root,
                worktree_path,
                branch,
                common_git,
                workspace_base,
                checkout_kind: CheckoutKind::IsolatedWorktree,
                claim_exclusive_root: true,
            })
        }
        WorkspaceBindingSpec::Main => {
            let branch = current_branch(&repo_root).await;
            let workspace_base = resolve_workspace_base(&repo_root).await?;
            Ok(ThreadWorkspacePlan {
                root_event_id: root_event_id.to_string(),
                repository_path: repo_root.clone(),
                worktree_path: repo_root,
                branch,
                common_git,
                workspace_base,
                checkout_kind: CheckoutKind::MainCheckout,
                claim_exclusive_root: false,
            })
        }
        WorkspaceBindingSpec::ExistingBranch { name } => {
            let listed = list_worktrees(&repo_root).await.unwrap_or_default();
            let existing = find_worktree_for_branch(&listed, name);
            let worktree_path = match existing {
                Some(found) => found.path.clone(),
                None => existing_branch_worktree_path(&repo_root, name)?,
            };
            let checkout_kind = if existing.is_some_and(|found| found.is_primary) {
                CheckoutKind::MainCheckout
            } else {
                CheckoutKind::SharedBranch
            };
            let workspace_base =
                resolve_workspace_base_ref(&repo_root, Some(name.as_str())).await?;
            Ok(ThreadWorkspacePlan {
                root_event_id: root_event_id.to_string(),
                repository_path: repo_root,
                worktree_path,
                branch: name.clone(),
                common_git,
                workspace_base,
                checkout_kind,
                claim_exclusive_root: false,
            })
        }
    }
}

/// Ensure a previously planned worktree exists and return verified metadata
/// plus whether this call created or reattached the checkout.
///
/// Callers that coordinate with eviction must hold a shared active-turn lease
/// before invoking this (issue #59 BLOCKER 4).
pub async fn ensure_planned_thread_worktree(
    plan: &ThreadWorkspacePlan,
) -> Result<(ThreadWorkspace, EnsureKind)> {
    let repo_root = &plan.repository_path;
    if matches!(plan.checkout_kind, CheckoutKind::Folder) {
        return folder::ensure_folder_workspace(plan);
    }
    let worktree_path = &plan.worktree_path;
    let common_git = &plan.common_git;
    let branch = &plan.branch;
    let root_event_id = plan.root_event_id.as_str();
    let workspace_base = &plan.workspace_base;
    let claim_root = plan.claim_exclusive_root;

    if let Some(metadata) = verified_metadata(
        repo_root,
        worktree_path,
        common_git,
        branch,
        root_event_id,
        workspace_base,
        plan.checkout_kind,
        claim_root,
    )
    .await?
    {
        return Ok((metadata, EnsureKind::AlreadyPresent));
    }

    if matches!(plan.checkout_kind, CheckoutKind::MainCheckout) {
        let metadata = main_checkout_metadata(
            repo_root,
            worktree_path,
            common_git,
            branch,
            root_event_id,
            workspace_base,
        )
        .await?;
        return Ok((metadata, EnsureKind::AlreadyPresent));
    }

    if matches!(plan.checkout_kind, CheckoutKind::SharedBranch) {
        if let Err(error) = attach_existing_branch(repo_root, worktree_path, branch).await {
            if let Some(metadata) = verified_metadata(
                repo_root,
                worktree_path,
                common_git,
                branch,
                root_event_id,
                workspace_base,
                plan.checkout_kind,
                claim_root,
            )
            .await?
            {
                return Ok((metadata, EnsureKind::AlreadyPresent));
            }
            return Err(error);
        }
        let metadata = verified_metadata(
            repo_root,
            worktree_path,
            common_git,
            branch,
            root_event_id,
            workspace_base,
            plan.checkout_kind,
            claim_root,
        )
        .await?
        .context("attached worktree failed repository verification")?;
        return Ok((metadata, EnsureKind::AttachedExisting));
    }

    let parent = worktree_path
        .parent()
        .context("planned worktree path has no parent directory")?;
    fs::create_dir_all(parent).context("could not create Buzz worktree directory")?;

    let create = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "add", "-b", branch])
        .arg(worktree_path)
        .arg(&workspace_base.revision)
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git worktree add")?;

    if !create.status.success() {
        // Another harness may have won the same idempotent create race.
        for _ in 0..10 {
            if let Some(metadata) = verified_metadata(
                repo_root,
                worktree_path,
                common_git,
                branch,
                root_event_id,
                workspace_base,
                CheckoutKind::IsolatedWorktree,
                true,
            )
            .await?
            {
                return Ok((metadata, EnsureKind::AlreadyPresent));
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let branch_matches =
            branch_root_matches(repo_root, common_git, branch, root_event_id).await;
        let mut branch_conflict = if branch_matches {
            foreign_branch_conflict(worktree_path, common_git, branch).await
        } else {
            None
        };
        if let Some(conflict) = branch_conflict.as_mut() {
            if conflict.automatic_recovery_is_safe()
                && worktree_recovery_is_lossless(worktree_path).await
            {
                let checkout = Command::new("git")
                    .arg("-C")
                    .arg(worktree_path)
                    .args(["checkout", branch])
                    .kill_on_drop(true)
                    .output()
                    .await
                    .context("could not start git checkout for worktree recovery")?;
                if checkout.status.success() {
                    if let Some(metadata) = verified_metadata(
                        repo_root,
                        worktree_path,
                        common_git,
                        branch,
                        root_event_id,
                        workspace_base,
                        CheckoutKind::IsolatedWorktree,
                        true,
                    )
                    .await?
                    {
                        return Ok((metadata, EnsureKind::Reattached));
                    }
                } else {
                    conflict.record_git_error("checkout failed", &checkout.stderr);
                }
            }
        }
        // The deterministic branch can outlive a manually removed worktree.
        // Reattach it instead of treating that recoverable state as a task
        // failure. Git still rejects a branch checked out somewhere else.
        if branch_matches {
            let attach = Command::new("git")
                .arg("-C")
                .arg(repo_root)
                .args(["worktree", "add"])
                .arg(worktree_path)
                .arg(branch)
                .kill_on_drop(true)
                .output()
                .await
                .context("could not start git worktree reattach")?;
            if !attach.status.success() {
                if let Some(mut conflict) = branch_conflict {
                    conflict.record_git_error("reattach failed", &attach.stderr);
                    return Err(conflict.into());
                }
                let stderr = String::from_utf8_lossy(&attach.stderr);
                bail!("git worktree add failed: {}", stderr.trim());
            }
            if let Some(metadata) = verified_metadata(
                repo_root,
                worktree_path,
                common_git,
                branch,
                root_event_id,
                workspace_base,
                CheckoutKind::IsolatedWorktree,
                true,
            )
            .await?
            {
                return Ok((metadata, EnsureKind::Reattached));
            }
        }
        let stderr = String::from_utf8_lossy(&create.stderr);
        bail!("git worktree add failed: {}", stderr.trim());
    }

    let metadata = verified_metadata(
        repo_root,
        worktree_path,
        common_git,
        branch,
        root_event_id,
        workspace_base,
        CheckoutKind::IsolatedWorktree,
        true,
    )
    .await?
    .context("created worktree failed repository verification")?;
    Ok((metadata, EnsureKind::Created))
}

/// Ensure the deterministic worktree for a thread exists and return verified metadata.
///
/// Preference order for lease-aware callers: [`plan_thread_worktree`] then
/// acquire the shared lease, then [`ensure_planned_thread_worktree`].
///
/// Kept as a convenience for direct callers and unit tests that do not
/// coordinate with eviction leases.
#[allow(dead_code)] // exercised by `thread_workspace_tests`; pool uses plan+ensure_planned
pub async fn ensure_thread_worktree(
    workspace: &ProjectWorkspace,
    root_event_id: &str,
) -> Result<ThreadWorkspace> {
    let plan = plan_thread_worktree(workspace, root_event_id).await?;
    let (metadata, _) = ensure_planned_thread_worktree(&plan).await?;
    Ok(metadata)
}

async fn foreign_branch_conflict(
    path: &Path,
    expected_common_git: &Path,
    expected_branch: &str,
) -> Option<ThreadWorkspaceBranchConflict> {
    let root = git_output(path, ["rev-parse", "--show-toplevel"])
        .await
        .ok()?;
    let common = git_output(path, ["rev-parse", "--git-common-dir"])
        .await
        .ok()?;
    let root = fs::canonicalize(root.trim()).ok()?;
    let common = canonical_git_path(&root, common.trim()).ok()?;
    if root != path || common != expected_common_git {
        return None;
    }
    if let Ok(current_branch) = git_output(path, ["symbolic-ref", "--short", "HEAD"]).await {
        let current_branch = current_branch.trim();
        return (current_branch != expected_branch).then(|| {
            ThreadWorkspaceBranchConflict::new(path, current_branch.to_string(), expected_branch)
        });
    }

    let commit = git_output(path, ["rev-parse", "--verify", "HEAD^{commit}"])
        .await
        .ok()?
        .trim()
        .to_string();
    let reachable_from_named_ref = detached_head_reachable_from_named_ref(path, &commit).await;
    Some(ThreadWorkspaceBranchConflict::detached(
        path,
        commit,
        expected_branch,
        reachable_from_named_ref,
    ))
}

async fn detached_head_reachable_from_named_ref(path: &Path, commit: &str) -> bool {
    let contains = format!("--contains={commit}");
    let Ok(refs) = git_output(
        path,
        [
            "for-each-ref",
            "--count=1",
            contains.as_str(),
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )
    .await
    else {
        return false;
    };
    !refs.trim().is_empty()
}

async fn worktree_recovery_is_lossless(path: &Path) -> bool {
    let Ok(status) = git_output(path, ["status", "--porcelain"]).await else {
        return false;
    };
    if !status.trim().is_empty() {
        return false;
    }
    let Ok(git_dir) = git_output(path, ["rev-parse", "--git-dir"]).await else {
        return false;
    };
    let Ok(git_dir) = canonical_git_path(path, git_dir.trim()) else {
        return false;
    };
    !IN_PROGRESS_MARKERS
        .iter()
        .any(|marker| git_dir.join(marker).exists())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[allow(clippy::too_many_arguments)]
async fn verified_metadata(
    repo_root: &Path,
    path: &Path,
    expected_common_git: &Path,
    expected_branch: &str,
    root_event_id: &str,
    workspace_base: &WorkspaceBase,
    checkout_kind: CheckoutKind,
    claim_exclusive_root: bool,
) -> Result<Option<ThreadWorkspace>> {
    if !verify_worktree(
        path,
        expected_common_git,
        expected_branch,
        root_event_id,
        claim_exclusive_root,
    )
    .await
    {
        return Ok(None);
    }
    assemble_metadata(
        repo_root,
        path,
        expected_common_git,
        expected_branch,
        root_event_id,
        workspace_base,
        checkout_kind,
    )
    .await
    .map(Some)
}

async fn main_checkout_metadata(
    repo_root: &Path,
    path: &Path,
    expected_common_git: &Path,
    expected_branch: &str,
    root_event_id: &str,
    workspace_base: &WorkspaceBase,
) -> Result<ThreadWorkspace> {
    assemble_metadata(
        repo_root,
        path,
        expected_common_git,
        expected_branch,
        root_event_id,
        workspace_base,
        CheckoutKind::MainCheckout,
    )
    .await
}

async fn assemble_metadata(
    repo_root: &Path,
    path: &Path,
    expected_common_git: &Path,
    expected_branch: &str,
    root_event_id: &str,
    workspace_base: &WorkspaceBase,
    checkout_kind: CheckoutKind,
) -> Result<ThreadWorkspace> {
    let worktree_path =
        fs::canonicalize(path).context("could not canonicalize verified worktree path")?;
    let worktree_name = worktree_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("verified worktree name is not valid UTF-8")?
        .to_string();
    let head = git_output(&worktree_path, ["rev-parse", "HEAD"])
        .await?
        .trim()
        .to_string();
    let base_revision = git_output(
        &worktree_path,
        ["merge-base", "HEAD", workspace_base.revision.as_str()],
    )
    .await
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| {
        value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
    })
    .unwrap_or(head);
    if base_revision.len() != 40
        || !base_revision
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        bail!("git returned an invalid worktree base revision");
    }
    let commits_behind_remote = match workspace_base.source {
        BaseSource::Remote => git_output(
            &worktree_path,
            [
                "rev-list",
                "--count",
                &format!("HEAD..{}", workspace_base.revision),
            ],
        )
        .await
        .ok()
        .and_then(|value| value.trim().parse().ok()),
        BaseSource::LocalFallback => None,
    };
    let uncommitted_count = uncommitted_count(&worktree_path).await;
    Ok(ThreadWorkspace {
        root_event_id: root_event_id.to_string(),
        repository_path: repo_root.to_path_buf(),
        worktree_path,
        worktree_name,
        branch: expected_branch.to_string(),
        base_revision,
        base_source: workspace_base.source,
        remote_default_branch: workspace_base.remote_default_branch.clone(),
        commits_behind_remote,
        common_git: expected_common_git.to_path_buf(),
        checkout_kind,
        requested_base: workspace_base.requested_base.clone(),
        uncommitted_count,
    })
}

async fn verify_worktree(
    path: &Path,
    expected_common_git: &Path,
    expected_branch: &str,
    expected_root_event_id: &str,
    claim_exclusive_root: bool,
) -> bool {
    let Ok(root) = git_output(path, ["rev-parse", "--show-toplevel"]).await else {
        return false;
    };
    let Ok(common) = git_output(path, ["rev-parse", "--git-common-dir"]).await else {
        return false;
    };
    let Ok(root) = fs::canonicalize(root.trim()) else {
        return false;
    };
    let Ok(common_path) = canonical_git_path(&root, common.trim()) else {
        return false;
    };
    let Ok(path) = fs::canonicalize(path) else {
        return false;
    };
    let Ok(branch) = git_output(&path, ["symbolic-ref", "--short", "HEAD"]).await else {
        return false;
    };
    if root != path || common_path != expected_common_git || branch.trim() != expected_branch {
        return false;
    }
    if !claim_exclusive_root {
        return true;
    }
    verify_or_claim_branch_root(
        &path,
        expected_common_git,
        expected_branch,
        expected_root_event_id,
    )
    .await
}

async fn record_branch_root(repo_root: &Path, branch: &str, root_event_id: &str) -> Result<()> {
    let key = branch_root_config_key(branch);
    git_output(
        repo_root,
        ["config", "--local", "--add", key.as_str(), root_event_id],
    )
    .await?;
    Ok(())
}

async fn branch_root_matches(
    repo_root: &Path,
    common_git: &Path,
    branch: &str,
    root_event_id: &str,
) -> bool {
    let Ok(recorded_roots) = read_branch_roots(repo_root, branch).await else {
        return false;
    };
    !recorded_roots.is_empty()
        && recorded_roots
            .iter()
            .all(|recorded| recorded.eq_ignore_ascii_case(root_event_id))
        && root_claim_matches(common_git, root_event_id).await
}

async fn verify_or_claim_branch_root(
    repo_root: &Path,
    common_git: &Path,
    branch: &str,
    root_event_id: &str,
) -> bool {
    let Ok(recorded_roots) = read_branch_roots(repo_root, branch).await else {
        return false;
    };
    if recorded_roots
        .iter()
        .any(|recorded| !recorded.eq_ignore_ascii_case(root_event_id))
    {
        return false;
    }
    let Ok(claimed) = claim_root(common_git, root_event_id).await else {
        return false;
    };
    if !claimed {
        return false;
    }
    if recorded_roots.is_empty()
        && record_branch_root(repo_root, branch, root_event_id)
            .await
            .is_err()
    {
        return false;
    }
    branch_root_matches(repo_root, common_git, branch, root_event_id).await
}

async fn claim_root(common_git: &Path, root_event_id: &str) -> Result<bool> {
    let claim_directory = common_git.join(ROOT_CLAIM_DIRECTORY);
    fs::create_dir_all(&claim_directory).context("could not create thread root claim directory")?;
    let claim_path = root_claim_path(common_git, root_event_id);
    let normalized_root = root_event_id.to_ascii_lowercase();

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&claim_path)
    {
        Ok(mut claim_file) => {
            claim_file
                .write_all(normalized_root.as_bytes())
                .and_then(|()| claim_file.write_all(b"\n"))
                .context("could not write thread root claim")?;
            claim_file
                .sync_all()
                .context("could not persist thread root claim")?;
            fs::File::open(&claim_directory)
                .and_then(|directory| directory.sync_all())
                .context("could not persist thread root claim directory")?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            root_claim_matches_result(&claim_path, root_event_id).await
        }
        Err(error) => Err(error).context("could not create thread root claim"),
    }
}

async fn root_claim_matches(common_git: &Path, root_event_id: &str) -> bool {
    root_claim_matches_result(&root_claim_path(common_git, root_event_id), root_event_id)
        .await
        .unwrap_or(false)
}

async fn root_claim_matches_result(claim_path: &Path, root_event_id: &str) -> Result<bool> {
    for attempt in 0..ROOT_CLAIM_READ_ATTEMPTS {
        match fs::read_to_string(claim_path) {
            Ok(recorded_root) if !recorded_root.trim().is_empty() => {
                return Ok(recorded_root.trim().eq_ignore_ascii_case(root_event_id));
            }
            Ok(_) if attempt + 1 < ROOT_CLAIM_READ_ATTEMPTS => {}
            Ok(_) => return Ok(false),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && attempt + 1 < ROOT_CLAIM_READ_ATTEMPTS => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error).context("could not read thread root claim"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Ok(false)
}

fn root_claim_path(common_git: &Path, root_event_id: &str) -> PathBuf {
    common_git
        .join(ROOT_CLAIM_DIRECTORY)
        .join(format!("{}.root", root_event_id[..12].to_ascii_lowercase()))
}

async fn read_branch_roots(repo_root: &Path, branch: &str) -> Result<Vec<String>> {
    let key = branch_root_config_key(branch);
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["config", "--local", "--get-all", key.as_str()])
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git")?;
    if !output.status.success() {
        if output.stderr.is_empty() {
            return Ok(Vec::new());
        }
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8(output.stdout)
        .context("git returned non-UTF-8 output")?
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect())
}

fn branch_root_config_key(branch: &str) -> String {
    format!("branch.{branch}.buzzThreadRoot")
}

fn canonical_git_path(repo_root: &Path, path: &str) -> std::io::Result<PathBuf> {
    let path = Path::new(path);
    fs::canonicalize(if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    })
}

async fn git_output<I, S>(cwd: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    String::from_utf8(output.stdout).context("git returned non-UTF-8 output")
}

fn validate_root_event_id(root_event_id: &str) -> Result<()> {
    if root_event_id.len() != 64 || !root_event_id.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("thread root event ID must be 64 hex characters");
    }
    Ok(())
}
