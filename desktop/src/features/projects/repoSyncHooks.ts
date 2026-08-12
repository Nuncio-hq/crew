import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  cloneProjectRepository,
  getProjectRepoSyncStatus,
  pullProjectLocalRepository,
  pushProjectLocalRepository,
} from "@/shared/api/projectGit";
import type {
  ProjectPullRequest,
  Repository as Project,
} from "@/features/projects/hooks";
import { firstCloneUrl } from "@/features/projects/lib/projectCloneUrl";
import { useProjectRepoHost } from "@/features/projects/useProjectRepoHost";
import { useFocusedRefetchInterval } from "@/shared/lib/useDocumentVisible";
import { publishProjectPullRequestUpdate } from "./pullRequestMutations";

function mutableManagedProject(project: Project | null | undefined): Project {
  if (project?.localWorkspacePath) {
    throw new Error("Linked workspaces are read-only.");
  }
  const cloneUrl = firstCloneUrl(project);
  if (!project || !cloneUrl) throw new Error("No project selected.");
  return project;
}

/** Local-vs-remote git sync status for a project checkout (ahead/behind
 * counts, push/pull availability). Polls gently — each check runs a
 * `git fetch` — and refetches on focus to catch the common "committed in
 * a terminal, switched back to the app" flow. */
export function useProjectRepoSyncStatusQuery(
  project: Project | null | undefined,
  reposDir?: string | null,
  branchName?: string | null,
  baseBranch?: string | null,
) {
  const selectedBranch = branchName ?? project?.defaultBranch ?? null;
  const refetchInterval = useFocusedRefetchInterval(60_000);
  const selectedBaseBranch = baseBranch ?? project?.defaultBranch ?? null;
  const host = useProjectRepoHost(project);
  const cloneUrl = firstCloneUrl(project);

  return useQuery({
    enabled: Boolean(
      host.kind === "buzz" && cloneUrl && !project?.localWorkspacePath,
    ),
    queryKey: [
      "project",
      project?.id ?? "none",
      "repo-sync-status",
      project?.localWorkspacePath ?? "managed",
      reposDir ?? "default",
      selectedBranch ?? "default",
      selectedBaseBranch ?? "default",
    ],
    queryFn: () => {
      if (!cloneUrl || !project) throw new Error("No project selected.");
      return getProjectRepoSyncStatus({
        reposDir,
        projectDtag: project.dtag,
        cloneUrl,
        branchName: selectedBranch,
        baseBranch: selectedBaseBranch,
      });
    },
    staleTime: 10_000,
    refetchInterval,
    refetchOnWindowFocus: true,
    retry: 1,
  });
}

/** Pushes local commits to the project remote. */
export function usePushProjectLocalRepositoryMutation(
  project: Project | null | undefined,
  reposDir?: string | null,
  branchName?: string | null,
  pullRequest?: ProjectPullRequest | null,
) {
  const queryClient = useQueryClient();
  const selectedBranch = branchName ?? project?.defaultBranch ?? null;

  return useMutation({
    mutationFn: async () => {
      const mutableProject = mutableManagedProject(project);
      const cloneUrl = firstCloneUrl(mutableProject);
      if (!cloneUrl) throw new Error("No project selected.");
      const result = await pushProjectLocalRepository({
        reposDir,
        projectDtag: mutableProject.dtag,
        cloneUrl,
        branchName: selectedBranch,
        baseBranch: mutableProject.defaultBranch,
      });
      let pullRequestUpdate:
        | { status: "skipped" | "unchanged" | "updated" }
        | { status: "failed"; error: string } = { status: "skipped" };
      if (
        pullRequest &&
        (pullRequest.status === "Open" || pullRequest.status === "Draft")
      ) {
        try {
          const updated = await publishProjectPullRequestUpdate({
            commit: result.commit,
            mergeBase: result.mergeBase,
            project: mutableProject,
            pullRequest,
          });
          pullRequestUpdate = {
            status: updated ? "updated" : "unchanged",
          };
        } catch (error) {
          pullRequestUpdate = {
            status: "failed",
            error:
              error instanceof Error
                ? error.message
                : "The pull request update could not be published.",
          };
        }
      }
      return { ...result, pullRequestUpdate };
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["project", project?.id ?? "none"],
      });
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });
}

/** Clones a project into the workspace repositories directory. */
export function useCloneProjectRepositoryMutation(
  project: Project | null | undefined,
  reposDir?: string | null,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => {
      const mutableProject = mutableManagedProject(project);
      const cloneUrl = firstCloneUrl(mutableProject);
      if (!cloneUrl) throw new Error("No project selected.");
      return cloneProjectRepository({
        reposDir,
        projectDtag: mutableProject.dtag,
        cloneUrl,
        defaultBranch: mutableProject.defaultBranch,
      });
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: ["project", project?.id ?? "none", "local-repo-snapshot"],
        }),
        queryClient.invalidateQueries({
          queryKey: ["project", project?.id ?? "none", "repo-sync-status"],
        }),
        queryClient.invalidateQueries({
          queryKey: ["projects", "local-repositories"],
        }),
      ]);
    },
  });
}

/** Fast-forwards the local checkout to the remote branch head. */
export function usePullProjectLocalRepositoryMutation(
  project: Project | null | undefined,
  reposDir?: string | null,
  branchName?: string | null,
) {
  const queryClient = useQueryClient();
  const selectedBranch = branchName ?? project?.defaultBranch ?? null;

  return useMutation({
    mutationFn: () => {
      const mutableProject = mutableManagedProject(project);
      const cloneUrl = firstCloneUrl(mutableProject);
      if (!cloneUrl) throw new Error("No project selected.");
      return pullProjectLocalRepository({
        reposDir,
        projectDtag: mutableProject.dtag,
        cloneUrl,
        branchName: selectedBranch,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["project", project?.id ?? "none"],
      });
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });
}
