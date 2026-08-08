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
import { canonicalRelayUrl } from "../managedAgentRuntimeStatus";

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
    if (!name || name === HERMES_FORBIDDEN_PROFILE_NAME) continue;
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
  return normalized.filter((name) => name.toLowerCase().includes(q));
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

function relayIdentity(relayUrl: string): string {
  return canonicalRelayUrl(relayUrl) ?? relayUrl.trim();
}

/**
 * Project intentional profile reuse from the local managed-agent store.
 * Same-relay records stay out of `otherUses` because occupancy owns that
 * duplicate-binding boundary; records on other relays are informational.
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

  const currentRelay = relayIdentity(args.currentRelayUrl);
  const editingPubkey = args.editingPubkey?.trim() || null;
  const currentAgentName = args.currentAgentName?.trim() || "";
  const communityByRelay = new Map(
    args.communities.map((community) => [
      relayIdentity(community.relayUrl),
      community.name.trim() || community.relayUrl,
    ]),
  );
  const currentCommunityName = communityByRelay.get(currentRelay);
  const seenUses = new Set<string>();
  const otherUses: HermesProfileOtherUse[] = [];

  for (const agent of args.agents) {
    if (agent.hermesProfile?.trim() !== profile) continue;
    if (editingPubkey && agent.pubkey === editingPubkey) continue;
    const rawAgentRelay = agent.relayUrl.trim();
    if (!rawAgentRelay) continue;
    const agentRelay = relayIdentity(rawAgentRelay);
    if (agentRelay === currentRelay) continue;
    const key = `${agent.pubkey}\u0000${agentRelay}`;
    if (seenUses.has(key)) continue;
    seenUses.add(key);
    otherUses.push({
      agentName: agent.name.trim() || truncatePubkey(agent.pubkey),
      agentPubkey: agent.pubkey,
      communityName: communityByRelay.get(agentRelay) ?? rawAgentRelay,
      relayUrl: rawAgentRelay,
    });
  }

  otherUses.sort(
    (left, right) =>
      left.communityName.localeCompare(right.communityName) ||
      left.agentName.localeCompare(right.agentName),
  );
  const usedIn = [
    ...(currentCommunityName ? [currentCommunityName] : []),
    ...otherUses.map((usage) => usage.communityName),
  ].filter((name, index, values) => values.indexOf(name) === index);
  const hasPresentationMismatch = Boolean(
    currentAgentName &&
      otherUses.some(
        (usage) =>
          usage.agentName.localeCompare(currentAgentName, undefined, {
            sensitivity: "accent",
          }) !== 0,
      ),
  );

  return { usedIn, otherUses, hasPresentationMismatch };
}

/**
 * Join disk profiles with managed agents on one relay (C-10 early UX).
 * Server duplicate reject remains authoritative.
 */
export function buildHermesProfileOccupancy(args: {
  profiles: readonly string[];
  agents: readonly HermesProfileOccupancyAgent[];
  relayUrl: string;
  editingPubkey?: string | null;
}): Map<string, HermesProfileOccupancy> {
  const relay = args.relayUrl.trim();
  const editing = args.editingPubkey?.trim() || null;
  const map = new Map<string, HermesProfileOccupancy>();

  for (const name of normalizeHermesProfileList(args.profiles)) {
    map.set(name, { status: "free" });
  }

  for (const agent of args.agents) {
    const profile = agent.hermesProfile?.trim() || "";
    if (!profile || profile === HERMES_FORBIDDEN_PROFILE_NAME) continue;
    if (validateHermesProfileName(profile) != null) continue;
    if (agent.relayUrl.trim() !== relay) continue;

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
