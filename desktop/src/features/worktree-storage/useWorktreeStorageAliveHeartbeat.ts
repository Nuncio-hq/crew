import * as React from "react";

import { touchWorktreeStorageAlive } from "@/shared/api/agentControl";

const HEARTBEAT_MS = 60_000;

/**
 * App-scoped alive-interval heartbeat for observed-time idle (#174).
 * Gaps > 2× granularity close the previous interval at the last stamp.
 */
export function useWorktreeStorageAliveHeartbeat(enabled = true): void {
  React.useEffect(() => {
    if (!enabled) return;

    let cancelled = false;
    const tick = () => {
      if (cancelled) return;
      void touchWorktreeStorageAlive().catch(() => {
        // Heartbeat is best-effort; under-count only delays candidacy.
      });
    };

    tick();
    const id = globalThis.setInterval(tick, HEARTBEAT_MS);
    const onVisibility = () => {
      if (document.visibilityState === "visible") tick();
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cancelled = true;
      globalThis.clearInterval(id);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [enabled]);
}
