import * as React from "react";
import { useActiveAgentTurns } from "@/features/agents/activeAgentTurnsStore";
import { useActiveTurnSummariesForConversation } from "@/features/agents/activeConversationAgentTurnSummaries";
import {
  getLatestLiveSessionId,
  subscribeAgentObserverStore,
} from "@/features/agents/observerRelayStore";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  currentThreadTranscriptSession,
  isAgentTurnLive,
} from "./transcriptLiveScope";

/** Optional exact-thread scope over the existing live projections. */
export function useTranscriptLiveScope(
  agentPubkey: string,
  channelId: string | null,
  conversationId?: string | null,
) {
  const activeTurns = useActiveAgentTurns(agentPubkey);
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  const threadRuns = React.useMemo(
    () =>
      conversationId === undefined
        ? undefined
        : (summaries.find(
            (summary) => summary.agentPubkey === normalizePubkey(agentPubkey),
          )?.runs ?? []),
    [agentPubkey, conversationId, summaries],
  );
  const getLatestLive = React.useCallback(
    () =>
      threadRuns === undefined
        ? getLatestLiveSessionId(agentPubkey, channelId)
        : currentThreadTranscriptSession(threadRuns),
    [agentPubkey, channelId, threadRuns],
  );
  const latestLiveSessionId = React.useSyncExternalStore(
    subscribeAgentObserverStore,
    getLatestLive,
  );
  return {
    isTurnLive: isAgentTurnLive(activeTurns, channelId, threadRuns),
    latestLiveSessionId,
  };
}
