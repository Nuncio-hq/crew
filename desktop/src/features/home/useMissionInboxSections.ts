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
import { useSharedNowWhen } from "@/features/agents/lib/sharedNow";
import { useManagedAgentRuntimesQuery } from "@/features/agents/managedAgentRuntimeHooks";
import {
  findManagedAgentRuntime,
  isManagedAgentRuntimeSleeping,
} from "@/features/agents/managedAgentRuntimeStatus";
import { useCommunities } from "@/features/communities/useCommunities";

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
  // Attention states (stalled / lost contact) age out of `now` rather than
  // store events, and liveness frames no longer wake global subscribers —
  // re-derive on the shared 1s clock while any turn is live.
  const now = useSharedNowWhen(activeTurns.length > 0);
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

  return React.useMemo(
    () =>
      deriveMissionInboxSections({
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
        now,
      }),
    [
      activeTurns,
      channels,
      connectionState,
      connectionStateByAgent,
      effectiveDoneSet,
      inboxItems,
      needsYou,
      now,
      ownedAgentPubkeys,
      outcomes,
      receipts,
      sleepingAgentPubkeys,
      snoozedUntilByConversation,
    ],
  );
}
