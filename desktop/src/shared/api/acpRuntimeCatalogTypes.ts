/**
 * ACP runtime catalog entry types (projected from Rust KnownAcpRuntime).
 * Split from types.ts for the desktop file-size ratchet.
 */

export type AcpAvailabilityStatus =
  | "available"
  | "adapter_missing"
  | "adapter_outdated"
  | "cli_missing"
  | "not_installed";

/** Authentication/login status for a CLI-based ACP runtime. */
export type AuthStatus =
  | { status: "logged_in" }
  | { status: "logged_out" }
  | { status: "config_invalid"; diagnostic: string }
  | { status: "not_applicable" }
  | { status: "unknown" };

export type AcpRuntimeCatalogEntry = {
  id: string;
  label: string;
  avatarUrl: string;
  availability: AcpAvailabilityStatus;
  command: string | null;
  binaryPath: string | null;
  defaultArgs: string[];
  mcpCommand: string | null;
  /** Environment variable used to apply the initial model, when supported. */
  modelEnvVar: string | null;
  /** Environment variable used to apply the selected LLM provider, when supported. */
  providerEnvVar: string | null;
  /** Environment variable used to apply thinking effort, when supported. */
  thinkingEnvVar: string | null;
  maxTokensEnvVar: string | null;
  contextLimitEnvVar: string | null;
  maxRoundsEnvVar: string | null;
  installHint: string;
  installInstructionsUrl: string;
  canAutoInstall: boolean;
  /** True when the runtime depends on a separately installed vendor CLI. */
  requiresExternalCli: boolean;
  underlyingCliPath: string | null;
  /** True when an npm adapter step is pending but Node.js / npm is absent. */
  nodeRequired: boolean;
  /** Login/auth status for CLI-based runtimes. */
  authStatus: AuthStatus;
  /** Hint for completing authentication; null when not applicable or already logged in. */
  loginHint: string | null;
  /** "builtin" (compiled in), "preset" (PATH-probed, not editable), or "custom" (user JSON). Controls UI editability. */
  source: "builtin" | "preset" | "custom";
  /** Env for `source: custom` so the edit form can preserve vars on save. */
  definitionEnv?: Record<string, string>;
  profileArg?: string | null; // Rust profile_arg
  /** Provider owned by harness/profile; from KnownAcpRuntime.provider_locked. */
  providerLocked?: boolean;
  /** Maximum parallel agent invocations supported by the runtime harness. */
  maxParallelism?: number;
};

/** An AcpRuntimeCatalogEntry that is confirmed available — command and binaryPath are non-null. */
export type AcpRuntime = AcpRuntimeCatalogEntry & {
  availability: "available";
  command: string;
  binaryPath: string;
};
