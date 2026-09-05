//! Aggregate Local storage view for managed worktrees (#174).
//!
//! Enumerates managed checkouts across repository paths with lifecycle, PR
//! state, dual clocks, and Lean/Hibernate classification. Measurement is lazy
//! and concurrency-bounded. Mutation still goes through existing #72 commands.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::AppHandle;
use tokio::sync::Semaphore;

use super::project_worktree_details::branch_is_pushed;
use super::project_worktree_reclaim::preview_project_worktree_reclaim;
use super::project_worktree_registry::{
    get_project_worktree_registry, LifecycleIdentity, ProjectWorktreeEntry,
};
use super::project_worktree_registry_parse::ProjectWorktreeKind;
use super::worktree_storage_alive::load_intervals_and_threshold;
use super::worktree_storage_policy::{
    classify_reclaim_candidate, pr_link_state, AliveInterval, PolicyInput, PrLinkState, ReclaimTier,
};

/// Bound concurrent `du` / preview work so opening Storage cannot thrash disk.
const MEASUREMENT_CONCURRENCY: usize = 2;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeStorageRow {
    pub repository_path: String,
    pub worktree_path: String,
    pub worktree_name: String,
    pub branch: Option<String>,
    pub root_event_id: Option<String>,
    pub routing_channel_id: Option<String>,
    pub lifecycle_identity: LifecycleIdentity,
    pub pr_number: Option<u64>,
    pub pr_state: Option<String>,
    pub pr_title: Option<String>,
    pub last_used_at: Option<i64>,
    pub observed_idle_secs: i64,
    pub wall_idle_secs: Option<i64>,
    pub dirty: bool,
    pub busy: bool,
    pub branch_pushed: bool,
    pub disk_bytes: u64,
    pub cache_bytes: u64,
    pub checkout_bytes: u64,
    pub cache_category_ids: Vec<String>,
    pub candidate: bool,
    pub tier: Option<ReclaimTier>,
    pub reason: String,
    pub read_only: bool,
    pub refusal_reason: Option<String>,
    pub can_clear_cache: bool,
    pub can_evict: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeStorageSnapshot {
    pub rows: Vec<WorktreeStorageRow>,
    pub total_disk_bytes: u64,
    pub total_cache_bytes: u64,
    pub reclaimable_bytes: u64,
    pub candidate_count: u64,
    pub recent_absence_secs: i64,
    pub idle_threshold_secs: i64,
    pub observed_now: i64,
    pub intervals: Vec<AliveInterval>,
}

/// List managed worktrees across repositories with policy classification.
#[tauri::command]
pub async fn get_worktree_storage_snapshot(
    app: AppHandle,
    repository_paths: Vec<String>,
    idle_threshold_secs: Option<i64>,
) -> Result<WorktreeStorageSnapshot, String> {
    let (intervals, stored_threshold, recent_absence_secs) = load_intervals_and_threshold(&app)?;
    let threshold = idle_threshold_secs
        .filter(|value| *value >= 3600)
        .unwrap_or(stored_threshold)
        .max(3600);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    // Deduplicate repository paths while preserving order.
    let mut seen = std::collections::HashSet::new();
    let mut unique_paths = Vec::new();
    for path in repository_paths {
        let trimmed = path.trim().to_string();
        if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
            continue;
        }
        unique_paths.push(trimmed);
    }

    let mut pending = Vec::new();
    for repository_path in unique_paths {
        let registry = match get_project_worktree_registry(repository_path.clone()).await {
            Ok(registry) => registry,
            Err(_) => continue,
        };
        for entry in registry.entries {
            if entry.kind != ProjectWorktreeKind::Managed {
                continue;
            }
            pending.push((registry.repository_path.clone(), entry));
        }
    }

    let semaphore = Arc::new(Semaphore::new(MEASUREMENT_CONCURRENCY));
    let mut handles = Vec::with_capacity(pending.len());
    for (repository_path, entry) in pending {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "Measurement concurrency interrupted.".to_string())?;
        let intervals = intervals.clone();
        handles.push(tokio::spawn(async move {
            let row = measure_row(repository_path, entry, &intervals, now, threshold).await;
            drop(permit);
            row
        }));
    }

    let mut rows = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(Ok(row)) => rows.push(row),
            Ok(Err(_)) => {}
            Err(_) => {}
        }
    }

    rows.sort_by(|left, right| {
        // Candidates first by reclaimable size, then read-only refusals, then rest.
        let left_bytes = reclaimable_bytes_for(left);
        let right_bytes = reclaimable_bytes_for(right);
        right_bytes
            .cmp(&left_bytes)
            .then_with(|| left.worktree_name.cmp(&right.worktree_name))
    });

    let total_disk_bytes = rows.iter().map(|row| row.disk_bytes).sum();
    let total_cache_bytes = rows.iter().map(|row| row.cache_bytes).sum();
    let reclaimable_bytes = rows.iter().map(reclaimable_bytes_for).sum();
    let candidate_count = rows.iter().filter(|row| row.candidate).count() as u64;

    Ok(WorktreeStorageSnapshot {
        rows,
        total_disk_bytes,
        total_cache_bytes,
        reclaimable_bytes,
        candidate_count,
        recent_absence_secs,
        idle_threshold_secs: threshold,
        observed_now: now,
        intervals,
    })
}

