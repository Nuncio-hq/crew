//! Shadow repository: init, checkpoints, restore, compact, corruption rebuild.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::CoworkError;
use crate::git::{git, git_ok, git_status_ok, stdout_ok};
use crate::paths::{
    corrupt_backup_dir, empty_hooks_dir, exclude_path, history_git_dir, meta_path,
    CORRUPTION_NOTICE, DEFAULT_SIZE_THRESHOLD,
};

const AUTHOR_EXTERNAL_NAME: &str = "External changes";
const AUTHOR_EXTERNAL_EMAIL: &str = "external@nunciocrew.local";
const AUTHOR_SYSTEM_NAME: &str = "NuncioCrew";
const AUTHOR_SYSTEM_EMAIL: &str = "cowork@nunciocrew.local";
const AUTHOR_RESTORE_EMAIL: &str = "restore@nunciocrew.local";

/// Kind of checkpoint recorded in history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckpointKind {
    Baseline,
    External,
    Turn,
    Restore,
}

impl CheckpointKind {
    fn trailer(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::External => "external",
            Self::Turn => "turn",
            Self::Restore => "restore",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "baseline" => Some(Self::Baseline),
            "external" => Some(Self::External),
            "turn" => Some(Self::Turn),
            "restore" => Some(Self::Restore),
            _ => None,
        }
    }
}

/// Inputs for a checkpoint commit. Empty worktrees produce no commit.
#[derive(Debug, Clone)]
pub struct CheckpointSpec {
    pub kind: CheckpointKind,
    pub agent_name: Option<String>,
    pub thread_title: Option<String>,
    pub thread_id: Option<String>,
    pub turn_seq: Option<u64>,
}

/// One Versions timeline entry (business language).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    pub kind: CheckpointKind,
    pub summary: String,
    pub timestamp: i64,
    pub agent_name: Option<String>,
    pub thread_title: Option<String>,
    pub thread_id: Option<String>,
    pub files_changed: Vec<String>,
}

/// A file skipped because it is over the size threshold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExclusionNotice {
    pub path: String,
    pub size_bytes: u64,
}

/// Result of opening (or rebuilding) a shadow repo.
#[derive(Debug)]
pub struct OpenOutcome {
    pub repo: ShadowRepo,
    pub rebuilt: bool,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoworkMeta {
    schema: u32,
    repo_address: String,
    folder: String,
    size_threshold_bytes: u64,
    rebuilt_at: Option<i64>,
    last_notice: Option<String>,
}

/// Hidden git-dir + work-tree pair. The folder never contains `.git`.
#[derive(Debug, Clone)]
pub struct ShadowRepo {
    git_dir: PathBuf,
    work_tree: PathBuf,
    repo_address: String,
    size_threshold: u64,
}

impl ShadowRepo {
    /// Open an existing history or initialize a new one. Corrupt git-dirs are
    /// moved aside and rebuilt empty with a loud notice.
    pub fn open_or_init(
        history_root: &Path,
        repo_address: &str,
        folder: &Path,
        size_threshold: Option<u64>,
    ) -> Result<OpenOutcome, CoworkError> {
        if !folder.is_dir() {
            return Err(CoworkError::MissingFolder);
        }
        let work_tree = folder.canonicalize().map_err(|source| CoworkError::Io {
            path: folder.to_path_buf(),
            source,
        })?;
        fs::create_dir_all(history_root).map_err(|source| CoworkError::Io {
            path: history_root.to_path_buf(),
            source,
        })?;
        // Absolute git-dir: `git()` sets cwd to the work-tree so a deleted
        // process cwd cannot make spawn fail, and relative history roots
        // must not resolve inside the folder.
        let history_root = history_root
            .canonicalize()
            .map_err(|source| CoworkError::Io {
                path: history_root.to_path_buf(),
                source,
            })?;
        let git_dir = history_git_dir(&history_root, repo_address);
        let size_threshold = size_threshold.unwrap_or(DEFAULT_SIZE_THRESHOLD);
        let mut rebuilt = false;
        let mut notice = None;
        if git_dir.exists() && !is_healthy(&git_dir, &work_tree) {
            let backup = corrupt_backup_dir(&git_dir, unix_now());
            let _ = fs::rename(&git_dir, &backup);
            rebuilt = true;
            notice = Some(CORRUPTION_NOTICE.to_string());
        }
        if !git_dir.exists() {
            init_git_dir(&git_dir, &work_tree)?;
        }
        let repo = ShadowRepo {
            git_dir,
            work_tree,
            repo_address: repo_address.to_string(),
            size_threshold,
        };
        repo.write_meta(rebuilt, notice.as_deref())?;
        if !repo.has_head()? {
            repo.checkpoint(&CheckpointSpec {
                kind: CheckpointKind::Baseline,
                agent_name: None,
                thread_title: None,
                thread_id: None,
                turn_seq: None,
            })?;
        }
        Ok(OpenOutcome {
            repo,
            rebuilt,
            notice,
        })
    }

