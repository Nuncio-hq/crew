/**
 * Client-side Hermes profile binding helpers (D-019 / feature 0001).
 *
 * Mirrors `validate_hermes_profile_name` in
 * `desktop/src-tauri/src/managed_agents/hermes_profile.rs`. Server still
 * enforces; this gives create/edit surfaces immediate, friendly feedback.
 */

import type { AcpRuntimeCatalogEntry } from "@/shared/api/types";

/** Reserved manager-personal profile — never bindable to a Crew agent (P-7). */
export const HERMES_FORBIDDEN_PROFILE_NAME = "default";

/** Spike 0011 / Hermes CLI: `^[a-z0-9][a-z0-9_-]{0,63}$`. */
const HERMES_PROFILE_NAME_RE = /^[a-z0-9][a-z0-9_-]{0,63}$/;

/**
 * True when the runtime catalog projects a profile-owned model: the harness
 * declares a profile CLI flag, has no model env var, and is provider-locked.
 * Capability facts only — never compare `runtime.id`.
 */
export function runtimeOwnsModelViaProfile(
  runtime: AcpRuntimeCatalogEntry | undefined,
): boolean {
  if (!runtime) return false;
  return (
    Boolean(runtime.profileArg?.trim()) &&
    !runtime.modelEnvVar &&
    runtime.providerLocked === true
  );
}

/** True when create/edit should show the Hermes profile binding control. */
export function runtimeOffersProfileBinding(
  runtime: AcpRuntimeCatalogEntry | undefined,
): boolean {
  return Boolean(runtime?.profileArg?.trim());
}

/**
 * Validate a profile name for Crew binding.
 * Returns `null` when valid (or empty — emptiness is a separate requiredness
 * check); otherwise a friendly message.
 */
export function validateHermesProfileName(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return null;
  if (trimmed === HERMES_FORBIDDEN_PROFILE_NAME) {
    return 'The manager\'s personal "default" profile cannot be bound to a Crew agent. Create a named profile instead (see docs/crew/HERMES.md).';
  }
  if (!HERMES_PROFILE_NAME_RE.test(trimmed)) {
    return "Profile names must be lowercase letters, digits, underscores, or hyphens (1–64 characters), starting with a letter or digit.";
  }
  return null;
}

/** Required-field + format check for save gates. */
export function hermesProfileBindingError(
  raw: string,
  required: boolean,
): string | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return required
      ? "Bind a Hermes profile name (create one with: hermes profile create <name>)."
      : null;
  }
  return validateHermesProfileName(trimmed);
}

/** Read-only model row copy when the profile owns the model (C-04 / S-2.2). */
export function profileOwnedModelLabel(
  profileName: string | null | undefined,
  liveModel?: string | null,
): string {
  const name = profileName?.trim() || null;
  const model = liveModel?.trim() || null;
  if (name && model) {
    return `Model: decided by profile ${name} — currently ${model}`;
  }
  if (name) {
    return `Model: decided by profile ${name}`;
  }
  return "Model: decided by profile";
}
