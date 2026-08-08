import * as React from "react";

import {
  getActiveAgentsForConversation,
  getActiveTurnControlTargetsForAgent,
  getActiveTurnsByChannel,
  getActiveTurnsForAgent,
  subscribeActiveAgentTurns,
  syncActiveAgentTurnsFromObserver,
  type ActiveChannelTurnSummary,
  type ActiveTurnControlTarget,
  type ActiveTurnSummary,
} from "@/features/agents/activeAgentTurnsStore";
import {
  getActiveTurnsByConversation,
  type ActiveConversationTurnSummary,
} from "@/features/agents/activeConversationTurns";
import { subscribeAgentObserverStore } from "@/features/agents/observerRelayStore";

/**
 * Hook: returns the channels where the given agent is currently working, each
 * with the desktop-clock `anchorAt` to anchor a live elapsed counter.
 * Re-renders when the set of channels changes — not when the clock ticks.
 */
export function useActiveAgentTurns(
  agentPubkey: string | null | undefined,
): ActiveTurnSummary[] {
  const getSnapshot = React.useCallback(
    () => getActiveTurnsForAgent(agentPubkey),
    [agentPubkey],
  );

  return React.useSyncExternalStore(subscribeActiveAgentTurns, getSnapshot);
}

export function useActiveAgentTurnControlTargets(
  agentPubkey: string | null | undefined,
): ActiveTurnControlTarget[] {
  const getSnapshot = React.useCallback(
    () => getActiveTurnControlTargetsForAgent(agentPubkey),
    [agentPubkey],
  );
  return React.useSyncExternalStore(subscribeActiveAgentTurns, getSnapshot);
}

/**
 * Hook: returns channels with active agent work across all tracked agents.
 * Re-renders when the channel set changes — not when the clock ticks.
 */
export function useActiveAgentTurnsByChannel(): ActiveChannelTurnSummary[] {
  return React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getActiveTurnsByChannel,
  );
}

export function useActiveTurnsByConversation(): ActiveConversationTurnSummary[] {
  return React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getActiveTurnsByConversation,
  );
}

export function useActiveAgentsForConversation(
  conversationId: string | null | undefined,
): string[] {
  const getSnapshot = React.useCallback(
    () => getActiveAgentsForConversation(conversationId),
    [conversationId],
  );
  return React.useSyncExternalStore(subscribeActiveAgentTurns, getSnapshot);
}

/**
 * Bridge hook: processes observer events into the active-turns store.
 * Should be called by a parent component that has access to the observer events.
 */
export function useActiveAgentTurnsBridge(
  agents: readonly { pubkey: string; status: string }[],
) {
  React.useEffect(() => {
    function syncAll() {
      syncActiveAgentTurnsFromObserver(agents);
    }

    syncAll();
    return subscribeAgentObserverStore(syncAll);
  }, [agents]);
}
