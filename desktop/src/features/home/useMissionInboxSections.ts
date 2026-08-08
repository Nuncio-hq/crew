import * as React from "react";

import {
  ingestApprovalRequestFeedItem,
  ingestUserInputRequest,
} from "@/features/agents/needsYouStore";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import {
  deriveUserInputRootEventId,
  parseUserInputRequest,
} from "@/features/channels/lib/userInput";
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
  subscribeAgentReceipts,
} from "@/features/agents/agentReceiptStore";
import {
  getAgentAttentionSnoozeGeneration,
  getAgentAttentionSnoozedUntil,
  subscribeAgentAttentionSnoozes,
} from "@/features/agents/agentAttentionSnoozeStore";
import { useAgentObserverConnectionState } from "@/features/agents/useAgentObserverConnectionState";
import { KIND_AGENT_RECEIPT } from "@/shared/constants/kinds";

type UseMissionInboxSectionsInput = {
  channels?: readonly Pick<Channel, "id" | "name">[];
  effectiveDoneSet: ReadonlySet<string>;
  feed?: HomeFeedResponse;
  inboxItems: readonly InboxItem[];
};

export function useMissionInboxSections({
  channels,
  effectiveDoneSet,
  feed,
  inboxItems,
}: UseMissionInboxSectionsInput): MissionInboxSections {
  React.useEffect(() => {
    for (const item of feed?.feed.needsAction ?? []) {
      if (item.kind === 46010) {
        ingestApprovalRequestFeedItem(item);
        continue;
      }
      const event = relayEventFromFeedItem(item);
      const request = parseUserInputRequest(event);
      if (!request) continue;
      const rootEventId = deriveUserInputRootEventId(event);
      const channelId = request.channel_id || item.channelId;
      const conversationId = deriveAgentConversationIdOrNull(
        channelId,
        rootEventId,
      );
      if (!channelId || !conversationId) continue;
      ingestUserInputRequest({
        id: item.id,
        channelId,
        rootEventId,
        conversationId,
        agentPubkey: item.pubkey,
        createdAt: item.createdAt * 1_000,
      });
    }
  }, [feed?.feed.needsAction]);

  React.useEffect(() => {
    for (const item of feed?.feed.activity ?? []) {
      if (item.kind === KIND_AGENT_RECEIPT) {
        ingestAgentReceiptEvent(relayEventFromFeedItem(item));
      }
    }
  }, [feed?.feed.activity]);

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
    () => [...new Set(activeTurns.flatMap((turn) => turn.agentPubkeys))],
    [activeTurns],
  );
  const connectionState = useAgentObserverConnectionState(activeAgentPubkeys);
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
        outcomes,
        receipts,
        connectionState,
        snoozedUntilByConversation,
      }),
    [
      activeTurns,
      channels,
      connectionState,
      effectiveDoneSet,
      inboxItems,
      needsYou,
      outcomes,
      receipts,
      snoozedUntilByConversation,
    ],
  );
}
