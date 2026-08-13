//! Always-on shadow-git versioning for Crew Cowork folders.
//!
//! The user's folder stays byte-clean (no `.git`). History lives at
//! `git --git-dir=<app-data>/cowork-history/<id>.git --work-tree=<folder>`.

#![deny(unsafe_code)]

mod error;
mod git;
mod paths;
mod repo;

pub use error::CoworkError;
pub use paths::{
    history_dir_from_env, history_git_dir, project_history_id, CORRUPTION_NOTICE,
    DEFAULT_COMPACT_KEEP_DAYS, DEFAULT_SIZE_THRESHOLD, HISTORY_DIR_ENV,
};
pub use repo::{
    CheckpointKind, CheckpointSpec, ExclusionNotice, OpenOutcome, ShadowRepo, VersionEntry,
};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    fn setup() -> (tempfile::TempDir, ShadowRepo) {
        let temp = tempfile::tempdir().expect("temp");
        let folder = temp.path().join("docs");
        let history = temp.path().join("history");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("proposal.txt"), "v1").unwrap();
        let opened =
            ShadowRepo::open_or_init(&history, "30617:ab:docs", &folder, None).expect("init");
        assert!(!opened.rebuilt);
        (temp, opened.repo)
    }

    fn listing_without_dot_git(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn spec_turn(seq: u64, agent: &str, title: &str, thread: &str) -> CheckpointSpec {
        CheckpointSpec {
            kind: CheckpointKind::Turn,
            agent_name: Some(agent.into()),
            thread_title: Some(title.into()),
            thread_id: Some(thread.into()),
            turn_seq: Some(seq),
        }
    }

    fn spec_external() -> CheckpointSpec {
        CheckpointSpec {
            kind: CheckpointKind::External,
            agent_name: None,
            thread_title: None,
            thread_id: None,
            turn_seq: None,
        }
    }

    fn spec_restore(agent: &str) -> CheckpointSpec {
        CheckpointSpec {
            kind: CheckpointKind::Restore,
            agent_name: Some(agent.into()),
            thread_title: None,
            thread_id: None,
            turn_seq: None,
        }
    }

    #[test]
    fn open_or_init_creates_missing_history_git_dir() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("docs");
        let history = temp.path().join("history");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.txt"), "one").unwrap();
        assert!(!history.exists());
        let repo = ShadowRepo::open_or_init(&history, "30617:ab:docs", &folder, None)
            .unwrap()
            .repo;
        assert!(history.is_dir(), "history root must be created");
        assert!(
            repo.git_dir().is_dir(),
            "Versions git dir missing: {}",
            repo.git_dir().display()
        );
        assert!(repo.git_dir().join("HEAD").exists());
        assert!(!folder.join(".git").exists());
    }

    #[test]
    fn folder_stays_byte_clean_no_dot_git() {
        let (_temp, repo) = setup();
        let names = listing_without_dot_git(repo.work_tree());
        assert_eq!(names, vec!["proposal.txt".to_string()]);
        assert!(!repo.work_tree().join(".git").exists());
        assert!(repo.git_dir().exists());
        assert!(repo.git_dir().join("HEAD").exists());
    }

    #[test]
    fn pre_turn_commits_only_when_dirty() {
        let (_temp, repo) = setup();
        assert!(!repo.is_dirty().unwrap(), "baseline should be clean");
        assert!(repo.checkpoint(&spec_external()).unwrap().is_none());

        fs::write(repo.work_tree().join("proposal.txt"), "v2-user").unwrap();
        let id = repo
            .checkpoint(&spec_external())
            .unwrap()
            .expect("external commit");
        assert!(!id.is_empty());
        let versions = repo.list_versions().unwrap();
        assert!(versions
            .iter()
            .any(|entry| entry.kind == CheckpointKind::External
                && entry.summary == "External changes"));
    }

    #[test]
    fn post_turn_commits_changes_and_skips_empty() {
        let (_temp, repo) = setup();
        let thread = "aa".repeat(32);
        assert!(repo
            .checkpoint(&spec_turn(1, "Hermes", "Q3 proposal", &thread))
            .unwrap()
            .is_none());

        fs::write(repo.work_tree().join("proposal.txt"), "agent rewrite").unwrap();
        let id = repo
            .checkpoint(&spec_turn(1, "Hermes", "Q3 proposal", &thread))
            .unwrap()
            .expect("turn commit");
        let versions = repo.list_versions().unwrap();
        let turn = versions
            .iter()
            .find(|entry| entry.id == id)
            .expect("listed");
        assert_eq!(turn.kind, CheckpointKind::Turn);
        assert_eq!(turn.summary, "Turn 1 — Hermes · thread 'Q3 proposal'");
        assert_eq!(turn.agent_name.as_deref(), Some("Hermes"));
        assert_eq!(repo.next_turn_seq(&thread).unwrap(), 2);
    }

    #[test]
    fn attribution_matrix_agent_external_restore() {
        let (_temp, repo) = setup();
        let thread = "bb".repeat(32);
        fs::write(repo.work_tree().join("notes.txt"), "human").unwrap();
        repo.checkpoint(&spec_external()).unwrap();
        fs::write(repo.work_tree().join("notes.txt"), "agent").unwrap();
        repo.checkpoint(&spec_turn(1, "Codex", "Draft", &thread))
            .unwrap();
        fs::write(repo.work_tree().join("notes.txt"), "oops").unwrap();
        repo.checkpoint(&spec_restore("Codex")).unwrap();

        let kinds: Vec<_> = repo
            .list_versions()
            .unwrap()
            .into_iter()
            .map(|entry| entry.kind)
            .collect();
        assert!(kinds.contains(&CheckpointKind::External));
        assert!(kinds.contains(&CheckpointKind::Turn));
        assert!(kinds.contains(&CheckpointKind::Restore));
        assert!(kinds.contains(&CheckpointKind::Baseline));

        let log = std::process::Command::new("git")
            .args([
                "--git-dir",
                repo.git_dir().to_str().unwrap(),
                "--work-tree",
                repo.work_tree().to_str().unwrap(),
                "log",
                "--format=%an %s",
            ])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&log.stdout);
        assert!(text.contains("Codex Turn 1"));
        assert!(text.contains("External changes External changes"));
        assert!(text.contains("NuncioCrew Before restore"));
    }

    #[test]
    fn size_threshold_excludes_and_surfaces() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().join("docs");
        let history = temp.path().join("history");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("small.txt"), "ok").unwrap();
        fs::write(folder.join("huge.bin"), vec![0u8; 64]).unwrap();
        let repo = ShadowRepo::open_or_init(&history, "30617:ab:docs", &folder, Some(32))
            .unwrap()
            .repo;
        let excluded = repo.excluded_files().unwrap();
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].path, "huge.bin");
        assert!(excluded[0].size_bytes > 32);

        let versions = repo.list_versions().unwrap();
        let baseline = versions
            .iter()
            .find(|entry| entry.kind == CheckpointKind::Baseline)
            .unwrap();
        assert!(
            !baseline.files_changed.iter().any(|path| path == "huge.bin"),
            "over-limit file must not be versioned: {:?}",
            baseline.files_changed
        );
        assert!(!folder.join(".git").exists());
    }

    #[test]
    fn corruption_rebuilds_loudly() {
        let (temp, repo) = setup();
        fs::write(repo.work_tree().join("kept.txt"), "still here").unwrap();
        let git_dir = repo.git_dir().to_path_buf();
        fs::write(git_dir.join("HEAD"), "not-a-ref").unwrap();
        let objects = git_dir.join("objects");
        if objects.is_dir() {
            let _ = fs::remove_dir_all(&objects);
            fs::create_dir_all(&objects).unwrap();
        }

        let opened = ShadowRepo::open_or_init(
            temp.path().join("history").as_path(),
            "30617:ab:docs",
            repo.work_tree(),
            None,
        )
        .expect("rebuild");
        assert!(opened.rebuilt);
        assert_eq!(opened.notice.as_deref(), Some(CORRUPTION_NOTICE));
        assert_eq!(
            opened.repo.last_notice().unwrap().as_deref(),
            Some(CORRUPTION_NOTICE)
        );
        assert_eq!(
            fs::read_to_string(repo.work_tree().join("proposal.txt")).unwrap(),
            "v1"
        );
        assert!(!repo.work_tree().join(".git").exists());
    }

    #[test]
    fn restore_file_round_trips_and_checkpoints_first() {
        let (_temp, repo) = setup();
        let thread = "cc".repeat(32);
        fs::write(repo.work_tree().join("proposal.txt"), "v2").unwrap();
        let turn = repo
            .checkpoint(&spec_turn(1, "Hermes", "Draft", &thread))
            .unwrap()
            .unwrap();
        fs::write(repo.work_tree().join("proposal.txt"), "v3-bad").unwrap();
        repo.checkpoint(&spec_turn(2, "Hermes", "Draft", &thread))
            .unwrap();

        let before_count = repo.list_versions().unwrap().len();
        repo.restore_file(&turn, "proposal.txt", &spec_restore("Hermes"))
            .unwrap();
        assert_eq!(
            fs::read_to_string(repo.work_tree().join("proposal.txt")).unwrap(),
            "v2"
        );
        assert!(repo.list_versions().unwrap().len() >= before_count);
        assert!(repo
            .list_versions()
            .unwrap()
            .iter()
            .any(|entry| entry.kind == CheckpointKind::Restore));
    }

    #[test]
    fn restore_folder_then_restore_back() {
        let (_temp, repo) = setup();
        let baseline = repo
            .list_versions()
            .unwrap()
            .into_iter()
            .find(|entry| entry.kind == CheckpointKind::Baseline)
            .unwrap()
            .id;
        fs::write(repo.work_tree().join("proposal.txt"), "later").unwrap();
        fs::write(repo.work_tree().join("extra.txt"), "new").unwrap();
        let after = repo.checkpoint(&spec_external()).unwrap().unwrap();

        repo.restore_folder(&baseline, &spec_restore("owner"))
            .unwrap();
        assert_eq!(
            fs::read_to_string(repo.work_tree().join("proposal.txt")).unwrap(),
            "v1"
        );
        assert!(!repo.work_tree().join("extra.txt").exists());

        repo.restore_folder(&after, &spec_restore("owner")).unwrap();
        assert_eq!(
            fs::read_to_string(repo.work_tree().join("proposal.txt")).unwrap(),
            "later"
        );
        assert_eq!(
            fs::read_to_string(repo.work_tree().join("extra.txt")).unwrap(),
            "new"
        );
    }

    #[test]
    fn restore_rejects_path_escape() {
        let (_temp, repo) = setup();
        let id = repo.list_versions().unwrap()[0].id.clone();
        let err = repo
            .restore_file(&id, "../secret.txt", &spec_restore("owner"))
            .unwrap_err();
        assert!(matches!(err, CoworkError::PathEscape));
    }

    #[test]
    fn compact_keeps_recent_and_thins_old() {
        let (_temp, repo) = setup();
        for i in 0..3 {
            fs::write(repo.work_tree().join("proposal.txt"), format!("n{i}")).unwrap();
            repo.checkpoint(&spec_external()).unwrap();
        }
        let before = repo.list_versions().unwrap().len();
        repo.compact(DEFAULT_COMPACT_KEEP_DAYS).unwrap();
        let after = repo.list_versions().unwrap().len();
        assert_eq!(
            after, before,
            "all commits are within the keep window so compact is a no-op rewrite"
        );
        repo.compact(0).unwrap();
        let thinned = repo.list_versions().unwrap().len();
        assert!(thinned >= 1);
        assert!(thinned <= before);
        assert!(!repo.work_tree().join(".git").exists());
    }
}
