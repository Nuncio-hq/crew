import * as React from "react";

import { useProjectThreadWorkspace } from "@/features/agents/useProjectThreadWorkspace";
import { useProjectThreadGitHub } from "@/features/messages/lib/projectThreadGitHubStore";
import type { TimelineMessage } from "@/features/messages/types";
import type { ThreadPullRequestCheck } from "@/shared/api/thread-workspace-types";

/**
 * CI check rows for the thread PR linked to this evidence message.
 * Same GitHub cache as the cross-check badge — no extra HTTP.
 */
export function useEvidencePullRequestChecks(
  message: TimelineMessage,
): readonly ThreadPullRequestCheck[] {
  const rootEventId = message.rootId ?? message.id;
  const workspace = useProjectThreadWorkspace(rootEventId);
  const target = React.useMemo(() => {
    if (
      (workspace.status === "ready" || workspace.status === "derived") &&
      workspace.repositoryPath &&
      workspace.branch
    ) {
      return {
        branch: workspace.branch,
        repositoryPath: workspace.repositoryPath,
        rootEventId: workspace.rootEventId,
      };
    }
    return null;
  }, [workspace]);
  const { snapshot } = useProjectThreadGitHub(target);
  if (snapshot.status !== "ready") return [];
  return snapshot.value.pullRequest?.checks ?? [];
}
