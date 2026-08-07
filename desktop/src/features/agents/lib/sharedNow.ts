import * as React from "react";

let sharedNow = Date.now();
const listeners = new Set<() => void>();
let interval: ReturnType<typeof setInterval> | null = null;

function subscribeSharedNow(listener: () => void) {
  listeners.add(listener);
  if (listeners.size === 1) {
    interval = setInterval(() => {
      sharedNow = Date.now();
      for (const notify of listeners) notify();
    }, 1_000);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && interval) {
      clearInterval(interval);
      interval = null;
    }
  };
}

function getSharedNowSnapshot() {
  return sharedNow;
}

export function useSharedNowWhen(enabled: boolean): number {
  const subscribe = React.useCallback(
    (listener: () => void) =>
      enabled ? subscribeSharedNow(listener) : () => {},
    [enabled],
  );
  return React.useSyncExternalStore(
    subscribe,
    getSharedNowSnapshot,
    getSharedNowSnapshot,
  );
}
