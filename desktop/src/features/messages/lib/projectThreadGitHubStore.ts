import * as React from "react";

import { getThreadGitHubStatus } from "@/shared/api/agentControl";
import type { ThreadGitHubStatus } from "@/shared/api/thread-workspace-types";

type Target = {
  branch: string;
  repositoryPath: string;
  rootEventId: string;
};

type GitHubStatusFetcher = (input: Target) => Promise<ThreadGitHubStatus>;

export type ProjectThreadGitHubSnapshot =
  | { status: "pending" }
  | { status: "ready"; value: ThreadGitHubStatus };

const PENDING: ProjectThreadGitHubSnapshot = { status: "pending" };
const CACHE_TTL_MS = 30_000;
const entries = new Map<
  string,
  {
    expiresAt: number;
    promise?: Promise<void>;
    snapshot: ProjectThreadGitHubSnapshot;
  }
>();
const listeners = new Set<() => void>();
let cacheEpoch = 0;
let statusFetcher: GitHubStatusFetcher = getThreadGitHubStatus;

/** Test-only seam — ESM named exports are not redefinable via mock.method. */
export function setProjectThreadGitHubFetcherForTests(
  fetcher: GitHubStatusFetcher | null,
): void {
  statusFetcher = fetcher ?? getThreadGitHubStatus;
}

function cacheKey(target: Target): string {
  return `${target.repositoryPath}\u0000${target.branch}`;
}

function notify(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

async function load(target: Target, force: boolean): Promise<void> {
  const key = cacheKey(target);
  const stored = entries.get(key);
  if (!force && stored && stored.expiresAt > Date.now()) return;
  if (stored?.promise) return stored.promise;
  const epoch = cacheEpoch;
  const promise = statusFetcher(target)
    .then((value) => {
      if (cacheEpoch !== epoch) return;
      entries.set(key, {
        expiresAt: Date.now() + CACHE_TTL_MS,
        snapshot: { status: "ready", value },
      });
      notify();
    })
    .catch(() => {
      if (cacheEpoch !== epoch) return;
      // Invoke/IPC threw — not a gh binary miss. Treat as a failed probe so
      // the UI can show a degraded affordance instead of silently vanishing.
      entries.set(key, {
        expiresAt: Date.now() + CACHE_TTL_MS,
        snapshot: {
          status: "ready",
          value: { availability: "cli-failed", pullRequest: null },
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

export function useProjectThreadGitHub(target: Target | null) {
  const key = target ? cacheKey(target) : null;
  const getSnapshot = React.useCallback(
    () => (key ? (entries.get(key)?.snapshot ?? PENDING) : PENDING),
    [key],
  );
  const snapshot = React.useSyncExternalStore(subscribe, getSnapshot);
  React.useEffect(() => {
    if (target) void load(target, false);
  }, [target]);
  const refresh = React.useCallback(
    () => (target ? load(target, true) : Promise.resolve()),
    [target],
  );
  return { refresh, snapshot };
}

export function resetProjectThreadGitHubStore(): void {
  cacheEpoch += 1;
  entries.clear();
  statusFetcher = getThreadGitHubStatus;
  notify();
}
