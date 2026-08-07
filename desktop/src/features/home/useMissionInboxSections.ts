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

  const needsYou = useMissionInboxNeedsYou();
  const activeTurns = useMissionInboxActiveTurns();
  const outcomes = useMissionInboxOutcomes();

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
      }),
    [activeTurns, channels, effectiveDoneSet, inboxItems, needsYou, outcomes],
  );
}
