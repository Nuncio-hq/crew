use super::types::{
    ForgeAuthor, ForgeChangedFile, ForgeCheck, ForgeCheckConclusion, ForgeComment, ForgeCommit,
    ForgeFileViewedState, ForgeMergeStrategy, ForgePullRequestDetail, ForgePullRequestState,
    ForgeReview, ForgeReviewDecision, ForgeReviewThread,
};

pub const DETAIL_QUERY: &str = r#"query ForgePullRequestDetail($owner: String!, $name: String!, $number: Int!) {
  rateLimit { remaining resetAt }
  repository(owner: $owner, name: $name) {
    mergeCommitAllowed squashMergeAllowed rebaseMergeAllowed
    pullRequest(number: $number) {
      id number title body url state isDraft mergeable mergeStateStatus
      reviewDecision additions deletions changedFiles headRefName baseRefName
      createdAt updatedAt
      author { login }
      comments(first: 50) { nodes { id author { login } body createdAt url } }
      reviews(first: 40) { nodes { id author { login } body state submittedAt url } }
      reviewThreads(first: 40) {
        nodes {
          id isResolved isOutdated path line
          comments(first: 20) { nodes { id author { login } body createdAt url } }
        }
      }
      history: commits(last: 50) {
        nodes {
          commit {
            oid messageHeadline committedDate additions deletions
            author { name email }
          }
        }
      }
      files(first: 100) { nodes { path additions deletions viewerViewedState } }
      head: commits(last: 1) {
        nodes {
          commit {
            oid
            statusCheckRollup {
              state
              contexts(first: 80) {
                nodes {
                  __typename
                  ... on CheckRun {
                    name status conclusion detailsUrl startedAt completedAt databaseId
                    checkSuite { workflowRun { databaseId url workflow { name } } }
                  }
                  ... on StatusContext { context state targetUrl }
                }
              }
            }
          }
        }
      }
    }
  }
}"#;

pub const MARK_VIEWED_MUTATION: &str = r#"mutation($id: ID!, $path: String!) {
  markFileAsViewed(input: { pullRequestId: $id, path: $path }) {
    clientMutationId
  }
}"#;

pub const UNMARK_VIEWED_MUTATION: &str = r#"mutation($id: ID!, $path: String!) {
  unmarkFileAsViewed(input: { pullRequestId: $id, path: $path }) {
    clientMutationId
  }
}"#;

