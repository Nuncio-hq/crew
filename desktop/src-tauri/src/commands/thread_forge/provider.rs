#![allow(async_fn_in_trait)]

use super::error::{ForgeActionResult, ForgeError};
use super::types::{
    ForgeCheckLogResult, ForgeDetailResult, ForgeDiffResult, ForgeMergeStrategy,
    ForgePullRequestRef, ForgeReviewEvent,
};

/// Forge-neutral pull-request operations. GitHub implements this by wrapping `gh`.
pub trait ForgeProvider {
    async fn get_pr_detail(
        &self,
        target: &ForgePullRequestRef,
    ) -> Result<ForgeDetailResult, ForgeError>;

    async fn get_pr_diff(
        &self,
        target: &ForgePullRequestRef,
        worktree_path: Option<&str>,
        base_ref: Option<&str>,
    ) -> Result<ForgeDiffResult, ForgeError>;

    async fn get_check_log_tail(
        &self,
        target: &ForgePullRequestRef,
        run_id: u64,
    ) -> Result<ForgeCheckLogResult, ForgeError>;

    async fn rerun_checks(
        &self,
        target: &ForgePullRequestRef,
        run_id: u64,
        failed_only: bool,
    ) -> Result<ForgeActionResult, ForgeError>;

    async fn comment_pr(
        &self,
        target: &ForgePullRequestRef,
        body: &str,
    ) -> Result<ForgeActionResult, ForgeError>;

    async fn review_pr(
        &self,
        target: &ForgePullRequestRef,
        event: ForgeReviewEvent,
        body: &str,
    ) -> Result<ForgeActionResult, ForgeError>;

    async fn merge_pr(
        &self,
        target: &ForgePullRequestRef,
        strategy: ForgeMergeStrategy,
    ) -> Result<ForgeActionResult, ForgeError>;

    async fn set_file_viewed(
        &self,
        target: &ForgePullRequestRef,
        pull_request_id: &str,
        path: &str,
        viewed: bool,
    ) -> Result<ForgeActionResult, ForgeError>;
}
