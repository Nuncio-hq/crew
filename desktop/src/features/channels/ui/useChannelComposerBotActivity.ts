import * as React from "react";

import {
  mergeWorkingAgentPubkeys,
  useChannelWorkingAgentPubkeys,
} from "@/features/agents/agentWorkingSignal";
import {
  filterPendingToKnownAgents,
  getPendingAgentPubkeysForChannel,
  subscribeMessageEditApplied,
} from "@/features/agents/dispatchedEventIds";
import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import { useStableArrayShallow } from "@/shared/hooks/useStableReference";

/** Channel-dock working agents: observer turns ∪ typing ∪ queued/held requests. */
export function useChannelComposerBotActivity(
  channelId: string | null | undefined,
): string[] {
  const knownAgentPubkeys = useKnownAgentPubkeys();
  const observerPubkeys = useChannelWorkingAgentPubkeys(channelId);
  const [pendingVersion, setPendingVersion] = React.useState(0);
  React.useEffect(() => {
    return subscribeMessageEditApplied(() => {
      setPendingVersion((current) => current + 1);
    });
  }, []);
  // Identity filter (community known agents), not observer liveness — humans
  // never surface as stoppable even when the observer registry is populated.
  const pendingPubkeys = React.useMemo(() => {
    void pendingVersion;
    return filterPendingToKnownAgents(
      getPendingAgentPubkeysForChannel(channelId),
      knownAgentPubkeys,
    );
  }, [channelId, knownAgentPubkeys, pendingVersion]);
  return useStableArrayShallow(
    React.useMemo(
      () => mergeWorkingAgentPubkeys(observerPubkeys, pendingPubkeys),
      [observerPubkeys, pendingPubkeys],
    ),
  );
}
