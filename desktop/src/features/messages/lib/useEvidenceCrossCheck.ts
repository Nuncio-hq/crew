import * as React from "react";

import { useProjectThreadWorkspace } from "@/features/agents/useProjectThreadWorkspace";
import {
  compareEvidenceToPullRequest,
  type EvidenceCrossCheckResult,
} from "@/features/messages/lib/evidenceCrossCheck";
import type { EvidenceKind } from "@/features/messages/lib/evidenceTag";
import { useProjectThreadGitHub } from "@/features/messages/lib/projectThreadGitHubStore";
import type { TimelineMessage } from "@/features/messages/types";

/**
 * Live evidence↔CI cross-check against the thread's linked PR.
 * Uses the same GitHub status cache as ProjectThreadGitHubRow — no new HTTP.
 */
export function useEvidenceCrossCheck(
  kind: EvidenceKind,
  message: TimelineMessage,
): EvidenceCrossCheckResult {
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
  const pullRequest =
    snapshot.status === "ready" ? snapshot.value.pullRequest : null;

  return React.useMemo(
    () => compareEvidenceToPullRequest(kind, message.body, pullRequest),
    [kind, message.body, pullRequest],
  );
}
