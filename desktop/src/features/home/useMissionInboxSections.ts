import * as React from "react";

import {
  deriveMissionInboxSections,
  useMissionInboxActiveTurns,
  useMissionInboxNeedsYou,
  useMissionInboxOutcomes,
  type MissionInboxSections,
} from "@/features/home/lib/missionInbox";
import type { InboxItem } from "@/features/home/lib/inbox";
import type { Channel } from "@/shared/api/types";
import {
  getAgentReceipts,
  subscribeAgentReceipts,
} from "@/features/agents/agentReceiptStore";
import {
  getAgentAttentionSnoozeGeneration,
  getAgentAttentionSnoozedUntil,
  subscribeAgentAttentionSnoozes,
} from "@/features/agents/agentAttentionSnoozeStore";
import {
  useAgentObserverConnectionState,
  useAgentObserverConnectionStates,
} from "@/features/agents/useAgentObserverConnectionState";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import {
  findManagedAgentRuntime,
  isManagedAgentRuntimeSleeping,
} from "@/features/agents/managedAgentRuntimeStatus";
import { useCommunities } from "@/features/communities/useCommunities";
import { useOrgRosterQuery } from "@/features/org/hooks/useOrgRosterQuery";
import { escalationHopLabel } from "@/features/org/lib/escalationHop";
import { useUsersBatchQuery } from "@/features/profile/hooks";

type UseMissionInboxSectionsInput = {
  channels?: readonly Pick<Channel, "id" | "name">[];
  effectiveDoneSet: ReadonlySet<string>;
  inboxItems: readonly InboxItem[];
  currentPubkey?: string;
  ownedAgentPubkeys: ReadonlySet<string>;
};

export function useMissionInboxSections({
  channels,
  effectiveDoneSet,
  inboxItems,
  currentPubkey,
  ownedAgentPubkeys,
}: UseMissionInboxSectionsInput): MissionInboxSections {
  const storedNeedsYou = useMissionInboxNeedsYou();
  // Cleanup effects cannot be the authority boundary: an identity switch
  // renders once before they run. Filter synchronously on every snapshot.
  const needsYou = React.useMemo(
    () =>
      storedNeedsYou.filter(
        (request) =>
          request.ownerPubkey === undefined ||
          (request.ownerPubkey === (currentPubkey ?? "").toLowerCase() &&
            ownedAgentPubkeys.has(request.agentPubkey.toLowerCase())),
      ),
    [currentPubkey, ownedAgentPubkeys, storedNeedsYou],
  );
  const activeTurns = useMissionInboxActiveTurns();
  const outcomes = useMissionInboxOutcomes();
  const { activeCommunity } = useCommunities();
  const receipts = React.useSyncExternalStore(
    subscribeAgentReceipts,
    getAgentReceipts,
    getAgentReceipts,
  );
  const snoozeGeneration = React.useSyncExternalStore(
    subscribeAgentAttentionSnoozes,
    getAgentAttentionSnoozeGeneration,
    getAgentAttentionSnoozeGeneration,
  );
  const activeAgentPubkeys = React.useMemo(
    () => [
      ...new Set([
        ...activeTurns.flatMap((turn) => turn.agentPubkeys),
        ...outcomes.map(([, outcome]) => outcome.agentPubkey),
      ]),
    ],
    [activeTurns, outcomes],
  );
  const connectionState = useAgentObserverConnectionState(activeAgentPubkeys);
  const connectionStateByAgent =
    useAgentObserverConnectionStates(activeAgentPubkeys);
  const managedAgentRuntimesQuery = useManagedAgentRuntimesQuery({
    enabled: Boolean(
      activeCommunity?.relayUrl && activeAgentPubkeys.length > 0,
    ),
  });
  const sleepingAgentPubkeys = React.useMemo(() => {
    if (!activeCommunity?.relayUrl) return new Set<string>();
    const sleeping = new Set<string>();
    for (const pubkey of activeAgentPubkeys) {
      const runtime = findManagedAgentRuntime(
        managedAgentRuntimesQuery.data ?? [],
        pubkey,
        activeCommunity.relayUrl,
      );
      if (isManagedAgentRuntimeSleeping(runtime)) {
        sleeping.add(pubkey.toLowerCase());
      }
    }
    return sleeping;
  }, [
    activeAgentPubkeys,
    activeCommunity?.relayUrl,
    managedAgentRuntimesQuery.data,
  ]);
  const snoozedUntilByConversation = React.useMemo(() => {
    // The store generation invalidates values while active-turn identities
    // remain stable.
    void snoozeGeneration;
    return new Map(
      activeTurns.map((turn) => [
        turn.conversationId,
        getAgentAttentionSnoozedUntil(turn.conversationId),
      ]),
    );
  }, [activeTurns, snoozeGeneration]);
  const roster = useOrgRosterQuery().data;
  const hopPubkeys = React.useMemo(
    () => [...ownedAgentPubkeys],
    [ownedAgentPubkeys],
  );
  const hopProfiles = useUsersBatchQuery(hopPubkeys);

  return React.useMemo(() => {
    const sections = deriveMissionInboxSections({
      acknowledgedConversationIds: new Set(
        inboxItems
          .filter((item) => effectiveDoneSet.has(item.id))
          .map((item) => item.conversationId),
      ),
      activeTurns,
      channels: channels ?? [],
      inboxItems,
      needsYou,
      ownedAgentPubkeys,
      outcomes,
      receipts,
      connectionState,
      connectionStateByAgent,
      sleepingAgentPubkeys,
      snoozedUntilByConversation,
    });
    const profiles = hopProfiles.data?.profiles ?? {};
    return {
      ...sections,
      needsYou: sections.needsYou.map((row) => ({
        ...row,
        escalationHop: escalationHopLabel(
          roster ?? null,
          row.agentPubkey,
          profiles,
        ),
      })),
    };
  }, [
    activeTurns,
    channels,
    connectionState,
    connectionStateByAgent,
    effectiveDoneSet,
    hopProfiles.data?.profiles,
    inboxItems,
    needsYou,
    ownedAgentPubkeys,
    outcomes,
    receipts,
    roster,
    sleepingAgentPubkeys,
    snoozedUntilByConversation,
  ]);
}
