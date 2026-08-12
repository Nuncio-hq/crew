import * as React from "react";

/**
 * Keeps the logical tail stable while a reader is away from the bottom.
 *
 * Virtua remains the sole owner of mounting, measurement, and pixel anchoring.
 * This hook only controls when live output joins its keyed data model: older
 * history before the retained tail is admitted immediately, while newer output
 * is released atomically when the reader returns to the bottom.
 */
export function selectBufferedTimelineMessages<T extends { id: string }>({
  frozenMessageIds,
  isAtBottom,
  messages,
}: {
  frozenMessageIds: readonly string[] | null;
  isAtBottom: boolean;
  messages: T[];
}): T[] {
  if (isAtBottom || frozenMessageIds === null) return messages;
  if (frozenMessageIds.length === 0) return [];

  const currentById = new Map(messages.map((message) => [message.id, message]));
  if (frozenMessageIds.some((id) => !currentById.has(id))) {
    // A deletion or authoritative replacement removed part of the frozen
    // snapshot. Keeping stale message objects would be worse than accepting it.
    return messages;
  }

  // Cut on the NEWEST frozen id, not the oldest. Freezing at bottom captures
  // the full chronological list — including live-overlay rows that may sort
  // *older* than the authoritative page head (e.g. a pre-disconnect marker
  // retained across reconnect). Keying the cut on frozen[0] then treats that
  // overlay row as the start of the frozen block, so older-history pages that
  // insert *between* the overlay row and the previous head are neither "before
  // the frozen block" nor members of the frozen id set and are silently
  // dropped from the rendered model (#154).
  const newestFrozenId = frozenMessageIds[frozenMessageIds.length - 1];
  const newestFrozenIndex = messages.findIndex(
    (message) => message.id === newestFrozenId,
  );
  if (newestFrozenIndex === -1) return messages;

  const cut = newestFrozenIndex + 1;
  if (cut >= messages.length) {
    // No arrivals newer than the frozen newest. Keep the full list so any
    // older-history prepends above that floor remain visible.
    return messages;
  }
  return messages.slice(0, cut);
}

export function useBufferedTimelineMessages<T extends { id: string }>({
  channelId,
  isAtBottom,
  messages,
}: {
  channelId?: string | null;
  isAtBottom: boolean;
  messages: T[];
}): { messages: T[]; pendingCount: number } {
  const frozenMessageIdsRef = React.useRef<string[] | null>(null);
  const previousChannelIdRef = React.useRef(channelId);

  if (previousChannelIdRef.current !== channelId) {
    previousChannelIdRef.current = channelId;
    frozenMessageIdsRef.current = null;
  }

  if (isAtBottom) {
    frozenMessageIdsRef.current = messages.map((message) => message.id);
  } else if (frozenMessageIdsRef.current === null) {
    frozenMessageIdsRef.current = messages.map((message) => message.id);
  }

  const buffered = selectBufferedTimelineMessages({
    frozenMessageIds: frozenMessageIdsRef.current,
    isAtBottom,
    messages,
  });
  const previousBufferedRef = React.useRef<T[]>(buffered);
  const stableBuffered =
    previousBufferedRef.current.length === buffered.length &&
    previousBufferedRef.current.every(
      (message, index) => message === buffered[index],
    )
      ? previousBufferedRef.current
      : buffered;
  previousBufferedRef.current = stableBuffered;
  return {
    messages: stableBuffered,
    pendingCount: Math.max(0, messages.length - stableBuffered.length),
  };
}
