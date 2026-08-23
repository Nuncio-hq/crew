import {
  snapshotFromObserverEvent,
  type DeclaredPlanEntry,
  type DeclaredPlanSnapshot,
  type DeclaredPlanSource,
  type DeclaredPlanStatus,
} from "@/features/agents/declaredPlanSnapshot";
import { compareObserverEvents } from "@/features/agents/observerRelayRetention";
import type { ObserverEvent } from "@/features/agents/ui/agentSessionTypes";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";

export type AgentPlanLiveness =
  | "working"
  | "sleeping"
  | "disconnected"
  | "idle";

export type AgentDeclaredPlan = {
  agentPubkey: string;
  agentName: string;
  conversationId: string;
  entries: DeclaredPlanEntry[];
  updatedAt: string | null;
  source: DeclaredPlanSource | null;
  liveness: AgentPlanLiveness;
  unknown: boolean;
  sessionId: string | null;
};

export type DeclaredPlanAgentInput = {
  agentPubkey: string;
  agentName?: string;
  events: readonly ObserverEvent[];
  retiredSessionIds?: ReadonlySet<string>;
  liveSessionId?: string | null;
  liveness: AgentPlanLiveness;
};

/**
 * One latest snapshot per `(agentPubkey, conversationId)`. Never merges
 * across agents. Retired-session events are ignored so a failed
 * `session/load` / stale-lineage rebuild cannot keep the dead snapshot.
 */
export function projectAgentDeclaredPlan(
  conversationId: string,
  input: DeclaredPlanAgentInput,
): AgentDeclaredPlan {
  const agentPubkey = normalizePubkey(input.agentPubkey);
  const matching = input.events.filter((event) =>
    eventMatchesConversation(event, conversationId),
  );
  let snapshot: DeclaredPlanSnapshot | null = null;
  let latestAuthority: ObserverEvent | null = null;
  for (const event of matching) {
    if (!sessionIsLive(event, input.retiredSessionIds, input.liveSessionId)) {
      continue;
    }
    const parsed = snapshotFromObserverEvent(event);
    if (parsed === null) continue;
    if (
      !latestAuthority ||
      compareDeclaredPlanAuthorityEvents(event, latestAuthority) > 0
    ) {
      latestAuthority = event;
      snapshot = parsed === "clear" ? null : parsed;
    }
  }

  const unknown = snapshot == null;
  return {
    agentPubkey,
    agentName: input.agentName?.trim() || truncatePubkey(agentPubkey),
    conversationId,
    entries: snapshot?.entries ?? [],
    updatedAt: snapshot?.updatedAt ?? null,
    source: snapshot?.source ?? null,
    liveness: input.liveness,
    unknown,
    sessionId: snapshot?.sessionId ?? input.liveSessionId ?? null,
  };
}

export function projectDeclaredPlansForThread(
  conversationId: string,
  agents: readonly DeclaredPlanAgentInput[],
): AgentDeclaredPlan[] {
  return agents.map((agent) => projectAgentDeclaredPlan(conversationId, agent));
}

export function collectParticipatingAgentPubkeys(args: {
  knownAgentPubkeys: ReadonlySet<string>;
  messages: readonly TimelineMessage[];
  observerAgentPubkeys?: readonly string[];
  activeTurnPubkeys?: readonly string[];
}): string[] {
  const known = new Set(
    [...args.knownAgentPubkeys].map((pubkey) => normalizePubkey(pubkey)),
  );
  const ordered: string[] = [];
  const seen = new Set<string>();
  const consider = (raw: string | undefined) => {
    const pubkey = raw ? normalizePubkey(raw) : "";
    if (!pubkey || seen.has(pubkey) || !known.has(pubkey)) return;
    seen.add(pubkey);
    ordered.push(pubkey);
  };

  for (const message of args.messages) {
    consider(message.pubkey);
    consider(message.signerPubkey);
    for (const tag of message.tags ?? []) {
      if (tag[0] === "p") consider(tag[1]);
    }
  }
  for (const pubkey of args.activeTurnPubkeys ?? []) consider(pubkey);
  for (const pubkey of args.observerAgentPubkeys ?? []) consider(pubkey);
  return ordered;
}

export function resolveAgentPlanName(
  pubkey: string,
  profiles?: UserProfileLookup,
  managedName?: string,
): string {
  const normalized = normalizePubkey(pubkey);
  const profileName = profiles?.[normalized]?.displayName?.trim();
  if (profileName) return profileName;
  const managed = managedName?.trim();
  if (managed) return managed;
  return truncatePubkey(normalized);
}

export function eventMatchesConversation(
  event: ObserverEvent,
  conversationId: string,
): boolean {
  return event.conversationId === conversationId;
}

/**
 * Live session for this conversation, taken from the live observer window
 * only. Archived frames from a retired session must not become the live id.
 */
export function latestSessionIdFromEvents(
  events: readonly ObserverEvent[],
  conversationId: string,
): string | null {
  const matching = events.filter(
    (event) =>
      eventMatchesConversation(event, conversationId) &&
      Boolean(event.sessionId),
  );
  matching.sort(compareObserverEvents);
  for (let i = matching.length - 1; i >= 0; i--) {
    const sessionId = matching[i]?.sessionId;
    if (sessionId) return sessionId;
  }
  return null;
}

function sessionIsLive(
  event: ObserverEvent,
  retiredSessionIds: ReadonlySet<string> | undefined,
  liveSessionId: string | null | undefined,
): boolean {
  if (!event.sessionId) return true;
  if (retiredSessionIds?.has(event.sessionId)) return false;
  if (liveSessionId && event.sessionId !== liveSessionId) return false;
  return true;
}

/**
 * Plan authority is timestamp-first: `seq` resets when an agent process
 * restarts while the ACP session id can stay the same, so seq-only ordering
 * would keep a stale declared snapshot (#190).
 */
function compareDeclaredPlanAuthorityEvents(
  left: ObserverEvent,
  right: ObserverEvent,
): number {
  const leftTime = Date.parse(left.timestamp);
  const rightTime = Date.parse(right.timestamp);
  if (Number.isFinite(leftTime) && Number.isFinite(rightTime)) {
    const timeDiff = leftTime - rightTime;
    if (timeDiff !== 0) return timeDiff;
  }
  return compareObserverEvents(left, right);
}

export type { DeclaredPlanEntry, DeclaredPlanSource, DeclaredPlanStatus };
