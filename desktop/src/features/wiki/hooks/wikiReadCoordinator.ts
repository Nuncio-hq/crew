import {
  assertWikiSnapshotScope,
  MAX_AUTOMATIC_WIKI_REPO_READS,
  MAX_CONCURRENT_WIKI_SNAPSHOT_READS,
  MAX_RETAINED_WIKI_GRAPH_BYTES,
  readWikiSnapshotAtScope,
  type WikiRepositorySnapshotCache,
  type WikiSnapshotRead,
} from "@/shared/api/wikiSnapshot";
import {
  sameOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";
import {
  planWikiRepositoryReads,
  projectWikiRepositoryReads,
  type WikiRepositoryReadPlan,
  type WikiRepositoryReadResult,
} from "./wikiReadProjection";

type ConsumerRequest = {
  coordinates: string[];
  priorities: string[];
};

type ScopeCoordinatorState = {
  scope: OwnerOperationScope;
  consumers: Map<string, ConsumerRequest>;
  retryPriorities: Set<string>;
  cache: Map<string, WikiRepositorySnapshotCache>;
  revision: number;
  activeControllers: Set<AbortController>;
  retirementScheduled: boolean;
  retired: boolean;
};

type ReadAttempt =
  | { coordinate: string; snapshot: WikiSnapshotRead }
  | {
      coordinate: string;
      error: string;
      outcome?: "error" | "incomplete" | "over-budget" | "preserved";
    };

const coordinators = new Map<string, ScopeCoordinatorState>();

/** Shared admission control across every Wiki consumer and scope. */
class WikiReadAdmission {
  private active = 0;
  private queue: Array<{
    signal?: AbortSignal;
    resolve: (release: () => void) => void;
    reject: (error: unknown) => void;
  }> = [];

  acquire(signal?: AbortSignal): Promise<() => void> {
    if (signal?.aborted) return Promise.reject(abortError());
    return new Promise((resolve, reject) => {
      this.queue.push({ signal, resolve, reject });
      this.pump();
    });
  }

  private pump(): void {
    while (
      this.active < MAX_CONCURRENT_WIKI_SNAPSHOT_READS &&
      this.queue.length > 0
    ) {
      const next = this.queue.shift();
      if (!next) return;
      if (next.signal?.aborted) {
        next.reject(abortError());
        continue;
      }
      this.active += 1;
      let released = false;
      next.resolve(() => {
        if (released) return;
        released = true;
        this.active -= 1;
        this.pump();
      });
    }
  }
}

const admission = new WikiReadAdmission();

function abortError(): Error {
  const error = new Error("Wiki read was cancelled.");
  error.name = "AbortError";
  return error;
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) throw abortError();
}

function scopeKey(scope: OwnerOperationScope): string {
  return [
    scope.scope.owner,
    scope.scope.community,
    scope.workspace_generation,
    scope.identity_generation,
  ].join("\u0000");
}

function coordinatorFor(scope: OwnerOperationScope): ScopeCoordinatorState {
  const key = scopeKey(scope);
  const existing = coordinators.get(key);
  if (
    existing &&
    !existing.retired &&
    sameOwnerOperationScope(existing.scope, scope)
  ) {
    return existing;
  }
  const created: ScopeCoordinatorState = {
    scope,
    consumers: new Map(),
    retryPriorities: new Set(),
    cache: new Map(),
    revision: 0,
    activeControllers: new Set(),
    retirementScheduled: false,
    retired: false,
  };
  coordinators.set(key, created);
  return created;
}

function normalized(values: readonly string[]): string[] {
  return [...new Set(values)].filter(Boolean);
}

function mergedRequest(
  state: ScopeCoordinatorState,
  ownConsumerId: string,
  ownCoordinates: readonly string[],
  ownPriorities: readonly string[],
) {
  const coordinates: string[] = [];
  const priorities: string[] = [];
  const append = (target: string[], values: readonly string[]) => {
    for (const value of values) {
      if (!target.includes(value)) target.push(value);
    }
  };
  const registered = state.consumers.get(ownConsumerId);
  append(coordinates, registered?.coordinates ?? ownCoordinates);
  append(priorities, registered?.priorities ?? ownPriorities);
  for (const [consumerId, request] of state.consumers) {
    if (consumerId === ownConsumerId) continue;
    append(coordinates, request.coordinates);
    append(priorities, request.priorities);
  }
  append(priorities, [...state.retryPriorities]);
  return {
    coordinates: normalized(coordinates),
    priorities: normalized(priorities),
  };
}