pub fn parse_detail_json(stdout: &str) -> Result<ForgePullRequestDetail, String> {
    let value: serde_json::Value = serde_json::from_str(stdout)
        .map_err(|_| "Forge GraphQL returned invalid JSON.".to_string())?;
    if let Some(errors) = value.get("errors").and_then(|errors| errors.as_array()) {
        if !errors.is_empty() {
            let message = errors
                .iter()
                .filter_map(|error| error.get("message").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(if message.is_empty() {
                "Forge GraphQL returned errors.".to_string()
            } else {
                message
            });
        }
    }
    let repo = value
        .pointer("/data/repository")
        .ok_or_else(|| "Forge GraphQL response is missing repository.".to_string())?;
    let pr = repo
        .get("pullRequest")
        .filter(|value| !value.is_null())
        .ok_or_else(|| "Pull request was not found.".to_string())?;
    Ok(ForgePullRequestDetail {
        id: required_str(pr, "id")?,
        number: pr
            .get("number")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        title: required_str(pr, "title")?,
        body: optional_str(pr, "body").unwrap_or_default(),
        url: optional_str(pr, "url").unwrap_or_default(),
        state: map_state(
            optional_str(pr, "state").as_deref(),
            pr.get("isDraft")
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        ),
        is_draft: pr
            .get("isDraft")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        head_ref_name: optional_str(pr, "headRefName").unwrap_or_default(),
        base_ref_name: optional_str(pr, "baseRefName").unwrap_or_default(),
        additions: pr
            .get("additions")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        deletions: pr
            .get("deletions")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        changed_files: pr
            .get("changedFiles")
            .and_then(|value| value.as_u64())
            .unwrap_or(0),
        review_decision: map_review_decision(optional_str(pr, "reviewDecision").as_deref()),
        merge_state_status: optional_str(pr, "mergeStateStatus").unwrap_or_default(),
        author: pr.get("author").and_then(map_author),
        comments: map_comments(pr.pointer("/comments/nodes")),
        reviews: map_reviews(pr.pointer("/reviews/nodes")),
        review_threads: map_threads(pr.pointer("/reviewThreads/nodes")),
        commits: map_commits(pr.pointer("/history/nodes")),
        files: map_files(pr.pointer("/files/nodes")),
        checks: map_checks(pr.pointer("/head/nodes")),
        merge_strategies: map_merge_strategies(repo),
        files_truncated: pr
            .pointer("/files/nodes")
            .and_then(|value| value.as_array())
            .is_some_and(|nodes| nodes.len() >= 100),
        commits_truncated: pr
            .pointer("/history/nodes")
            .and_then(|value| value.as_array())
            .is_some_and(|nodes| nodes.len() >= 50),
        checks_truncated: pr
            .pointer("/head/nodes/0/commit/statusCheckRollup/contexts/nodes")
            .and_then(|value| value.as_array())
            .is_some_and(|nodes| nodes.len() >= 80),
    })
}

fn required_str(value: &serde_json::Value, key: &str) -> Result<String, String> {
    optional_str(value, key).ok_or_else(|| format!("Forge GraphQL is missing {key}."))
}

fn optional_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn map_author(value: &serde_json::Value) -> Option<ForgeAuthor> {
    Some(ForgeAuthor {
        login: value
            .get("login")
            .and_then(|value| value.as_str())?
            .to_string(),
    })
}

fn map_state(state: Option<&str>, is_draft: bool) -> ForgePullRequestState {
    match state.map(|value| value.to_ascii_uppercase()) {
        Some(value) if value == "MERGED" => ForgePullRequestState::Merged,
        Some(value) if value == "CLOSED" => ForgePullRequestState::Closed,
        _ if is_draft => ForgePullRequestState::Draft,
        _ => ForgePullRequestState::Open,
    }
}

fn map_review_decision(value: Option<&str>) -> ForgeReviewDecision {
    match value.map(|value| value.to_ascii_uppercase()).as_deref() {
        Some("APPROVED") => ForgeReviewDecision::Approved,
        Some("CHANGES_REQUESTED") => ForgeReviewDecision::ChangesRequested,
        Some("REVIEW_REQUIRED") => ForgeReviewDecision::ReviewRequired,
        _ => ForgeReviewDecision::None,
    }
}

fn map_viewed(value: Option<&str>) -> ForgeFileViewedState {
    match value.map(|value| value.to_ascii_uppercase()).as_deref() {
        Some("VIEWED") => ForgeFileViewedState::Viewed,
        Some("DISMISSED") => ForgeFileViewedState::Dismissed,
        _ => ForgeFileViewedState::Unviewed,
    }
}

fn map_comments(nodes: Option<&serde_json::Value>) -> Vec<ForgeComment> {
    nodes
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|node| {
            Some(ForgeComment {
                id: optional_str(node, "id")?,
                author: node.get("author").and_then(map_author),
                body: optional_str(node, "body").unwrap_or_default(),
                created_at: optional_str(node, "createdAt").unwrap_or_default(),
                url: optional_str(node, "url").unwrap_or_default(),
            })
        })
        .collect()
}

fn map_reviews(nodes: Option<&serde_json::Value>) -> Vec<ForgeReview> {
    nodes
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|node| {
            Some(ForgeReview {
                id: optional_str(node, "id")?,
                author: node.get("author").and_then(map_author),
                body: optional_str(node, "body").unwrap_or_default(),
                state: optional_str(node, "state").unwrap_or_default(),
                submitted_at: optional_str(node, "submittedAt"),
                url: optional_str(node, "url").unwrap_or_default(),
            })
        })
        .collect()
}

fn map_threads(nodes: Option<&serde_json::Value>) -> Vec<ForgeReviewThread> {
    nodes
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|node| {
            Some(ForgeReviewThread {
                id: optional_str(node, "id")?,
                is_resolved: node
                    .get("isResolved")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                is_outdated: node
                    .get("isOutdated")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                path: optional_str(node, "path"),
                line: node.get("line").and_then(|value| value.as_u64()),
                comments: map_comments(node.pointer("/comments/nodes")),
            })
        })
        .collect()
}

fn map_commits(nodes: Option<&serde_json::Value>) -> Vec<ForgeCommit> {
    nodes
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|node| {
            let commit = node.get("commit")?;
            Some(ForgeCommit {
                oid: optional_str(commit, "oid")?,
                message_headline: optional_str(commit, "messageHeadline").unwrap_or_default(),
                committed_at: optional_str(commit, "committedDate").unwrap_or_default(),
                additions: commit
                    .get("additions")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                deletions: commit
                    .get("deletions")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                author_name: commit
                    .pointer("/author/name")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                author_email: commit
                    .pointer("/author/email")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
            })
        })
        .collect()
}

