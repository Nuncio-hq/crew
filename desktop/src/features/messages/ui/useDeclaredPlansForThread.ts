import * as React from "react";

import { useActiveTurnSummariesForConversation } from "@/features/agents/activeConversationAgentTurnSummaries";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import {
  collectParticipatingAgentPubkeys,
  latestSessionIdFromEvents,
  projectDeclaredPlansForThread,
  resolveAgentPlanName,
  type AgentDeclaredPlan,
  type AgentPlanLiveness,
  type DeclaredPlanAgentInput,
} from "@/features/agents/declaredPlanProjection";
import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import {
  findManagedAgentRuntime,
  isManagedAgentRuntimeSleeping,
} from "@/features/agents/managedAgentRuntimeStatus";
import {
  getAgentObserverSnapshot,
  getArchivedChannelEvents,
  subscribeAgentObserverStore,
} from "@/features/agents/observerRelayStore";
import { mergeObserverEventWindows } from "@/features/agents/ui/agentSessionPanelLayout";
import type { ObserverEvent } from "@/features/agents/ui/agentSessionTypes";
import { useLoadArchivedObserverEvents } from "@/features/agents/ui/useObserverEvents";
import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import { useCommunities } from "@/features/communities/useCommunities";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";

const subscribeToObserverStore = (onStoreChange: () => void) =>
  subscribeAgentObserverStore(onStoreChange);

export function useDeclaredPlansForThread(args: {
  channelId: string | null;
  profiles?: UserProfileLookup;
  threadHead: TimelineMessage | null | undefined;
  threadMessages: readonly TimelineMessage[];
}): {
  conversationId: string | null;
  plans: AgentDeclaredPlan[];
} {
  const { channelId, profiles, threadHead, threadMessages } = args;
  const conversationId = React.useMemo(
    () => deriveAgentConversationIdOrNull(channelId, threadHead?.id),
    [channelId, threadHead?.id],
  );
  const knownAgentPubkeys = useKnownAgentPubkeys();
  const managedAgentsQuery = useManagedAgentsQuery();
  const { activeCommunity } = useCommunities();
  const runtimesQuery = useManagedAgentRuntimesQuery({
    enabled: Boolean(activeCommunity?.relayUrl),
  });
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  useLoadArchivedObserverEvents(Boolean(channelId), channelId);
  const observerGeneration = React.useSyncExternalStore(
    subscribeToObserverStore,
    getObserverGeneration,
    getObserverGeneration,
  );

  const messages = React.useMemo(() => {
    if (!threadHead) return [];
    return [threadHead, ...threadMessages];
  }, [threadHead, threadMessages]);

  const managedNames = React.useMemo(() => {
    const names = new Map<string, string>();
    for (const agent of managedAgentsQuery.data ?? []) {
      names.set(normalizePubkey(agent.pubkey), agent.name);
    }
    return names;
  }, [managedAgentsQuery.data]);

  const workingPubkeys = React.useMemo(() => {
    const set = new Set<string>();
    for (const summary of summaries) {
      set.add(normalizePubkey(summary.agentPubkey));
    }
    return set;
  }, [summaries]);

  const plans = React.useMemo(() => {
    void observerGeneration;
    if (!conversationId) return [];
    const observerPubkeys: string[] = [];
    const eventsByPubkey = new Map<string, ObserverEvent[]>();
    for (const pubkey of knownAgentPubkeys) {
      const snapshot = getAgentObserverSnapshot(pubkey);
      const archived = channelId
        ? getArchivedChannelEvents(pubkey, channelId)
        : [];
      const events = mergeObserverEventWindows(snapshot.events, archived);
      eventsByPubkey.set(pubkey, events);
      if (events.some((event) => event.conversationId === conversationId)) {
        observerPubkeys.push(pubkey);
      }
    }
    const participating = collectParticipatingAgentPubkeys({
      knownAgentPubkeys,
      messages,
      observerAgentPubkeys: observerPubkeys,
      activeTurnPubkeys: [...workingPubkeys],
    });
    const agents: DeclaredPlanAgentInput[] = participating.map((pubkey) => {
      const snapshot = getAgentObserverSnapshot(pubkey);
      const events = eventsByPubkey.get(pubkey) ?? snapshot.events;
      return {
        agentPubkey: pubkey,
        agentName: resolveAgentPlanName(
          pubkey,
          profiles,
          managedNames.get(pubkey),
        ),
        events,
        liveSessionId: latestSessionIdFromEvents(
          snapshot.events,
          conversationId,
        ),
        liveness: planLiveness({
          working: workingPubkeys.has(pubkey),
          sleeping: isManagedAgentRuntimeSleeping(
            findManagedAgentRuntime(
              runtimesQuery.data ?? [],
              pubkey,
              activeCommunity?.relayUrl ?? "",
            ),
          ),
          connectionState: snapshot.connectionState,
        }),
      };
    });
    return projectDeclaredPlansForThread(conversationId, agents);
  }, [
    activeCommunity?.relayUrl,
    channelId,
    conversationId,
    knownAgentPubkeys,
    managedNames,
    messages,
    observerGeneration,
    profiles,
    runtimesQuery.data,
    workingPubkeys,
  ]);

  return { conversationId, plans };
}

let observerGeneration = 0;
subscribeAgentObserverStore(() => {
  observerGeneration += 1;
});

function getObserverGeneration() {
  return observerGeneration;
}

function planLiveness(args: {
  working: boolean;
  sleeping: boolean;
  connectionState: string;
}): AgentPlanLiveness {
  if (args.working) return "working";
  if (args.sleeping) return "sleeping";
  if (args.connectionState === "open") return "idle";
  return "disconnected";
}
