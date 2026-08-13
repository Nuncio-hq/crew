import type { Project } from "@/features/projects/projectModels";

export type GitProjectChannelWorkspace = {
  localPath: string;
  repoAddress: string;
  defaultBranch: string;
};

export function gitProjectWorkspaceForChannel(
  channelId: string | null | undefined,
  projects: readonly Project[] | undefined,
): GitProjectChannelWorkspace | null {
  if (!channelId || !projects?.length) return null;
  for (const project of projects) {
    const viaProject = project.projectChannelId === channelId;
    const viaRepo = project.repositories.filter(
      (repository) => repository.channelId === channelId,
    );
    const candidates = viaRepo.length
      ? viaRepo
      : viaProject
        ? project.repositories
        : [];
    for (const repository of candidates) {
      if (
        repository.localWorkspaceStatus === "linked" &&
        repository.localWorkspacePath &&
        repository.workspaceMode !== "folder"
      ) {
        return {
          localPath: repository.localWorkspacePath,
          repoAddress: repository.repoAddress,
          defaultBranch: repository.defaultBranch,
        };
      }
    }
  }
  return null;
}
