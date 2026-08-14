import { KIND_REPO_ANNOUNCEMENT } from "@/shared/constants/kinds";

export type PendingWikiFileOpen = {
  projectId: string;
  path: string;
  startLine: number;
  endLine: number;
};

const REPO_ADDRESS_PREFIX = `${KIND_REPO_ANNOUNCEMENT}:`;

let pending: PendingWikiFileOpen | null = null;

/** Strip `30617:` so entity-link route ids match Repository.id (`owner:dtag`). */
export function wikiFileOpenRepoId(projectId: string): string {
  const value = projectId.trim();
  if (value.startsWith(REPO_ADDRESS_PREFIX)) {
    return value.slice(REPO_ADDRESS_PREFIX.length).toLowerCase();
  }
  return value.toLowerCase();
}

export function wikiFileOpenMatchesProject(
  pendingProjectId: string,
  candidateId: string,
): boolean {
  return (
    wikiFileOpenRepoId(pendingProjectId) === wikiFileOpenRepoId(candidateId)
  );
}

export function setPendingWikiFileOpen(next: PendingWikiFileOpen): void {
  pending = next;
}

export function peekPendingWikiFileOpen(
  projectId?: string,
): PendingWikiFileOpen | null {
  if (!pending) return null;
  if (projectId && !wikiFileOpenMatchesProject(pending.projectId, projectId)) {
    return null;
  }
  return pending;
}

export function consumePendingWikiFileOpen(
  projectId?: string,
): PendingWikiFileOpen | null {
  if (!pending) return null;
  if (projectId && !wikiFileOpenMatchesProject(pending.projectId, projectId)) {
    return null;
  }
  const value = pending;
  pending = null;
  return value;
}
