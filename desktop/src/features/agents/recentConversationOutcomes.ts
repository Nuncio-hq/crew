import * as React from "react";

import {
  CONVERSATION_OUTCOME_TTL_MS,
  getActiveAgentsForConversation,
  getActiveTurnsGeneration,
  getConversationOutcomeEntry,
  subscribeActiveAgentTurns,
  type ConversationOutcomeEntry,
} from "@/features/agents/activeAgentTurnsStore";

/** UI-facing terminal outcome for one conversation (TTL + active-suppressed). */
export type RecentConversationOutcome = {
  outcome: "completed" | "error" | "lost-contact";
  agentPubkey: string;
  endedAt: number;
  channelId: string;
  failedEventIds?: string[];
};

const cache = new Map<string, RecentConversationOutcome | null>();
let cacheGeneration = -1;
const NULL_OUTCOME: RecentConversationOutcome | null = null;

function ensureCacheGeneration() {
  const generation = getActiveTurnsGeneration();
  if (generation === cacheGeneration) return;
  cache.clear();
  cacheGeneration = generation;
}

function toRecent(
  entry: ConversationOutcomeEntry,
  now: number,
): RecentConversationOutcome | null {
  if (now - entry.endedAt > CONVERSATION_OUTCOME_TTL_MS) return null;
  return {
    outcome: entry.outcome,
    agentPubkey: entry.agentPubkey,
    endedAt: entry.endedAt,
    channelId: entry.channelId,
    failedEventIds: entry.failedEventIds,
  };
}

/**
 * Latest terminal outcome for a conversation, or null when:
 * - no ledger entry,
 * - the entry is past the TTL,
 * - or the conversation still has any ACTIVE turn (running wins).
 *
 * Reference-stable until the active-turns generation bumps.
 */
export function getRecentOutcomeForConversation(
  conversationId: string | null | undefined,
  now: number = Date.now(),
): RecentConversationOutcome | null {
  if (!conversationId) return NULL_OUTCOME;
  ensureCacheGeneration();
  const cached = cache.get(conversationId);
  if (cached !== undefined) {
    // Re-check TTL against `now` without rebuilding — expired entries drop.
    if (cached === null) return NULL_OUTCOME;
    if (now - cached.endedAt > CONVERSATION_OUTCOME_TTL_MS) return NULL_OUTCOME;
    return cached;
  }

  if (getActiveAgentsForConversation(conversationId).length > 0) {
    cache.set(conversationId, null);
    return NULL_OUTCOME;
  }

  const entry = getConversationOutcomeEntry(conversationId);
  const recent = entry ? toRecent(entry, now) : null;
  cache.set(conversationId, recent);
  return recent;
}

export function useRecentOutcomeForConversation(
  conversationId: string | null | undefined,
): RecentConversationOutcome | null {
  const getSnapshot = React.useCallback(
    () => getRecentOutcomeForConversation(conversationId),
    [conversationId],
  );
  return React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getSnapshot,
    getSnapshot,
  );
}
