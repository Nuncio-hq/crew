use serde::{Deserialize, Serialize};

/// CLI availability for the active forge provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeAvailability {
    Available,
    CliMissing,
    CliFailed,
    RateLimited,
}

/// Locator for a pull request on any forge that uses owner/name#number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgePullRequestRef {
    pub owner: String,
    pub name: String,
    pub number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgePullRequestState {
    Open,
    Draft,
    Merged,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeReviewDecision {
    None,
    ReviewRequired,
    Approved,
    ChangesRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeFileViewedState {
    Viewed,
    Unviewed,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeCheckConclusion {
    Success,
    Failure,
    Neutral,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    Pending,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeMergeStrategy {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ForgeDiffSource {
    Worktree,
    Api,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeAuthor {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeComment {
    pub id: String,
    pub author: Option<ForgeAuthor>,
    pub body: String,
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeReview {
    pub id: String,
    pub author: Option<ForgeAuthor>,
    pub body: String,
    pub state: String,
    pub submitted_at: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeReviewThread {
    pub id: String,
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub path: Option<String>,
    pub line: Option<u64>,
    pub comments: Vec<ForgeComment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCommit {
    pub oid: String,
    pub message_headline: String,
    pub committed_at: String,
    pub additions: u64,
    pub deletions: u64,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeChangedFile {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
    pub viewed_state: ForgeFileViewedState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCheck {
    pub name: String,
    pub status: String,
    pub conclusion: ForgeCheckConclusion,
    pub url: Option<String>,
    pub workflow: Option<String>,
    pub run_id: Option<u64>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgePullRequestDetail {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: String,
    pub state: ForgePullRequestState,
    pub is_draft: bool,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub review_decision: ForgeReviewDecision,
    pub merge_state_status: String,
    pub author: Option<ForgeAuthor>,
    pub comments: Vec<ForgeComment>,
    pub reviews: Vec<ForgeReview>,
    pub review_threads: Vec<ForgeReviewThread>,
    pub commits: Vec<ForgeCommit>,
    pub files: Vec<ForgeChangedFile>,
    pub checks: Vec<ForgeCheck>,
    pub merge_strategies: Vec<ForgeMergeStrategy>,
    pub files_truncated: bool,
    pub commits_truncated: bool,
    pub checks_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeDetailResult {
    pub availability: ForgeAvailability,
    pub rate_limited_until: Option<String>,
    pub detail: Option<ForgePullRequestDetail>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeDiffFile {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
    pub patch: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeDiff {
    pub files: Vec<ForgeDiffFile>,
    pub additions: u64,
    pub deletions: u64,
    pub source: ForgeDiffSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeDiffResult {
    pub availability: ForgeAvailability,
    pub rate_limited_until: Option<String>,
    pub diff: Option<ForgeDiff>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCheckLogTail {
    pub job: String,
    pub step: String,
    pub lines: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeCheckLogResult {
    pub availability: ForgeAvailability,
    pub rate_limited_until: Option<String>,
    pub tails: Vec<ForgeCheckLogTail>,
    pub message: Option<String>,
}

impl ForgeDetailResult {
    pub fn missing() -> Self {
        Self {
            availability: ForgeAvailability::CliMissing,
            rate_limited_until: None,
            detail: None,
            message: Some("Forge CLI was not found.".to_string()),
        }
    }

    pub fn from_error(error: &super::error::ForgeError) -> Self {
        Self {
            availability: error.availability(),
            rate_limited_until: error.rate_limited_until(),
            detail: None,
            message: Some(error.message()),
        }
    }
}

/// Parse `owner/name#123` or `https://host/owner/name/pull/123` /
/// `https://host/owner/name/-/merge_requests/123`.
pub fn parse_pull_request_locator(input: &str) -> Result<ForgePullRequestRef, String> {
    let trimmed = input.trim();
    if let Some((owner_name, number)) = trimmed.split_once('#') {
        let (owner, name) = owner_name
            .split_once('/')
            .ok_or_else(|| "Pull request locator must look like owner/name#number.".to_string())?;
        return build_ref(owner, name, number);
    }
    let url = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    let mut segments = url.split('/').filter(|segment| !segment.is_empty());
    let _host = segments.next();
    let owner = segments
        .next()
        .ok_or_else(|| "Pull request URL is missing an owner.".to_string())?;
    let name = segments
        .next()
        .ok_or_else(|| "Pull request URL is missing a repository name.".to_string())?;
    let marker = segments.next();
    let number = match marker {
        Some("pull") | Some("pulls") => segments.next(),
        Some("-") => {
            let kind = segments.next();
            if kind != Some("merge_requests") {
                return Err("Pull request URL path is not recognized.".to_string());
            }
            segments.next()
        }
        _ => None,
    }
    .ok_or_else(|| "Pull request URL is missing a number.".to_string())?;
    let number = number.split('/').next().unwrap_or(number);
    build_ref(owner, name, number)
}

fn build_ref(owner: &str, name: &str, number: &str) -> Result<ForgePullRequestRef, String> {
    let owner = owner.trim().trim_end_matches(".git");
    let name = name.trim().trim_end_matches(".git");
    if owner.is_empty() || name.is_empty() {
        return Err("Pull request locator is missing owner or name.".to_string());
    }
    let number = number
        .trim()
        .parse::<u64>()
        .map_err(|_| "Pull request number is not a positive integer.".to_string())?;
    if number == 0 {
        return Err("Pull request number must be greater than zero.".to_string());
    }
    Ok(ForgePullRequestRef {
        owner: owner.to_string(),
        name: name.to_string(),
        number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hash_and_https_locators() {
        assert_eq!(
            parse_pull_request_locator("Nuncio-hq/crew#202").unwrap(),
            ForgePullRequestRef {
                owner: "Nuncio-hq".into(),
                name: "crew".into(),
                number: 202,
            }
        );
        assert_eq!(
            parse_pull_request_locator("https://github.com/Nuncio-hq/crew/pull/202/files")
                .unwrap()
                .number,
            202
        );
        assert_eq!(
            parse_pull_request_locator("https://gitlab.example/group/proj/-/merge_requests/9")
                .unwrap()
                .number,
            9
        );
    }

    #[test]
    fn rejects_zero_and_garbage() {
        assert!(parse_pull_request_locator("owner/name#0").is_err());
        assert!(parse_pull_request_locator("not a locator").is_err());
    }
}
