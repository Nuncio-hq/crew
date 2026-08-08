import * as React from "react";

import {
  getAgentObserverSnapshot,
  subscribeAgentObserverStore,
} from "@/features/agents/observerRelayStore";
import type { ConnectionState } from "@/features/agents/ui/agentSessionTypes";

const CONNECTION_PRIORITY: Record<ConnectionState, number> = {
  error: 5,
  closed: 4,
  connecting: 3,
  idle: 2,
  open: 1,
};

export function getAgentObserverConnectionState(
  agentPubkeys: readonly string[],
): ConnectionState {
  let state: ConnectionState = "idle";
  let priority = 0;
  for (const pubkey of agentPubkeys) {
    const candidate = getAgentObserverSnapshot(pubkey, true).connectionState;
    const candidatePriority = CONNECTION_PRIORITY[candidate];
    if (candidatePriority > priority) {
      priority = candidatePriority;
      state = candidate;
    }
  }
  return state;
}

export function useAgentObserverConnectionState(
  agentPubkeys: readonly string[],
): ConnectionState {
  const key = agentPubkeys.join(",");
  const getSnapshot = React.useCallback(
    () => getAgentObserverConnectionState(key ? key.split(",") : []),
    [key],
  );
  return React.useSyncExternalStore(
    subscribeAgentObserverStore,
    getSnapshot,
    getSnapshot,
  );
}

/** Per-agent connection states for conversation-scoped attention projection. */
export function useAgentObserverConnectionStates(
  agentPubkeys: readonly string[],
): ReadonlyMap<string, ConnectionState> {
  const key = [...new Set(agentPubkeys)].sort().join(",");
  const getSnapshot = React.useCallback(
    () =>
      (key ? key.split(",") : [])
        .map(
          (pubkey) =>
            `${pubkey}:${getAgentObserverSnapshot(pubkey, true).connectionState}`,
        )
        .join("|"),
    [key],
  );
  const snapshot = React.useSyncExternalStore(
    subscribeAgentObserverStore,
    getSnapshot,
    getSnapshot,
  );
  return React.useMemo(() => {
    const states = new Map<string, ConnectionState>();
    for (const entry of snapshot.split("|")) {
      if (!entry) continue;
      const separator = entry.lastIndexOf(":");
      states.set(
        entry.slice(0, separator),
        entry.slice(separator + 1) as ConnectionState,
      );
    }
    return states;
  }, [snapshot]);
}
