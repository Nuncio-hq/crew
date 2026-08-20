import type { Project, Repository } from "@/features/projects/projectModels";

export type ChannelLocalWorkspace = {
  repoAddress: string;
  owner: string;
  dtag: string;
  localPath: string;
  workspaceMode: "git" | "folder";
};

function isAbsoluteLocalPath(path: string): boolean {
  return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path);
}

function repositoriesForChannel(
  project: Project,
  channelId: string,
): Repository[] {
  const viaRepo = project.repositories.filter(
    (repository) => repository.channelId === channelId,
  );
  if (viaRepo.length) return viaRepo;
  if (project.projectChannelId === channelId) return project.repositories;
  return [];
}

export function exclusiveChannelLocalWorkspace(
  channelId: string | null | undefined,
  projects: readonly Project[] | undefined,
): ChannelLocalWorkspace | null {
  if (!channelId || !projects?.length) return null;

  const matches: Repository[] = [];
  for (const project of projects) {
    matches.push(...repositoriesForChannel(project, channelId));
  }
  if (matches.length !== 1) return null;

  const repository = matches[0];
  if (repository.localWorkspaceStatus === "unlinked") return null;
  const localPath = repository.localWorkspacePath?.trim() ?? "";
  if (!localPath || !isAbsoluteLocalPath(localPath)) return null;

  return {
    repoAddress: repository.repoAddress,
    owner: repository.owner,
    dtag: repository.dtag,
    localPath,
    workspaceMode: repository.workspaceMode === "folder" ? "folder" : "git",
  };
}
