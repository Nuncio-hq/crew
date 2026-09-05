import { toast } from "sonner";

import { editMessage } from "@/shared/api/tauri";
import { Markdown } from "@/shared/ui/markdown";
import { VideoReviewTimecodeButton } from "@/shared/ui/VideoReviewTimecodeButton";
import { useOpenVideoReviewAt } from "@/shared/ui/VideoReviewNavigation";
import { parseVideoReviewTimecode } from "@/shared/ui/videoReviewTimecode";
import { cn } from "@/shared/lib/cn";
import { getConfigNudgeAuthorPubkey } from "@/features/messages/ui/configNudgeAuthPubkey";
import { hasLinkPreviewSuppression } from "@/features/messages/lib/formatTimelineMessages";
import type { TimelineMessage } from "@/features/messages/types";
import type { CustomEmoji } from "@/shared/lib/remarkCustomEmoji";
import type { ImetaLookup } from "@/shared/ui/markdown/types";
import type { VideoReviewContext } from "@/shared/ui/VideoPlayer";
import type { TimelineReaction } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { EvidenceCard } from "./EvidenceCard";
import { HandoverNoteCard } from "./HandoverNoteCard";
import { parseEvidenceKind } from "@/features/messages/lib/evidenceTag";
import { parseHandoverModel } from "@/features/messages/lib/handoverTag";

export function MessageRowDefaultBody({
  message,
  channelId,
  onEdit,
  videoReviewCommentRootId,
  videoReviewContext,
  channelNames,
  emojiOnly,
  leadingInlineContent,
  customEmoji,
  imetaByUrl,
  agentMentionPubkeysByName,
  agentMentionAvatarsByName,
  mentionNames,
  mentionPubkeysByName,
  searchQuery,
  snapshotSharedBy,
  isKnownAgentPubkey,
  canToggleReactions,
  currentPubkey,
  onReply,
  onToggleEvidenceReaction,
  profiles,
  reactionPending,
  reactions,
}: {
  message: TimelineMessage;
  channelId?: string | null;
  onEdit?: (message: TimelineMessage) => void;
  videoReviewCommentRootId?: string;
  videoReviewContext?: VideoReviewContext;
  channelNames?: string[];
  emojiOnly: boolean;
  leadingInlineContent?: import("react").ReactNode;
  customEmoji?: CustomEmoji[];
  imetaByUrl?: ImetaLookup;
  agentMentionPubkeysByName?: Record<string, string>;
  agentMentionAvatarsByName?: Record<string, string>;
  mentionNames?: string[];
  mentionPubkeysByName?: Record<string, string>;
  searchQuery?: string;
  snapshotSharedBy?: string;
  isKnownAgentPubkey: (pubkey: string) => boolean;
  canToggleReactions: boolean;
  currentPubkey?: string;
  onReply?: (message: TimelineMessage) => void;
  onToggleEvidenceReaction?: (emoji: string) => Promise<void>;
  profiles?: UserProfileLookup;
  reactionPending: boolean;
  reactions: readonly TimelineReaction[];
}) {
  const openVideoReviewAt = useOpenVideoReviewAt();
  const linkPreviewsSuppressed = hasLinkPreviewSuppression(message.tags);
  const removeLinkPreviewsForEveryone =
    channelId && onEdit && !message.pending && !linkPreviewsSuppressed
      ? async () => {
          const tags = message.tags ?? [];
          try {
            await editMessage(
              channelId,
              message.id,
              message.body,
              tags.filter((tag) => tag[0] === "imeta"),
              tags.filter((tag) => tag[0] === "emoji"),
              undefined,
              true,
            );
          } catch (error) {
            toast.error(
              `Failed to remove previews: ${error instanceof Error ? error.message : String(error)}`,
            );
            throw error;
          }
        }
      : undefined;

  const reviewRootEventId = videoReviewCommentRootId;
  const reviewTimecode = reviewRootEventId
    ? parseVideoReviewTimecode(message.body)
    : null;

  const markdown = (
    <Markdown
      channelNames={channelNames}
      className={cn(
        "max-w-full text-sm",
        emojiOnly &&
          "text-4xl leading-tight [&_p]:leading-tight [&_img[data-custom-emoji]]:h-[1.45em] [&_img[data-custom-emoji]]:align-middle [&_button:has(img[data-custom-emoji])]:align-middle",
      )}
      configNudgeAuthorPubkey={getConfigNudgeAuthorPubkey(
        message,
        isKnownAgentPubkey,
      )}
      content={reviewTimecode?.text ?? message.body}
      messageId={message.id}
      linkPreviewsSuppressed={linkPreviewsSuppressed}
      linkPreviewTags={message.tags}
      leadingInlineContent={leadingInlineContent}
      onRemoveLinkPreviewsForEveryone={removeLinkPreviewsForEveryone}
      customEmoji={customEmoji}
      imetaByUrl={imetaByUrl}
      agentMentionPubkeysByName={agentMentionPubkeysByName}
      agentMentionAvatarsByName={agentMentionAvatarsByName}
      mentionNames={mentionNames}
      mentionPubkeysByName={mentionPubkeysByName}
      searchQuery={searchQuery}
      snapshotSharedBy={snapshotSharedBy}
      videoReviewContext={videoReviewContext}
    />
  );

  // Dispatch order is a Crew/upstream seam: crew-evidence → handover →
  // compact Buzz permalinks (inside Markdown) → ordinary Markdown.
  const evidenceKind = parseEvidenceKind(message.tags);
  if (evidenceKind) {
    return (
      <EvidenceCard
        canToggleReactions={canToggleReactions}
        currentPubkey={currentPubkey}
        imetaByUrl={imetaByUrl}
        kind={evidenceKind}
        message={message}
        onReply={onReply}
        onToggleReaction={onToggleEvidenceReaction}
        profiles={profiles}
        reactionPending={reactionPending}
        reactions={reactions}
      />
    );
  }

  const handoverModel = parseHandoverModel(message.tags);
  if (handoverModel) {
    return <HandoverNoteCard message={message} modelId={handoverModel} />;
  }

  if (!reviewRootEventId || !reviewTimecode || !openVideoReviewAt) {
    return markdown;
  }

  return (
    <div className="flex min-w-0 items-start gap-1.5">
      <VideoReviewTimecodeButton
        surface="message"
        timecode={reviewTimecode.timecode}
        onClick={(event) => {
          event.stopPropagation();
          openVideoReviewAt(reviewRootEventId, reviewTimecode.seconds);
        }}
      />
      <div className="min-w-0 flex-1">{markdown}</div>
    </div>
  );
}
