//! Cache-first reclaim for managed Project worktrees.
//!
//! Mutation accepts category IDs only. Paths are derived from a validated
//! worktree root with symlink/traversal refusal. Active exclusive leases block
//! cache clearing. Legacy / no-root / other-channel refuse at the Rust boundary.

use std::fs;
use std::path::{Path, PathBuf};

use buzz_worktree::{
    read_lifecycle_record, try_acquire_exclusive, try_acquire_path_exclusive, LeaseError,
    PathLeaseHolder,
};
use serde::Serialize;

use super::project_worktree_auth::{
    authorize_verified_channel_mutation, project_reclaim_capabilities,
};
use super::project_worktree_cleanup::{prepare_managed_removal, PreparedRemoval};
use super::project_worktree_details::{disk_bytes_of, has_ignored_local_state};
use super::project_worktree_registry::LifecycleIdentity;
use super::thread_workspace::ThreadWorkspaceActionStatus;
use super::thread_workspace_git::path_text;

/// Allowlisted cache category identifiers accepted from the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheCategoryId {
    CargoTarget,
    DesktopTauriTarget,
    DesktopDist,
    NodeModules,
}

impl CacheCategoryId {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "cargo-target" => Some(Self::CargoTarget),
            "desktop-tauri-target" => Some(Self::DesktopTauriTarget),
            "desktop-dist" => Some(Self::DesktopDist),
            "node-modules" => Some(Self::NodeModules),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::CargoTarget => "cargo-target",
            Self::DesktopTauriTarget => "desktop-tauri-target",
            Self::DesktopDist => "desktop-dist",
            Self::NodeModules => "node-modules",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CargoTarget => "Cargo target/",
            Self::DesktopTauriTarget => "Desktop Tauri target/",
            Self::DesktopDist => "Desktop dist/",
            Self::NodeModules => "node_modules/",
        }
    }

    fn relative_path(self) -> &'static str {
        match self {
            Self::CargoTarget => "target",
            Self::DesktopTauriTarget => "desktop/src-tauri/target",
            Self::DesktopDist => "desktop/dist",
            Self::NodeModules => "node_modules",
        }
    }

    fn all() -> &'static [Self] {
        &[
            Self::CargoTarget,
            Self::DesktopTauriTarget,
            Self::DesktopDist,
            Self::NodeModules,
        ]
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheCategoryPreview {
    pub id: String,
    pub label: String,
    pub bytes: u64,
    pub present: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeReclaimPreview {
    pub worktree_path: String,
    /// Deprecated aggregate — prefer `can_clear_cache` / `can_evict`.
    pub actionable: bool,
    pub refusal_reason: Option<String>,
    pub can_clear_cache: bool,
    pub can_evict: bool,
    pub clear_cache_refusal: Option<String>,
    pub eviction_refusal: Option<String>,
    pub dirty: bool,
    pub busy: bool,
    pub branch_retained: bool,
    pub disk_bytes: u64,
    pub cache_categories: Vec<CacheCategoryPreview>,
    pub has_ignored_local_state: bool,
    pub lifecycle_identity: LifecycleIdentity,
    pub last_used_at: Option<i64>,
    pub routing_channel_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheCategoryClearResult {
    pub id: String,
    pub status: ThreadWorkspaceActionStatus,
    pub message: String,
    pub bytes_removed: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearProjectWorktreeCacheResult {
    pub worktree_path: String,
    pub results: Vec<CacheCategoryClearResult>,
}

/// Advisory preview for cache-first reclaim. Mutation revalidates under lease.
#[tauri::command]
pub async fn preview_project_worktree_reclaim(
    repository_path: String,
    worktree_path: String,
    expected_routing_channel_id: Option<String>,
) -> Result<ProjectWorktreeReclaimPreview, String> {
    let prepared = prepare_managed_removal(&repository_path, &worktree_path).await?;
    let dirty =
        !super::thread_workspace_git::git_output_at(&prepared.worktree, ["status", "--porcelain"])
            .await?
            .trim()
            .is_empty();
    let has_ignored = has_ignored_local_state(&prepared.worktree).await?;
    let disk_bytes = disk_bytes_of(&prepared.worktree).await.unwrap_or(0);
    let busy = probe_busy(&prepared);
    let (lifecycle_identity, last_used_at, routing_channel_id) = lifecycle_projection(&prepared);

    let mut cache_categories = Vec::new();
    for category in CacheCategoryId::all() {
        match resolve_category_path(&prepared.worktree, *category) {
            Ok(Some(path)) => {
                let bytes = disk_bytes_of(&path).await.unwrap_or(0);
                cache_categories.push(CacheCategoryPreview {
                    id: category.as_str().to_string(),
                    label: category.label().to_string(),
                    bytes,
                    present: true,
                });
            }
            Ok(None) | Err(_) => {
                cache_categories.push(CacheCategoryPreview {
                    id: category.as_str().to_string(),
                    label: category.label().to_string(),
                    bytes: 0,
                    present: false,
                });
            }
        }
    }

    let caps = project_reclaim_capabilities(
        &prepared,
        expected_routing_channel_id.as_deref(),
        dirty,
        busy,
        has_ignored,
    );
    let actionable = caps.can_clear_cache || caps.can_evict;
    let refusal_reason = if !caps.can_evict {
        caps.eviction_refusal.clone()
    } else if !caps.can_clear_cache {
        caps.clear_cache_refusal.clone()
    } else {
        None
    };

    Ok(ProjectWorktreeReclaimPreview {
        worktree_path: path_text(&prepared.worktree)?.to_string(),
        actionable,
        refusal_reason,
        can_clear_cache: caps.can_clear_cache,
        can_evict: caps.can_evict,
        clear_cache_refusal: caps.clear_cache_refusal,
        eviction_refusal: caps.eviction_refusal,
        dirty,
        busy,
        branch_retained: true,
        disk_bytes,
        cache_categories,
        has_ignored_local_state: has_ignored,
        lifecycle_identity,
        last_used_at,
        routing_channel_id,
    })
}

/// Clear allowlisted generated cache directories. Accepts category IDs only.
#[tauri::command]
pub async fn clear_project_worktree_cache(
    repository_path: String,
    worktree_path: String,
    category_ids: Vec<String>,
    expected_routing_channel_id: String,
) -> Result<ClearProjectWorktreeCacheResult, String> {
    let authorized = match authorize_verified_channel_mutation(
        &repository_path,
        &worktree_path,
        &expected_routing_channel_id,
    )
    .await?
    {
        Ok(authorized) => authorized,
        Err(refusal) => {
            return Ok(ClearProjectWorktreeCacheResult {
                worktree_path,
                results: category_ids
                    .into_iter()
                    .map(|id| CacheCategoryClearResult {
                        id,
                        status: ThreadWorkspaceActionStatus::Refused,
                        message: refusal.message.clone(),
                        bytes_removed: 0,
                    })
                    .collect(),
            });
        }
    };

    let mut results = Vec::with_capacity(category_ids.len());
    for raw_id in category_ids {
        let Some(category) = CacheCategoryId::parse(&raw_id) else {
            results.push(CacheCategoryClearResult {
                id: raw_id,
                status: ThreadWorkspaceActionStatus::Refused,
                message: "Unknown cache category.".to_string(),
                bytes_removed: 0,
            });
            continue;
        };
        match resolve_category_path(&authorized.prepared.worktree, category) {
            Ok(Some(path)) => {
                let bytes = disk_bytes_of(&path).await.unwrap_or(0);
                match remove_dir_contents(&path) {
                    Ok(()) => results.push(CacheCategoryClearResult {
                        id: category.as_str().to_string(),
                        status: ThreadWorkspaceActionStatus::Completed,
                        message: format!("Cleared {}.", category.label()),
                        bytes_removed: bytes,
                    }),
                    Err(message) => results.push(CacheCategoryClearResult {
                        id: category.as_str().to_string(),
                        status: ThreadWorkspaceActionStatus::Refused,
                        message,
                        bytes_removed: 0,
                    }),
                }
            }
            Ok(None) => results.push(CacheCategoryClearResult {
                id: category.as_str().to_string(),
                status: ThreadWorkspaceActionStatus::Completed,
                message: format!("{} was not present.", category.label()),
                bytes_removed: 0,
            }),
            Err(message) => results.push(CacheCategoryClearResult {
                id: category.as_str().to_string(),
                status: ThreadWorkspaceActionStatus::Refused,
                message,
                bytes_removed: 0,
            }),
        }
    }

    let path = path_text(&authorized.prepared.worktree)?.to_string();
    drop(authorized.lease);
    Ok(ClearProjectWorktreeCacheResult {
        worktree_path: path,
        results,
    })
}

fn probe_busy(prepared: &PreparedRemoval) -> bool {
    if let Some(root) = prepared.root_event_id.as_deref() {
        match try_acquire_exclusive(&prepared.common_git, root) {
            Ok(_lease) => {}
            Err(LeaseError::Busy) => return true,
            Err(_) => {}
        }
    }
    let holder = PathLeaseHolder {
        root_event_id: prepared
            .root_event_id
            .clone()
            .unwrap_or_else(|| "0".repeat(64)),
        label: "storage-probe".into(),
    };
    match try_acquire_path_exclusive(&prepared.common_git, &prepared.worktree, &holder) {
        Ok(_lease) => false,
        Err(LeaseError::Busy) => true,
        Err(_) => false,
    }
}

fn lifecycle_projection(
    prepared: &PreparedRemoval,
) -> (LifecycleIdentity, Option<i64>, Option<String>) {
    let Some(root) = prepared.root_event_id.as_deref() else {
        return (LifecycleIdentity::Legacy, None, None);
    };
    match read_lifecycle_record(&prepared.common_git, root) {
        Ok(Some(record)) => {
            let path_mismatch = Path::new(&record.worktree_path) != prepared.worktree.as_path();
            let branch_mismatch = record.branch != prepared.branch;
            if path_mismatch || branch_mismatch {
                (
                    LifecycleIdentity::Conflict,
                    Some(record.last_used_at),
                    Some(record.routing_channel_id),
                )
            } else {
                (
                    LifecycleIdentity::Verified,
                    Some(record.last_used_at),
                    Some(record.routing_channel_id),
                )
            }
        }
        Ok(None) => (LifecycleIdentity::Legacy, None, None),
        Err(_) => (LifecycleIdentity::Conflict, None, None),
    }
}

/// Resolve an allowlisted category to a real directory inside the worktree.
/// Returns `Ok(None)` when absent. Refuses any symlink component from the
/// worktree root to the allowlisted target.
pub(crate) fn resolve_category_path(
    worktree: &Path,
    category: CacheCategoryId,
) -> Result<Option<PathBuf>, String> {
    let relative = Path::new(category.relative_path());
    let mut cursor = worktree.to_path_buf();
    for component in relative.components() {
        use std::path::Component;
        match component {
            Component::Normal(name) => {
                cursor.push(name);
                if !cursor.exists() {
                    return Ok(None);
                }
                let meta = fs::symlink_metadata(&cursor)
                    .map_err(|_| "Cache path is not accessible.".to_string())?;
                if meta.file_type().is_symlink() {
                    return Err(
                        "Cache path contains a symlink component and cannot be cleared."
                            .to_string(),
                    );
                }
            }
            _ => return Err("Cache path is invalid.".to_string()),
        }
    }
    let meta =
        fs::symlink_metadata(&cursor).map_err(|_| "Cache path is not accessible.".to_string())?;
    if !meta.is_dir() {
        return Err("Cache path is not a directory.".to_string());
    }
    let canonical =
        fs::canonicalize(&cursor).map_err(|_| "Cache path is not accessible.".to_string())?;
    let worktree_canon =
        fs::canonicalize(worktree).map_err(|_| "Worktree path is not accessible.".to_string())?;
    if !canonical.starts_with(&worktree_canon) {
        return Err("Cache path escapes the worktree.".to_string());
    }
    let expected = worktree_canon.join(category.relative_path());
    if canonical != expected {
        return Err("Cache path resolves unexpectedly.".to_string());
    }
    Ok(Some(canonical))
}

fn remove_dir_contents(path: &Path) -> Result<(), String> {
    fs::remove_dir_all(path).map_err(|_| "Could not clear cache directory.".to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    const CHANNEL: &str = "11111111-1111-1111-1111-111111111111";

    struct Fixture {
        _temp: TempDir,
        repository: PathBuf,
        managed_worktree: PathBuf,
        root: String,
        common_git: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let temp = TempDir::new().expect("temp");
            let root_dir = temp.path();
            let repository = root_dir.join("crew");
            let managed_root = root_dir.join(".buzz-worktrees");
            let managed_worktree = managed_root.join("crew-aaaaaaaaaaaa");
            fs::create_dir_all(&repository).expect("repo");
            fs::create_dir_all(&managed_root).expect("managed root");
            git(&repository, &["init", "-b", "main"]);
            git(&repository, &["config", "user.email", "test@example.com"]);
            git(&repository, &["config", "user.name", "Test"]);
            fs::write(repository.join("README.md"), "fixture").expect("readme");
            git(&repository, &["add", "README.md"]);
            git(&repository, &["commit", "-m", "fixture"]);
            git(
                &repository,
                &[
                    "worktree",
                    "add",
                    "-b",
                    "buzz/aaaaaaaaaaaa",
                    managed_worktree.to_str().expect("utf8"),
                    "HEAD",
                ],
            );
            let root = "a".repeat(64);
            git(
                &repository,
                &[
                    "config",
                    "branch.buzz/aaaaaaaaaaaa.buzzThreadRoot",
                    root.as_str(),
                ],
            );
            let common_git = git_common(&repository);
            buzz_worktree::adopt_or_create_record(
                &common_git,
                &root,
                CHANNEL,
                None,
                "buzz/aaaaaaaaaaaa",
                managed_worktree.to_str().expect("utf8"),
                None,
            )
            .expect("record");
            Self {
                _temp: temp,
                repository,
                managed_worktree,
                root,
                common_git,
            }
        }

        fn repo(&self) -> String {
            self.repository.to_string_lossy().into_owned()
        }

        fn managed_path(&self) -> String {
            self.managed_worktree.to_string_lossy().into_owned()
        }
    }

    #[test]
    fn resolves_cargo_target_inside_worktree() {
        let fixture = Fixture::new();
        let target = fixture.managed_worktree.join("target");
        fs::create_dir_all(&target).expect("target");
        fs::write(target.join("x"), "1").expect("file");
        let resolved =
            resolve_category_path(&fixture.managed_worktree, CacheCategoryId::CargoTarget)
                .expect("resolve")
                .expect("present");
        assert_eq!(resolved, fs::canonicalize(&target).unwrap());
    }

    #[test]
    fn refuses_symlink_cache_path() {
        let fixture = Fixture::new();
        let outside = fixture.repository.join("outside-target");
        fs::create_dir_all(&outside).expect("outside");
        let link = fixture.managed_worktree.join("target");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");
        #[cfg(not(unix))]
        {
            let _ = (outside, link);
            return;
        }
        let err = resolve_category_path(&fixture.managed_worktree, CacheCategoryId::CargoTarget)
            .expect_err("symlink refused");
        assert!(err.to_lowercase().contains("symlink"));
    }

    #[test]
    fn refuses_intermediate_symlink_component() {
        let fixture = Fixture::new();
        let outside = fixture.repository.join("outside-desktop");
        fs::create_dir_all(outside.join("src-tauri/target")).expect("outside");
        let desktop = fixture.managed_worktree.join("desktop");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &desktop).expect("symlink desktop");
            let err = resolve_category_path(
                &fixture.managed_worktree,
                CacheCategoryId::DesktopTauriTarget,
            )
            .expect_err("intermediate symlink refused");
            assert!(err.to_lowercase().contains("symlink"));
        }
        #[cfg(not(unix))]
        {
            let _ = (outside, desktop);
        }
    }

    #[test]
    fn refuses_unknown_category_id() {
        assert!(CacheCategoryId::parse("../etc").is_none());
        assert!(CacheCategoryId::parse("cargo-target").is_some());
    }

    #[tokio::test]
    async fn clears_cargo_target_leaves_source() {
        let fixture = Fixture::new();
        let target = fixture.managed_worktree.join("target");
        fs::create_dir_all(&target).expect("target");
        fs::write(target.join("artifact"), "big").expect("artifact");
        let source = fixture.managed_worktree.join("src-keep.txt");
        fs::write(&source, "keep").expect("source");

        let result = clear_project_worktree_cache(
            fixture.repo(),
            fixture.managed_path(),
            vec!["cargo-target".to_string()],
            CHANNEL.to_string(),
        )
        .await
        .expect("clear");
        assert_eq!(result.results.len(), 1);
        assert_eq!(
            result.results[0].status,
            ThreadWorkspaceActionStatus::Completed
        );
        assert!(!target.exists());
        assert_eq!(fs::read_to_string(&source).unwrap(), "keep");
    }

    #[tokio::test]
    async fn dirty_does_not_block_cache_clear() {
        let fixture = Fixture::new();
        let target = fixture.managed_worktree.join("target");
        fs::create_dir_all(&target).expect("target");
        fs::write(target.join("a"), "1").expect("a");
        fs::write(fixture.managed_worktree.join("dirty.txt"), "dirty").expect("dirty");

        let result = clear_project_worktree_cache(
            fixture.repo(),
            fixture.managed_path(),
            vec!["cargo-target".to_string()],
            CHANNEL.to_string(),
        )
        .await
        .expect("clear");
        assert_eq!(
            result.results[0].status,
            ThreadWorkspaceActionStatus::Completed
        );
        assert!(!target.exists());
        assert!(fixture.managed_worktree.join("dirty.txt").exists());
    }

    #[tokio::test]
    async fn other_channel_cannot_clear_cache() {
        let fixture = Fixture::new();
        let target = fixture.managed_worktree.join("target");
        fs::create_dir_all(&target).expect("target");
        let result = clear_project_worktree_cache(
            fixture.repo(),
            fixture.managed_path(),
            vec!["cargo-target".to_string()],
            "22222222-2222-2222-2222-222222222222".to_string(),
        )
        .await
        .expect("clear");
        assert_eq!(
            result.results[0].status,
            ThreadWorkspaceActionStatus::Refused
        );
        assert!(target.is_dir());
        let _ = (&fixture.root, &fixture.common_git);
    }

    #[tokio::test]
    async fn preview_separates_cache_and_eviction_when_dirty() {
        let fixture = Fixture::new();
        fs::write(fixture.managed_worktree.join("dirty.txt"), "dirty").expect("dirty");
        let preview = preview_project_worktree_reclaim(
            fixture.repo(),
            fixture.managed_path(),
            Some(CHANNEL.to_string()),
        )
        .await
        .expect("preview");
        assert!(preview.dirty);
        assert!(preview.can_clear_cache);
        assert!(!preview.can_evict);
        assert!(preview
            .eviction_refusal
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("uncommitted"));
    }

    #[tokio::test]
    async fn ignored_only_state_blocks_eviction_preview_but_allows_cache_clear() {
        let fixture = Fixture::new();
        fs::write(
            fixture.managed_worktree.join(".gitignore"),
            "ignored-local/\ntarget/\n",
        )
        .expect("gitignore");
        git(&fixture.managed_worktree, &["add", ".gitignore"]);
        git(&fixture.managed_worktree, &["commit", "-m", "ignore"]);
        let ignored = fixture.managed_worktree.join("ignored-local");
        fs::create_dir_all(&ignored).expect("ignored");
        fs::write(ignored.join("secret.txt"), "secret").expect("secret");
        let target = fixture.managed_worktree.join("target");
        fs::create_dir_all(&target).expect("target");
        fs::write(target.join("artifact"), "big").expect("artifact");

        let preview = preview_project_worktree_reclaim(
            fixture.repo(),
            fixture.managed_path(),
            Some(CHANNEL.to_string()),
        )
        .await
        .expect("preview");
        assert!(!preview.dirty);
        assert!(preview.has_ignored_local_state);
        assert!(preview.can_clear_cache);
        assert!(!preview.can_evict);
        assert!(preview
            .eviction_refusal
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("ignored"));

        let clear = clear_project_worktree_cache(
            fixture.repo(),
            fixture.managed_path(),
            vec!["cargo-target".to_string()],
            CHANNEL.to_string(),
        )
        .await
        .expect("clear");
        assert_eq!(
            clear.results[0].status,
            ThreadWorkspaceActionStatus::Completed
        );
        assert!(!target.exists());
        assert!(ignored.join("secret.txt").is_file());
        assert!(fixture.managed_worktree.is_dir());
    }

    #[tokio::test]
    async fn unknown_category_refused() {
        let fixture = Fixture::new();
        let result = clear_project_worktree_cache(
            fixture.repo(),
            fixture.managed_path(),
            vec!["not-a-category".to_string()],
            CHANNEL.to_string(),
        )
        .await
        .expect("clear");
        assert_eq!(
            result.results[0].status,
            ThreadWorkspaceActionStatus::Refused
        );
    }

    fn git_common(repo: &Path) -> PathBuf {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--git-common-dir"])
            .output()
            .expect("git common");
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).expect("utf8");
        let path = PathBuf::from(text.trim());
        if path.is_absolute() {
            path
        } else {
            repo.join(path)
        }
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git starts");
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
