import * as React from "react";

import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import { useThreadPanelBreadcrumb } from "@/features/messages/ui/ThreadPanelOrientation";

/**
 * Owns the orientation breadcrumb for the open thread so the panel title and
 * timeline anchor share one source of truth (`anchorMessageId`).
 *
 * Do not derive the timeline anchor from `rootId` alone — nested replies can
 * carry `rootId === parentId` (depth ≥ 2), which is not a timeline row.
 */
export function useChannelThreadOrientation({
  channelName,
  messages,
  threadAllMessages,
  threadHeadMessage,
  threadMessages,
}: {
  channelName: string;
  messages: readonly TimelineMessage[];
  threadAllMessages: readonly TimelineMessage[];
  threadHeadMessage: TimelineMessage | null;
  threadMessages: readonly MainTimelineEntry[];
}) {
  const orientationLookupMessages = React.useMemo(
    () =>
      messages.length ? messages.concat(threadAllMessages) : threadAllMessages,
    [messages, threadAllMessages],
  );
  return useThreadPanelBreadcrumb({
    channelName,
    orientationLookupMessages,
    threadHead: threadHeadMessage,
    threadReplies: threadMessages,
  });
}
