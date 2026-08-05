import * as React from "react";

import {
  CONVERSATION_OUTCOME_TTL_MS,
  getActiveTurnsGeneration,
  subscribeActiveAgentTurns,
  walkActiveAgentTurns,
  walkConversationOutcomes,
} from "@/features/agents/activeAgentTurnsStore";

/** One thread/conversation surfaced in the channel agent digest. */
export type ConversationRef = {
  conversationId: string;
  agentPubkeys: string[];
  /** Earliest live anchor (running) or terminal endedAt (done/failed). */
  anchorAt: number;
};

export type AgentThreadDigest = {
  running: ConversationRef[];
  failed: ConversationRef[];
  done: ConversationRef[];
};

const cache = new Map<string, AgentThreadDigest | null>();
let cacheGeneration = -1;

function ensureCacheGeneration() {
  const generation = getActiveTurnsGeneration();
  if (generation === cacheGeneration) return;
  cache.clear();
  cacheGeneration = generation;
}

function sortRefs(refs: ConversationRef[]): ConversationRef[] {
  return refs.sort((a, b) => {
    if (a.anchorAt !== b.anchorAt) return a.anchorAt - b.anchorAt;
    return a.conversationId.localeCompare(b.conversationId);
  });
}

/**
 * Channel-scoped rollup of agent threads: running (active turns) plus recent
 * failed/done outcomes. Conversations with any active turn are omitted from
 * failed/done. Returns null when every bucket is empty (nothing to render).
 * Reference-stable until the active-turns generation bumps.
 */
export function getAgentThreadDigestForChannel(
  channelId: string | null | undefined,
  now: number = Date.now(),
): AgentThreadDigest | null {
  if (!channelId) return null;
  ensureCacheGeneration();
  const cached = cache.get(channelId);
  if (cached !== undefined) return cached;

  const runningByConversation = new Map<
    string,
    { agentPubkeys: Set<string>; anchorAt: number }
  >();

  walkActiveAgentTurns((agentKey, turn, offset) => {
    if (turn.channelId !== channelId) return;
    const conversationId = turn.conversationId;
    if (!conversationId) return;
    const anchorAt = turn.startedAt + offset;
    const prior = runningByConversation.get(conversationId);
    if (!prior) {
      runningByConversation.set(conversationId, {
        agentPubkeys: new Set([agentKey]),
        anchorAt,
      });
      return;
    }
    prior.agentPubkeys.add(agentKey);
    if (anchorAt < prior.anchorAt) prior.anchorAt = anchorAt;
  });

  const running: ConversationRef[] = [...runningByConversation.entries()].map(
    ([conversationId, summary]) => ({
      conversationId,
      agentPubkeys: [...summary.agentPubkeys].sort(),
      anchorAt: summary.anchorAt,
    }),
  );

  const failed: ConversationRef[] = [];
  const done: ConversationRef[] = [];

  walkConversationOutcomes((conversationId, entry) => {
    if (entry.channelId !== channelId) return;
    if (now - entry.endedAt > CONVERSATION_OUTCOME_TTL_MS) return;
    if (runningByConversation.has(conversationId)) return;
    const ref: ConversationRef = {
      conversationId,
      agentPubkeys: [entry.agentPubkey],
      anchorAt: entry.endedAt,
    };
    if (entry.outcome === "error") failed.push(ref);
    else done.push(ref);
  });

  if (running.length === 0 && failed.length === 0 && done.length === 0) {
    cache.set(channelId, null);
    return null;
  }

  const result: AgentThreadDigest = {
    running: sortRefs(running),
    failed: sortRefs(failed),
    done: sortRefs(done),
  };
  cache.set(channelId, result);
  return result;
}

export function useAgentThreadDigestForChannel(
  channelId: string | null | undefined,
): AgentThreadDigest | null {
  const getSnapshot = React.useCallback(
    () => getAgentThreadDigestForChannel(channelId),
    [channelId],
  );
  return React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getSnapshot,
    getSnapshot,
  );
}
