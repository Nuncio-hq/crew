import * as React from "react";

import {
  getActiveTurnsGeneration,
  subscribeActiveAgentTurns,
  walkActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";

/** Per-agent active turn summary scoped to one conversation/thread. */
export type ActiveConversationAgentTurnSummary = {
  agentPubkey: string;
  anchorAt: number;
};

const EMPTY: ActiveConversationAgentTurnSummary[] = [];
const cache = new Map<string, ActiveConversationAgentTurnSummary[]>();
let cacheGeneration = -1;

function ensureCacheGeneration() {
  const generation = getActiveTurnsGeneration();
  if (generation === cacheGeneration) return;
  cache.clear();
  cacheGeneration = generation;
}

/**
 * Returns per-agent active-turn summaries for one conversation, sorted by
 * pubkey and anchored to each agent's earliest live turn in that conversation.
 * Array reference is cached until the active-turns generation bumps.
 */
export function getActiveTurnSummariesForConversation(
  conversationId: string | null | undefined,
): ActiveConversationAgentTurnSummary[] {
  if (!conversationId) return EMPTY;
  ensureCacheGeneration();
  const cached = cache.get(conversationId);
  if (cached) return cached;

  const earliestByAgent = new Map<string, number>();
  walkActiveAgentTurns((agentKey, turn, offset) => {
    if (turn.conversationId !== conversationId) return;
    const anchorAt = turn.startedAt + offset;
    const prior = earliestByAgent.get(agentKey);
    if (prior === undefined || anchorAt < prior) {
      earliestByAgent.set(agentKey, anchorAt);
    }
  });

  const result =
    earliestByAgent.size === 0
      ? EMPTY
      : [...earliestByAgent.entries()]
          .map(([agentPubkey, anchorAt]) => ({ agentPubkey, anchorAt }))
          .sort((a, b) => a.agentPubkey.localeCompare(b.agentPubkey));
  cache.set(conversationId, result);
  return result;
}

export function useActiveTurnSummariesForConversation(
  conversationId: string | null | undefined,
): ActiveConversationAgentTurnSummary[] {
  const getSnapshot = React.useCallback(
    () => getActiveTurnSummariesForConversation(conversationId),
    [conversationId],
  );
  return React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getSnapshot,
    getSnapshot,
  );
}