/** Register one mounted consumer in the canonical per-scope repository set. */
export function registerWikiReadConsumer(
  scope: OwnerOperationScope,
  consumerId: string,
  coordinates: readonly string[],
  priorities: readonly string[],
): void {
  const state = coordinatorFor(scope);
  state.retirementScheduled = false;
  const next = {
    coordinates: normalized(coordinates),
    priorities: normalized(priorities),
  };
  const previous = state.consumers.get(consumerId);
  if (
    previous &&
    previous.coordinates.join("\u0000") === next.coordinates.join("\u0000") &&
    previous.priorities.join("\u0000") === next.priorities.join("\u0000")
  ) {
    return;
  }
  state.consumers.set(consumerId, next);
  state.revision += 1;
}

/** Remove one mounted consumer and retire an unused scope after this commit. */
export function unregisterWikiReadConsumer(
  scope: OwnerOperationScope,
  consumerId: string,
): void {
  const state = coordinators.get(scopeKey(scope));
  if (!state?.consumers.delete(consumerId)) return;
  state.revision += 1;
  if (state.consumers.size === 0) {
    state.retirementScheduled = true;
    queueMicrotask(() => {
      if (
        !state.retirementScheduled ||
        state.retired ||
        state.consumers.size > 0
      ) {
        return;
      }
      state.retirementScheduled = false;
      state.retired = true;
      for (const controller of state.activeControllers) controller.abort();
      state.activeControllers.clear();
      state.cache.clear();
      if (coordinators.get(scopeKey(scope)) === state) {
        coordinators.delete(scopeKey(scope));
      }
    });
  }
}

/** Put one coordinate at the front of the next canonical pass. */
export function retryWikiRepository(
  scope: OwnerOperationScope,
  coordinate: string,
): boolean {
  const state = coordinatorFor(scope);
  if (state.retryPriorities.has(coordinate)) return false;
  state.retryPriorities.add(coordinate);
  state.revision += 1;
  return true;
}

async function readWithTwoWorkers(
  coordinates: readonly string[],
  expected: OwnerOperationScope,
  signal: AbortSignal | undefined,
  onAttempt: (attempt: ReadAttempt) => void,
): Promise<void> {
  let cursor = 0;
  let scopeFailure: unknown;

  const worker = async () => {
    while (scopeFailure === undefined) {
      throwIfAborted(signal);
      const coordinate = coordinates[cursor++];
      if (!coordinate) return;
      const release = await admission.acquire(signal);
      try {
        throwIfAborted(signal);
        onAttempt({
          coordinate,
          snapshot: await readWikiSnapshotAtScope(coordinate, expected, signal),
        });
      } catch (error) {
        if (error instanceof Error && error.name === "AbortError") throw error;
        if (
          error instanceof Error &&
          error.name === "WikiSnapshotScopeChanged"
        ) {
          scopeFailure = error;
          return;
        }
        onAttempt({
          coordinate,
          error: error instanceof Error ? error.message : "Wiki read failed.",
        });
      } finally {
        release();
      }
    }
  };

  await Promise.all([worker(), worker()]);
  if (scopeFailure !== undefined) throw scopeFailure;
  throwIfAborted(signal);
}

function preservedResult(coordinate: string): WikiRepositoryReadResult {
  return { coordinate, error: "", outcome: "preserved" };
}

