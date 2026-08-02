use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::thread_github::parse_check;
use super::thread_github_target::origin_repo_target;
use super::thread_workspace_git::command_output;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegistryChecksState {
    Passing,
    Failing,
    Pending,
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryPullRequest {
    pub number: u64,
    pub state: String,
    pub is_draft: bool,
    pub review_decision: String,
    pub checks: RegistryChecksState,
    pub additions: u64,
    pub deletions: u64,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullRequestRow {
    number: u64,
    state: String,
    is_draft: bool,
    #[serde(default)]
    review_decision: String,
    #[serde(default)]
    additions: u64,
    #[serde(default)]
    deletions: u64,
    title: String,
    url: String,
    head_ref_name: String,
    #[serde(default, rename = "statusCheckRollup")]
    status_check_rollup: Vec<serde_json::Value>,
}

/// One `gh pr list --state all` for the whole repo, grouped by head branch.
/// Failure → `None` so the caller can mark GitHub unavailable without failing.
pub async fn fetch_pull_requests_by_branch(
    repository: &Path,
) -> Option<HashMap<String, Vec<RegistryPullRequest>>> {
    let repo = origin_repo_target(repository).await;
    let fields = "headRefName,number,state,isDraft,reviewDecision,statusCheckRollup,additions,deletions,title,url";
    let mut command = Command::new("gh");
    command
        .args([
            "pr", "list", "--state", "all", "--limit", "100", "--json", fields,
        ])
        .current_dir(repository);
    if let Some(repo) = repo.as_deref() {
        command.args(["--repo", repo]);
    }
    let output = command_output(&mut command).await.ok()?;
    let rows: Vec<GhPullRequestRow> = serde_json::from_slice(&output.stdout).ok()?;
    Some(group_pull_requests(rows))
}

fn group_pull_requests(rows: Vec<GhPullRequestRow>) -> HashMap<String, Vec<RegistryPullRequest>> {
    let mut map: HashMap<String, Vec<RegistryPullRequest>> = HashMap::new();
    for row in rows {
        let pr = RegistryPullRequest {
            number: row.number,
            state: row.state,
            is_draft: row.is_draft,
            review_decision: row.review_decision,
            checks: reduce_checks(&row.status_check_rollup),
            additions: row.additions,
            deletions: row.deletions,
            title: row.title,
            url: row.url,
        };
        map.entry(row.head_ref_name).or_default().push(pr);
    }
    for list in map.values_mut() {
        sort_pull_requests(list);
    }
    map
}

fn sort_pull_requests(list: &mut [RegistryPullRequest]) {
    list.sort_by(|a, b| {
        rank_pr(a)
            .cmp(&rank_pr(b))
            .then_with(|| b.number.cmp(&a.number))
    });
}

fn rank_pr(pr: &RegistryPullRequest) -> u8 {
    let state = pr.state.to_ascii_uppercase();
    if state == "MERGED" {
        2
    } else if state == "CLOSED" {
        3
    } else if pr.is_draft {
        1
    } else {
        0
    }
}

pub fn reduce_checks(rollup: &[serde_json::Value]) -> RegistryChecksState {
    let states: Vec<String> = rollup
        .iter()
        .filter_map(parse_check)
        .map(|check| check.state.to_ascii_uppercase())
        .collect();
    if states.is_empty() {
        return RegistryChecksState::None;
    }
    let mut pending = false;
    for state in &states {
        if matches!(
            state.as_str(),
            "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT"
        ) {
            return RegistryChecksState::Failing;
        }
        if !matches!(state.as_str(), "SUCCESS" | "NEUTRAL" | "SKIPPED") {
            pending = true;
        }
    }
    if pending {
        RegistryChecksState::Pending
    } else {
        RegistryChecksState::Passing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        head: &str,
        number: u64,
        state: &str,
        is_draft: bool,
        rollup: Vec<serde_json::Value>,
    ) -> GhPullRequestRow {
        GhPullRequestRow {
            number,
            state: state.to_string(),
            is_draft,
            review_decision: String::new(),
            additions: 1,
            deletions: 0,
            title: format!("PR {number}"),
            url: format!("https://example.test/{number}"),
            head_ref_name: head.to_string(),
            status_check_rollup: rollup,
        }
    }

    #[test]
    fn groups_and_ranks_open_before_merged_before_closed() {
        let map = group_pull_requests(vec![
            row("buzz/aaaa", 10, "CLOSED", false, vec![]),
            row("buzz/aaaa", 12, "OPEN", false, vec![]),
            row("buzz/aaaa", 11, "MERGED", false, vec![]),
            row("buzz/aaaa", 13, "OPEN", true, vec![]),
            row("buzz/bbbb", 1, "OPEN", false, vec![]),
        ]);
        let a = map.get("buzz/aaaa").expect("branch a");
        assert_eq!(
            a.iter().map(|pr| pr.number).collect::<Vec<_>>(),
            vec![12, 13, 11, 10]
        );
        assert!(!map.contains_key("buzz/cccc"));
        assert_eq!(map.get("buzz/bbbb").map(|v| v.len()), Some(1));
    }

    #[test]
    fn empty_branch_returns_empty_vec_not_error() {
        let map = group_pull_requests(vec![]);
        assert!(map.is_empty());
    }

    #[test]
    fn reduces_check_rollup() {
        assert_eq!(reduce_checks(&[]), RegistryChecksState::None);
        let passing = vec![serde_json::json!({"name": "ci", "conclusion": "SUCCESS"})];
        assert_eq!(reduce_checks(&passing), RegistryChecksState::Passing);
        let failing = vec![serde_json::json!({"name": "ci", "conclusion": "FAILURE"})];
        assert_eq!(reduce_checks(&failing), RegistryChecksState::Failing);
        let pending = vec![serde_json::json!({"name": "ci", "status": "IN_PROGRESS"})];
        assert_eq!(reduce_checks(&pending), RegistryChecksState::Pending);
    }
}
