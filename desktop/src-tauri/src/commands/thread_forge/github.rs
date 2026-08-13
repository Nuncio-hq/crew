use std::path::Path;
use std::process::Output;
use std::time::Duration;

use tokio::process::Command;

use super::super::gh_cli::gh_command;
use super::diff::{diff_from_files, parse_unified_diff, select_diff_source};
use super::error::{classify_gh_output, ForgeActionResult, ForgeError};
use super::graphql::{
    parse_detail_json, DETAIL_QUERY, MARK_VIEWED_MUTATION, UNMARK_VIEWED_MUTATION,
};
use super::log_tail::parse_log_failed;
use super::provider::ForgeProvider;
use super::types::{
    ForgeAvailability, ForgeCheckLogResult, ForgeDetailResult, ForgeDiffResult, ForgeDiffSource,
    ForgeMergeStrategy, ForgePullRequestRef, ForgeReviewEvent,
};

const GH_TIMEOUT: Duration = Duration::from_secs(45);

pub struct GitHubGhProvider;

impl ForgeProvider for GitHubGhProvider {
    async fn get_pr_detail(
        &self,
        target: &ForgePullRequestRef,
    ) -> Result<ForgeDetailResult, ForgeError> {
        let stdout = gh_graphql(
            DETAIL_QUERY,
            &[
                ("owner", target.owner.as_str()),
                ("name", target.name.as_str()),
                ("number", &target.number.to_string()),
            ],
        )
        .await?;
        let detail = parse_detail_json(&stdout).map_err(ForgeError::CliFailed)?;
        Ok(ForgeDetailResult {
            availability: ForgeAvailability::Available,
            rate_limited_until: None,
            detail: Some(detail),
            message: None,
        })
    }

    async fn get_pr_diff(
        &self,
        target: &ForgePullRequestRef,
        worktree_path: Option<&str>,
        base_ref: Option<&str>,
    ) -> Result<ForgeDiffResult, ForgeError> {
        let source = select_diff_source(worktree_path);
        let diff = match source {
            ForgeDiffSource::Worktree => match worktree_diff(worktree_path, base_ref).await {
                Ok(diff) => diff,
                Err(_) => api_diff(target).await?,
            },
            ForgeDiffSource::Api => api_diff(target).await?,
        };
        Ok(ForgeDiffResult {
            availability: ForgeAvailability::Available,
            rate_limited_until: None,
            diff: Some(diff),
            message: None,
        })
    }

    async fn get_check_log_tail(
        &self,
        target: &ForgePullRequestRef,
        run_id: u64,
    ) -> Result<ForgeCheckLogResult, ForgeError> {
        let stdout = gh_text(
            &[
                "run",
                "view",
                &run_id.to_string(),
                "--log-failed",
                "--repo",
                &repo_slug(target),
            ],
            None,
        )
        .await?;
        Ok(ForgeCheckLogResult {
            availability: ForgeAvailability::Available,
            rate_limited_until: None,
            tails: parse_log_failed(&stdout),
            message: None,
        })
    }

    async fn rerun_checks(
        &self,
        target: &ForgePullRequestRef,
        run_id: u64,
        failed_only: bool,
    ) -> Result<ForgeActionResult, ForgeError> {
        let mut args = vec![
            "run".to_string(),
            "rerun".to_string(),
            run_id.to_string(),
            "--repo".to_string(),
            repo_slug(target),
        ];
        if failed_only {
            args.push("--failed".to_string());
        }
        gh_text(&args.iter().map(String::as_str).collect::<Vec<_>>(), None).await?;
        Ok(ForgeActionResult {
            ok: true,
            message: if failed_only {
                "Re-running failed checks.".to_string()
            } else {
                "Re-running checks.".to_string()
            },
        })
    }

