import {
  getActiveTurnsGeneration,
  walkActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import type { AgentProgressKind } from "@/features/agents/agentAttention";

/** One conversation/thread with active agent work, aggregated across agents. */
export type ActiveConversationTurnSummary = {
  channelId: string;
  conversationId: string;
  anchorAt: number;
  agentCount: number;
  agentPubkeys: string[];
  agentNames?: string[];
  lastSeenAt: number;
  lastSubstantiveProgressAt: number;
  progressKind: AgentProgressKind;
  progressLabel: string;
  triggeringEventIds: string[];
  agentTriggerPairs: Array<{
    agentPubkey: string;
    eventId: string;
    sessionId: string;
    turnId: string;
  }>;
};

const EMPTY: ActiveConversationTurnSummary[] = [];
let cachedGeneration = -1;
let cached: ActiveConversationTurnSummary[] = EMPTY;

/** Active conversations sorted by id, with the oldest agent state projected. */
export function getActiveTurnsByConversation(): ActiveConversationTurnSummary[] {
  const generation = getActiveTurnsGeneration();
  if (generation === cachedGeneration) return cached;

  const summaries = new Map<
    string,
    Omit<
      ActiveConversationTurnSummary,
      | "conversationId"
      | "agentCount"
      | "agentPubkeys"
      | "triggeringEventIds"
      | "agentTriggerPairs"
    > & {
      agentPubkeys: Set<string>;
      triggeringEventIds: Set<string>;
      agentTriggerPairs: Set<string>;
    }
  >();
  walkActiveAgentTurns((agentPubkey, turn, offset) => {
    if (!turn.conversationId) return;
    const anchorAt = turn.startedAt + offset;
    const summary = summaries.get(turn.conversationId);
    if (!summary) {
      summaries.set(turn.conversationId, {
        anchorAt,
        channelId: turn.channelId,
        agentPubkeys: new Set([agentPubkey]),
        lastSeenAt: turn.lastSeenAt,
        lastSubstantiveProgressAt: turn.lastSubstantiveProgressAt,
        progressKind: turn.progressKind,
        progressLabel: turn.progressLabel,
        triggeringEventIds: new Set(turn.triggeringEventIds),
        agentTriggerPairs: new Set(
          turn.sessionId
            ? turn.triggeringEventIds.map(
                (eventId) =>
                  `${agentPubkey}\0${eventId}\0${turn.sessionId}\0${turn.turnId}`,
              )
            : [],
        ),
      });
      return;
    }
    summary.agentPubkeys.add(agentPubkey);
    for (const eventId of turn.triggeringEventIds) {
      summary.triggeringEventIds.add(eventId);
      if (turn.sessionId)
        summary.agentTriggerPairs.add(
          `${agentPubkey}\0${eventId}\0${turn.sessionId}\0${turn.turnId}`,
        );
    }
    if (anchorAt < summary.anchorAt) summary.anchorAt = anchorAt;
    if (turn.lastSeenAt < summary.lastSeenAt)
      summary.lastSeenAt = turn.lastSeenAt;
    if (turn.lastSubstantiveProgressAt < summary.lastSubstantiveProgressAt) {
      summary.lastSubstantiveProgressAt = turn.lastSubstantiveProgressAt;
      summary.progressKind = turn.progressKind;
      summary.progressLabel = turn.progressLabel;
    }
  });

  cached =
    summaries.size === 0
      ? EMPTY
      : [...summaries.entries()]
          .map(([conversationId, summary]) => ({
            ...summary,
            conversationId,
            agentCount: summary.agentPubkeys.size,
            agentPubkeys: [...summary.agentPubkeys].sort(),
            triggeringEventIds: [...summary.triggeringEventIds].sort(),
            agentTriggerPairs: [...summary.agentTriggerPairs]
              .sort()
              .map((pair) => {
                const [agentPubkey, eventId, sessionId, turnId] =
                  pair.split("\0");
                return {
                  agentPubkey: agentPubkey ?? "",
                  eventId: eventId ?? "",
                  sessionId: sessionId ?? "",
                  turnId: turnId ?? "",
                };
              }),
          }))
          .sort((a, b) => a.conversationId.localeCompare(b.conversationId));
  cachedGeneration = generation;
  return cached;
}

export function resetActiveConversationTurnsCache(): void {
  cachedGeneration = -1;
  cached = EMPTY;
}
