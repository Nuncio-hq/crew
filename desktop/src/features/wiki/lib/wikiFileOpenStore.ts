export type PendingWikiFileOpen = {
  projectId: string;
  path: string;
  startLine: number;
  endLine: number;
};

let pending: PendingWikiFileOpen | null = null;

export function setPendingWikiFileOpen(next: PendingWikiFileOpen): void {
  pending = next;
}

export function peekPendingWikiFileOpen(
  projectId?: string,
): PendingWikiFileOpen | null {
  if (!pending) return null;
  if (projectId && pending.projectId !== projectId) return null;
  return pending;
}

export function consumePendingWikiFileOpen(
  projectId?: string,
): PendingWikiFileOpen | null {
  if (!pending) return null;
  if (projectId && pending.projectId !== projectId) return null;
  const value = pending;
  pending = null;
  return value;
}
