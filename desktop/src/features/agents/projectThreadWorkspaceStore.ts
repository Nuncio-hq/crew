import type { ObserverEvent } from "./ui/agentSessionTypes";
import { invalidateProjectWorktreeDetails } from "./projectWorktreeDetailsStore";
import { invalidateProjectWorktreeRegistry } from "./projectWorktreeRegistryStore";

export type ProjectThreadWorkspaceSnapshot =
  | { status: "pending" }
  | {
      status: "ready";
      agentPubkey: string;
      baseSource: "remote" | "local-fallback";
      baseRevision: string;
      branch: string;
      conversationId: string | null;
      rootEventId: string;
      repositoryPath: string | null;
      remoteDefaultBranch: string | null;
      commitsBehindRemote: number | null;
      worktreeName: string;
      worktreePath: string;
    }
  | {
      status: "derived";
      branch: string;
      rootEventId: string;
      repositoryPath: string;
      worktreeName: string;
      worktreePath: string;
    }
  | {
      status: "error";
      agentPubkey: string;
      conversationId: string | null;
      message: string;
      reason?: "missing-folder";
      rootEventId: string;
    };

const PENDING: ProjectThreadWorkspaceSnapshot = { status: "pending" };

/**
 * Bounds the root projection retained for the active community. Reads refresh
 * recency, so an overflow evicts the least-recently-used thread root.
 */
export const PROJECT_THREAD_WORKSPACE_ROOT_CAP = 256;

type WorkspaceWatermark = {
  agentPubkey: string;
  seq: number;
  timestampMs: number;
};

type WorkspaceEntry = {
  snapshot: ProjectThreadWorkspaceSnapshot;
  watermark: WorkspaceWatermark;
};

const workspaceByRoot = new Map<string, WorkspaceEntry>();
const savedByCommunity = new Map<string, Map<string, WorkspaceEntry>>();

function nonEmpty(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}

function payloadRecord(payload: unknown): Record<string, unknown> | null {
  return payload !== null && typeof payload === "object"
    ? (payload as Record<string, unknown>)
    : null;
}

function isAfterWatermark(
  candidate: WorkspaceWatermark,
  stored: WorkspaceWatermark,
): boolean {
  if (candidate.timestampMs !== stored.timestampMs) {
    return candidate.timestampMs > stored.timestampMs;
  }
  if (candidate.seq !== stored.seq) {
    return candidate.seq > stored.seq;
  }
  return candidate.agentPubkey.toLowerCase() > stored.agentPubkey.toLowerCase();
}

function setWorkspaceEntry(rootEventId: string, entry: WorkspaceEntry): void {
  const stored = workspaceByRoot.get(rootEventId);
  if (stored && !isAfterWatermark(entry.watermark, stored.watermark)) {
    return;
  }

  // Map insertion order is the LRU order. Reinsert updates an existing root's
  // recency before applying the bounded projection cap.
  workspaceByRoot.delete(rootEventId);
  workspaceByRoot.set(rootEventId, entry);
  if (workspaceByRoot.size > PROJECT_THREAD_WORKSPACE_ROOT_CAP) {
    const leastRecentlyUsed = workspaceByRoot.keys().next().value;
    if (leastRecentlyUsed !== undefined) {
      workspaceByRoot.delete(leastRecentlyUsed);
    }
  }
}

