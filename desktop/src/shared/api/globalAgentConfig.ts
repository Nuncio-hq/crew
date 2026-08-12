/**
 * Global agent configuration defaults applied to ALL agents.
 *
 * Lowest user-settable layer — per-agent and persona values win on any key
 * collision. Mirrors the Rust `GlobalAgentConfig` struct.
 *
 * Precedence: baked floor < global < persona < per-agent.
 */
export type GlobalAgentConfig = {
  /** Global env vars injected into all agents unconditionally. */
  env_vars: Record<string, string>;
  /** Global fallback provider (e.g. "anthropic", "databricks_v2"). Null = no global default. */
  provider: string | null;
  /** Global fallback model identifier. Null = no global default. */
  model: string | null;
  /** Preferred ACP runtime for agents without a persona-specific runtime. */
  preferred_runtime: string | null;
  /**
   * Per-app model id for owner-triggered guided handover summarizer (#173).
   * Not a per-agent attribute — same format for every agent.
   */
  handover_summarizer_model: string | null;
  /** Compaction aging threshold (1–10). Default 3. */
  compaction_aging_threshold: number | null;
  /** Turn-count aging safety net. Default 100. */
  turn_aging_threshold: number | null;
};

/**
 * Result returned by `set_global_agent_config`.
 *
 * Mirrors the Rust `GlobalAgentConfigSaveResult` struct.
 */
export type GlobalAgentConfigSaveResult = {
  /** The persisted global config (after strip-on-write). */
  config: GlobalAgentConfig;
  /** Number of local agents successfully stopped and restarted. */
  restarted_count: number;
  /** Number of agents whose stop succeeded but respawn failed. */
  failed_restart_count: number;
};
