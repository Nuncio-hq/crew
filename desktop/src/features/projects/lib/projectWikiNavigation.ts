import type { Project } from "../projectModels";

/** Preserve an unavailable declared primary; never substitute another member. */
export function projectWikiNavigation(
  project: Pick<Project, "primaryRepositoryAddress" | "repositoryAddresses">,
) {
  return {
    tab: "wiki",
    repositoryAddress:
      project.primaryRepositoryAddress ??
      (project.repositoryAddresses.length === 1
        ? project.repositoryAddresses[0]
        : undefined),
  };
}
