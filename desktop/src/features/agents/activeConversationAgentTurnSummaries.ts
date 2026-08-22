import * as React from "react";

import {
  getActiveTurnsDataVersion,
  subscribeAgentLiveness,
} from "@/features/agents/activeAgentTurnsLiveness";
import {
  subscribeActiveAgentTurns,
  walkActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import type { AgentProgressKind } from "@/features/agents/agentAttention";

/** Per-agent active turn summary scoped to one conversation/thread. */
export type ActiveConversationAgentTurnSummary = {
  agentPubkey: string;
  anchorAt: number;
  lastSeenAt: number;
  lastSubstantiveProgressAt: number;
  progressKind: AgentProgressKind;
  progressLabel: string;
  triggeringEventIds: string[];
  runs: Array<{
    sessionId: string;
    turnId: string;
    triggeringEventIds: string[];
  }>;
};

const EMPTY: ActiveConversationAgentTurnSummary[] = [];
const cache = new Map<string, ActiveConversationAgentTurnSummary[]>();
let cacheGeneration = -1;

function ensureCacheGeneration() {
  // Keyed on the data version (not the generation): summaries snapshot the
  // in-place-mutated lastSeenAt/progress fields, so a read after a liveness
  // frame must rebuild even though membership is unchanged.
  const generation = getActiveTurnsDataVersion();
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

  const byAgent = new Map<string, ActiveConversationAgentTurnSummary>();
  walkActiveAgentTurns((agentKey, turn, offset) => {
    if (turn.conversationId !== conversationId) return;
    const anchorAt = turn.startedAt + offset;
    const prior = byAgent.get(agentKey);
    if (!prior) {
      byAgent.set(agentKey, {
        agentPubkey: agentKey,
        anchorAt,
        lastSeenAt: turn.lastSeenAt,
        lastSubstantiveProgressAt: turn.lastSubstantiveProgressAt,
        progressKind: turn.progressKind,
        progressLabel: turn.progressLabel,
        triggeringEventIds: [...turn.triggeringEventIds],
        runs: turn.sessionId
          ? [
              {
                sessionId: turn.sessionId,
                turnId: turn.turnId,
                triggeringEventIds: [...turn.triggeringEventIds],
              },
            ]
          : [],
      });
      return;
    }
    if (anchorAt < prior.anchorAt) prior.anchorAt = anchorAt;
    if (turn.lastSeenAt < prior.lastSeenAt) prior.lastSeenAt = turn.lastSeenAt;
    if (turn.lastSubstantiveProgressAt < prior.lastSubstantiveProgressAt) {
      prior.lastSubstantiveProgressAt = turn.lastSubstantiveProgressAt;
      prior.progressKind = turn.progressKind;
      prior.progressLabel = turn.progressLabel;
    }
    prior.triggeringEventIds = [
      ...new Set([...prior.triggeringEventIds, ...turn.triggeringEventIds]),
    ];
    if (turn.sessionId) {
      prior.runs.push({
        sessionId: turn.sessionId,
        turnId: turn.turnId,
        triggeringEventIds: [...turn.triggeringEventIds],
      });
    }
  });

  const result =
    byAgent.size === 0
      ? EMPTY
      : [...byAgent.values()].sort((a, b) =>
          a.agentPubkey.localeCompare(b.agentPubkey),
        );
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
  // Membership changes arrive on the global subscription; per-frame liveness
  // for the conversation's agents arrives on their per-agent subscriptions,
  // resubscribed whenever the agent set changes.
  const agentKey = getSnapshot()
    .map((summary) => summary.agentPubkey)
    .join(",");
  const subscribe = React.useCallback(
    (onStoreChange: () => void) => {
      const unsubscribers = [
        subscribeActiveAgentTurns(onStoreChange),
        ...(agentKey === ""
          ? []
          : agentKey
              .split(",")
              .map((pubkey) => subscribeAgentLiveness(pubkey, onStoreChange))),
      ];
      return () => {
        for (const unsubscribe of unsubscribers) unsubscribe();
      };
    },
    [agentKey],
  );
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
