import * as React from "react";

import {
  type InboxItem,
  type InboxReply,
  formatInboxFullTimestamp,
} from "@/features/home/lib/inbox";
import type { MissionInboxEventTarget } from "@/features/home/lib/missionInbox";
import { formatTime } from "@/features/messages/lib/dateFormatters";
import { splitOutgoingTags } from "@/features/messages/lib/imetaMediaMarkdown";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { sendChannelMessage } from "@/shared/api/tauri";

export function useHomeInboxSendReply({
  activeVerifiedMissionTarget,
  canReply,
  contextMessageIds,
  currentPubkey,
  feedProfiles,
  onRefresh,
  selectedConversationId,
  selectedItem,
}: {
  activeVerifiedMissionTarget: MissionInboxEventTarget | null;
  canReply: boolean;
  contextMessageIds: ReadonlySet<string>;
  currentPubkey?: string;
  feedProfiles: UserProfileLookup | undefined;
  onRefresh: () => void;
  selectedConversationId: string | null;
  selectedItem: InboxItem | null;
}) {
  const [isSendingReply, setIsSendingReply] = React.useState(false);
  const [localRepliesByItemId, setLocalRepliesByItemId] = React.useState<
    Record<string, InboxReply[]>
  >({});

  React.useEffect(() => {
    void selectedConversationId;
    setIsSendingReply(false);
  }, [selectedConversationId]);

  const selectedItemReplies = React.useMemo<InboxReply[]>(() => {
    if (!selectedItem) return [];
    const localReplies =
      localRepliesByItemId[selectedItem.conversationId] ?? [];
    return localReplies.filter((reply) => !contextMessageIds.has(reply.id));
  }, [contextMessageIds, localRepliesByItemId, selectedItem]);

  const handleSendReply = React.useCallback(
    async ({
      content,
      mediaTags = [],
      mentionPubkeys,
      parentEventId,
    }: {
      content: string;
      mediaTags?: string[][];
      mentionPubkeys: string[];
      parentEventId: string | null;
    }) => {
      const channelId =
        activeVerifiedMissionTarget?.channelId ?? selectedItem?.item.channelId;
      if (!selectedItem || !channelId || !canReply) {
        throw new Error("Replies are not available for this item.");
      }

      const itemToReply = selectedItem;
      setIsSendingReply(true);
      try {
        const {
          mediaTags: imetaTags,
          emojiTags,
          mentionTags,
        } = splitOutgoingTags(mediaTags);
        const result = await sendChannelMessage(
          channelId,
          content,
          parentEventId,
          imetaTags,
          mentionPubkeys,
          undefined,
          emojiTags,
          mentionTags,
        );
        const authorPubkey = currentPubkey ?? itemToReply.item.pubkey;
        const reply: InboxReply = {
          authorLabel: currentPubkey
            ? resolveUserLabel({
                currentPubkey,
                profiles: feedProfiles,
                pubkey: authorPubkey,
              })
            : "You",
          authorPubkey,
          avatarUrl:
            currentPubkey && feedProfiles
              ? (feedProfiles[currentPubkey.trim().toLowerCase()]?.avatarUrl ??
                null)
              : null,
          content,
          createdAt: result.createdAt,
          depth: result.depth,
          fullTimestampLabel: formatInboxFullTimestamp(result.createdAt),
          id: result.eventId,
          parentId: result.parentEventId,
          rootId: result.rootEventId,
          tags: [...imetaTags, ...emojiTags, ...mentionTags],
          timeLabel: formatTime(result.createdAt),
        };
        setLocalRepliesByItemId((current) => ({
          ...current,
          [itemToReply.conversationId]: [
            ...(current[itemToReply.conversationId] ?? []),
            reply,
          ],
        }));
        onRefresh();
      } finally {
        setIsSendingReply(false);
      }
    },
    [
      activeVerifiedMissionTarget,
      canReply,
      currentPubkey,
      feedProfiles,
      onRefresh,
      selectedItem,
    ],
  );

  return {
    handleSendReply,
    isSendingReply,
    selectedItemReplies,
  };
}
