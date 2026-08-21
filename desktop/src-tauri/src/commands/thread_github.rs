use serde::{Deserialize, Serialize};

use super::gh_cli::{gh_command, GhUnavailable};
use super::thread_github_target::origin_repo_target;
use super::thread_workspace_git::{command_output, validate_target};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGitHubStatus {
    pub availability: ThreadGitHubAvailability,
    pub detail: Option<String>,
    pub pull_request: Option<ThreadPullRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreadGitHubAvailability {
    Available,
    CliMissing,
    CliFailed,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadIssue {
    pub number: u64,
    pub state: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestComment {
    pub author: Option<ThreadGitHubAuthor>,
    pub body: String,
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ThreadGitHubAuthor {
    pub login: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestCheck {
    pub name: String,
    pub state: String,
    pub url: Option<String>,
    pub workflow: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequest {
    pub additions: u64,
    pub base_ref_name: String,
    pub changed_files: u64,
    pub closing_issues_references: Vec<ThreadIssue>,
    pub comments: Vec<ThreadPullRequestComment>,
    pub deletions: u64,
    pub head_ref_name: String,
    pub is_draft: bool,
    pub merge_state_status: String,
    pub number: u64,
    pub review_decision: String,
    pub state: String,
    pub title: String,
    pub url: String,
    #[serde(skip_deserializing)]
    pub checks: Vec<ThreadPullRequestCheck>,
    #[serde(rename = "statusCheckRollup", skip_serializing)]
    status_check_rollup: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GhLookupError {
    CliMissing,
    CliFailed(String),
}

impl From<GhUnavailable> for GhLookupError {
    fn from(_: GhUnavailable) -> Self {
        Self::CliMissing
    }
}

#[tauri::command]
pub async fn get_thread_github_status(
    repository_path: String,
    branch: String,
    root_event_id: String,
) -> Result<ThreadGitHubStatus, String> {
    let target = validate_target(&repository_path, &branch, &root_event_id).await?;
    let repo = origin_repo_target(&target.repository_path).await;
    let number =
        match find_pull_request_number(&target.repository_path, repo.as_deref(), &branch).await {
            Ok(number) => number,
            Err(GhLookupError::CliMissing) => return Ok(cli_missing()),
            Err(GhLookupError::CliFailed(detail)) => return Ok(cli_failed(detail)),
        };
    if number == 0 {
        return Ok(ThreadGitHubStatus {
            availability: ThreadGitHubAvailability::Available,
            detail: None,
            pull_request: None,
        });
    }
    let mut pull_request =
        match read_pull_request(&target.repository_path, repo.as_deref(), number).await {
            Ok(pull_request) => pull_request,
            Err(GhLookupError::CliMissing) => return Ok(cli_missing()),
            Err(GhLookupError::CliFailed(detail)) => return Ok(cli_failed(detail)),
        };
    pull_request.checks = pull_request
        .status_check_rollup
        .iter()
        .filter_map(parse_check)
        .collect();
    pull_request.comments = pull_request.comments.into_iter().rev().take(20).collect();
    pull_request.comments.reverse();
    Ok(ThreadGitHubStatus {
        availability: ThreadGitHubAvailability::Available,
        detail: None,
        pull_request: Some(pull_request),
    })
}

async fn find_pull_request_number(
    repository: &std::path::Path,
    repo: Option<&str>,
    branch: &str,
) -> Result<u64, GhLookupError> {
    let mut command = gh_command().await?;
    command
        .args(["pr", "list", "--state", "all", "--head", branch])
        .args(["--json", "number", "--limit", "1"])
        .current_dir(repository);
    if let Some(repo) = repo {
        command.args(["--repo", repo]);
    }
    let output = command_output(&mut command)
        .await
        .map_err(|error| GhLookupError::CliFailed(bounded_detail(&error)))?;
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)
        .map_err(|_| GhLookupError::CliFailed("GitHub CLI returned invalid JSON.".to_string()))?;
    Ok(rows
        .first()
        .and_then(|row| row["number"].as_u64())
        .unwrap_or(0))
}

async fn read_pull_request(
    repository: &std::path::Path,
    repo: Option<&str>,
    number: u64,
) -> Result<ThreadPullRequest, GhLookupError> {
    let fields = "number,title,state,url,headRefName,baseRefName,isDraft,mergeStateStatus,reviewDecision,additions,deletions,changedFiles,comments,closingIssuesReferences,statusCheckRollup";
    let mut command = gh_command().await?;
    command
        .args(["pr", "view", &number.to_string(), "--json", fields])
        .current_dir(repository);
    if let Some(repo) = repo {
        command.args(["--repo", repo]);
    }
    let output = command_output(&mut command)
        .await
        .map_err(|error| GhLookupError::CliFailed(bounded_detail(&error)))?;
    serde_json::from_slice(&output.stdout)
        .map_err(|_| GhLookupError::CliFailed("GitHub CLI returned invalid JSON.".to_string()))
}

pub(crate) fn parse_check(value: &serde_json::Value) -> Option<ThreadPullRequestCheck> {
    let name = value["name"]
        .as_str()
        .or_else(|| value["context"].as_str())?
        .to_string();
    let state = ["conclusion", "state", "status"]
        .iter()
        .find_map(|key| value[*key].as_str().filter(|state| !state.is_empty()))?
        .to_string();
    Some(ThreadPullRequestCheck {
        name,
        state,
        url: value["detailsUrl"]
            .as_str()
            .or_else(|| value["targetUrl"].as_str())
            .map(str::to_string),
        workflow: value["workflowName"].as_str().map(str::to_string),
    })
}

fn cli_missing() -> ThreadGitHubStatus {
    ThreadGitHubStatus {
        availability: ThreadGitHubAvailability::CliMissing,
        detail: Some("Install GitHub CLI and run gh auth login.".to_string()),
        pull_request: None,
    }
}

fn cli_failed(detail: String) -> ThreadGitHubStatus {
    ThreadGitHubStatus {
        availability: ThreadGitHubAvailability::CliFailed,
        detail: Some(detail),
        pull_request: None,
    }
}

fn bounded_detail(detail: &str) -> String {
    let collapsed = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    let bounded: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else if bounded.is_empty() {
        "GitHub CLI command failed.".to_string()
    } else {
        bounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_check_runs_and_commit_statuses() {
        let check_run = serde_json::json!({
            "name": "Desktop E2E",
            "status": "IN_PROGRESS",
            "detailsUrl": "https://github.com/org/repo/actions/runs/1",
            "workflowName": "Crew CI"
        });
        let commit_status = serde_json::json!({
            "context": "release/ready",
            "state": "SUCCESS",
            "targetUrl": "https://example.test/status"
        });

        let run = parse_check(&check_run).expect("check run should parse");
        assert_eq!(run.name, "Desktop E2E");
        assert_eq!(run.state, "IN_PROGRESS");
        assert_eq!(run.workflow.as_deref(), Some("Crew CI"));
        let status = parse_check(&commit_status).expect("commit status should parse");
        assert_eq!(status.name, "release/ready");
        assert_eq!(status.state, "SUCCESS");
        assert_eq!(status.url.as_deref(), Some("https://example.test/status"));
    }

    #[test]
    fn availability_wire_values_are_kebab_case() {
        assert_eq!(
            serde_json::to_value(ThreadGitHubAvailability::Available).unwrap(),
            "available"
        );
        assert_eq!(
            serde_json::to_value(ThreadGitHubAvailability::CliMissing).unwrap(),
            "cli-missing"
        );
        assert_eq!(
            serde_json::to_value(ThreadGitHubAvailability::CliFailed).unwrap(),
            "cli-failed"
        );
    }

    #[test]
    fn degraded_helpers_leave_pull_request_empty() {
        assert_eq!(
            cli_missing().availability,
            ThreadGitHubAvailability::CliMissing
        );
        assert!(cli_missing().pull_request.is_none());
        let failed = cli_failed("auth failed".to_string());
        assert_eq!(failed.availability, ThreadGitHubAvailability::CliFailed);
        assert_eq!(failed.detail.as_deref(), Some("auth failed"));
        assert!(failed.pull_request.is_none());
    }

    #[test]
    fn gh_unavailable_maps_to_cli_missing() {
        assert_eq!(
            GhLookupError::from(GhUnavailable::CliMissing),
            GhLookupError::CliMissing
        );
    }
}
