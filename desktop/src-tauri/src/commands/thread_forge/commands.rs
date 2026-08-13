use super::error::{ForgeActionResult, ForgeError};
use super::github::GitHubGhProvider;
use super::provider::ForgeProvider;
use super::types::{
    parse_pull_request_locator, ForgeAvailability, ForgeCheckLogResult, ForgeDetailResult,
    ForgeDiffResult, ForgeMergeStrategy, ForgePullRequestRef, ForgeReviewEvent,
};

fn provider() -> GitHubGhProvider {
    GitHubGhProvider
}

fn result_from_error(error: ForgeError) -> ForgeDetailResult {
    ForgeDetailResult::from_error(&error)
}

#[tauri::command]
pub async fn get_thread_forge_pr_detail(
    owner: String,
    name: String,
    number: u64,
) -> Result<ForgeDetailResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    match provider().get_pr_detail(&target).await {
        Ok(result) => Ok(result),
        Err(error) => Ok(result_from_error(error)),
    }
}

#[tauri::command]
pub async fn get_thread_forge_pr_diff(
    owner: String,
    name: String,
    number: u64,
    worktree_path: Option<String>,
    base_ref: Option<String>,
) -> Result<ForgeDiffResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    match provider()
        .get_pr_diff(&target, worktree_path.as_deref(), base_ref.as_deref())
        .await
    {
        Ok(result) => Ok(result),
        Err(error) => Ok(ForgeDiffResult {
            availability: error.availability(),
            rate_limited_until: error.rate_limited_until(),
            diff: None,
            message: Some(error.message()),
        }),
    }
}

#[tauri::command]
pub async fn get_forge_check_log_tail(
    owner: String,
    name: String,
    number: u64,
    run_id: u64,
) -> Result<ForgeCheckLogResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    match provider().get_check_log_tail(&target, run_id).await {
        Ok(result) => Ok(result),
        Err(error) => Ok(ForgeCheckLogResult {
            availability: error.availability(),
            rate_limited_until: error.rate_limited_until(),
            tails: Vec::new(),
            message: Some(error.message()),
        }),
    }
}

#[tauri::command]
pub async fn rerun_forge_checks(
    owner: String,
    name: String,
    number: u64,
    run_id: u64,
    failed_only: bool,
) -> Result<ForgeActionResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    provider()
        .rerun_checks(&target, run_id, failed_only)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn comment_forge_pr(
    owner: String,
    name: String,
    number: u64,
    body: String,
) -> Result<ForgeActionResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    provider()
        .comment_pr(&target, &body)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn review_forge_pr(
    owner: String,
    name: String,
    number: u64,
    event: String,
    body: String,
) -> Result<ForgeActionResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    let event = parse_review_event(&event)?;
    provider()
        .review_pr(&target, event, &body)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn merge_forge_pr(
    owner: String,
    name: String,
    number: u64,
    strategy: String,
) -> Result<ForgeActionResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    let strategy = parse_merge_strategy(&strategy)?;
    provider()
        .merge_pr(&target, strategy)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_forge_file_viewed(
    owner: String,
    name: String,
    number: u64,
    pull_request_id: String,
    path: String,
    viewed: bool,
) -> Result<ForgeActionResult, String> {
    let target = ForgePullRequestRef {
        owner,
        name,
        number,
    };
    provider()
        .set_file_viewed(&target, &pull_request_id, &path, viewed)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn resolve_forge_pr_by_url(url: String) -> Result<ForgeDetailResult, String> {
    let target = match parse_pull_request_locator(&url) {
        Ok(target) => target,
        Err(message) => {
            return Ok(ForgeDetailResult {
                availability: ForgeAvailability::CliFailed,
                rate_limited_until: None,
                detail: None,
                message: Some(message),
            });
        }
    };
    match provider().get_pr_detail(&target).await {
        Ok(result) => Ok(result),
        Err(error) => Ok(result_from_error(error)),
    }
}

#[tauri::command]
pub async fn create_forge_pr(
    owner: String,
    name: String,
    worktree_path: String,
    title: String,
    body: String,
    base: String,
    head: Option<String>,
) -> Result<ForgeActionResult, String> {
    let repo = format!("{owner}/{name}");
    provider()
        .create_pr(&repo, &worktree_path, &title, &body, &base, head.as_deref())
        .await
        .map_err(Into::into)
}

fn parse_review_event(value: &str) -> Result<ForgeReviewEvent, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "approve" => Ok(ForgeReviewEvent::Approve),
        "request-changes" | "request_changes" => Ok(ForgeReviewEvent::RequestChanges),
        "comment" => Ok(ForgeReviewEvent::Comment),
        _ => Err("Unknown review event.".to_string()),
    }
}

fn parse_merge_strategy(value: &str) -> Result<ForgeMergeStrategy, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "merge" => Ok(ForgeMergeStrategy::Merge),
        "squash" => Ok(ForgeMergeStrategy::Squash),
        "rebase" => Ok(ForgeMergeStrategy::Rebase),
        _ => Err("Unknown merge strategy.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_and_merge_parsers() {
        assert_eq!(
            parse_review_event("request-changes").unwrap(),
            ForgeReviewEvent::RequestChanges
        );
        assert_eq!(
            parse_merge_strategy("squash").unwrap(),
            ForgeMergeStrategy::Squash
        );
        assert!(parse_review_event("nonesuch").is_err());
    }
}