fn map_files(nodes: Option<&serde_json::Value>) -> Vec<ForgeChangedFile> {
    nodes
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|node| {
            Some(ForgeChangedFile {
                path: optional_str(node, "path")?,
                additions: node
                    .get("additions")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                deletions: node
                    .get("deletions")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0),
                viewed_state: map_viewed(optional_str(node, "viewerViewedState").as_deref()),
            })
        })
        .collect()
}

fn map_checks(head_nodes: Option<&serde_json::Value>) -> Vec<ForgeCheck> {
    let nodes = head_nodes
        .and_then(|value| value.as_array())
        .and_then(|nodes| nodes.last())
        .and_then(|node| node.pointer("/commit/statusCheckRollup/contexts/nodes"))
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten();
    nodes
        .filter_map(|node| {
            let typename = node
                .get("__typename")
                .and_then(|value| value.as_str())
                .unwrap_or("");
            if typename == "StatusContext" {
                return Some(ForgeCheck {
                    name: optional_str(node, "context")?,
                    status: optional_str(node, "state").unwrap_or_default(),
                    conclusion: map_conclusion(optional_str(node, "state").as_deref()),
                    url: optional_str(node, "targetUrl"),
                    workflow: None,
                    run_id: None,
                    started_at: None,
                    completed_at: None,
                });
            }
            let workflow_run = node.pointer("/checkSuite/workflowRun");
            Some(ForgeCheck {
                name: optional_str(node, "name")?,
                status: optional_str(node, "status").unwrap_or_default(),
                conclusion: map_conclusion(
                    optional_str(node, "conclusion")
                        .or_else(|| optional_str(node, "status"))
                        .as_deref(),
                ),
                url: optional_str(node, "detailsUrl"),
                workflow: workflow_run
                    .and_then(|run| run.pointer("/workflow/name"))
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                run_id: workflow_run
                    .and_then(|run| run.get("databaseId"))
                    .and_then(|value| value.as_u64()),
                started_at: optional_str(node, "startedAt"),
                completed_at: optional_str(node, "completedAt"),
            })
        })
        .collect()
}

fn map_conclusion(value: Option<&str>) -> ForgeCheckConclusion {
    match value.map(|value| value.to_ascii_uppercase()).as_deref() {
        Some("SUCCESS") => ForgeCheckConclusion::Success,
        Some("NEUTRAL") => ForgeCheckConclusion::Neutral,
        Some("FAILURE") | Some("ERROR") => ForgeCheckConclusion::Failure,
        Some("CANCELLED") | Some("CANCELED") => ForgeCheckConclusion::Cancelled,
        Some("SKIPPED") => ForgeCheckConclusion::Skipped,
        Some("TIMED_OUT") => ForgeCheckConclusion::TimedOut,
        Some("ACTION_REQUIRED") => ForgeCheckConclusion::ActionRequired,
        Some("IN_PROGRESS") | Some("QUEUED") | Some("PENDING") | Some("EXPECTED") => {
            ForgeCheckConclusion::Pending
        }
        _ => ForgeCheckConclusion::Unknown,
    }
}

fn map_merge_strategies(repo: &serde_json::Value) -> Vec<ForgeMergeStrategy> {
    let mut strategies = Vec::new();
    if repo
        .get("mergeCommitAllowed")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        strategies.push(ForgeMergeStrategy::Merge);
    }
    if repo
        .get("squashMergeAllowed")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        strategies.push(ForgeMergeStrategy::Squash);
    }
    if repo
        .get("rebaseMergeAllowed")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        strategies.push(ForgeMergeStrategy::Rebase);
    }
    strategies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_slim_fixture() {
        let raw = include_str!("fixtures/detail.json");
        let detail = parse_detail_json(raw).expect("parse");
        assert_eq!(detail.number, 202);
        assert_eq!(detail.state, ForgePullRequestState::Merged);
        assert_eq!(detail.review_decision, ForgeReviewDecision::None);
        assert_eq!(detail.files.len(), 3);
        assert_eq!(detail.files[0].viewed_state, ForgeFileViewedState::Unviewed);
        assert_eq!(detail.commits.len(), 2);
        assert!(!detail.checks.is_empty());
        assert!(detail
            .checks
            .iter()
            .any(|check| check.run_id == Some(31696638935)));
        assert_eq!(
            detail.merge_strategies,
            vec![
                ForgeMergeStrategy::Merge,
                ForgeMergeStrategy::Squash,
                ForgeMergeStrategy::Rebase
            ]
        );
    }

    #[test]
    fn graphql_errors_surface() {
        let err = parse_detail_json(r#"{"errors":[{"message":"Could not resolve"}]}"#).unwrap_err();
        assert!(err.contains("Could not resolve"));
    }
}
