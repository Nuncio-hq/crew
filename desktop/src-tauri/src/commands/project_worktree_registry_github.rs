use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::gh_cli::{gh_command, GhUnavailable};
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

/// Issue linked to a worktree via a PR closing reference.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryIssue {
    pub number: u64,
    /// `"open"` or `"closed"` (normalized from `gh` state).
    pub state: String,
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
    #[serde(default)]
    body: String,
    #[serde(default, rename = "statusCheckRollup")]
    status_check_rollup: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhIssueRow {
    number: u64,
    state: String,
    title: String,
    url: String,
}

/// Result of one repo-wide `gh pr list`, including closing-ref numbers per branch.
#[derive(Debug, Default)]
pub struct FetchedPullRequests {
    pub by_branch: HashMap<String, Vec<RegistryPullRequest>>,
    /// Distinct issue numbers referenced by closing keywords in PR bodies.
    pub linked_issue_numbers_by_branch: HashMap<String, Vec<u64>>,
}

/// Why a registry-wide `gh` list could not produce results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchPullRequestsError {
    CliMissing,
    CliFailed,
}

impl From<GhUnavailable> for FetchPullRequestsError {
    fn from(_: GhUnavailable) -> Self {
        Self::CliMissing
    }
}

/// Closing keywords: close(s|d)?, fix(e[sd])?, resolve(s|d)? followed by `#N`.
/// Cross-repo refs like `owner/repo#5` are ignored (no bare `#` after the keyword).
static CLOSING_ISSUE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+#(\d+)\b")
        .expect("closing-issue regex compiles")
});

/// Parse GitHub closing references from a PR body.
///
/// Returns distinct issue numbers in first-seen order. Cross-repo refs
/// (`owner/repo#N`) are ignored — only bare `#N` after a closing keyword counts.
pub fn parse_closing_issue_refs(body: &str) -> Vec<u64> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for caps in CLOSING_ISSUE_RE.captures_iter(body) {
        let Some(num) = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()) else {
            continue;
        };
        if seen.insert(num) {
            out.push(num);
        }
    }
    out
}

/// One `gh pr list --state all` for the whole repo, grouped by head branch.
/// Failure carries the cause so the caller can mark GitHub degraded without
/// failing the whole registry load.
pub async fn fetch_pull_requests_by_branch(
    repository: &Path,
) -> Result<FetchedPullRequests, FetchPullRequestsError> {
    let repo = origin_repo_target(repository).await;
    let fields = "headRefName,number,state,isDraft,reviewDecision,statusCheckRollup,additions,deletions,title,url,body";
    let mut command = gh_command().await?;
    command
        .args([
            "pr", "list", "--state", "all", "--limit", "100", "--json", fields,
        ])
        .current_dir(repository);
    if let Some(repo) = repo.as_deref() {
        command.args(["--repo", repo]);
    }
    let output = command_output(&mut command)
        .await
        .map_err(|_| FetchPullRequestsError::CliFailed)?;
    let rows: Vec<GhPullRequestRow> =
        serde_json::from_slice(&output.stdout).map_err(|_| FetchPullRequestsError::CliFailed)?;
    Ok(group_pull_requests(rows))
}

/// One `gh issue list --state all` for the whole repo, keyed by issue number.
pub async fn fetch_issues_by_number(
    repository: &Path,
) -> Result<HashMap<u64, RegistryIssue>, FetchPullRequestsError> {
    let repo = origin_repo_target(repository).await;
    let fields = "number,state,title,url";
    let mut command = gh_command().await?;
    command
        .args([
            "issue", "list", "--state", "all", "--limit", "100", "--json", fields,
        ])
        .current_dir(repository);
    if let Some(repo) = repo.as_deref() {
        command.args(["--repo", repo]);
    }
    let output = command_output(&mut command)
        .await
        .map_err(|_| FetchPullRequestsError::CliFailed)?;
    let rows: Vec<GhIssueRow> =
        serde_json::from_slice(&output.stdout).map_err(|_| FetchPullRequestsError::CliFailed)?;
    Ok(index_issues(rows))
}

/// Resolve linked issues for a branch from closing-ref numbers + the issue index.
pub fn resolve_linked_issues(
    numbers: &[u64],
    issues_by_number: &HashMap<u64, RegistryIssue>,
) -> Vec<RegistryIssue> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for number in numbers {
        if !seen.insert(*number) {
            continue;
        }
        if let Some(issue) = issues_by_number.get(number) {
            out.push(issue.clone());
        }
    }
    sort_linked_issues(&mut out);
    out
}

fn group_pull_requests(rows: Vec<GhPullRequestRow>) -> FetchedPullRequests {
    let mut by_branch: HashMap<String, Vec<RegistryPullRequest>> = HashMap::new();
    let mut linked_nums: HashMap<String, Vec<u64>> = HashMap::new();
    let mut linked_seen: HashMap<String, HashSet<u64>> = HashMap::new();

    for row in rows {
        let closing = parse_closing_issue_refs(&row.body);
        if !closing.is_empty() {
            let seen = linked_seen.entry(row.head_ref_name.clone()).or_default();
            let list = linked_nums.entry(row.head_ref_name.clone()).or_default();
            for number in closing {
                if seen.insert(number) {
                    list.push(number);
                }
            }
        }
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
        by_branch.entry(row.head_ref_name).or_default().push(pr);
    }
    for list in by_branch.values_mut() {
        sort_pull_requests(list);
    }
    FetchedPullRequests {
        by_branch,
        linked_issue_numbers_by_branch: linked_nums,
    }
}

