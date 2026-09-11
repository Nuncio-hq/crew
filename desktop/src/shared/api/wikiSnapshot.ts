import { invokeTauri } from "@/shared/api/tauri";
import {
  captureOwnerOperationScope,
  sameOwnerOperationScope,
  type OwnerOperationScope,
  type ScopedOwnerOperation,
} from "@/shared/api/ownerOperations";
import type { RelayEvent } from "@/shared/api/types";

export type WikiSnapshotState =
  | "complete"
  | "legacy"
  | "missing"
  | "incomplete";

/** Hard limits for renderer-owned repository fan-out and retained data. */
export const MAX_CONCURRENT_WIKI_SNAPSHOT_READS = 2;
export const MAX_AUTOMATIC_WIKI_REPO_READS = 128;
export const MAX_RETAINED_WIKI_GRAPH_BYTES = 64 * 1024 * 1024;

export type WikiRepositoryReadOutcome =
  | "complete"
  | "legacy"
  | "missing"
  | "incomplete"
  | "error"
  | "skipped"
  | "over-budget"
  | "preserved";

/** Per-coordinate status exposed alongside a partial Wiki projection. */
export type WikiRepositoryReadStatus = {
  coordinate: string;
  state: "ready" | "missing" | "stale" | "unavailable";
  outcome: WikiRepositoryReadOutcome;
  /** True when the visible graph is an older verified read in this scope. */
  stale: boolean;
  /** True when this refresh could not produce a fresh graph for the repo. */
  unavailable: boolean;
  message: string | null;
};

/** Query-cache value for one owner/community/generation/coordinate tuple. */
export type WikiRepositorySnapshotCache = {
  scope: OwnerOperationScope;
  coordinate: string;
  snapshot: WikiSnapshotRead | null;
  /** Current-scope state ref, even when the Wiki head is missing. */
  repoState: RelayEvent | null;
  /** Retained state is never treated as the latest cadence trigger. */
  repoStateFresh: boolean;
  status: WikiRepositoryReadStatus;
  serializedBytes: number;
};

export type WikiSnapshotScopeKey = readonly [string, string, number, number];

/** Native repository Wiki projection after one coherent scoped read. */
export type WikiSnapshotRead = {
  state: WikiSnapshotState;
  head: RelayEvent | null;
  manifest: RelayEvent | null;
  pages: RelayEvent[];
  error?: string;
  repoState: RelayEvent | null;
};

export class WikiSnapshotScopeChanged extends Error {
  constructor() {
    super("The active owner or community changed while reading the Wiki.");
    this.name = "WikiSnapshotScopeChanged";
  }
}

type RawWikiSnapshotRead = Omit<WikiSnapshotRead, "repoState"> & {
  repo_state?: RelayEvent | null;
};

/**
 * Read one repository Wiki from the native coherent projection.
 *
 * The coordinate is only a selector. Native code captures the signing owner,
 * canonical community origin, and generation fence before querying; the
 * returned token is checked again here before the renderer can apply it.
 */
export async function readWikiSnapshot(
  coordinate: string,
): Promise<WikiSnapshotRead> {
  const expected = await captureOwnerOperationScope();
  return readWikiSnapshotAtScope(coordinate, expected);
}

/**
 * Read several repositories under one captured owner/community scope. A
 * mid-flight scope change rejects the batch instead of allowing a mixed
 * owner/workspace projection into the query cache.
 */
export async function readWikiSnapshotAtScope(
  coordinate: string,
  expected: OwnerOperationScope,
  signal?: AbortSignal,
): Promise<WikiSnapshotRead> {
  throwIfAborted(signal);
  let result: ScopedOwnerOperation<RawWikiSnapshotRead>;
  try {
    result = await invokeTauri<ScopedOwnerOperation<RawWikiSnapshotRead>>(
      "wiki_snapshot_read",
      { expected, coordinate },
    );
  } catch (error) {
    throwIfAborted(signal);
    // A native command failure can race a workspace/identity change. Recheck
    // before exposing the ordinary failure to the retention layer.
    await assertWikiSnapshotScope(expected);
    throw error;
  }
  if (!sameOwnerOperationScope(result.token, expected)) {
    throw new WikiSnapshotScopeChanged();
  }
  await assertWikiSnapshotScope(expected, signal);
  return {
    ...result.value,
    repoState: result.value.repo_state ?? null,
  };
}

/** Recheck after an ambient relay/native await before applying the result. */
export async function assertWikiSnapshotScope(
  expected: OwnerOperationScope,
  signal?: AbortSignal,
): Promise<void> {
  throwIfAborted(signal);
  const current = await captureOwnerOperationScope();
  if (!sameOwnerOperationScope(current, expected)) {
    throw new WikiSnapshotScopeChanged();
  }
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) {
    const error = new Error("Wiki read was cancelled.");
    error.name = "AbortError";
    throw error;
  }
}

export function wikiSnapshotScopeKey(
  scope: OwnerOperationScope,
): WikiSnapshotScopeKey {
  return [
    scope.scope.owner,
    scope.scope.community,
    scope.workspace_generation,
    scope.identity_generation,
  ];
}

/** Canonical NIP-33 repository coordinate used by native Wiki reads. */
export function wikiRepositoryCoordinate(owner: string, repoD: string): string {
  return `30617:${owner.toLowerCase()}:${repoD}`;
}

/** Estimate the UTF-8 serialized graph retained for one repository. */
export function estimateWikiSnapshotBytes(
  snapshot: WikiSnapshotRead | null,
): number {
  if (!snapshot) return 0;
  try {
    return new TextEncoder().encode(JSON.stringify(snapshot)).byteLength;
  } catch {
    return Number.POSITIVE_INFINITY;
  }
}
