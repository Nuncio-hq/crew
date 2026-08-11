import * as React from "react";

import type { PanelValueSetter } from "@/features/channels/ui/useChannelPanelHistoryState";
import type { TimelineMessage } from "@/features/messages/types";

/**
 * A `?thread=<id>` deep link is committed to panel state before the timeline
 * can contain the linked head: the head and its ancestors are fetched by the
 * channel route. While that fetch is in flight the head is legitimately absent
 * rather than deleted, so teardown is held — but only for that exact head, so
 * an unrelated missing head still closes, and only until the fetch settles, so
 * a bogus id closes instead of leaving the panel open forever.
 */
export function shouldHoldMissingThreadHead({
  isRouteTargetResolving,
  openThreadHeadId,
  routeThreadTargetId,
}: {
  isRouteTargetResolving: boolean;
  openThreadHeadId: string | null;
  routeThreadTargetId: string | null;
}): boolean {
  return (
    isRouteTargetResolving &&
    openThreadHeadId !== null &&
    openThreadHeadId === routeThreadTargetId
  );
}

/**
 * Keeps thread-panel and edit-composer targets consistent with the messages
 * that are actually loaded: closes the thread panel when its head message
 * disappears, seeds the reply target from the thread head, and clears stale
 * reply/edit targets that no longer resolve to a message.
 */
export function useThreadTargetSync({
  clearOptimisticThreadOverride,
  editTargetId,
  editTargetMessage,
  isRouteTargetResolving,
  isTimelineLoading,
  openThreadHeadId,
  openThreadHeadMessage,
  routeThreadTargetId,
  setEditTargetId,
  setExpandedThreadReplyIds,
  setOpenThreadHeadId,
  setThreadReplyTargetId,
  setThreadScrollTargetId,
  threadReplyTargetId,
  threadReplyTargetMessage,
}: {
  clearOptimisticThreadOverride: () => void;
  editTargetId: string | null;
  editTargetMessage: TimelineMessage | null;
  isRouteTargetResolving: boolean;
  isTimelineLoading: boolean;
  openThreadHeadId: string | null;
  openThreadHeadMessage: TimelineMessage | null;
  routeThreadTargetId: string | null;
  setEditTargetId: (id: string | null) => void;
  setExpandedThreadReplyIds: (ids: Set<string>) => void;
  setOpenThreadHeadId: PanelValueSetter;
  setThreadReplyTargetId: (id: string | null) => void;
  setThreadScrollTargetId: (id: string | null) => void;
  threadReplyTargetId: string | null;
  threadReplyTargetMessage: TimelineMessage | null;
}) {
  React.useEffect(() => {
    if (openThreadHeadId && !openThreadHeadMessage) {
      if (
        isTimelineLoading ||
        shouldHoldMissingThreadHead({
          isRouteTargetResolving,
          openThreadHeadId,
          routeThreadTargetId,
        })
      ) {
        return;
      }
      clearOptimisticThreadOverride();
      setOpenThreadHeadId(null, { replace: true });
      setExpandedThreadReplyIds(new Set());
      setThreadScrollTargetId(null);
      return;
    }

    if (openThreadHeadMessage && !threadReplyTargetId) {
      setThreadReplyTargetId(openThreadHeadMessage.id);
      return;
    }

    if (threadReplyTargetId && !threadReplyTargetMessage) {
      setThreadReplyTargetId(openThreadHeadMessage?.id ?? null);
    }
    if (editTargetId && !editTargetMessage) {
      setEditTargetId(null);
    }
  }, [
    clearOptimisticThreadOverride,
    editTargetId,
    editTargetMessage,
    isRouteTargetResolving,
    isTimelineLoading,
    openThreadHeadId,
    openThreadHeadMessage,
    routeThreadTargetId,
    setEditTargetId,
    setExpandedThreadReplyIds,
    setOpenThreadHeadId,
    setThreadReplyTargetId,
    setThreadScrollTargetId,
    threadReplyTargetId,
    threadReplyTargetMessage,
  ]);
}
