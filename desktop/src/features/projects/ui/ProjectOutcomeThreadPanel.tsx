import * as React from "react";
import { useQuery } from "@tanstack/react-query";

import { useChannelsQuery } from "@/features/channels/hooks";
import {
  useChannelSubscription,
  useSendMessageMutation,
  useToggleReactionMutation,
} from "@/features/messages/hooks";
import { formatTimelineMessages } from "@/features/messages/lib/formatTimelineMessages";
import { buildThreadPanelData } from "@/features/messages/lib/threadPanel";
import { useThreadReplies } from "@/features/messages/useThreadReplies";
import { MessageThreadPanel } from "@/features/messages/ui/MessageThreadPanel";
import { MessageThreadPanelSkeleton } from "@/features/messages/ui/MessageThreadPanelSkeleton";
import { VideoReviewNavigationProvider } from "@/shared/ui/VideoReviewNavigation";
import type { TimelineMessage } from "@/features/messages/types";
import { useProfileQuery } from "@/features/profile/hooks";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { useIdentityQuery } from "@/shared/api/hooks";
import type { Channel } from "@/shared/api/types";
import { relayClient } from "@/shared/api/relayClient";
import {
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_V2,
} from "@/shared/constants/kinds";
import { AuxiliaryPanel } from "@/shared/layout/AuxiliaryPanel";

export function ProjectOutcomeThreadPanel({
  channelId,
  conversationId,
  onClose,
  profiles,
}: {
  channelId: string;
  conversationId: string;
  onClose: () => void;
  profiles?: UserProfileLookup;
}) {
  const channelsQuery = useChannelsQuery();
  const channel = React.useMemo<Channel | null>(
    () =>
      (channelsQuery.data ?? []).find(
        (candidate) => candidate.id === channelId,
      ) ?? null,
    [channelId, channelsQuery.data],
  );
  const identityQuery = useIdentityQuery();
  const profileQuery = useProfileQuery();
  useChannelSubscription(channel);
  const rootQuery = useQuery({
    enabled: channel !== null,
    queryKey: ["project-outcome-thread-root", channelId, conversationId],
    queryFn: async () => {
      const events = await relayClient.fetchEvents({
        ids: [conversationId],
        kinds: [KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_V2],
        "#h": [channelId],
        limit: 1,
      });
      return events[0] ?? null;
    },
    staleTime: 0,
  });
  const threadRepliesQuery = useThreadReplies(channel, conversationId);
  const sendMessageMutation = useSendMessageMutation(
    channel,
    identityQuery.data,
  );
  const toggleReactionMutation = useToggleReactionMutation();
  const [replyTargetId, setReplyTargetId] = React.useState<string | null>(
    conversationId,
  );
  const [expandedReplyIds, setExpandedReplyIds] = React.useState<Set<string>>(
    () => new Set(),
  );

  const messages = React.useMemo(
    () =>
      formatTimelineMessages(
        [
          ...(rootQuery.data ? [rootQuery.data] : []),
          ...(threadRepliesQuery.data ?? []),
        ],
        channel,
        identityQuery.data?.pubkey,
        profileQuery.data?.avatarUrl ?? null,
        profiles,
      ),
    [
      channel,
      identityQuery.data?.pubkey,
      rootQuery.data,
      threadRepliesQuery.data,
      profileQuery.data?.avatarUrl,
      profiles,
    ],
  );
  const thread = React.useMemo(
    () =>
      buildThreadPanelData(
        messages,
        conversationId,
        replyTargetId,
        expandedReplyIds,
      ),
    [conversationId, expandedReplyIds, messages, replyTargetId],
  );

  const send = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
      targetChannelId?: string | null,
      threadContext?: {
        parentEventId: string | null;
        threadHeadId: string | null;
      } | null,
    ) => {
      const parentEventId =
        threadContext?.parentEventId ?? replyTargetId ?? conversationId;
      await sendMessageMutation.mutateAsync({
        channelId: targetChannelId ?? channelId,
        content,
        mediaTags,
        mentionPubkeys,
        parentEventId,
        threadHeadId: threadContext?.threadHeadId ?? conversationId,
      });
    },
    [channelId, conversationId, replyTargetId, sendMessageMutation.mutateAsync],
  );
  const toggleReaction = React.useCallback(
    async (message: TimelineMessage, emoji: string, remove: boolean) => {
      await toggleReactionMutation.mutateAsync({
        emoji,
        eventId: message.id,
        remove,
      });
    },
    [toggleReactionMutation.mutateAsync],
  );

  if (!channel || rootQuery.isPending || threadRepliesQuery.isPending) {
    return (
      <div className="shrink-0" data-testid="project-in-flight-panel">
        <MessageThreadPanelSkeleton
          isFocusMode={false}
          onClose={onClose}
          widthPx={360}
        />
      </div>
    );
  }

  if (rootQuery.isError || threadRepliesQuery.isError || !thread.threadHead) {
    return (
      <AuxiliaryPanel
        className="shrink-0"
        onClose={onClose}
        testId="project-in-flight-panel"
        widthPx={360}
      >
        <div className="flex h-full items-center justify-center p-6 text-center text-sm text-muted-foreground">
          Unable to load this thread. Close the panel and try again.
        </div>
      </AuxiliaryPanel>
    );
  }

  return (
    <VideoReviewNavigationProvider>
      <div className="shrink-0" data-testid="project-in-flight-panel">
        <MessageThreadPanel
          activityAccessoryVisible={false}
          channel={channel}
          channelId={channel.id}
          channelName={channel.name}
          currentPubkey={identityQuery.data?.pubkey}
          isFocusMode={false}
          isSending={sendMessageMutation.isPending}
          onCancelReply={() => setReplyTargetId(conversationId)}
          onClose={onClose}
          onExpandReplies={(message) => {
            setExpandedReplyIds((current) => {
              const next = new Set(current);
              if (next.has(message.id)) next.delete(message.id);
              else next.add(message.id);
              return next;
            });
          }}
          onScrollTargetResolved={() => undefined}
          onSelectReplyTarget={(message) => setReplyTargetId(message.id)}
          onSend={send}
          onToggleReaction={toggleReaction}
          profiles={profiles}
          replyTargetMessage={thread.replyTargetMessage}
          scrollTargetId={null}
          threadHead={thread.threadHead}
          threadReplies={thread.visibleReplies}
          threadRepliesPending={threadRepliesQuery.isFetching}
          threadTypingPubkeys={[]}
          widthPx={360}
        />
      </div>
    </VideoReviewNavigationProvider>
  );
}
