import * as React from "react";

import {
  mergeWorkingAgentPubkeys,
  useChannelWorkingAgentPubkeys,
} from "@/features/agents/agentWorkingSignal";
import {
  getPendingAgentPubkeysForChannel,
  subscribeMessageEditApplied,
} from "@/features/agents/dispatchedEventIds";
import { getRegisteredObserverAgentPubkeys } from "@/features/agents/observerRelayStore";
import { useStableArrayShallow } from "@/shared/hooks/useStableReference";

function agentOnlyPendingPubkeys(pending: readonly string[]): string[] {
  const known = getRegisteredObserverAgentPubkeys();
  if (known.size === 0) return [...pending];
  return pending.filter((pubkey) => known.has(pubkey));
}

/** Channel-dock working agents: observer turns ∪ typing ∪ queued/held requests. */
export function useChannelComposerBotActivity(
  channelId: string | null | undefined,
): string[] {
  const observerPubkeys = useChannelWorkingAgentPubkeys(channelId);
  const [pendingVersion, setPendingVersion] = React.useState(0);
  React.useEffect(() => {
    return subscribeMessageEditApplied(() => {
      setPendingVersion((current) => current + 1);
    });
  }, []);
  const pendingPubkeys = React.useMemo(() => {
    void pendingVersion;
    return agentOnlyPendingPubkeys(getPendingAgentPubkeysForChannel(channelId));
  }, [channelId, pendingVersion]);
  return useStableArrayShallow(
    React.useMemo(
      () => mergeWorkingAgentPubkeys(observerPubkeys, pendingPubkeys),
      [observerPubkeys, pendingPubkeys],
    ),
  );
}
