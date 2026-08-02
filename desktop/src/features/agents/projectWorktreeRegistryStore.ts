import * as React from "react";

import { getProjectWorktreeRegistry } from "@/shared/api/agentControl";
import type {
  ProjectWorktreeEntry,
  ProjectWorktreeRegistry,
} from "@/shared/api/thread-workspace-types";

export type ProjectWorktreeRegistrySnapshot =
  | { status: "pending" }
  | { status: "ready"; value: ProjectWorktreeRegistry }
  | { status: "error"; message: string };

const PENDING: ProjectWorktreeRegistrySnapshot = { status: "pending" };
const CACHE_TTL_MS = 60_000;

type CacheEntry = {
  expiresAt: number;
  promise?: Promise<void>;
  snapshot: ProjectWorktreeRegistrySnapshot;
};

const entries = new Map<string, CacheEntry>();
const listeners = new Set<() => void>();
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

function cacheKey(repositoryPath: string): string {
  return repositoryPath;
}

async function load(repositoryPath: string, force: boolean): Promise<void> {
  const key = cacheKey(repositoryPath);
  const stored = entries.get(key);
  if (!force && stored && stored.expiresAt > Date.now()) return;
  if (stored?.promise) return stored.promise;
  const epoch = cacheEpoch;
  const promise = getProjectWorktreeRegistry(repositoryPath)
    .then((value) => {
      if (cacheEpoch !== epoch) return;
      entries.set(key, {
        expiresAt: Date.now() + CACHE_TTL_MS,
        snapshot: { status: "ready", value },
      });
      notify();
    })
    .catch((error: unknown) => {
      if (cacheEpoch !== epoch) return;
      entries.set(key, {
        expiresAt: Date.now() + CACHE_TTL_MS,
        snapshot: {
          status: "error",
          message: error instanceof Error ? error.message : "Registry failed",
        },
      });
      notify();
    });
  entries.set(key, {
    expiresAt: stored?.expiresAt ?? 0,
    promise,
    snapshot: stored?.snapshot ?? PENDING,
  });
  await promise;
}

export function useProjectWorktreeRegistry(repositoryPath: string | null) {
  const key = repositoryPath ? cacheKey(repositoryPath) : null;
  const getSnapshot = React.useCallback(
    () => (key ? (entries.get(key)?.snapshot ?? PENDING) : PENDING),
    [key],
  );
  const snapshot = React.useSyncExternalStore(subscribe, getSnapshot);
  React.useEffect(() => {
    if (repositoryPath) void load(repositoryPath, false);
  }, [repositoryPath]);
  const refresh = React.useCallback(
    () => (repositoryPath ? load(repositoryPath, true) : Promise.resolve()),
    [repositoryPath],
  );
  return { refresh, snapshot };
}

export function getProjectWorktreeEntryByRoot(
  repositoryPath: string | null | undefined,
  rootEventId: string | null | undefined,
): ProjectWorktreeEntry | null {
  if (!repositoryPath || !rootEventId) return null;
  const snapshot = entries.get(cacheKey(repositoryPath))?.snapshot;
  if (snapshot?.status !== "ready") return null;
  const needle = rootEventId.toLowerCase();
  return (
    snapshot.value.entries.find(
      (entry) => entry.rootEventId?.toLowerCase() === needle,
    ) ?? null
  );
}

/** Drop TTL so the next read refetches — used when a live worktree event lands. */
export function invalidateProjectWorktreeRegistry(
  repositoryPath?: string | null,
): void {
  if (repositoryPath) {
    entries.delete(cacheKey(repositoryPath));
  } else {
    entries.clear();
  }
  notify();
}

export function resetProjectWorktreeRegistryStore(): void {
  cacheEpoch += 1;
  entries.clear();
  notify();
}

/** Test helper — seed a ready registry without invoking Tauri. */
export function __setProjectWorktreeRegistryForTests(
  repositoryPath: string,
  value: ProjectWorktreeRegistry,
): void {
  entries.set(cacheKey(repositoryPath), {
    expiresAt: Date.now() + CACHE_TTL_MS,
    snapshot: { status: "ready", value },
  });
  notify();
}