    async fn comment_pr(
        &self,
        target: &ForgePullRequestRef,
        body: &str,
    ) -> Result<ForgeActionResult, ForgeError> {
        if body.trim().is_empty() {
            return Err(ForgeError::InvalidInput(
                "Comment body must not be empty.".to_string(),
            ));
        }
        gh_text(
            &[
                "pr",
                "comment",
                &target.number.to_string(),
                "--repo",
                &repo_slug(target),
                "--body",
                body,
            ],
            None,
        )
        .await?;
        Ok(ForgeActionResult {
            ok: true,
            message: "Commented on the pull request.".to_string(),
        })
    }

    async fn review_pr(
        &self,
        target: &ForgePullRequestRef,
        event: ForgeReviewEvent,
        body: &str,
    ) -> Result<ForgeActionResult, ForgeError> {
        let flag = match event {
            ForgeReviewEvent::Approve => "--approve",
            ForgeReviewEvent::RequestChanges => "--request-changes",
            ForgeReviewEvent::Comment => "--comment",
        };
        if matches!(
            event,
            ForgeReviewEvent::RequestChanges | ForgeReviewEvent::Comment
        ) && body.trim().is_empty()
        {
            return Err(ForgeError::InvalidInput(
                "Review body is required for this action.".to_string(),
            ));
        }
        let mut args = vec![
            "pr",
            "review",
            &target.number.to_string(),
            "--repo",
            &repo_slug(target),
            flag,
        ];
        if !body.trim().is_empty() {
            args.push("--body");
            args.push(body);
        }
        gh_text(&args, None).await?;
        Ok(ForgeActionResult {
            ok: true,
            message: "Submitted the review.".to_string(),
        })
    }

    async fn merge_pr(
        &self,
        target: &ForgePullRequestRef,
        strategy: ForgeMergeStrategy,
    ) -> Result<ForgeActionResult, ForgeError> {
        let flag = match strategy {
            ForgeMergeStrategy::Merge => "--merge",
            ForgeMergeStrategy::Squash => "--squash",
            ForgeMergeStrategy::Rebase => "--rebase",
        };
        gh_text(
            &[
                "pr",
                "merge",
                &target.number.to_string(),
                "--repo",
                &repo_slug(target),
                flag,
            ],
            None,
        )
        .await?;
        Ok(ForgeActionResult {
            ok: true,
            message: "Merged the pull request.".to_string(),
        })
    }

    async fn set_file_viewed(
        &self,
        _target: &ForgePullRequestRef,
        pull_request_id: &str,
        path: &str,
        viewed: bool,
    ) -> Result<ForgeActionResult, ForgeError> {
        if pull_request_id.trim().is_empty() || path.trim().is_empty() {
            return Err(ForgeError::InvalidInput(
                "Pull request id and path are required.".to_string(),
            ));
        }
        let query = if viewed {
            MARK_VIEWED_MUTATION
        } else {
            UNMARK_VIEWED_MUTATION
        };
        gh_graphql(query, &[("id", pull_request_id), ("path", path)]).await?;
        Ok(ForgeActionResult {
            ok: true,
            message: if viewed {
                "Marked the file as viewed.".to_string()
            } else {
                "Marked the file as unviewed.".to_string()
            },
        })
    }
}

impl GitHubGhProvider {
    pub async fn create_pr(
        &self,
        target_repo: &str,
        worktree_path: &str,
        title: &str,
        body: &str,
        base: &str,
        head: Option<&str>,
    ) -> Result<ForgeActionResult, ForgeError> {
        if title.trim().is_empty() {
            return Err(ForgeError::InvalidInput(
                "Pull request title must not be empty.".to_string(),
            ));
        }
        let mut args = vec![
            "pr".to_string(),
            "create".to_string(),
            "--repo".to_string(),
            target_repo.to_string(),
            "--title".to_string(),
            title.to_string(),
            "--body".to_string(),
            body.to_string(),
            "--base".to_string(),
            base.to_string(),
        ];
        if let Some(head) = head {
            args.push("--head".to_string());
            args.push(head.to_string());
        }
        gh_text(
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            Some(Path::new(worktree_path)),
        )
        .await?;
        Ok(ForgeActionResult {
            ok: true,
            message: "Created the pull request.".to_string(),
        })
    }
}

