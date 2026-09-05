import * as React from "react";

export function useUpwardPaginationWheel(
  hostRef: React.RefObject<HTMLDivElement | null>,
  onWheel: () => void,
  onStartReached?: () => boolean,
) {
  const onStartReachedRef = React.useRef(onStartReached);
  onStartReachedRef.current = onStartReached;
  const suppressRef = React.useRef(false);
  const lastUpwardWheelAtRef = React.useRef(Number.NEGATIVE_INFINITY);
  const clear = React.useCallback(() => {
    suppressRef.current = false;
  }, []);

  const arm = React.useCallback(
    (startedPaging: boolean) => {
      const scroller = hostRef.current?.firstElementChild;
      if (
        startedPaging &&
        scroller instanceof HTMLDivElement &&
        scroller.scrollHeight - scroller.clientHeight > 400 &&
        performance.now() - lastUpwardWheelAtRef.current < 120
      ) {
        suppressRef.current = true;
      }
    },
    [hostRef],
  );

  React.useLayoutEffect(() => {
    const scroller = hostRef.current?.firstElementChild;
    if (!(scroller instanceof HTMLDivElement)) return;
    let releaseTimer: number | null = null;
    const handleWheel = (event: WheelEvent) => {
      // Ctrl+wheel belongs to browser zoom. It must not retire bottom intent or
      // arm upward-pagination momentum because it does not move the reader.
      if (event.ctrlKey) return;
      onWheel();
      if (event.deltaY >= 0) {
        clear();
        if (releaseTimer !== null) window.clearTimeout(releaseTimer);
        releaseTimer = null;
        return;
      }
      lastUpwardWheelAtRef.current = performance.now();
      // At the hard top, wheel input need not produce a scroll event. Retry
      // through the current fetch/settle guards so a blocked page is reachable
      // on the next upward gesture without requiring a down-and-up nudge.
      if (scroller.scrollTop <= 200) {
        arm(onStartReachedRef.current?.() ?? false);
      }
      if (!suppressRef.current) return;
      event.preventDefault();
      if (releaseTimer !== null) window.clearTimeout(releaseTimer);
      releaseTimer = window.setTimeout(() => {
        clear();
        releaseTimer = null;
      }, 80);
    };
    scroller.addEventListener("wheel", handleWheel, { passive: false });
    return () => {
      scroller.removeEventListener("wheel", handleWheel);
      if (releaseTimer !== null) window.clearTimeout(releaseTimer);
    };
  }, [arm, clear, hostRef, onWheel]);

  return { arm, clear };
}