async function readPass(
  state: ScopeCoordinatorState,
  request: { coordinates: string[]; priorities: string[] },
  automaticLimit: number,
  signal: AbortSignal | undefined,
  preserveUnselected: boolean,
): Promise<{
  plan: WikiRepositoryReadPlan;
  projected: Map<string, WikiRepositorySnapshotCache>;
}> {
  const plan = planWikiRepositoryReads(
    request.coordinates,
    request.priorities,
    automaticLimit,
  );
  const readCoordinates = [...plan.prioritized, ...plan.automatic];
  const preserve = preserveUnselected
    ? request.coordinates
        .filter((coordinate) => !readCoordinates.includes(coordinate))
        .map(preservedResult)
    : [];
  const attempted = new Set<string>();
  let projected = state.cache;

  const apply = (attempt: ReadAttempt) => {
    const preserved = [...attempted]
      .filter((coordinate) => coordinate !== attempt.coordinate)
      .map(preservedResult);
    projected = projectWikiRepositoryReads(
      state.scope,
      request.coordinates,
      [...preserve, ...preserved, attempt],
      preserveUnselected ? [] : plan.skipped,
      projected,
      MAX_RETAINED_WIKI_GRAPH_BYTES,
      plan.prioritized,
    );
    attempted.add(attempt.coordinate);
  };

  if (readCoordinates.length === 0) {
    projected = projectWikiRepositoryReads(
      state.scope,
      request.coordinates,
      preserve,
      preserveUnselected ? [] : plan.skipped,
      projected,
      MAX_RETAINED_WIKI_GRAPH_BYTES,
      plan.prioritized,
    );
  } else {
    await readWithTwoWorkers(readCoordinates, state.scope, signal, apply);
  }
  await assertWikiSnapshotScope(state.scope, signal);
  return { plan, projected };
}

/** Run the single canonical repository projection for one active scope. */
export async function readWikiRepositoryProjection(
  scope: OwnerOperationScope,
  consumerId: string,
  ownCoordinates: readonly string[],
  ownPriorities: readonly string[],
  signal?: AbortSignal,
): Promise<Map<string, WikiRepositorySnapshotCache>> {
  const state = coordinatorFor(scope);
  const controller = new AbortController();
  const onAbort = () => controller.abort();
  if (signal?.aborted) controller.abort();
  else signal?.addEventListener("abort", onAbort, { once: true });
  state.activeControllers.add(controller);
  const readSignal = controller.signal;
  // The observer effect is the authoritative live registration. A query
  // function can start before that effect on first mount, so seed only a
  // missing consumer; never overwrite a newer priority selection with the
  // query function's stale closure while a batch is in flight.
  if (!state.consumers.has(consumerId)) {
    registerWikiReadConsumer(scope, consumerId, ownCoordinates, ownPriorities);
  }
  const revisionAtStart = state.revision;
  const firstRequest = mergedRequest(
    state,
    consumerId,
    ownCoordinates,
    ownPriorities,
  );
  try {
    const first = await readPass(
      state,
      firstRequest,
      MAX_AUTOMATIC_WIKI_REPO_READS,
      readSignal,
      false,
    );
    throwIfAborted(readSignal);
    if (state.retired) throw abortError();
    state.cache = first.projected;
    for (const coordinate of first.plan.prioritized) {
      state.retryPriorities.delete(coordinate);
    }
    const currentRequest = mergedRequest(
      state,
      consumerId,
      ownCoordinates,
      ownPriorities,
    );
    if (state.revision !== revisionAtStart) {
      const second = await readPass(state, currentRequest, 0, readSignal, true);
      throwIfAborted(readSignal);
      if (state.retired) throw abortError();
      state.cache = second.projected;
      for (const coordinate of second.plan.prioritized) {
        state.retryPriorities.delete(coordinate);
      }
    }
    throwIfAborted(readSignal);
    if (state.retired) throw abortError();
    return state.cache;
  } finally {
    state.activeControllers.delete(controller);
    signal?.removeEventListener("abort", onAbort);
  }
}

/** Clear test/process-local coordinator state after a QueryClient is gone. */
export function clearWikiReadCoordinators(): void {
  for (const state of coordinators.values()) {
    state.retired = true;
    for (const controller of state.activeControllers) controller.abort();
    state.activeControllers.clear();
    state.cache.clear();
  }
  coordinators.clear();
}