    /// Open an already-initialized history git-dir (harness turn path).
    pub fn open_existing(git_dir: &Path, work_tree: &Path) -> Result<Self, CoworkError> {
        if !work_tree.is_dir() {
            return Err(CoworkError::MissingFolder);
        }
        if !git_dir.exists() || !is_healthy(git_dir, work_tree) {
            return Err(CoworkError::operation(
                "Version history is not available for this folder",
            ));
        }
        let work_tree = work_tree.canonicalize().map_err(|source| CoworkError::Io {
            path: work_tree.to_path_buf(),
            source,
        })?;
        let git_dir = git_dir.canonicalize().map_err(|source| CoworkError::Io {
            path: git_dir.to_path_buf(),
            source,
        })?;
        Ok(Self {
            git_dir,
            work_tree,
            repo_address: String::new(),
            size_threshold: DEFAULT_SIZE_THRESHOLD,
        })
    }

    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }

    pub fn repo_address(&self) -> &str {
        &self.repo_address
    }

    /// True when the work tree differs from HEAD (after applying exclusions).
    pub fn is_dirty(&self) -> Result<bool, CoworkError> {
        self.refresh_excludes()?;
        let status = git_ok(
            &self.git_dir,
            &self.work_tree,
            ["status", "--porcelain", "--untracked-files=all"],
        )?;
        Ok(status.lines().any(|line| !line.trim().is_empty()))
    }

