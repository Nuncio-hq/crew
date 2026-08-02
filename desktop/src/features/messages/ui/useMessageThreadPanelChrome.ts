import * as React from "react";

import { useKnownAgentPubkeys } from "@/features/agents/useKnownAgentPubkeys";
import {
  collectProjectThreadAgentMentions,
  projectThreadRootAudiencePubkeys,
  projectThreadStickyBarOwnsAgentSignal,
} from "@/features/messages/lib/projectThreadWorkspace";
import type { ThreadBreadcrumb } from "@/features/messages/lib/threadOrientation";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";

/**
 * Chrome helpers for MessageThreadPanel — kept out of the panel file so the
 * merge of orientation + sticky status bar stays under the file-size ratchet.
 */
export function useMessageThreadPanelChrome(args: {
  activityAccessoryVisible: boolean;
  breadcrumb: ThreadBreadcrumb | null | undefined;
  currentPubkey?: string;
  isFocusMode: boolean;
  onClose: () => void;
  onJumpToTimelineMessage?: ((messageId: string) => boolean) | undefined;
  profiles: UserProfileLookup;
  threadHead: TimelineMessage | null | undefined;
  threadMessages: TimelineMessage[];
  threadTypingCount: number;
}) {
  const {
    activityAccessoryVisible,
    breadcrumb,
    currentPubkey,
    isFocusMode,
    onClose,
    onJumpToTimelineMessage,
    profiles,
    threadHead,
    threadMessages,
    threadTypingCount,
  } = args;

  const knownAgentPubkeys = useKnownAgentPubkeys();
  const projectThreadAgentMentions = React.useMemo(() => {
    if (
      !threadHead ||
      !currentPubkey ||
      normalizePubkey(threadHead.signerPubkey ?? threadHead.pubkey ?? "") !==
        normalizePubkey(currentPubkey)
    ) {
      return [];
    }
    return collectProjectThreadAgentMentions({
      knownAgentPubkeys,
      profiles,
      replies: threadMessages,
      threadHead,
    });
  }, [currentPubkey, knownAgentPubkeys, profiles, threadHead, threadMessages]);

  const initialAgentPubkeys = React.useMemo(
    () => projectThreadRootAudiencePubkeys(projectThreadAgentMentions),
    [projectThreadAgentMentions],
  );

  const handleNavigateToAnchor = React.useCallback(() => {
    if (!breadcrumb || !onJumpToTimelineMessage) return;
    const jumped = onJumpToTimelineMessage(breadcrumb.anchorMessageId);
    // Focus mode: close after a successful jump so the flash is visible.
    if (jumped && isFocusMode) onClose();
  }, [breadcrumb, isFocusMode, onClose, onJumpToTimelineMessage]);

  // Project threads: the sticky status bar owns the agent signal, so the
  // composer drops its duplicate. Typing still shows; only bot activity is
  // suppressed. The rule lives in projectThreadWorkspace so it stays testable
  // and cannot drift from the bar's own visibility condition.
  // threadHead is only narrowed below; the helper accepts a nullish body.
  const stickyBarOwnsAgentSignal = projectThreadStickyBarOwnsAgentSignal(
    threadHead?.body,
    projectThreadAgentMentions.length,
  );
  const showComposerBotActivity =
    activityAccessoryVisible && !stickyBarOwnsAgentSignal;
  const hasComposerBottomActivity =
    showComposerBotActivity || threadTypingCount > 0;

  return {
    handleNavigateToAnchor,
    hasComposerBottomActivity,
    initialAgentPubkeys,
    projectThreadAgentMentions,
    showComposerBotActivity,
  };
}
