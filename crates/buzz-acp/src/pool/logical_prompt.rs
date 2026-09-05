//! One logical turn includes its optional plan continuation and decision wait.
use crate::acp::{AcpClient, AcpError, PlanContinueDecision, StopReason, PLAN_CONTINUE_PROMPT};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub(super) async fn run_logical_prompt(
    acp: &mut AcpClient,
    session_id: &str,
    blocks: &[&str],
    idle_timeout: Duration,
    max_duration: Duration,
    allow_continuation: bool,
    deciding: &AtomicBool,
) -> Result<StopReason, AcpError> {
    let reason = acp
        .session_prompt_blocks_with_idle_timeout(session_id, blocks, idle_timeout, max_duration)
        .await?;
    if !allow_continuation || !matches!(reason, StopReason::EndTurn) {
        return Ok(reason);
    }
    // Snapshot the effective budget before asking the human. ACP already
    // accounts for permission waits and accepted native steering renewals.
    let remaining = acp.remaining_prompt_budget().unwrap_or(max_duration);
    deciding.store(true, Ordering::Relaxed);
    let decision = acp.ask_founder_to_continue_after_plan(session_id).await;
    deciding.store(false, Ordering::Relaxed);
    if decision != PlanContinueDecision::Continue {
        return Ok(reason);
    }
    // One continuation shares the remaining execution budget. Human decision
    // time is excluded, matching the existing ACP elicitation wait policy.
    acp.session_prompt_blocks_with_idle_timeout(
        session_id,
        &[PLAN_CONTINUE_PROMPT],
        idle_timeout,
        remaining,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn adapter(second: &str, delay_first: &str) -> AcpClient {
        let script = format!(
            r#"
IFS= read -r line
{delay_first}
printf '%s\n' '{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"session","update":{{"sessionUpdate":"plan","entries":[{{"content":"Reply","status":"pending","priority":"medium"}}]}}}}}}'
printf '%s\n' '{{"jsonrpc":"2.0","id":0,"result":{{"stopReason":"end_turn"}}}}'
IFS= read -r line
{second}
while IFS= read -r line; do :; done
"#
        );
        AcpClient::spawn("bash", &["-c".into(), script], &[], false)
            .await
            .expect("adapter")
    }

    #[tokio::test]
    async fn continuation_provider_error_replaces_initial_end_turn() {
        let mut acp = adapter("printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32000,\"message\":\"ResourceExhausted\"}}'", "").await;
        let result = run_logical_prompt(
            &mut acp,
            "session",
            &["plan then work"],
            Duration::from_secs(2),
            Duration::from_secs(4),
            true,
            &AtomicBool::new(false),
        )
        .await;
        assert!(
            matches!(result, Err(AcpError::AgentError { .. })),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn continuation_idle_expiry_does_not_inherit_success() {
        let mut acp = adapter("", "").await;
        let result = run_logical_prompt(
            &mut acp,
            "session",
            &["plan then work"],
            Duration::from_millis(60),
            Duration::from_secs(3),
            true,
            &AtomicBool::new(false),
        )
        .await;
        assert!(
            matches!(result, Err(AcpError::IdleTimeout(_))),
            "{result:?}"
        );
        assert!(
            acp.has_in_flight_prompt(),
            "normal timeout cleanup must still drain the continuation"
        );
    }

    #[tokio::test]
    async fn continuation_cannot_restart_the_original_hard_budget() {
        let mut acp = adapter("sleep 0.2; printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"stopReason\":\"end_turn\"}}'", "sleep 0.2").await;
        let result = run_logical_prompt(
            &mut acp,
            "session",
            &["plan then work"],
            Duration::from_secs(2),
            Duration::from_millis(300),
            true,
            &AtomicBool::new(false),
        )
        .await;
        assert!(
            matches!(result, Err(AcpError::HardTimeout { .. })),
            "{result:?}"
        );
    }
}
