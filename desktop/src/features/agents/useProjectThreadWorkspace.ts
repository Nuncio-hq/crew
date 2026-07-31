import * as React from "react";

import { subscribeAgentObserverStore } from "./observerRelayStore";
import { getProjectThreadWorkspaceSnapshot } from "./projectThreadWorkspaceStore";

export function useProjectThreadWorkspace(
  rootEventId: string | null | undefined,
) {
  const getSnapshot = React.useCallback(
    () => getProjectThreadWorkspaceSnapshot(rootEventId),
    [rootEventId],
  );
  return React.useSyncExternalStore(subscribeAgentObserverStore, getSnapshot);
}
