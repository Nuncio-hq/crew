import * as React from "react";

import { ingestApprovalRequestFeedItem } from "@/features/agents/needsYouStore";
import { projectAuthorizedUserInputEvent } from "@/features/agents/userInputAttentionProjection";
import {
  deriveMissionInboxSections,
  useMissionInboxActiveTurns,
  useMissionInboxNeedsYou,
  useMissionInboxOutcomes,
  type MissionInboxSections,
} from "@/features/home/lib/missionInbox";
import {
  relayEventFromFeedItem,
  type InboxItem,
} from "@/features/home/lib/inbox";
import type { Channel, HomeFeedResponse } from "@/shared/api/types";
import {
  getAgentReceipts,
  ingestAgentReceiptEvent,
  ingestAgentReceiptReviewEvent,
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
import { KIND_AGENT_RECEIPT, KIND_REACTION } from "@/shared/constants/kinds";

type UseMissionInboxSectionsInput = {
  channels?: readonly Pick<Channel, "id" | "name">[];
  effectiveDoneSet: ReadonlySet<string>;
  feed?: HomeFeedResponse;
  inboxItems: readonly InboxItem[];
  currentPubkey?: string;
  ownedAgentPubkeys: ReadonlySet<string>;
};

export function useMissionInboxSections({
  channels,
  effectiveDoneSet,
  feed,
  inboxItems,
  currentPubkey,
  ownedAgentPubkeys,
}: UseMissionInboxSectionsInput): MissionInboxSections {
  React.useEffect(() => {
    for (const item of feed?.feed.needsAction ?? []) {
      if (item.kind === 46010) {
        ingestApprovalRequestFeedItem(item);
        continue;
      }
      const event = relayEventFromFeedItem(item);
      projectAuthorizedUserInputEvent(
        event,
        item.channelId ?? "",
        currentPubkey ?? "",
        ownedAgentPubkeys,
      );
    }
  }, [currentPubkey, feed?.feed.needsAction, ownedAgentPubkeys]);

  React.useEffect(() => {
    const activity = feed?.feed.activity ?? [];
    // Receipts must exist before review authority can be checked. A two-pass
    // replay is deterministic even when relay rows share a created_at second.
    for (const item of activity) {
      if (item.kind === KIND_AGENT_RECEIPT) {
        ingestAgentReceiptEvent(relayEventFromFeedItem(item));
      }
    }
    if (!currentPubkey) return;
    for (const item of activity) {
      if (item.kind === KIND_REACTION) {
        ingestAgentReceiptReviewEvent(
          relayEventFromFeedItem(item),
          currentPubkey,
          ownedAgentPubkeys,
        );
      }
    }
  }, [currentPubkey, feed?.feed.activity, ownedAgentPubkeys]);

  const needsYou = useMissionInboxNeedsYou();
  const activeTurns = useMissionInboxActiveTurns();
  const outcomes = useMissionInboxOutcomes();
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
        snoozedUntilByConversation,
      }),
    [
      activeTurns,
      channels,
      connectionState,
      connectionStateByAgent,
      effectiveDoneSet,
      inboxItems,
      needsYou,
      ownedAgentPubkeys,
      outcomes,
      receipts,
      snoozedUntilByConversation,
    ],
  );
}
