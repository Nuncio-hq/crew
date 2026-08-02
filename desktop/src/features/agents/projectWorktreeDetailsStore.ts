import * as React from "react";

import { getProjectWorktreeDetails } from "@/shared/api/agentControl";
import type { ProjectWorktreeDetails } from "@/shared/api/thread-workspace-types";

type Snapshot =
  | { status: "idle" }
  | { status: "pending" }
  | { status: "ready"; value: ProjectWorktreeDetails }
  | { status: "error"; message: string };

const IDLE: Snapshot = { status: "idle" };
const EMPTY_MAP = new Map<string, ProjectWorktreeDetails>();
const entries = new Map<string, Snapshot>();
const inflight = new Map<string, Promise<void>>();
const listeners = new Set<() => void>();
/** Stable ready-maps keyed by repository path. */
const readyMaps = new Map<string, Map<string, ProjectWorktreeDetails>>();
let cacheEpoch = 0;

function notify(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function key(repositoryPath: string, worktreePath: string): string {
  return `${repositoryPath}\0${worktreePath}`;
}

function rebuildReadyMap(
  repositoryPath: string,
): Map<string, ProjectWorktreeDetails> {
  const map = new Map<string, ProjectWorktreeDetails>();
  const prefix = `${repositoryPath}\0`;
  for (const [cacheKey, snapshot] of entries) {
    if (!cacheKey.startsWith(prefix) || snapshot.status !== "ready") continue;
    map.set(snapshot.value.worktreePath, snapshot.value);
  }
  readyMaps.set(repositoryPath, map);
  return map;
}

export async function loadProjectWorktreeDetails(
  repositoryPath: string,
  worktreePath: string,
  force = false,
): Promise<void> {
  const cacheKey = key(repositoryPath, worktreePath);
  const existing = entries.get(cacheKey);
  if (!force && existing?.status === "ready") return;
  const pending = inflight.get(cacheKey);
  if (pending) return pending;
  const epoch = cacheEpoch;
  entries.set(cacheKey, { status: "pending" });
  notify();
  const promise = getProjectWorktreeDetails(repositoryPath, worktreePath)
    .then((value) => {
      if (cacheEpoch !== epoch) return;
      entries.set(cacheKey, { status: "ready", value });
      rebuildReadyMap(repositoryPath);
      notify();
    })
    .catch((error: unknown) => {
      if (cacheEpoch !== epoch) return;
      entries.set(cacheKey, {
        status: "error",
        message: error instanceof Error ? error.message : "Details failed",
      });
      notify();
    })
    .finally(() => {
      inflight.delete(cacheKey);
    });
  inflight.set(cacheKey, promise);
  await promise;
}

export function useProjectWorktreeDetails(
  repositoryPath: string | null,
  worktreePath: string | null,
  enabled: boolean,
): Snapshot {
  const cacheKey =
    repositoryPath && worktreePath ? key(repositoryPath, worktreePath) : null;
  const getSnapshot = React.useCallback(
    () => (cacheKey ? (entries.get(cacheKey) ?? IDLE) : IDLE),
    [cacheKey],
  );
  const snapshot = React.useSyncExternalStore(subscribe, getSnapshot);
  React.useEffect(() => {
    if (enabled && repositoryPath && worktreePath) {
      void loadProjectWorktreeDetails(repositoryPath, worktreePath, false);
    }
  }, [enabled, repositoryPath, worktreePath]);
  return snapshot;
}

export function useProjectWorktreeDetailsMap(
  repositoryPath: string | null,
): Map<string, ProjectWorktreeDetails> {
  const getSnapshot = React.useCallback(() => {
    if (!repositoryPath) return EMPTY_MAP;
    return readyMaps.get(repositoryPath) ?? EMPTY_MAP;
  }, [repositoryPath]);
  return React.useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

export function prefetchManagedWorktreeDetails(
  repositoryPath: string,
  worktreePaths: readonly string[],
): void {
  for (const worktreePath of worktreePaths) {
    void loadProjectWorktreeDetails(repositoryPath, worktreePath, false);
  }
}

export function invalidateProjectWorktreeDetails(
  repositoryPath?: string | null,
): void {
  if (!repositoryPath) {
    entries.clear();
    inflight.clear();
    readyMaps.clear();
    notify();
    return;
  }
  const prefix = `${repositoryPath}\0`;
  for (const cacheKey of [...entries.keys()]) {
    if (cacheKey.startsWith(prefix)) entries.delete(cacheKey);
  }
  for (const cacheKey of [...inflight.keys()]) {
    if (cacheKey.startsWith(prefix)) inflight.delete(cacheKey);
  }
  readyMaps.delete(repositoryPath);
  notify();
}

export function resetProjectWorktreeDetailsStore(): void {
  cacheEpoch += 1;
  entries.clear();
  inflight.clear();
  readyMaps.clear();
  notify();
}
