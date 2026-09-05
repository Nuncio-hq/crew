//! Resolved process environment and harness launch descriptors.
use std::collections::BTreeMap;

// ── EffectiveAgentEnv ─────────────────────────────────────────────────────────

/// The resolved environment that a spawn of `record` would actually receive.
///
/// Assembled from: baked build defaults (floor) → runtime metadata env vars
/// → merged user env_vars (last-wins) → reserved-key filtered.
///
/// `config_file_path` is the harness config file path (if any) — not part of
/// the process env but relevant for display and future write-back dispatch.
/// `effective_command` is the resolved harness binary name (e.g. `"buzz-agent"`,
/// `"goose"`) after persona and override resolution.
#[derive(Debug, Clone)]
pub(crate) struct EffectiveAgentEnv {
    /// The process-env map the spawned harness would receive.
    pub env: BTreeMap<String, String>,
    /// Harness config file path, if any (e.g. `~/.config/goose/config.yaml`).
    // Not read yet; kept for the unified-agent-record rewrite (chunk A) which
    // replaces this resolution path wholesale.
    #[allow(dead_code)]
    pub config_file_path: Option<&'static str>,
    /// The resolved harness binary name (e.g. `"buzz-agent"`, `"goose"`).
    pub effective_command: String,
    pub hermes_profile: Option<String>, // D-019; readiness only
}

// ── Typed effective-harness descriptor ───────────────────────────────────────
// Produced by resolve_effective_harness_descriptor; consumed by spawn,
// spawn_snapshot, summaries, get_agent_models, and readiness.

/// Complete effective harness spawn description: command, args, and layered env.
#[derive(Debug, Clone)]
pub(crate) struct EffectiveHarnessDescriptor {
    /// The raw effective command string (e.g. `"buzz-agent"`, `"my-acp-agent"`).
    /// Used for `known_acp_runtime` lookup and hashing.
    pub command: String,
    /// Normalized effective args.  Instance args win when non-empty; otherwise
    /// the harness definition's args apply.
    pub args: Vec<String>,
    /// The full layered process env: baked floor → runtime metadata → definition
    /// env → global → persona → agent.
    pub env: BTreeMap<String, String>,
}
