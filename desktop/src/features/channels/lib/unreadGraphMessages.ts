import type { TimelineMessage } from "@/features/messages/types";

/**
 * Union the formatted timeline with thread replies the timeline does not carry,
 * for unread accounting only.
 *
 * A non-broadcast thread reply is deliberately not a timeline row, so every
 * reply that lives only in the per-root thread cache is invisible to the reply
 * graph the unread counts read (parent → direct children, root → subtree). The
 * in-panel badges walk that graph by parent adjacency, so a single missing
 * intermediate reply severs the whole branch beneath it and its collapsed rows
 * count zero unread.
 *
 * Merging the cached replies in restores the graph without putting them on
 * screen: the caller keeps passing the plain timeline to everything that
 * renders rows or positions the channel divider.
 *
 * Deduplicated by event id, so a reply carried by both a window page and the
 * thread cache is counted exactly once. Returns the timeline array itself when
 * it already contains every reply, keeping the downstream memo chain stable.
 */
export function mergeUnreadGraphMessages(
  timelineMessages: TimelineMessage[],
  threadReplyMessages: readonly TimelineMessage[] | undefined,
): TimelineMessage[] {
  if (!threadReplyMessages || threadReplyMessages.length === 0) {
    return timelineMessages;
  }
  const timelineIds = new Set(timelineMessages.map((message) => message.id));
  const additions = threadReplyMessages.filter(
    (message) => !timelineIds.has(message.id),
  );
  if (additions.length === 0) return timelineMessages;
  return timelineMessages
    .concat(additions)
    .sort((left, right) => left.createdAt - right.createdAt);
}
