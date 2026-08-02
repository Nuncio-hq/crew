import * as React from "react";

import { subscribeAgentObserverStore } from "./observerRelayStore";
import { getProjectThreadWorkspaceSnapshot } from "./projectThreadWorkspaceStore";
import {
  getProjectWorktreeEntryByRoot,
  useProjectWorktreeRegistry,
} from "./projectWorktreeRegistryStore";

export function useProjectThreadWorkspace(
  rootEventId: string | null | undefined,
  repositoryPath?: string | null,
) {
  const getObserverSnapshot = React.useCallback(
    () => getProjectThreadWorkspaceSnapshot(rootEventId),
    [rootEventId],
  );
  const observer = React.useSyncExternalStore(
    subscribeAgentObserverStore,
    getObserverSnapshot,
  );
  const { snapshot: registry } = useProjectWorktreeRegistry(
    repositoryPath ?? null,
  );

  return React.useMemo(() => {
    if (observer.status === "ready" || observer.status === "error") {
      return observer;
    }
    if (!rootEventId || !repositoryPath || registry.status !== "ready") {
      return observer;
    }
    const entry = getProjectWorktreeEntryByRoot(repositoryPath, rootEventId);
    if (!entry?.branch || entry.kind !== "managed") {
      return observer;
    }
    return {
      status: "derived" as const,
      branch: entry.branch,
      rootEventId,
      repositoryPath: registry.value.repositoryPath,
      worktreeName: entry.worktreeName,
      worktreePath: entry.worktreePath,
    };
  }, [observer, registry, repositoryPath, rootEventId]);
}
