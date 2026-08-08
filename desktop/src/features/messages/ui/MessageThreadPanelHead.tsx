import { canManageMessageForCurrentUser } from "@/features/messages/lib/canManageMessage";
import { THREAD_PANEL_MESSAGE_GUTTER_CLASS } from "@/features/messages/lib/messageThreadPanelLayout";
import type { TimelineMessage } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { HuddleTranscriptIntro } from "@/features/huddle/components/HuddleTranscriptIntro";
import { MessageRow } from "./MessageRow";
import type { VideoReviewPresentation } from "@/features/messages/lib/videoReviewContext";

export type MessageThreadPanelHeadProps = {
  channelId: string | null;
  currentPubkey?: string;
  huddleMemberPubkeys?: readonly string[];
  huddleMemberPubkeysPending?: boolean;
  isFollowingThread?: boolean;
  isHuddleTranscript?: boolean;
  isMessageUnreadById?: (messageId: string) => boolean;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onFollowThread?: () => void;
  onMarkRead?: (message: TimelineMessage) => void;
  onMarkUnread?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  onUnfollowThread?: () => void;
  profiles?: UserProfileLookup;
  shouldShowThreadBranchGuides?: boolean;
  threadHead: TimelineMessage;
  videoReviewPresentation?: VideoReviewPresentation;
};

export function MessageThreadPanelHead({
  channelId,
  currentPubkey,
  huddleMemberPubkeys,
  huddleMemberPubkeysPending,
  isFollowingThread,
  isHuddleTranscript,
  isMessageUnreadById,
  onDelete,
  onEdit,
  onFollowThread,
  onMarkRead,
  onMarkUnread,
  onToggleReaction,
  onUnfollowThread,
  profiles,
  shouldShowThreadBranchGuides,
  threadHead,
  videoReviewPresentation,
}: MessageThreadPanelHeadProps) {
  if (isHuddleTranscript) {
    return (
      <div className={cn(THREAD_PANEL_MESSAGE_GUTTER_CLASS, "pb-2 pt-4")}>
        <HuddleTranscriptIntro />
      </div>
    );
  }

  return (
    <div
      className={cn(THREAD_PANEL_MESSAGE_GUTTER_CLASS, "pb-1 pt-0")}
      data-testid="message-thread-head"
    >
      <div className="rounded-2xl">
        <MessageRow
          actionBarPlacement="inside"
          channelId={channelId}
          huddleMemberPubkeys={huddleMemberPubkeys}
          huddleMemberPubkeysPending={huddleMemberPubkeysPending}
          isFollowingThread={isFollowingThread}
          isUnread={isMessageUnreadById?.(threadHead.id)}
          layoutVariant="thread-reply"
          message={threadHead}
          onDelete={
            onDelete &&
            canManageMessageForCurrentUser(threadHead, currentPubkey, profiles)
              ? onDelete
              : undefined
          }
          onEdit={
            onEdit &&
            canManageMessageForCurrentUser(threadHead, currentPubkey, profiles)
              ? onEdit
              : undefined
          }
          onFollowThread={
            onFollowThread
              ? (_msg: TimelineMessage) => onFollowThread()
              : undefined
          }
          onMarkUnread={onMarkUnread}
          onMarkRead={onMarkRead}
          onToggleReaction={onToggleReaction}
          onUnfollowThread={
            onUnfollowThread
              ? (_msg: TimelineMessage) => onUnfollowThread()
              : undefined
          }
          profiles={profiles}
          showDepthGuides={shouldShowThreadBranchGuides}
          videoReviewCommentRootId={videoReviewPresentation?.commentRootIdsByMessageId.get(
            threadHead.id,
          )}
          videoReviewContext={videoReviewPresentation?.contextsByMessageId.get(
            threadHead.id,
          )}
        />
      </div>
    </div>
  );
}
