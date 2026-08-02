import * as React from "react";

import { useStableSet } from "@/shared/hooks/useStableReference";

import {
  deriveEditAsUndoUiState,
  getMessageEditAppliedResult,
  subscribeMessageEditApplied,
  type EditAsUndoUiState,
  type MessageEditAppliedResult,
} from "./dispatchedEventIds";
import {
  collectTriggeringEventIds,
  subscribeAgentObserverStore,
} from "./observerRelayStore";

/**
 * Stable Set of event ids that have appeared in any `turn_started`.
 * Rebuilds when the observer store notifies; reference-stable when membership
 * is unchanged so React.memo timeline rows do not churn.
 */
export function useDispatchedEventIds(): ReadonlySet<string> {
  const [version, setVersion] = React.useState(0);
  React.useEffect(() => {
    return subscribeAgentObserverStore(() => {
      setVersion((current) => current + 1);
    });
  }, []);
  const next = React.useMemo(() => {
    void version;
    return collectTriggeringEventIds();
  }, [version]);
  return useStableSet(next);
}

export function useMessageEditAppliedResult(
  eventId: string | null | undefined,
): MessageEditAppliedResult | null {
  const [version, setVersion] = React.useState(0);
  React.useEffect(() => {
    return subscribeMessageEditApplied(() => {
      setVersion((current) => current + 1);
    });
  }, []);
  void version;
  if (!eventId) {
    return null;
  }
  return getMessageEditAppliedResult(eventId);
}

export function useEditAsUndoUiState(args: {
  mentionsAgent: boolean;
  eventId: string | null | undefined;
}): EditAsUndoUiState | null {
  const dispatchedIds = useDispatchedEventIds();
  const editResult = useMessageEditAppliedResult(args.eventId);
  return React.useMemo(() => {
    if (!args.eventId || !args.mentionsAgent) {
      return null;
    }
    return deriveEditAsUndoUiState({
      mentionsAgent: args.mentionsAgent,
      eventId: args.eventId,
      dispatchedIds,
      editResult,
    });
  }, [args.eventId, args.mentionsAgent, dispatchedIds, editResult]);
}
