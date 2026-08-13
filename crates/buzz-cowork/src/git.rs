//! `git --git-dir --work-tree` invocation. Never creates a `.git` in the folder.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

use crate::error::CoworkError;
use crate::paths::empty_hooks_dir;

pub(crate) fn git(
    git_dir: &Path,
    work_tree: &Path,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<Output, CoworkError> {
    let hooks = empty_hooks_dir(git_dir);
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(git_dir)
        .arg("--work-tree")
        .arg(work_tree)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            &format!("core.hooksPath={}", hooks.display()),
        ])
        .args(args)
        .output()
        .map_err(|source| CoworkError::Io {
            path: git_dir.to_path_buf(),
            source,
        })?;
    Ok(output)
}

pub(crate) fn git_ok(
    git_dir: &Path,
    work_tree: &Path,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Result<String, CoworkError> {
    let output = git(git_dir, work_tree, args)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(CoworkError::operation(if stderr.is_empty() {
            "Could not update Versions".into()
        } else {
            stderr
        }));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| CoworkError::operation("Versions returned invalid text"))
}

pub(crate) fn git_status_ok(
    git_dir: &Path,
    work_tree: &Path,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> bool {
    git(git_dir, work_tree, args)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn stdout_ok(output: &Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout.clone())
        .ok()
        .map(|text| text.trim().to_string())
}
