//! Cowork Versions commands: list, restore, compact, init.
//!
//! Copy stays in the business register. Restore takes the path-keyed exclusive
//! lease so a running agent turn cannot have the folder yanked out from under it.

use std::path::{Path, PathBuf};

use buzz_cowork::{
    CheckpointKind, CheckpointSpec, ExclusionNotice, ShadowRepo, VersionEntry,
    DEFAULT_COMPACT_KEEP_DAYS, DEFAULT_SIZE_THRESHOLD,
};
use buzz_worktree::{read_path_lease_holder, try_acquire_path_exclusive, PathLeaseHolder};
use serde::Serialize;
use tauri::{AppHandle, Manager};

const RESTORE_LEASE_ROOT: &str = "c0de188c0de188c0de188c0de188c0de188c0de188c0de188c0de188c0de188c";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoworkVersionsSnapshot {
    pub versions: Vec<VersionEntry>,
    pub excluded: Vec<ExclusionNotice>,
    pub notice: Option<String>,
    pub rebuilt: bool,
    pub size_threshold_bytes: u64,
}

fn history_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("cowork-history"))
        .map_err(|error| error.to_string())
}

fn open_repo(
    history_root: &Path,
    repo_address: &str,
    folder: &str,
) -> Result<(ShadowRepo, bool, Option<String>), String> {
    let folder = PathBuf::from(folder.trim());
    let opened = ShadowRepo::open_or_init(history_root, repo_address.trim(), &folder, None)
        .map_err(|error| error.to_string())?;
    Ok((opened.repo, opened.rebuilt, opened.notice))
}

fn restore_holder() -> PathLeaseHolder {
    PathLeaseHolder {
        root_event_id: RESTORE_LEASE_ROOT.to_string(),
        label: "Versions restore".into(),
    }
}

fn acquire_restore_lease(
    git_dir: &Path,
    folder: &Path,
) -> Result<buzz_worktree::PathExclusiveLease, String> {
    match try_acquire_path_exclusive(git_dir, folder, &restore_holder()) {
        Ok(lease) => Ok(lease),
        Err(buzz_worktree::LeaseError::Busy) => {
            let label = read_path_lease_holder(git_dir, folder)
                .map(|holder| holder.label)
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| "another thread".into());
            Err(format!("Can't restore while thread '{label}' is working"))
        }
        Err(error) => Err(error.to_string()),
    }
}

fn before_restore_spec() -> CheckpointSpec {
    CheckpointSpec {
        kind: CheckpointKind::Restore,
        agent_name: Some("you".into()),
        thread_title: None,
        thread_id: None,
        turn_seq: None,
    }
}

#[tauri::command]
pub fn init_cowork_history(
    app: AppHandle,
    repo_address: String,
    folder: String,
) -> Result<CoworkVersionsSnapshot, String> {
    snapshot(&history_root(&app)?, &repo_address, &folder)
}

#[tauri::command]
pub fn list_cowork_versions(
    app: AppHandle,
    repo_address: String,
    folder: String,
) -> Result<CoworkVersionsSnapshot, String> {
    snapshot(&history_root(&app)?, &repo_address, &folder)
}

#[tauri::command]
pub fn restore_cowork_file(
    app: AppHandle,
    repo_address: String,
    folder: String,
    commit: String,
    relative_path: String,
) -> Result<CoworkVersionsSnapshot, String> {
    let root = history_root(&app)?;
    let (repo, _, _) = open_repo(&root, &repo_address, &folder)?;
    let _lease = acquire_restore_lease(repo.git_dir(), repo.work_tree())?;
    repo.restore_file(&commit, &relative_path, &before_restore_spec())
        .map_err(|error| error.to_string())?;
    snapshot(&root, &repo_address, &folder)
}

#[tauri::command]
pub fn restore_cowork_folder(
    app: AppHandle,
    repo_address: String,
    folder: String,
    commit: String,
) -> Result<CoworkVersionsSnapshot, String> {
    let root = history_root(&app)?;
    let (repo, _, _) = open_repo(&root, &repo_address, &folder)?;
    let _lease = acquire_restore_lease(repo.git_dir(), repo.work_tree())?;
    repo.restore_folder(&commit, &before_restore_spec())
        .map_err(|error| error.to_string())?;
    snapshot(&root, &repo_address, &folder)
}

#[tauri::command]
pub fn compact_cowork_history(
    app: AppHandle,
    repo_address: String,
    folder: String,
    keep_days: Option<u64>,
) -> Result<CoworkVersionsSnapshot, String> {
    let root = history_root(&app)?;
    let (repo, _, _) = open_repo(&root, &repo_address, &folder)?;
    let _lease = acquire_restore_lease(repo.git_dir(), repo.work_tree())?;
    repo.compact(keep_days.unwrap_or(DEFAULT_COMPACT_KEEP_DAYS))
        .map_err(|error| error.to_string())?;
    snapshot(&root, &repo_address, &folder)
}

fn snapshot(
    history_root: &Path,
    repo_address: &str,
    folder: &str,
) -> Result<CoworkVersionsSnapshot, String> {
    let (repo, rebuilt, notice) = open_repo(history_root, repo_address, folder)?;
    let notice = notice.or_else(|| repo.last_notice().ok().flatten());
    Ok(CoworkVersionsSnapshot {
        versions: repo.list_versions().map_err(|error| error.to_string())?,
        excluded: repo.excluded_files().map_err(|error| error.to_string())?,
        notice,
        rebuilt,
        size_threshold_bytes: DEFAULT_SIZE_THRESHOLD,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn restore_during_held_lease_is_named_refusal() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("docs");
        let history = temp.path().join("history");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.txt"), "one").unwrap();
        let (repo, _, _) = open_repo(&history, "30617:ab:docs", folder.to_str().unwrap()).unwrap();
        let holder = PathLeaseHolder {
            root_event_id: "ab".repeat(32),
            label: "Q3 proposal".into(),
        };
        let _lease = try_acquire_path_exclusive(repo.git_dir(), repo.work_tree(), &holder).unwrap();
        let error = acquire_restore_lease(repo.git_dir(), repo.work_tree()).unwrap_err();
        assert!(
            error.contains("Can't restore while thread 'Q3 proposal' is working"),
            "{error}"
        );
    }

    #[test]
    fn restore_round_trip_under_lease() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("docs");
        let history = temp.path().join("history");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.txt"), "one").unwrap();
        let (repo, _, _) = open_repo(&history, "30617:ab:docs", folder.to_str().unwrap()).unwrap();
        fs::write(folder.join("a.txt"), "two").unwrap();
        repo.checkpoint(&CheckpointSpec {
            kind: CheckpointKind::Turn,
            agent_name: Some("Hermes".into()),
            thread_title: Some("Draft".into()),
            thread_id: Some("ab".repeat(32)),
            turn_seq: Some(1),
        })
        .unwrap();
        let baseline = repo
            .list_versions()
            .unwrap()
            .into_iter()
            .find(|entry| entry.kind == CheckpointKind::Baseline)
            .unwrap()
            .id;
        let _lease = acquire_restore_lease(repo.git_dir(), repo.work_tree()).unwrap();
        repo.restore_file(&baseline, "a.txt", &before_restore_spec())
            .unwrap();
        assert_eq!(fs::read_to_string(folder.join("a.txt")).unwrap(), "one");
    }
}
