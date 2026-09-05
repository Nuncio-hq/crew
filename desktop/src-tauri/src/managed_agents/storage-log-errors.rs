use std::path::Path;

use super::read_log_tail;

/// A meaningful error recovered from an exited agent's log tail.
pub struct AgentLogError {
    /// The full log line, wrapped as `Agent reported error…` for display.
    pub message: String,
    /// JSON-RPC error code parsed from the line's `(code N)` marker, or a
    /// synthetic code for known bare prefixes. `None` for legacy-format
    /// lines that carry no code (or when the code fails to parse as i64).
    pub code: Option<i64>,
}

fn with_optional_dependency_repair_hint(message: String) -> String {
    let lower = message.to_ascii_lowercase();
    let matches = lower.contains("missing optional dependency")
        && (lower.contains("@openai/codex-") || lower.contains("@anthropic-ai/claude-agent-sdk-"));
    if !matches {
        return message;
    }
    if message.contains("Settings → Agent runtimes") {
        return message;
    }
    format!(
        "{message}\n\nA managed ACP adapter is missing its native package for this architecture. Open Settings → Agent runtimes and click Install again so Buzz can repair its private Node tools directory."
    )
}

/// Recover the latest actionable runtime error, including adapter repair guidance.
pub fn meaningful_agent_error_from_log(path: &Path) -> Option<AgentLogError> {
    let tail = read_log_tail(path, 200).ok()?;
    tail.lines().rev().map(str::trim).find_map(|line| {
        // New format: "Agent reported error (code -32002): ..."
        if let Some(rest) = line.strip_prefix("Agent reported error (code ") {
            if let Some(paren_end) = rest.find("): ") {
                let code = rest[..paren_end].parse::<i64>().ok();
                return Some(AgentLogError {
                    message: with_optional_dependency_repair_hint(line.to_string()),
                    code,
                });
            }
        }
        // Legacy format (older buzz-acp builds): "Agent reported error: ..."
        if line.starts_with("Agent reported error:") {
            return Some(AgentLogError {
                message: with_optional_dependency_repair_hint(line.to_string()),
                code: None,
            });
        }
        // Bare prefixes emitted by older agent binaries whose Display still leaks
        // unwrapped errors. Promote these so they surface instead of the generic
        // "harness exited with status N" fallback.
        if line.starts_with("llm auth:") {
            return Some(AgentLogError {
                message: format!("Agent reported error: {line}"),
                code: Some(-32001),
            });
        }
        if line.starts_with("llm model not found:") {
            return Some(AgentLogError {
                message: format!("Agent reported error: {line}"),
                code: Some(-32002),
            });
        }
        // Upstream Codex/Claude optional-dep crash text often appears as a bare
        // multi-line stderr dump before any "Agent reported error" wrapper.
        if line
            .to_ascii_lowercase()
            .contains("missing optional dependency")
        {
            return Some(AgentLogError {
                message: with_optional_dependency_repair_hint(line.to_string()),
                code: Some(1001),
            });
        }
        None
    })
}