fn repo_slug(target: &ForgePullRequestRef) -> String {
    format!("{}/{}", target.owner, target.name)
}

async fn api_diff(target: &ForgePullRequestRef) -> Result<super::types::ForgeDiff, ForgeError> {
    let stdout = gh_text(
        &[
            "pr",
            "diff",
            &target.number.to_string(),
            "--repo",
            &repo_slug(target),
        ],
        None,
    )
    .await?;
    Ok(diff_from_files(
        parse_unified_diff(&stdout),
        ForgeDiffSource::Api,
    ))
}

async fn worktree_diff(
    worktree_path: Option<&str>,
    base_ref: Option<&str>,
) -> Result<super::types::ForgeDiff, ForgeError> {
    let path = worktree_path
        .ok_or_else(|| ForgeError::CliFailed("Worktree path is missing.".to_string()))?;
    let range = match base_ref.filter(|value| !value.is_empty()) {
        Some(base) => format!("{base}...HEAD"),
        None => "HEAD^...HEAD".to_string(),
    };
    let mut command = Command::new("git");
    command.arg("-C").arg(path).args([
        "diff",
        "--find-renames",
        "--no-ext-diff",
        "--unified=80",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        &range,
    ]);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(GH_TIMEOUT, command.output())
        .await
        .map_err(|_| ForgeError::CliFailed("git diff timed out.".to_string()))?
        .map_err(|error| ForgeError::CliFailed(format!("Could not start git: {error}")))?;
    if !output.status.success() {
        return Err(ForgeError::CliFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(diff_from_files(
        parse_unified_diff(&stdout),
        ForgeDiffSource::Worktree,
    ))
}

async fn gh_graphql(query: &str, fields: &[(&str, &str)]) -> Result<String, ForgeError> {
    let mut args = vec![
        "api".to_string(),
        "graphql".to_string(),
        "-f".to_string(),
        format!("query={query}"),
    ];
    for (key, value) in fields {
        if *key == "number" {
            args.push("-F".to_string());
            args.push(format!("{key}={value}"));
        } else {
            args.push("-f".to_string());
            args.push(format!("{key}={value}"));
        }
    }
    gh_text(&args.iter().map(String::as_str).collect::<Vec<_>>(), None).await
}

async fn gh_text(args: &[&str], cwd: Option<&Path>) -> Result<String, ForgeError> {
    let output = run_gh(args, cwd).await?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    classify_gh_output(&stdout, &stderr, output.status.success())?;
    Ok(stdout)
}

async fn run_gh(args: &[&str], cwd: Option<&Path>) -> Result<Output, ForgeError> {
    let mut command = gh_command().await?;
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.kill_on_drop(true);
    tokio::time::timeout(GH_TIMEOUT, command.output())
        .await
        .map_err(|_| ForgeError::CliFailed("Forge CLI timed out.".to_string()))?
        .map_err(|error| ForgeError::CliFailed(format!("Could not start forge CLI: {error}")))
}

#[cfg(test)]
mod tests {
    use super::super::super::gh_cli::clear_gh_binary_cache;
    use super::*;

    fn write_executable(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    #[cfg(unix)]
    #[test]
    fn fake_gh_graphql_parses_fixture() {
        let _guard = crate::managed_agents::lock_path_mutex();
        let dir = tempfile::tempdir().expect("dir");
        let fixture = include_str!("fixtures/detail.json");
        let script = format!(
            "#!/bin/sh\nif echo \"$*\" | grep -q graphql; then cat <<'EOF'\n{fixture}\nEOF\nexit 0\nfi\necho unexpected >&2\nexit 1\n"
        );
        write_executable(dir.path(), "gh", &script);
        let original = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{original}", dir.path().display()));
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();

        let provider = GitHubGhProvider;
        let result = block_on(provider.get_pr_detail(&ForgePullRequestRef {
            owner: "Nuncio-hq".into(),
            name: "crew".into(),
            number: 202,
        }))
        .expect("detail");
        assert_eq!(result.availability, ForgeAvailability::Available);
        assert_eq!(result.detail.expect("pr").number, 202);

        std::env::set_var("PATH", original);
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();
    }

    #[cfg(unix)]
    #[test]
    fn fake_gh_rate_limit_is_classified() {
        let _guard = crate::managed_agents::lock_path_mutex();
        let dir = tempfile::tempdir().expect("dir");
        write_executable(
            dir.path(),
            "gh",
            "#!/bin/sh\necho '{\"errors\":[{\"type\":\"RATE_LIMITED\",\"message\":\"API rate limit exceeded\"}]}'\nexit 1\n",
        );
        let original = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{original}", dir.path().display()));
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();

        let provider = GitHubGhProvider;
        let err = block_on(provider.get_pr_detail(&ForgePullRequestRef {
            owner: "o".into(),
            name: "r".into(),
            number: 1,
        }))
        .expect_err("rate limit");
        assert!(matches!(err, ForgeError::RateLimited { .. }));

        std::env::set_var("PATH", original);
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();
    }

    #[test]
    fn worktree_fallback_when_path_missing() {
        assert_eq!(select_diff_source(None), ForgeDiffSource::Api);
    }

    #[cfg(unix)]
    #[test]
    fn fake_gh_missing_is_cli_missing() {
        let _guard = crate::managed_agents::lock_path_mutex();
        let dir = tempfile::tempdir().expect("dir");
        let original = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", dir.path());
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();

        if crate::managed_agents::find_command("gh").is_some() {
            std::env::set_var("PATH", original);
            crate::managed_agents::clear_resolve_cache();
            clear_gh_binary_cache();
            return;
        }

        let provider = GitHubGhProvider;
        let err = block_on(provider.get_pr_detail(&ForgePullRequestRef {
            owner: "o".into(),
            name: "r".into(),
            number: 1,
        }))
        .expect_err("missing");
        assert!(matches!(err, ForgeError::CliMissing));

        std::env::set_var("PATH", original);
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();
    }

    #[cfg(unix)]
    #[test]
    fn fake_gh_write_and_log_and_diff() {
        let _guard = crate::managed_agents::lock_path_mutex();
        let dir = tempfile::tempdir().expect("dir");
        let log = include_str!("fixtures/log-failed.txt");
        let script = format!(
            "#!/bin/sh\ncase \" $* \" in\n  *' pr comment '*|*' pr review '*|*' pr merge '*|*' run rerun '*) echo ok; exit 0 ;;\n  *' run view '*) cat <<'EOF'\n{log}\nEOF\nexit 0 ;;\n  *' pr diff '*) printf '%s\\n' 'diff --git a/a.rs b/a.rs' '--- a/a.rs' '+++ b/a.rs' '@@ -1 +1 @@' '-old' '+new'; exit 0 ;;\n  *) echo unexpected >&2; exit 1 ;;\nesac\n"
        );
        write_executable(dir.path(), "gh", &script);
        let original = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{original}", dir.path().display()));
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();

        let provider = GitHubGhProvider;
        let target = ForgePullRequestRef {
            owner: "Nuncio-hq".into(),
            name: "crew".into(),
            number: 1,
        };
        let comment = block_on(provider.comment_pr(&target, "hi")).expect("comment");
        assert!(comment.ok);
        let tail = block_on(provider.get_check_log_tail(&target, 99)).expect("log");
        assert_eq!(tail.availability, ForgeAvailability::Available);
        assert!(!tail.tails.is_empty());
        let diff = block_on(provider.get_pr_diff(&target, None, None)).expect("diff");
        assert_eq!(diff.diff.expect("files").source, ForgeDiffSource::Api);

        std::env::set_var("PATH", original);
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();
    }
}
