import * as React from "react";

import { buildThreadBreadcrumb } from "@/features/messages/lib/threadOrientation";
import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import { THREAD_PANEL_MESSAGE_GUTTER_CLASS } from "@/features/messages/lib/messageThreadPanelLayout";
import { ThreadAncestryStrip } from "./ThreadAncestryStrip";
import { ThreadBreadcrumb } from "./ThreadBreadcrumb";

export function useThreadPanelBreadcrumb({
  channelName,
  orientationLookupMessages,
  threadHead,
  threadReplies,
}: {
  channelName: string;
  orientationLookupMessages?: readonly TimelineMessage[];
  threadHead: TimelineMessage | null;
  threadReplies: readonly MainTimelineEntry[];
}) {
  return React.useMemo(() => {
    const messageById = new Map<string, TimelineMessage>();
    if (threadHead) messageById.set(threadHead.id, threadHead);
    for (const entry of threadReplies) {
      messageById.set(entry.message.id, entry.message);
    }
    for (const message of orientationLookupMessages ?? []) {
      messageById.set(message.id, message);
    }
    return buildThreadBreadcrumb({
      channelName,
      threadHead,
      messageById,
    });
  }, [channelName, orientationLookupMessages, threadHead, threadReplies]);
}

/** e2e-only: force the <h2> "Thread" fallback so #31 cannot regress. */
function forceThreadTitleFallback(): boolean {
  return Boolean(
    (
      window as Window & {
        __BUZZ_E2E__?: { forceThreadTitleFallback?: boolean };
      }
    ).__BUZZ_E2E__?.forceThreadTitleFallback,
  );
}

/** Header title: clickable breadcrumb, or the legacy "Thread" fallback. */
export function ThreadPanelOrientationTitle({
  breadcrumb,
  onNavigate,
}: {
  breadcrumb: ReturnType<typeof useThreadPanelBreadcrumb>;
  onNavigate?: () => void;
}) {
  const navigate = forceThreadTitleFallback() ? undefined : onNavigate;
  if (breadcrumb && navigate) {
    return <ThreadBreadcrumb breadcrumb={breadcrumb} onNavigate={navigate} />;
  }
  return <>Thread</>;
}

/** Collapsed ancestor rows above a nested thread head. */
export function ThreadPanelAncestry({
  breadcrumb,
  onOpenAncestorThread,
}: {
  breadcrumb: ReturnType<typeof useThreadPanelBreadcrumb>;
  onOpenAncestorThread?: (message: TimelineMessage) => void;
}) {
  const ancestorSegments = breadcrumb?.segments.slice(0, -1) ?? [];
  if (ancestorSegments.length === 0 || !onOpenAncestorThread) {
    return null;
  }
  return (
    <div className={cn(THREAD_PANEL_MESSAGE_GUTTER_CLASS, "pt-2")}>
      <ThreadAncestryStrip
        onOpenThread={onOpenAncestorThread}
        segments={ancestorSegments}
        truncated={breadcrumb?.truncated ?? false}
      />
    </div>
  );
}