fn reclaimable_bytes_for(row: &WorktreeStorageRow) -> u64 {
    if !row.candidate {
        return 0;
    }
    match row.tier {
        Some(ReclaimTier::Hibernate) => row.disk_bytes,
        Some(ReclaimTier::Lean) => row.cache_bytes,
        None => 0,
    }
}

async fn measure_row(
    repository_path: String,
    entry: ProjectWorktreeEntry,
    intervals: &[AliveInterval],
    now: i64,
    threshold: i64,
) -> Result<WorktreeStorageRow, String> {
    // Managed-path guard lives inside preview via prepare_managed_removal.
    let preview = preview_project_worktree_reclaim(
        repository_path.clone(),
        entry.worktree_path.clone(),
        entry.routing_channel_id.clone(),
    )
    .await?;

    let branch_pushed = branch_is_pushed(Path::new(&preview.worktree_path)).await;
    let cache_bytes: u64 = preview
        .cache_categories
        .iter()
        .filter(|category| category.present)
        .map(|category| category.bytes)
        .sum();
    let cache_category_ids: Vec<String> = preview
        .cache_categories
        .iter()
        .filter(|category| category.present && category.bytes > 0)
        .map(|category| category.id.clone())
        .collect();
    let checkout_bytes = preview.disk_bytes.saturating_sub(cache_bytes);

    let pr_pairs: Vec<(u64, &str)> = entry
        .pull_requests
        .iter()
        .map(|pr| (pr.number, pr.state.as_str()))
        .collect();
    let pr = pr_link_state(&pr_pairs);
    let (pr_number, pr_state, pr_title) = match entry.pull_requests.first() {
        Some(pr) => (
            Some(pr.number),
            Some(pr.state.clone()),
            Some(pr.title.clone()),
        ),
        None => (None, None, None),
    };
    // Prefer merged PR identity in the row when present.
    let (pr_number, pr_state, pr_title) = if let PrLinkState::Merged { number } = pr {
        let merged = entry
            .pull_requests
            .iter()
            .find(|row| row.number == number)
            .or(entry.pull_requests.first());
        (
            Some(number),
            Some("MERGED".to_string()),
            merged.map(|row| row.title.clone()),
        )
    } else {
        (pr_number, pr_state, pr_title)
    };

    let last_used_at = git_common_dir(Path::new(&repository_path))
        .and_then(|common| {
            buzz_worktree::max_last_used_at_for_path(&common, Path::new(&entry.worktree_path))
        })
        .or(preview.last_used_at)
        .or(entry.last_used_at);
    let class = classify_reclaim_candidate(&PolicyInput {
        last_used_at,
        intervals,
        now,
        idle_threshold_secs: threshold,
        pr,
        dirty: preview.dirty,
        busy: preview.busy,
        branch_pushed,
        can_clear_cache: preview.can_clear_cache,
        can_evict: preview.can_evict,
        clear_cache_refusal: preview.clear_cache_refusal.as_deref(),
        eviction_refusal: preview.eviction_refusal.as_deref(),
        cache_bytes,
    });

    Ok(WorktreeStorageRow {
        repository_path,
        worktree_path: entry.worktree_path,
        worktree_name: entry.worktree_name,
        branch: entry.branch,
        root_event_id: entry.root_event_id,
        routing_channel_id: entry.routing_channel_id,
        lifecycle_identity: entry.lifecycle_identity,
        pr_number,
        pr_state,
        pr_title,
        last_used_at,
        observed_idle_secs: class.observed_idle_secs,
        wall_idle_secs: class.wall_idle_secs,
        dirty: preview.dirty,
        busy: preview.busy,
        branch_pushed,
        disk_bytes: preview.disk_bytes,
        cache_bytes,
        checkout_bytes,
        cache_category_ids,
        candidate: class.candidate,
        tier: class.tier,
        reason: class.reason,
        read_only: class.read_only,
        refusal_reason: class.refusal_reason,
        can_clear_cache: preview.can_clear_cache,
        can_evict: preview.can_evict,
    })
}