export function ingestProjectThreadWorkspaceEvent(
  agentPubkey: string,
  event: ObserverEvent,
): void {
  if (
    event.kind !== "thread_workspace_ready" &&
    event.kind !== "thread_workspace_error"
  ) {
    return;
  }
  const payload = payloadRecord(event.payload);
  const rootEventId = nonEmpty(payload?.rootEventId);
  const timestampMs = Date.parse(event.timestamp);
  if (
    !payload ||
    !rootEventId ||
    !/^[0-9a-f]{64}$/i.test(rootEventId) ||
    !Number.isFinite(timestampMs) ||
    !Number.isSafeInteger(event.seq)
  ) {
    return;
  }
  const watermark = { agentPubkey, seq: event.seq, timestampMs };

  if (event.kind === "thread_workspace_error") {
    const message = nonEmpty(payload.message);
    if (!message) return;
    const reason =
      payload.reason === "missing-folder" ? "missing-folder" : undefined;
    setWorkspaceEntry(rootEventId, {
      snapshot: {
        status: "error",
        agentPubkey,
        conversationId: event.conversationId ?? null,
        message,
        ...(reason ? { reason } : {}),
        rootEventId,
      },
      watermark,
    });
    const erroredRepo = nonEmpty(payload.repositoryPath);
    invalidateProjectWorktreeRegistry(erroredRepo);
    invalidateProjectWorktreeDetails(erroredRepo);
    return;
  }

  const branch = nonEmpty(payload.branch);
  const worktreePath = nonEmpty(payload.worktreePath);
  const worktreeName = nonEmpty(payload.worktreeName);
  const repositoryPath = nonEmpty(payload.repositoryPath);
  const baseRevision = nonEmpty(payload.baseRevision);
  const rawBaseSource = nonEmpty(payload.baseSource);
  const baseSource =
    rawBaseSource === null
      ? "local-fallback"
      : rawBaseSource === "remote" || rawBaseSource === "local-fallback"
        ? rawBaseSource
        : null;
  const remoteDefaultBranch = nonEmpty(payload.remoteDefaultBranch);
  const commitsBehindRemote =
    typeof payload.commitsBehindRemote === "number" &&
    Number.isSafeInteger(payload.commitsBehindRemote) &&
    payload.commitsBehindRemote >= 0
      ? payload.commitsBehindRemote
      : null;
  if (!branch || !worktreePath || !worktreeName || !baseRevision || !baseSource)
    return;
  setWorkspaceEntry(rootEventId, {
    snapshot: {
      status: "ready",
      agentPubkey,
      baseSource,
      baseRevision,
      branch,
      conversationId: event.conversationId ?? null,
      rootEventId,
      repositoryPath,
      remoteDefaultBranch,
      commitsBehindRemote,
      worktreeName,
      worktreePath,
    },
    watermark,
  });
  invalidateProjectWorktreeRegistry(repositoryPath);
  invalidateProjectWorktreeDetails(repositoryPath);
}

export function getProjectThreadWorkspaceSnapshot(
  rootEventId: string | null | undefined,
): ProjectThreadWorkspaceSnapshot {
  if (!rootEventId) return PENDING;
  const entry = workspaceByRoot.get(rootEventId);
  if (!entry) return PENDING;
  workspaceByRoot.delete(rootEventId);
  workspaceByRoot.set(rootEventId, entry);
  return entry.snapshot;
}

/** Snapshot list that does not bump LRU recency. */
export function listProjectThreadWorkspaceSnapshots(): Array<{
  rootEventId: string;
  snapshot: ProjectThreadWorkspaceSnapshot;
}> {
  return [...workspaceByRoot.entries()].map(([rootEventId, entry]) => ({
    rootEventId,
    snapshot: entry.snapshot,
  }));
}

/**
 * Clear only the active community projection. Saved community snapshots must
 * survive the observer-store reset between save and restore.
 */
export function resetProjectThreadWorkspaceStore(): void {
  workspaceByRoot.clear();
}

export function saveProjectThreadWorkspacesForCommunity(
  communityId: string,
): void {
  if (workspaceByRoot.size === 0) {
    savedByCommunity.delete(communityId);
    return;
  }
  savedByCommunity.set(communityId, new Map(workspaceByRoot));
}

export function restoreProjectThreadWorkspacesForCommunity(
  communityId: string,
): void {
  const saved = savedByCommunity.get(communityId);
  if (!saved) return;
  savedByCommunity.delete(communityId);
  workspaceByRoot.clear();
  for (const [rootEventId, entry] of saved) {
    workspaceByRoot.set(rootEventId, entry);
  }
}

export function clearSavedProjectThreadWorkspaceSnapshot(
  communityId: string,
): void {
  savedByCommunity.delete(communityId);
}
