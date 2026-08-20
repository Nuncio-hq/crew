/**
 * Client-side Hermes profile binding helpers (D-019 / feature 0001).
 *
 * Mirrors `validate_hermes_profile_name` in
 * `desktop/src-tauri/src/managed_agents/hermes_profile.rs`. Server still
 * enforces; this gives create/edit surfaces immediate, friendly feedback.
 */

import type { AcpRuntimeCatalogEntry, RespondToMode } from "@/shared/api/types";
import { truncatePubkey } from "@/shared/lib/pubkey";
import type { AgentRunLocation } from "./agentAccessWarning";
import { deriveRuntimeCapabilities } from "@/shared/api/runtimeCapabilities";

/** Manager personal home profile (`~/.hermes`). Bindable after confirmation. */
export const HERMES_HOME_PROFILE_NAME = "default";

/** @deprecated Use {@link HERMES_HOME_PROFILE_NAME} / {@link isHermesHomeProfile}. */
export const HERMES_FORBIDDEN_PROFILE_NAME = HERMES_HOME_PROFILE_NAME;

/** Spike 0011 / Hermes CLI: `^[a-z0-9][a-z0-9_-]{0,63}$`. */
const HERMES_PROFILE_NAME_RE = /^[a-z0-9][a-z0-9_-]{0,63}$/;

export function isHermesHomeProfile(name: string): boolean {
  return name.trim() === HERMES_HOME_PROFILE_NAME;
}

/** False for the home profile — Crew never writes `~/.hermes`. */
export function crewMayMutateHermesProfile(name: string): boolean {
  return !isHermesHomeProfile(name);
}

export function hermesHomeProfileDisplayLabel(name: string): string {
  return isHermesHomeProfile(name) ? "Personal (default)" : name.trim();
}

export function hermesHomeProfileEditInHermesCopy(): string {
  return "Edit this profile in Hermes, not Crew.";
}

export function hermesHomeProfileConfirmSurfaces(): string[] {
  return [
    "Desktop chat",
    "SOUL.md",
    "memory",
    "skills",
    "credentials",
    "cron",
    "gateways",
  ];
}

export function hermesHomeProfileUnconfirmedError(
  name: string,
  confirmed: boolean,
): string | null {
  if (!isHermesHomeProfile(name) || confirmed) return null;
  return "Confirm binding your personal default profile before continuing.";
}

export function ensureHermesHomeProfileOption(
  profiles: readonly string[],
): string[] {
  const normalized = normalizeHermesProfileList(profiles).filter(
    (name) => !isHermesHomeProfile(name),
  );
  return [HERMES_HOME_PROFILE_NAME, ...normalized];
}

/**
 * True when the runtime catalog projects a profile-owned model: the harness
 * declares a profile CLI flag, has no model env var, and is provider-locked.
 * Capability facts only — never compare `runtime.id`.
 */
export function runtimeOwnsModelViaProfile(
  runtime: AcpRuntimeCatalogEntry | undefined,
): boolean {
  return (
    deriveRuntimeCapabilities(runtime).modelSource === "profileWriteThrough"
  );
}

/** True when create/edit should show the Hermes profile binding control. */
export function runtimeOffersProfileBinding(
  runtime: AcpRuntimeCatalogEntry | undefined,
): boolean {
  return Boolean(runtime?.profileArg?.trim());
}

/** Clear only for known non-profile metadata or an explicit custom runtime. */
export function shouldClearHermesProfileOnRuntimeChange(
  runtime: AcpRuntimeCatalogEntry | undefined,
  explicitCustomRuntime = false,
): boolean {
  return (
    explicitCustomRuntime ||
    (runtime !== undefined && !runtimeOffersProfileBinding(runtime))
  );
}

function normalizedHermesProfile(
  raw: string | null | undefined,
): string | null {
  return raw?.trim() || null;
}

