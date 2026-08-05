import type { Project, Repository } from "@/features/projects/hooks";
import { firstCloneUrl } from "@/features/projects/lib/projectCloneUrl";

function localRepoNameCandidate(value: string | null | undefined) {
  const trimmed = value?.trim().replace(/\.git$/i, "") ?? "";
  if (
    !trimmed ||
    trimmed === "." ||
    trimmed === ".." ||
    trimmed.includes("/") ||
    trimmed.includes("\\")
  ) {
    return null;
  }
  return trimmed;
}

function cloneUrlRepoName(cloneUrl: string | undefined) {
  if (!cloneUrl) return null;
  try {
    const parsed = new URL(cloneUrl);
    const segments = parsed.pathname.split("/").filter(Boolean);
    const lastSegment = segments[segments.length - 1];
    return localRepoNameCandidate(lastSegment);
  } catch {
    return null;
  }
}

function localRepoCandidates(repository: Repository) {
  return [
    localRepoNameCandidate(repository.dtag),
    cloneUrlRepoName(firstCloneUrl(repository)),
  ].filter((candidate, index, candidates): candidate is string =>
    Boolean(candidate && candidates.indexOf(candidate) === index),
  );
}

export function hasLocalCheckout(
  project: Project,
  localRepoNames: Set<string>,
) {
  return project.repositories.some((repository) =>
    hasLocalRepositoryCheckout(repository, localRepoNames),
  );
}

export function hasLocalRepositoryCheckout(
  repository: Repository,
  localRepoNames: Set<string>,
) {
  if (
    repository.localWorkspacePath ||
    repository.localWorkspaceStatus === "invalid"
  ) {
    return false;
  }
  return localRepoCandidates(repository).some((candidate) =>
    localRepoNames.has(candidate),
  );
}

export function isProjectLocal(
  repository: Repository,
  localRepoNames: Set<string>,
) {
  return (
    Boolean(repository.localWorkspacePath) ||
    hasLocalRepositoryCheckout(repository, localRepoNames)
  );
}
