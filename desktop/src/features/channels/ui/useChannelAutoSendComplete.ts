import * as React from "react";

/**
 * Clear the ?autoSend search param once the auto-submit fires so
 * back-navigation cannot re-trigger the send.
 * When `onAutoSendComplete` is provided it does a surgical single-key clear
 * that preserves `?thread` and all other panel search state (required for
 * the thread-draft send path so the thread panel does not unmount before the
 * deferred setTimeout(0) submit fires). The goChannel fallback is kept for
 * callers that do not supply the prop (e.g. isolated tests / older wrappers).
 */
export function useChannelAutoSendComplete({
  activeChannelId,
  goChannel,
  onAutoSendComplete,
}: {
  activeChannelId: string | null;
  goChannel: (channelId: string, options?: { replace?: boolean }) => unknown;
  onAutoSendComplete?: (() => void) | null;
}) {
  return React.useCallback(() => {
    if (onAutoSendComplete) {
      onAutoSendComplete();
    } else if (activeChannelId) {
      void goChannel(activeChannelId, { replace: true });
    }
  }, [activeChannelId, goChannel, onAutoSendComplete]);
}