/** Profile value for create: hidden state never survives a runtime switch. */
export function resolveHermesProfileForCreate(
  rawProfile: string,
  runtime: AcpRuntimeCatalogEntry | undefined,
): string | null {
  return runtimeOffersProfileBinding(runtime)
    ? normalizedHermesProfile(rawProfile)
    : null;
}

/**
 * Profile patch for edit: clear a stored binding when the prospective runtime
 * no longer advertises profile binding; otherwise preserve ordinary no-op
 * omission semantics.
 */
export function resolveHermesProfileForUpdate(
  currentProfile: string | null | undefined,
  rawProfile: string,
  runtime: AcpRuntimeCatalogEntry | undefined,
  confirmedNoProfileBinding = false,
): string | null | undefined {
  if (runtime === undefined && !confirmedNoProfileBinding) return undefined;
  const current = normalizedHermesProfile(currentProfile);
  const next = runtimeOffersProfileBinding(runtime)
    ? normalizedHermesProfile(rawProfile)
    : null;
  return next === current ? undefined : next;
}

export type ProfileBoundAgentBoundary = {
  access: "Owner only";
  autonomy: "Full";
  backend: "This Mac";
  profile: string;
  usedIn: string[];
};

/**
 * Trusted-autonomy/local-boundary projection for profile-binding runtimes.
 * The caller supplies the capability result; this helper never identifies a
 * harness by id.
 */
export function deriveProfileBoundAgentBoundary(args: {
  profileBindingOffered: boolean;
  profile: string;
  usedIn: readonly string[];
}): ProfileBoundAgentBoundary | null {
  if (!args.profileBindingOffered) return null;
  return {
    access: "Owner only",
    autonomy: "Full",
    backend: "This Mac",
    profile: args.profile.trim(),
    usedIn: [...args.usedIn],
  };
}

export function profileBoundAccessError(
  profileBindingOffered: boolean,
  respondTo: RespondToMode | null | undefined,
): string | null {
  if (!profileBindingOffered || !respondTo || respondTo === "owner-only") {
    return null;
  }
  return "Hermes profile agents use full autonomy and must stay owner-only. Choose Only me to continue.";
}

export function profileBoundBackendError(
  profileBindingOffered: boolean,
  runLocation: AgentRunLocation | null,
  editing = false,
): string | null {
  if (!profileBindingOffered || runLocation !== "remote") return null;
  if (editing) {
    return "Hermes profiles live on this Mac and cannot run on a remote backend. Delete and recreate this agent on This computer to continue.";
  }
  return "Hermes profiles live on this Mac and cannot run on a remote backend. Choose This computer to continue.";
}

/**
 * Validate a profile name for Crew binding.
 * Returns `null` when valid (or empty — emptiness is a separate requiredness
 * check); otherwise a friendly message.
 */
export function validateHermesProfileName(raw: string): string | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return null;
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

/**
 * Normalize a disk/IPC profile list for the picker: drop empty/`default`/invalid
 * names, de-dupe, sort. Directory IPC already filters; this is belt-and-suspenders
 * for mocks and future CLI sources.
 */
export function normalizeHermesProfileList(
  profiles: readonly string[],
): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of profiles) {
    const name = raw.trim();
    if (!name) continue;
    if (validateHermesProfileName(name) != null) continue;
    if (seen.has(name)) continue;
    seen.add(name);
    out.push(name);
  }
  out.sort((a, b) => a.localeCompare(b));
  return out;
}

/** Typeahead filter over normalized profile names (substring, case-insensitive). */
export function filterHermesProfileOptions(
  profiles: readonly string[],
  query: string,
): string[] {
  const normalized = normalizeHermesProfileList(profiles);
  const q = query.trim().toLowerCase();
  if (!q) return normalized;
  return normalized.filter(
    (name) =>
      name.toLowerCase().includes(q) ||
      hermesHomeProfileDisplayLabel(name).toLowerCase().includes(q),
  );
}

/**
 * Show create-in-place only for a valid name that is not already on disk.
 * Never silent-create — callers still require an explicit button click.
 */
