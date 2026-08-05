import { useQuery } from "@tanstack/react-query";
import * as React from "react";

import { getProjectRepoSnapshot } from "@/shared/api/projectGit";
import type { ProjectRepoSnapshot } from "@/shared/api/types";
import type { Project } from "./hooks";
import { firstCloneUrl } from "./lib/projectCloneUrl";
import { readProjectLocalRepoSnapshot } from "./lib/project-exact-local-workspace";
import {
  type ProjectRepoUnavailableReason,
  projectRepoUnavailableReason,
} from "./lib/projectRepoAvailability";
import { selectProjectRepository } from "./projectModels";

// Remote snapshots are backed by a blobless `git clone` per repository, so the
// overview scan is deliberately throttled and cached for a long time.
const OVERVIEW_SNAPSHOT_CONCURRENCY = 3;

function snapshotHasData(snapshot: ProjectRepoSnapshot | null | undefined) {
  return Boolean(
    snapshot && (snapshot.files.length > 0 || snapshot.latestCommit),
  );
}

/**
 * Local checkouts are instant (no network, no clone) and keep working when
 * the relay's git storage is empty or unreachable, so they are preferred.
 * Only repositories without usable local data fall back to a remote clone.
 */
async function fetchProjectSnapshot(
  project: Project,
  reposDir: string | null | undefined,
): Promise<ProjectRepoSnapshot | null> {
  const repository = selectProjectRepository(project, null);
  if (!repository) return null;
  try {
    const local = await readProjectLocalRepoSnapshot({
      cloneUrl: firstCloneUrl(repository) ?? null,
      localWorkspacePath: repository.localWorkspacePath,
      localWorkspaceStatus: repository.localWorkspaceStatus,
      reposDir,
      projectDtag: repository.dtag,
      defaultBranch: repository.defaultBranch,
      baseBranch: repository.defaultBranch,
    });
    if (snapshotHasData(local?.snapshot)) return local?.snapshot ?? null;
  } catch {
    // Best-effort: fall through to the remote snapshot.
  }

  if (repository.localWorkspacePath) return null;
  const cloneUrl = firstCloneUrl(repository);
  if (!cloneUrl) return null;
  return getProjectRepoSnapshot({
    cloneUrl,
    defaultBranch: repository.defaultBranch,
    baseBranch: repository.defaultBranch,
  });
}

async function fetchProjectsRepoSnapshots(
  projects: Project[],
  reposDir: string | null | undefined,
): Promise<{
  snapshots: Record<string, ProjectRepoSnapshot>;
  unavailable: Record<string, ProjectRepoUnavailableReason>;
}> {
  const snapshots: Record<string, ProjectRepoSnapshot> = {};
  const unavailable: Record<string, ProjectRepoUnavailableReason> = {};
  const queue = [...projects];

  const workers = Array.from(
    { length: Math.min(OVERVIEW_SNAPSHOT_CONCURRENCY, queue.length) },
    async () => {
      for (;;) {
        const project = queue.shift();
        if (!project) return;
        try {
          const snapshot = await fetchProjectSnapshot(project, reposDir);
          if (snapshot) {
            snapshots[project.id] = snapshot;
          } else {
            unavailable[project.id] = "missing";
          }
        } catch (error) {
          unavailable[project.id] = projectRepoUnavailableReason(error);
        }
      }
    },
  );

  await Promise.all(workers);
  return { snapshots, unavailable };
}

/**
 * Fetches repo snapshots for a set of projects (throttled, failure-tolerant)
 * for community-wide aggregates like the overview language breakdown.
 * Prefers local checkouts under `reposDir`; falls back to remote clones.
 * Callers should pre-filter and cap `projects` — up to one git clone per entry.
 */
export function useProjectsRepoSnapshotsQuery(
  projects: Project[],
  reposDir?: string | null,
) {
  const projectLocations = React.useMemo(
    () =>
      projects
        .map((project) => {
          const repository = selectProjectRepository(project, null);
          return `${project.id}:${repository?.localWorkspaceStatus ?? "unlinked"}:${repository?.localWorkspacePath ?? "managed"}:${firstCloneUrl(repository) ?? "no-clone"}:${repository?.defaultBranch ?? "none"}`;
        })
        .sort(),
    [projects],
  );

  return useQuery({
    enabled: projects.length > 0,
    queryKey: [
      "projects",
      "repo-snapshots",
      reposDir ?? "default",
      projectLocations,
    ],
    queryFn: () => fetchProjectsRepoSnapshots(projects, reposDir),
    staleTime: 15 * 60_000,
    retry: 0,
  });
}
