//! Local path probe for desktop-governed generate.
//!
//! A missing or gone checkout is not an empty GitHub repository.

use std::path::{Path, PathBuf};

/// Why generate cannot snapshot a local tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikiGenerateRoot {
    /// `repo_path` is unset or not a directory. Do not fall back to cwd.
    MissingLocalPath,
    /// Directory exists; caller may `RepoSnapshot::from_git`.
    Ready(PathBuf),
}

/// Resolve the generate root. Never substitutes the process cwd.
pub fn resolve_wiki_generate_root(repo_path: Option<&str>) -> WikiGenerateRoot {
    match repo_path {
        Some(path) if Path::new(path).is_dir() => WikiGenerateRoot::Ready(PathBuf::from(path)),
        _ => WikiGenerateRoot::MissingLocalPath,
    }
}

/// `from_git` failed or listed no files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikiLocalSnapshotError {
    /// Bound path is gone or not a git worktree.
    MissingLocalPath,
    /// Git tree exists but has no files / no HEAD.
    EmptyTree,
}

/// Classify a `from_git` failure for a root that `resolve` already accepted.
pub fn classify_from_git_failure(
    root: &Path,
    git_error: &str,
    files_empty: bool,
) -> WikiLocalSnapshotError {
    let err = git_error.to_ascii_lowercase();
    if err.contains("not a git repository") || !root.join(".git").exists() {
        return WikiLocalSnapshotError::MissingLocalPath;
    }
    if files_empty
        || err.contains("ambiguous argument")
        || err.contains("unknown revision")
        || err.contains("bad revision")
        || err.contains("needed a single revision")
    {
        return WikiLocalSnapshotError::EmptyTree;
    }
    WikiLocalSnapshotError::EmptyTree
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("crew-wiki-probe-{label}-{nanos}"));
        fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    #[test]
    fn missing_path_is_not_cwd_and_not_empty_github() {
        let resolved = resolve_wiki_generate_root(None);
        assert_eq!(
            resolved,
            WikiGenerateRoot::MissingLocalPath,
            "unbound repo_path must not fall back to the desktop cwd"
        );
    }

    #[test]
    fn gone_directory_is_missing_local_path() {
        let gone = std::env::temp_dir().join("crew-wiki-probe-does-not-exist-222");
        let resolved = resolve_wiki_generate_root(Some(gone.to_str().expect("utf8")));
        assert_eq!(resolved, WikiGenerateRoot::MissingLocalPath);
    }

    #[test]
    fn existing_directory_is_ready() {
        let dir = tmp_dir("ready");
        let resolved = resolve_wiki_generate_root(Some(dir.to_str().expect("utf8")));
        assert_eq!(resolved, WikiGenerateRoot::Ready(dir.clone()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_git_tree_is_empty_tree_not_missing_path() {
        let dir = tmp_dir("empty-git");
        let status = Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .status()
            .expect("git init");
        assert!(status.success());
        let err = classify_from_git_failure(&dir, "ambiguous argument 'HEAD'", true);
        assert_eq!(err, WikiLocalSnapshotError::EmptyTree);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn not_a_git_directory_is_missing_local_path() {
        let dir = tmp_dir("not-git");
        let err = classify_from_git_failure(&dir, "not a git repository", false);
        assert_eq!(err, WikiLocalSnapshotError::MissingLocalPath);
        let _ = fs::remove_dir_all(dir);
    }
}