export function shouldShowHermesProfileCreate(
  name: string,
  profiles: readonly string[],
): boolean {
  const trimmed = name.trim();
  if (!trimmed || validateHermesProfileName(trimmed) != null) return false;
  if (isHermesHomeProfile(trimmed)) return false;
  const existing = new Set(
    normalizeHermesProfileList(profiles).map((p) => p.toLowerCase()),
  );
  return !existing.has(trimmed.toLowerCase());
}

export type HermesProfileOccupancy =
  | { status: "free" }
  | { status: "self" }
  | { status: "bound"; agentName: string; agentPubkey: string };

export type HermesProfileOccupancyAgent = {
  pubkey: string;
  name: string;
  hermesProfile: string | null | undefined;
  relayUrl: string;
};

export type HermesProfileCommunity = {
  name: string;
  relayUrl: string;
};

export type HermesProfileOtherUse = {
  agentName: string;
  agentPubkey: string;
  communityName: string;
  relayUrl: string;
};

export type HermesProfileUsage = {
  usedIn: string[];
  otherUses: HermesProfileOtherUse[];
  hasPresentationMismatch: boolean;
};

/**
 * Project the communities served by one installation-wide managed agent.
 * `ManagedAgent.relayUrl` is a legacy pin and cannot identify community
 * occupancy now that one agent owns runtime pairs for every community.
 */
export function deriveHermesProfileUsage(args: {
  profile: string;
  agents: readonly HermesProfileOccupancyAgent[];
  communities: readonly HermesProfileCommunity[];
  currentRelayUrl: string;
  editingPubkey?: string | null;
  currentAgentName?: string | null;
}): HermesProfileUsage {
  const profile = args.profile.trim();
  if (!profile || validateHermesProfileName(profile) != null) {
    return { usedIn: [], otherUses: [], hasPresentationMismatch: false };
  }

  const usedIn = args.communities
    .map((community) => community.name.trim() || community.relayUrl.trim())
    .filter(
      (name, index, values) => Boolean(name) && values.indexOf(name) === index,
    );
  return { usedIn, otherUses: [], hasPresentationMismatch: false };
}

/**
 * Join disk profiles with installation-wide managed agents (C-10 early UX).
 * Server duplicate reject remains authoritative.
 */
export function buildHermesProfileOccupancy(args: {
  profiles: readonly string[];
  agents: readonly HermesProfileOccupancyAgent[];
  editingPubkey?: string | null;
}): Map<string, HermesProfileOccupancy> {
  const editing = args.editingPubkey?.trim() || null;
  const map = new Map<string, HermesProfileOccupancy>();

  for (const name of normalizeHermesProfileList(args.profiles)) {
    map.set(name, { status: "free" });
  }

  for (const agent of args.agents) {
    const profile = agent.hermesProfile?.trim() || "";
    if (!profile || validateHermesProfileName(profile) != null) continue;
    if (editing && agent.pubkey === editing) {
      map.set(profile, { status: "self" });
      continue;
    }

    // Prefer first other binder; do not overwrite self if listed twice.
    const current = map.get(profile);
    if (current?.status === "self") continue;

    map.set(profile, {
      status: "bound",
      agentName: agent.name.trim() || truncatePubkey(agent.pubkey),
      agentPubkey: agent.pubkey,
    });
  }

  return map;
}

/** Occupancy gate for save: bound-to-other blocks; free/self/unknown OK. */
export function hermesProfileOccupancyError(
  raw: string,
  occupancy: ReadonlyMap<string, HermesProfileOccupancy>,
): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const entry = occupancy.get(trimmed);
  if (entry?.status !== "bound") return null;
  return `Hermes profile '${trimmed}' is already bound to agent '${entry.agentName}'.`;
}

/** Badge / secondary label for a profile option row. */
export function hermesProfileOccupancyLabel(
  occupancy: HermesProfileOccupancy | undefined,
): string {
  if (!occupancy || occupancy.status === "free") return "free";
  if (occupancy.status === "self") return "this agent";
  return `bound · ${occupancy.agentName}`;
}
