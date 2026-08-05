import * as React from "react";

import type { Project } from "@/features/projects/hooks";
import { firstCloneUrl } from "@/features/projects/lib/projectCloneUrl";
import { selectProjectRepository } from "@/features/projects/projectModels";
import { openProjectMergeRecoveryTerminal } from "@/shared/api/projectGit";

type MergeRecoveryInput = {
  expectedCommit: string;
  sourceBranch: string;
  sourceCloneUrl: string;
  targetBranch: string;
};

export function useProjectMergeRecoveryTerminal(input: {
  project: Project | null | undefined;
  reposDir?: string | null;
  restricted: boolean;
}) {
  return React.useCallback(
    async (recovery: MergeRecoveryInput) => {
      const repository = selectProjectRepository(input.project, null);
      const targetCloneUrl = firstCloneUrl(repository);
      if (!repository || !targetCloneUrl || input.restricted) {
        throw new Error("No mutable managed checkout is available.");
      }
      return openProjectMergeRecoveryTerminal({
        ...recovery,
        projectDtag: repository.dtag,
        reposDir: input.reposDir,
        targetCloneUrl,
      });
    },
    [input.project, input.reposDir, input.restricted],
  );
}
