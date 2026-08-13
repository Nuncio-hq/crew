import type { Project, Repository } from "@/features/projects/projectModels";

export function isCoworkRepository(
  repository: Pick<Repository, "workspaceMode"> | null | undefined,
): boolean {
  return repository?.workspaceMode === "folder";
}

export function isCoworkProject(
  project: Pick<Project, "repositories"> | null | undefined,
): boolean {
  return project?.repositories.some(isCoworkRepository) === true;
}
