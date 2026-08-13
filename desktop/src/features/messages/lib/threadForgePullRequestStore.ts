import * as React from "react";

import {
  getThreadForgePrDetail,
  getThreadForgePrDiff,
} from "@/shared/api/threadForge";
import type {
  ForgeDetailResult,
  ForgeDiffResult,
  ForgePullRequestRef,
} from "@/shared/api/threadForgeTypes";

const CACHE_TTL_MS = 30_000;

export type ThreadForgeDetailSnapshot =
  | { status: "pending" }
  | {
      status: "ready";
      detail: ForgeDetailResult;
      diff: ForgeDiffResult | null;
    };

type CacheEntry = {
  expiresAt: number;
  promise?: Promise<void>;
  snapshot: ThreadForgeDetailSnapshot;
};

const PENDING: ThreadForgeDetailSnapshot = { status: "pending" };
const entries = new Map<string, CacheEntry>();
const listeners = new Set<() => void>();
let cacheEpoch = 0;
let reloadGeneration = 0;

function cacheKey(
  ref: ForgePullRequestRef,
  worktreePath: string | null | undefined,
): string {
  return `${ref.owner}/${ref.name}#${ref.number}\u0000${worktreePath ?? ""}`;
}

function notify(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

async function load(
  ref: ForgePullRequestRef,
  worktreePath: string | null | undefined,
  baseRef: string | null | undefined,
  force: boolean,
): Promise<void> {
  const key = cacheKey(ref, worktreePath);
  const stored = entries.get(key);
  if (!force && stored && stored.expiresAt > Date.now()) return;
  if (stored?.promise) return stored.promise;
  const epoch = cacheEpoch;
  const promise = Promise.all([
    getThreadForgePrDetail(ref),
    getThreadForgePrDiff({
      ...ref,
      worktreePath: worktreePath ?? undefined,
      baseRef: baseRef ?? undefined,
    }).catch(
      (): ForgeDiffResult => ({
        availability: "cli-failed",
        rateLimitedUntil: null,
        diff: null,
        message: "Could not load the diff.",
      }),
    ),
  ])
    .then(([detail, diff]) => {
      if (cacheEpoch !== epoch) return;
      entries.set(key, {
        expiresAt: Date.now() + CACHE_TTL_MS,
        snapshot: { status: "ready", detail, diff },
      });
      notify();
    })
    .catch(() => {
      if (cacheEpoch !== epoch) return;
      entries.set(key, {
        expiresAt: Date.now() + CACHE_TTL_MS,
        snapshot: {
          status: "ready",
          detail: {
            availability: "cli-failed",
            rateLimitedUntil: null,
            detail: null,
            message: "Could not load the pull request.",
          },
          diff: null,
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

export function useThreadForgePullRequest(
  ref: ForgePullRequestRef | null,
  worktreePath?: string | null,
  baseRef?: string | null,
) {
  const key = ref ? cacheKey(ref, worktreePath) : null;
  const getSnapshot = React.useCallback(
    () => (key ? (entries.get(key)?.snapshot ?? PENDING) : PENDING),
    [key],
  );
  const snapshot = React.useSyncExternalStore(subscribe, getSnapshot);
  const generation = React.useSyncExternalStore(
    subscribe,
    () => reloadGeneration,
  );
  React.useEffect(() => {
    if (ref) void load(ref, worktreePath, baseRef, generation > 0);
  }, [ref, worktreePath, baseRef, generation]);
  const refresh = React.useCallback(
    () => (ref ? load(ref, worktreePath, baseRef, true) : Promise.resolve()),
    [ref, worktreePath, baseRef],
  );
  return { refresh, snapshot };
}

export function invalidateThreadForgePullRequestStore(): void {
  cacheEpoch += 1;
  for (const entry of entries.values()) {
    entry.expiresAt = 0;
    entry.promise = undefined;
  }
  reloadGeneration += 1;
  notify();
}

export function resetThreadForgePullRequestStore(): void {
  cacheEpoch += 1;
  entries.clear();
  reloadGeneration = 0;
  notify();
}
