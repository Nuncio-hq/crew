import { useQueryClient } from "@tanstack/react-query";
import * as React from "react";

import {
  createLocalWorkspaceSnapshotFocusRefresh,
  isLocalRepoSnapshotQueryKey,
} from "./lib/project-local-workspace-focus-refresh";

/**
 * On app focus (and visibility→visible), debounced-invalidate active
 * `local-repo-snapshot` queries so visible Project surfaces re-read through
 * the D-015 exact local workspace reader. Does not invent React state —
 * React Query remains the cache owner.
 *
 * Listeners match `useRelayResumeTriggers` (window `focus` +
 * `visibilitychange`). Debounce/min-interval policy lives in
 * `project-local-workspace-focus-refresh.ts`.
 */
export function useLocalWorkspaceSnapshotFocusRefresh(): void {
  const queryClient = useQueryClient();

  React.useEffect(() => {
    const controller = createLocalWorkspaceSnapshotFocusRefresh(() => {
      void queryClient.invalidateQueries({
        predicate: (query) => isLocalRepoSnapshotQueryKey(query.queryKey),
      });
    });

    const onFocus = () => {
      controller.onAppFocus();
    };
    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        controller.onAppFocus();
      }
    };

    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      controller.cancel();
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
  }, [queryClient]);
}
