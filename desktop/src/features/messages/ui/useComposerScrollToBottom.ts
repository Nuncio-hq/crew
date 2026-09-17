import * as React from "react";

/**
 * Scrolls the composer's own message-input container to its bottom on the
 * next animation frame. Self-contained: it only ever reads the ref, so it
 * lives here rather than inline in `MessageComposer.tsx`.
 */
export function useComposerScrollToBottom(
  composerScrollRef: React.RefObject<HTMLDivElement | null>,
) {
  return React.useCallback(() => {
    window.requestAnimationFrame(() => {
      const scrollElement = composerScrollRef.current;
      if (!scrollElement) return;
      scrollElement.scrollTop = scrollElement.scrollHeight;
    });
  }, [composerScrollRef]);
}
