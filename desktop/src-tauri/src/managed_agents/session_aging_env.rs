//! Inject #173 session-aging / guided-handover env into managed-agent spawns.

use std::process::Command;

use super::global_config::GlobalAgentConfig;

/// Apply per-app handover summarizer + aging thresholds from global config.
pub fn apply_session_aging_env(command: &mut Command, global: &GlobalAgentConfig) {
    if let Some(model) = global
        .handover_summarizer_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        command.env("BUZZ_ACP_HANDOVER_MODEL", model);
    }
    if let Some(threshold) = global.compaction_aging_threshold {
        command.env(
            "BUZZ_ACP_COMPACTION_THRESHOLD",
            threshold.clamp(1, 10).to_string(),
        );
    }
    if let Some(threshold) = global.turn_aging_threshold.filter(|v| *v > 0) {
        command.env("BUZZ_ACP_TURN_AGING_THRESHOLD", threshold.to_string());
    }
}