fn git_common_dir(repo: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    let path = Path::new(raw.trim());
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo.join(path)
    };
    std::fs::canonicalize(resolved).ok()
}

/// Revalidate a suggested row immediately before mutation (TOCTOU).
///
/// Returns Ok(None) when still actionable for `tier`; Ok(Some(refusal)) when
/// the row must be skipped. Hard path errors surface as Err.
#[tauri::command]
pub async fn revalidate_worktree_storage_action(
    repository_path: String,
    worktree_path: String,
    expected_routing_channel_id: String,
    tier: ReclaimTier,
) -> Result<Option<String>, String> {
    let preview = preview_project_worktree_reclaim(
        repository_path,
        worktree_path,
        Some(expected_routing_channel_id),
    )
    .await?;
    match tier {
        ReclaimTier::Lean => {
            if preview.can_clear_cache {
                Ok(None)
            } else {
                Ok(Some(
                    preview
                        .clear_cache_refusal
                        .unwrap_or_else(|| "Cache clear refused.".to_string()),
                ))
            }
        }
        ReclaimTier::Hibernate => {
            if preview.can_evict && !preview.dirty {
                Ok(None)
            } else {
                Ok(Some(
                    preview
                        .eviction_refusal
                        .or(preview.clear_cache_refusal)
                        .unwrap_or_else(|| {
                            if preview.dirty {
                                "skipped: became dirty".to_string()
                            } else {
                                "Eviction refused.".to_string()
                            }
                        }),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::project_worktree_cleanup::evict_project_worktree;
    use crate::commands::thread_workspace::ThreadWorkspaceActionStatus;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use tempfile::TempDir;

    const CHANNEL: &str = "11111111-1111-1111-1111-111111111111";

    struct Fixture {
        // Exclude tests that temporarily replace PATH while git operations run.
        _path_guard: std::sync::MutexGuard<'static, ()>,
        _temp: TempDir,
        repository: PathBuf,
        managed_worktree: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let path_guard = crate::managed_agents::lock_path_mutex();
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
            let common = git_stdout(&repository, &["rev-parse", "--git-common-dir"]);
            let common_git = if Path::new(common.trim()).is_absolute() {
                PathBuf::from(common.trim())
            } else {
                repository.join(common.trim())
            };
            let common_git = fs::canonicalize(common_git).expect("common");
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
                _path_guard: path_guard,
                _temp: temp,
                repository,
                managed_worktree,
            }
        }
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_stdout(cwd: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    #[tokio::test]
    async fn aggregate_respects_managed_path_guard() {
        let fixture = Fixture::new();
        let outside = fixture._temp.path().join("outside");
        fs::create_dir_all(&outside).expect("outside");
        let err = preview_project_worktree_reclaim(
            fixture.repository.to_string_lossy().to_string(),
            outside.to_string_lossy().to_string(),
            Some(CHANNEL.to_string()),
        )
        .await
        .expect_err("must refuse unmanaged path");
        assert!(
            err.to_ascii_lowercase().contains("managed")
                || err.to_ascii_lowercase().contains("worktree")
                || err.to_ascii_lowercase().contains("buzz"),
            "unexpected guard message: {err}"
        );
    }

    #[tokio::test]
    async fn toctou_revalidation_refuses_row_that_became_dirty() {
        let fixture = Fixture::new();
        let repo = fixture.repository.to_string_lossy().to_string();
        let worktree = fixture.managed_worktree.to_string_lossy().to_string();

        // Suggestion time: clean → hibernate would be allowed by preview.
        let preview = preview_project_worktree_reclaim(
            repo.clone(),
            worktree.clone(),
            Some(CHANNEL.to_string()),
        )
        .await
        .expect("preview");
        assert!(preview.can_evict);
        assert!(!preview.dirty);

        // Race: dirty between suggest and run.
        fs::write(fixture.managed_worktree.join("dirty.txt"), "race").expect("dirty");

        let refusal = revalidate_worktree_storage_action(
            repo.clone(),
            worktree.clone(),
            CHANNEL.to_string(),
            ReclaimTier::Hibernate,
        )
        .await
        .expect("revalidate");
        assert!(refusal.is_some(), "dirty race must refuse hibernate");

        let result = evict_project_worktree(repo, worktree, CHANNEL.to_string())
            .await
            .expect("evict call");
        assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
        assert!(fixture.managed_worktree.exists(), "checkout must remain");
    }

    #[test]
    fn default_threshold_is_48_observed_hours() {
        assert_eq!(
            crate::commands::worktree_storage_policy::DEFAULT_IDLE_THRESHOLD_SECS,
            48 * 3600
        );
    }
}