    /// Commit current filesystem truth if dirty. Returns the new commit id.
    pub fn checkpoint(&self, spec: &CheckpointSpec) -> Result<Option<String>, CoworkError> {
        self.refresh_excludes()?;
        git_ok(&self.git_dir, &self.work_tree, ["add", "-A"])?;
        if !self.is_dirty_cached()? && self.has_head()? {
            return Ok(None);
        }
        let message = commit_message(spec);
        let (name, email) = author_for(spec);
        let output = git(
            &self.git_dir,
            &self.work_tree,
            [
                "-c",
                &format!("user.name={name}"),
                "-c",
                &format!("user.email={email}"),
                "commit",
                "--no-verify",
                "-m",
                &message,
            ],
        )?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("nothing to commit") {
                return Ok(None);
            }
            return Err(CoworkError::operation(stderr.trim()));
        }
        let id = git_ok(&self.git_dir, &self.work_tree, ["rev-parse", "HEAD"])?;
        Ok(Some(id.trim().to_string()))
    }

    /// Next turn sequence for `thread_id` (1-based).
    pub fn next_turn_seq(&self, thread_id: &str) -> Result<u64, CoworkError> {
        let count = self
            .list_versions()?
            .into_iter()
            .filter(|entry| {
                entry.kind == CheckpointKind::Turn && entry.thread_id.as_deref() == Some(thread_id)
            })
            .count() as u64;
        Ok(count + 1)
    }

    pub fn list_versions(&self) -> Result<Vec<VersionEntry>, CoworkError> {
        if !self.has_head()? {
            return Ok(Vec::new());
        }
        let log = git_ok(
            &self.git_dir,
            &self.work_tree,
            [
                "log",
                "--date-order",
                "--format=%H%x1f%ct%x1f%an%x1f%s%x1f%b%x1e",
            ],
        )?;
        let mut entries = Vec::new();
        for record in log.split('\u{1e}') {
            let record = record.trim();
            if record.is_empty() {
                continue;
            }
            let mut parts = record.split('\u{1f}');
            let Some(id) = parts.next() else { continue };
            let timestamp = parts
                .next()
                .and_then(|value| value.trim().parse::<i64>().ok())
                .unwrap_or(0);
            let _author = parts.next().unwrap_or("");
            let subject = parts.next().unwrap_or("").trim().to_string();
            let body = parts.next().unwrap_or("");
            let kind = trailer(body, "Crew-kind")
                .and_then(CheckpointKind::parse)
                .unwrap_or(CheckpointKind::External);
            let thread_id = trailer(body, "Crew-thread").map(str::to_string);
            let agent_name = trailer(body, "Crew-agent").map(str::to_string);
            let thread_title = trailer(body, "Crew-title").map(str::to_string);
            let files_changed = self.files_changed(id.trim())?;
            entries.push(VersionEntry {
                id: id.trim().to_string(),
                kind,
                summary: subject,
                timestamp,
                agent_name,
                thread_title,
                thread_id,
                files_changed,
            });
        }
        Ok(entries)
    }

    pub fn excluded_files(&self) -> Result<Vec<ExclusionNotice>, CoworkError> {
        collect_exclusions(&self.work_tree, self.size_threshold)
    }

    /// Last loud notice, if history was rebuilt.
    pub fn last_notice(&self) -> Result<Option<String>, CoworkError> {
        Ok(self.read_meta()?.and_then(|meta| meta.last_notice))
    }

    /// Restore one relative file from `commit`. Checkpoints first (and again
    /// after the restore so the action is itself a Versions entry).
    pub fn restore_file(
        &self,
        commit: &str,
        relative_path: &str,
        before: &CheckpointSpec,
    ) -> Result<(), CoworkError> {
        let rel = validated_relative(relative_path)?;
        self.ensure_commit(commit)?;
        self.checkpoint(before)?;
        git_ok(
            &self.git_dir,
            &self.work_tree,
            [
                "checkout",
                commit,
                "--",
                rel.to_str().unwrap_or(relative_path),
            ],
        )?;
        self.checkpoint(&restore_applied_spec(before))?;
        Ok(())
    }

    /// Restore the whole folder to `commit`. Checkpoints first. Leaves
    /// never-versioned (over-threshold) files in place.
    pub fn restore_folder(&self, commit: &str, before: &CheckpointSpec) -> Result<(), CoworkError> {
        self.ensure_commit(commit)?;
        self.checkpoint(before)?;
        let target_files = self.ls_tree(commit)?;
        let current_tracked = self.ls_tracked()?;
        git_ok(
            &self.git_dir,
            &self.work_tree,
            ["checkout", commit, "--", "."],
        )?;
        for path in current_tracked {
            if !target_files.contains(&path) {
                let abs = self.work_tree.join(&path);
                if abs.is_file() {
                    let _ = fs::remove_file(&abs);
                }
            }
        }
        self.checkpoint(&restore_applied_spec(before))?;
        Ok(())
    }

    /// Owner-invoked compact: keep all checkpoints for `keep_days`, then one
    /// per UTC day. Never runs unless the owner asked.
    pub fn compact(&self, keep_days: u64) -> Result<(), CoworkError> {
        let entries = self.list_versions()?;
        if entries.len() < 2 {
            let _ = git_ok(&self.git_dir, &self.work_tree, ["gc", "--prune=now"]);
            return Ok(());
        }
        let cutoff = unix_now().saturating_sub((keep_days as i64).saturating_mul(86_400));
        let mut keep: Vec<&VersionEntry> = Vec::new();
        let mut seen_days = std::collections::HashSet::new();
        // list_versions is newest-first (git log date-order).
        for entry in &entries {
            if entry.timestamp >= cutoff {
                keep.push(entry);
                continue;
            }
            let day = entry.timestamp.div_euclid(86_400);
            if seen_days.insert(day) {
                keep.push(entry);
            }
        }
        if keep.len() == entries.len() {
            let _ = git_ok(&self.git_dir, &self.work_tree, ["gc", "--prune=now"]);
            return Ok(());
        }
        keep.reverse(); // oldest first for replay
        self.replay_commits(&keep)?;
        git_ok(&self.git_dir, &self.work_tree, ["gc", "--prune=now"])?;
        Ok(())
    }

    fn replay_commits(&self, keep: &[&VersionEntry]) -> Result<(), CoworkError> {
        let mut parent: Option<String> = None;
        for entry in keep {
            let tree = git_ok(
                &self.git_dir,
                &self.work_tree,
                ["rev-parse", &format!("{}^{{tree}}", entry.id)],
            )?;
            let tree = tree.trim();
            let (name, email) = author_from_entry(entry);
            let message = original_message(entry);
            let mut args = vec![
                "-c".to_string(),
                format!("user.name={name}"),
                "-c".to_string(),
                format!("user.email={email}"),
                "commit-tree".to_string(),
                tree.to_string(),
            ];
            if let Some(parent) = &parent {
                args.push("-p".into());
                args.push(parent.clone());
            }
            args.push("-m".into());
            args.push(message);
            let new_id = git_ok(&self.git_dir, &self.work_tree, args)?
                .trim()
                .to_string();
            parent = Some(new_id);
        }
        let Some(head) = parent else {
            return Ok(());
        };
        git_ok(
            &self.git_dir,
            &self.work_tree,
            ["update-ref", "HEAD", &head],
        )?;
        Ok(())
    }

    fn refresh_excludes(&self) -> Result<(), CoworkError> {
        let exclusions = collect_exclusions(&self.work_tree, self.size_threshold)?;
        let info = self.git_dir.join("info");
        fs::create_dir_all(&info).map_err(|source| CoworkError::Io {
            path: info.clone(),
            source,
        })?;
        let mut body = String::from("# Cowork size-threshold exclusions — do not edit\n.git\n");
        for notice in &exclusions {
            body.push_str(&notice.path);
            body.push('\n');
        }
        fs::write(exclude_path(&self.git_dir), body).map_err(|source| CoworkError::Io {
            path: exclude_path(&self.git_dir),
            source,
        })?;
        Ok(())
    }

    fn is_dirty_cached(&self) -> Result<bool, CoworkError> {
        let status = git_ok(
            &self.git_dir,
            &self.work_tree,
            ["status", "--porcelain", "--untracked-files=all"],
        )?;
        Ok(status.lines().any(|line| !line.trim().is_empty()))
    }

    fn has_head(&self) -> Result<bool, CoworkError> {
        Ok(git_status_ok(
            &self.git_dir,
            &self.work_tree,
            ["rev-parse", "--verify", "HEAD"],
        ))
    }

    fn ensure_commit(&self, commit: &str) -> Result<(), CoworkError> {
        if !git_status_ok(
            &self.git_dir,
            &self.work_tree,
            ["cat-file", "-e", &format!("{commit}^{{commit}}")],
        ) {
            return Err(CoworkError::MissingVersion);
        }
        Ok(())
    }

    fn files_changed(&self, commit: &str) -> Result<Vec<String>, CoworkError> {
        let output = git(
            &self.git_dir,
            &self.work_tree,
            ["diff-tree", "--no-commit-id", "--name-only", "-r", commit],
        )?;
        let text = stdout_ok(&output).unwrap_or_default();
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn ls_tree(&self, commit: &str) -> Result<Vec<String>, CoworkError> {
        let text = git_ok(
            &self.git_dir,
            &self.work_tree,
            ["ls-tree", "-r", "--name-only", commit],
        )?;
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn ls_tracked(&self) -> Result<Vec<String>, CoworkError> {
        let text = git_ok(&self.git_dir, &self.work_tree, ["ls-files"])?;
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn write_meta(&self, rebuilt: bool, notice: Option<&str>) -> Result<(), CoworkError> {
        let existing = self.read_meta()?;
        let meta = CoworkMeta {
            schema: 1,
            repo_address: self.repo_address.clone(),
            folder: self.work_tree.to_string_lossy().into_owned(),
            size_threshold_bytes: self.size_threshold,
            rebuilt_at: if rebuilt {
                Some(unix_now())
            } else {
                existing.as_ref().and_then(|meta| meta.rebuilt_at)
            },
            last_notice: notice
                .map(str::to_string)
                .or_else(|| existing.and_then(|meta| meta.last_notice)),
        };
        let json = serde_json::to_vec_pretty(&meta)
            .map_err(|error| CoworkError::operation(error.to_string()))?;
        fs::write(meta_path(&self.git_dir), json).map_err(|source| CoworkError::Io {
            path: meta_path(&self.git_dir),
            source,
        })
    }

    fn read_meta(&self) -> Result<Option<CoworkMeta>, CoworkError> {
        let path = meta_path(&self.git_dir);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|source| CoworkError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(serde_json::from_slice(&bytes).ok())
    }
}

fn init_git_dir(git_dir: &Path, work_tree: &Path) -> Result<(), CoworkError> {
    if let Some(parent) = git_dir.parent() {
        fs::create_dir_all(parent).map_err(|source| CoworkError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    git_ok(git_dir, work_tree, ["init"])?;
    fs::create_dir_all(empty_hooks_dir(git_dir)).map_err(|source| CoworkError::Io {
        path: empty_hooks_dir(git_dir),
        source,
    })?;
    git_ok(git_dir, work_tree, ["config", "core.bare", "false"])?;
    git_ok(
        git_dir,
        work_tree,
        ["config", "user.name", AUTHOR_SYSTEM_NAME],
    )?;
    git_ok(
        git_dir,
        work_tree,
        ["config", "user.email", AUTHOR_SYSTEM_EMAIL],
    )?;
    // A work-tree `.git` file would violate the byte-clean contract.
    let marker = work_tree.join(".git");
    if marker.exists() {
        let _ = fs::remove_file(&marker);
        if marker.is_dir() {
            return Err(CoworkError::operation(
                "This folder already has versioning files Crew must not touch",
            ));
        }
    }
    Ok(())
}

fn is_healthy(git_dir: &Path, work_tree: &Path) -> bool {
    git_status_ok(git_dir, work_tree, ["rev-parse", "--git-dir"])
        && git_status_ok(
            git_dir,
            work_tree,
            ["fsck", "--no-dangling", "--no-progress"],
        )
}

fn collect_exclusions(root: &Path, threshold: u64) -> Result<Vec<ExclusionNotice>, CoworkError> {
    let mut notices = Vec::new();
    walk_files(root, root, &mut notices, threshold)?;
    notices.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(notices)
}

fn walk_files(
    dir: &Path,
    root: &Path,
    notices: &mut Vec<ExclusionNotice>,
    threshold: u64,
) -> Result<(), CoworkError> {
    let entries = fs::read_dir(dir).map_err(|source| CoworkError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| CoworkError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            walk_files(&path, root, notices, threshold)?;
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.len() <= threshold {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| CoworkError::PathEscape)?;
        notices.push(ExclusionNotice {
            path: rel.to_string_lossy().replace('\\', "/"),
            size_bytes: meta.len(),
        });
    }
    Ok(())
}

fn commit_message(spec: &CheckpointSpec) -> String {
    let subject = match spec.kind {
        CheckpointKind::Baseline => "Starting point".to_string(),
        CheckpointKind::External => "External changes".to_string(),
        CheckpointKind::Turn => {
            let seq = spec.turn_seq.unwrap_or(1);
            let agent = spec
                .agent_name
                .as_deref()
                .filter(|name| !name.is_empty())
                .unwrap_or("agent");
            let title = spec
                .thread_title
                .as_deref()
                .filter(|title| !title.is_empty())
                .unwrap_or("thread");
            format!("Turn {seq} — {agent} · thread '{title}'")
        }
        CheckpointKind::Restore => {
            let agent = spec.agent_name.as_deref().unwrap_or("a turn");
            if spec.thread_title.as_deref() == Some("applied") {
                "Restored version".to_string()
            } else {
                format!("Before restore — {agent}")
            }
        }
    };
    let mut body = format!("{subject}\n\nCrew-kind: {}\n", spec.kind.trailer());
    if let Some(thread) = spec.thread_id.as_deref().filter(|value| !value.is_empty()) {
        body.push_str("Crew-thread: ");
        body.push_str(thread);
        body.push('\n');
    }
    if let Some(agent) = spec.agent_name.as_deref().filter(|value| !value.is_empty()) {
        body.push_str("Crew-agent: ");
        body.push_str(agent);
        body.push('\n');
    }
    if let Some(title) = spec
        .thread_title
        .as_deref()
        .filter(|value| !value.is_empty() && *value != "applied")
    {
        body.push_str("Crew-title: ");
        body.push_str(title);
        body.push('\n');
    }
    body
}

fn restore_applied_spec(before: &CheckpointSpec) -> CheckpointSpec {
    CheckpointSpec {
        kind: CheckpointKind::Restore,
        agent_name: before.agent_name.clone(),
        thread_title: Some("applied".into()),
        thread_id: before.thread_id.clone(),
        turn_seq: None,
    }
}

fn original_message(entry: &VersionEntry) -> String {
    let spec = CheckpointSpec {
        kind: entry.kind,
        agent_name: entry.agent_name.clone(),
        thread_title: entry.thread_title.clone(),
        thread_id: entry.thread_id.clone(),
        turn_seq: None,
    };
    // Keep the already-rendered summary so compact does not renumber turns.
    let mut body = format!("{}\n\nCrew-kind: {}\n", entry.summary, entry.kind.trailer());
    if let Some(thread) = &entry.thread_id {
        body.push_str("Crew-thread: ");
        body.push_str(thread);
        body.push('\n');
    }
    if let Some(agent) = &entry.agent_name {
        body.push_str("Crew-agent: ");
        body.push_str(agent);
        body.push('\n');
    }
    if let Some(title) = &entry.thread_title {
        body.push_str("Crew-title: ");
        body.push_str(title);
        body.push('\n');
    }
    let _ = spec;
    body
}

fn author_for(spec: &CheckpointSpec) -> (String, String) {
    match spec.kind {
        CheckpointKind::Turn => {
            let name = spec
                .agent_name
                .as_deref()
                .filter(|name| !name.is_empty())
                .unwrap_or("agent");
            let slug = name
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect::<String>();
            (name.to_string(), format!("{slug}@nunciocrew.local"))
        }
        CheckpointKind::External => (
            AUTHOR_EXTERNAL_NAME.to_string(),
            AUTHOR_EXTERNAL_EMAIL.to_string(),
        ),
        CheckpointKind::Restore => (
            AUTHOR_SYSTEM_NAME.to_string(),
            AUTHOR_RESTORE_EMAIL.to_string(),
        ),
        CheckpointKind::Baseline => (
            AUTHOR_SYSTEM_NAME.to_string(),
            AUTHOR_SYSTEM_EMAIL.to_string(),
        ),
    }
}

fn author_from_entry(entry: &VersionEntry) -> (String, String) {
    author_for(&CheckpointSpec {
        kind: entry.kind,
        agent_name: entry.agent_name.clone(),
        thread_title: entry.thread_title.clone(),
        thread_id: entry.thread_id.clone(),
        turn_seq: None,
    })
}

fn trailer<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("{key}: ");
    body.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn validated_relative(path: &str) -> Result<PathBuf, CoworkError> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') || trimmed.contains('\0') {
        return Err(CoworkError::PathEscape);
    }
    let rel = PathBuf::from(trimmed);
    if rel
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(CoworkError::PathEscape);
    }
    Ok(rel)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
