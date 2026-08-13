import {
  snapshotFromObserverEvent,
  type DeclaredPlanEntry,
  type DeclaredPlanSnapshot,
  type DeclaredPlanSource,
  type DeclaredPlanStatus,
} from "@/features/agents/declaredPlanSnapshot";
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
  matching.sort(compareObserverSeq);
  let snapshot: DeclaredPlanSnapshot | null = null;
  for (let i = matching.length - 1; i >= 0; i--) {
    const event = matching[i];
    if (!event) continue;
    if (!sessionIsLive(event, input.retiredSessionIds, input.liveSessionId)) {
      continue;
    }
    const parsed = snapshotFromObserverEvent(event);
    if (parsed === "clear") {
      snapshot = null;
      break;
    }
    if (parsed) {
      snapshot = parsed;
      break;
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
  matching.sort(compareObserverSeq);
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

function compareObserverSeq(left: ObserverEvent, right: ObserverEvent): number {
  if (left.seq !== right.seq) return left.seq - right.seq;
  return left.timestamp.localeCompare(right.timestamp);
}

export type { DeclaredPlanEntry, DeclaredPlanSource, DeclaredPlanStatus };
