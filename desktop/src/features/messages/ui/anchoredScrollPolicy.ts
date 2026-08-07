import type { TimelineMessageDelta } from "@/features/messages/lib/timelineSnapshot";

export function getPinnedCenterDrift({
  contentTop,
  currentContentTop,
}: {
  contentTop: number;
  currentContentTop: number;
}): number | null {
  const drift = currentContentTop - contentTop;
  return Math.abs(drift) > 0.5 ? drift : null;
}

export function shouldIgnorePinnedCenterScroll({
  currentScrollTop,
  expectedScrollTop,
  isWritingScroll,
}: {
  currentScrollTop: number;
  expectedScrollTop: number | null;
  isWritingScroll: boolean;
}): boolean {
  return isWritingScroll || expectedScrollTop === currentScrollTop;
}

// Programmatic bottom pins require the physical floor, not merely the looser
// UI at-bottom threshold used for unread affordances.
const TRUE_BOTTOM_THRESHOLD_PX = 1;

type BottomSettleContainer = Pick<
  HTMLDivElement,
  "scrollHeight" | "clientHeight" | "scrollTop" | "scrollTo"
>;

export function settleProgrammaticBottomPin(
  container: BottomSettleContainer,
): boolean {
  container.scrollTo({ top: container.scrollHeight, behavior: "auto" });
  return (
    container.scrollHeight - container.clientHeight - container.scrollTop <=
    TRUE_BOTTOM_THRESHOLD_PX
  );
}

export function shouldSettleForSplitPanel({
  isAtBottom,
  splitPanelOpen,
}: {
  isAtBottom: boolean;
  splitPanelOpen: boolean;
}): boolean {
  return isAtBottom && splitPanelOpen;
}

export function shouldSettleVirtualizedBottom({
  isAtBottom,
  messageDelta,
  messagesArrived,
  messagesChanged,
}: {
  isAtBottom: boolean;
  messageDelta: TimelineMessageDelta;
  messagesArrived: number;
  messagesChanged: boolean;
}): boolean {
  return (
    isAtBottom &&
    messageDelta !== "prepend" &&
    (messagesArrived > 0 || messagesChanged)
  );
}

/**
 * Grace window for attributing a virtualizer non-bottom report to a real user
 * scroll. Container scroll events only fire when scrollTop actually changes,
 * so a remeasure blip (scrollHeight growth under a pinned floor) arrives with
 * no scroll event while a genuine departure always has one just before it.
 */
export const VIRTUALIZED_BOTTOM_SCROLL_ATTRIBUTION_MS = 200;

/**
 * Virtua can report a transient non-bottom state while an at-bottom list is
 * remeasured after an in-place row update (no scrollTop change, so no scroll
 * event). The anchored-scroll owner has not released its bottom anchor in
 * that case, so later live arrivals must not be frozen behind the timeline
 * buffer. A report backed by a recent container scroll event is a REAL
 * departure and must be honored.
 */
export function shouldIgnoreTransientVirtualizedAwayFromBottom({
  anchorKind,
  atBottom,
  msSinceContainerScroll,
}: {
  anchorKind: "at-bottom" | "message" | "pinned-center";
  atBottom: boolean;
  msSinceContainerScroll: number;
}): boolean {
  return (
    !atBottom &&
    anchorKind === "at-bottom" &&
    msSinceContainerScroll > VIRTUALIZED_BOTTOM_SCROLL_ATTRIBUTION_MS
  );
}