fn index_issues(rows: Vec<GhIssueRow>) -> HashMap<u64, RegistryIssue> {
    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        map.insert(
            row.number,
            RegistryIssue {
                number: row.number,
                state: normalize_issue_state(&row.state),
                title: row.title,
                url: row.url,
            },
        );
    }
    map
}

fn normalize_issue_state(state: &str) -> String {
    match state.to_ascii_uppercase().as_str() {
        "OPEN" => "open".to_string(),
        _ => "closed".to_string(),
    }
}

fn sort_pull_requests(list: &mut [RegistryPullRequest]) {
    list.sort_by(|a, b| {
        rank_pr(a)
            .cmp(&rank_pr(b))
            .then_with(|| b.number.cmp(&a.number))
    });
}

fn sort_linked_issues(list: &mut [RegistryIssue]) {
    list.sort_by(|a, b| {
        rank_issue(a)
            .cmp(&rank_issue(b))
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

fn rank_issue(issue: &RegistryIssue) -> u8 {
    if issue.state == "open" {
        0
    } else {
        1
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
        body: &str,
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
            body: body.to_string(),
            status_check_rollup: rollup,
        }
    }

    #[test]
    fn groups_and_ranks_open_before_merged_before_closed() {
        let fetched = group_pull_requests(vec![
            row("buzz/aaaa", 10, "CLOSED", false, "", vec![]),
            row("buzz/aaaa", 12, "OPEN", false, "", vec![]),
            row("buzz/aaaa", 11, "MERGED", false, "", vec![]),
            row("buzz/aaaa", 13, "OPEN", true, "", vec![]),
            row("buzz/bbbb", 1, "OPEN", false, "", vec![]),
        ]);
        let a = fetched.by_branch.get("buzz/aaaa").expect("branch a");
        assert_eq!(
            a.iter().map(|pr| pr.number).collect::<Vec<_>>(),
            vec![12, 13, 11, 10]
        );
        assert!(!fetched.by_branch.contains_key("buzz/cccc"));
        assert_eq!(fetched.by_branch.get("buzz/bbbb").map(|v| v.len()), Some(1));
    }

    #[test]
    fn empty_branch_returns_empty_vec_not_error() {
        let fetched = group_pull_requests(vec![]);
        assert!(fetched.by_branch.is_empty());
        assert!(fetched.linked_issue_numbers_by_branch.is_empty());
    }

    #[test]
    fn groups_closing_refs_per_branch() {
        let fetched = group_pull_requests(vec![
            row(
                "buzz/aaaa",
                12,
                "OPEN",
                false,
                "Closes #10\nFixes #11",
                vec![],
            ),
            row("buzz/aaaa", 13, "OPEN", true, "Resolves #10", vec![]),
            row("buzz/bbbb", 1, "OPEN", false, "Close #99", vec![]),
        ]);
        assert_eq!(
            fetched
                .linked_issue_numbers_by_branch
                .get("buzz/aaaa")
                .cloned()
                .unwrap_or_default(),
            vec![10, 11]
        );
        assert_eq!(
            fetched
                .linked_issue_numbers_by_branch
                .get("buzz/bbbb")
                .cloned()
                .unwrap_or_default(),
            vec![99]
        );
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

    #[test]
    fn parse_closing_refs_multiple_and_case_variants() {
        let body = "This PR Closes #12 and also FIXES #3.\nResolved #9";
        assert_eq!(parse_closing_issue_refs(body), vec![12, 3, 9]);
    }

    #[test]
    fn parse_closing_refs_punctuation() {
        assert_eq!(parse_closing_issue_refs("Closes #12."), vec![12]);
        assert_eq!(parse_closing_issue_refs("Fixed #7!"), vec![7]);
        assert_eq!(parse_closing_issue_refs("close #1,"), vec![1]);
    }

    #[test]
    fn parse_closing_refs_ignores_cross_repo() {
        let body = "Closes owner/repo#5 and also closes #8";
        assert_eq!(parse_closing_issue_refs(body), vec![8]);
        assert!(parse_closing_issue_refs("Fixes block/buzz#100").is_empty());
    }

    #[test]
    fn parse_closing_refs_dedupes() {
        assert_eq!(
            parse_closing_issue_refs("Closes #4\nFixes #4\nResolves #4"),
            vec![4]
        );
    }

    #[test]
    fn resolve_linked_issues_open_first() {
        let mut index = HashMap::new();
        index.insert(
            1,
            RegistryIssue {
                number: 1,
                state: "closed".to_string(),
                title: "done".to_string(),
                url: "https://example.test/1".to_string(),
            },
        );
        index.insert(
            2,
            RegistryIssue {
                number: 2,
                state: "open".to_string(),
                title: "todo".to_string(),
                url: "https://example.test/2".to_string(),
            },
        );
        let linked = resolve_linked_issues(&[1, 2, 99], &index);
        assert_eq!(
            linked.iter().map(|i| i.number).collect::<Vec<_>>(),
            vec![2, 1]
        );
    }

    #[test]
    fn normalize_issue_state_maps_gh_values() {
        assert_eq!(normalize_issue_state("OPEN"), "open");
        assert_eq!(normalize_issue_state("CLOSED"), "closed");
        assert_eq!(normalize_issue_state("closed"), "closed");
    }
}
