import type { WikiJobState } from "./wikiEvents";
import {
  sameOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";

let jobs = new Map<string, WikiJobState>();
let scopes = new Map<string, OwnerOperationScope>();
/**
 * The one owner/community scope whose durable Wiki projections are visible.
 *
 * A scope token is a snapshot of native identity and workspace state.  The
 * generation counters make it possible to accept a newer token without
 * allowing an older A1 completion to bring A1 back after the view moved to
 * A2.  Tokens with equal generations but different origin are ambiguous and
 * are rejected until native supplies a strictly newer generation.
 */
let activeScope: OwnerOperationScope | null = null;
let storeGeneration = 0;
const jobGenerations = new Map<string, number>();
const listeners = new Set<() => void>();

function notify(): void {
  for (const listener of listeners) listener();
}

export function getWikiJobs(): Map<string, WikiJobState> {
  return jobs;
}

/** Monotonic renderer-store generation used to fence late native reads. */
export function getWikiStoreGeneration(): number {
  return storeGeneration;
}

export function getWikiActiveScope(): OwnerOperationScope | null {
  return activeScope;
}

function canAdvanceScope(
  current: OwnerOperationScope,
  next: OwnerOperationScope,
): boolean {
  if (sameOwnerOperationScope(current, next)) return true;
  // A token from an older workspace or identity generation is stale even if
  // the other counter advanced.  Equal counters with a different owner or
  // community do not prove which scope is current, so reject that ambiguity.
  if (
    next.workspace_generation < current.workspace_generation ||
    next.identity_generation < current.identity_generation
  ) {
    return false;
  }
  return (
    next.workspace_generation > current.workspace_generation ||
    next.identity_generation > current.identity_generation
  );
}

/**
 * Adopt a validated native scope for the renderer's durable Wiki state.
 *
 * The transition is atomic from the store's point of view: every job and
 * per-repository scope claim from the previous owner/workspace is removed
 * before the caller can hydrate the new scope.  A rejected token makes no
 * mutation, which is the stale-result fence for A1 -> A2 -> late A1 races.
 */
export function activateWikiJobScope(scope: OwnerOperationScope): boolean {
  if (activeScope && !canAdvanceScope(activeScope, scope)) return false;
  if (activeScope && sameOwnerOperationScope(activeScope, scope)) return true;
  activeScope = scope;
  jobs = new Map();
  scopes = new Map();
  jobGenerations.clear();
  markChanged();
  notify();
  return true;
}

function markChanged(repoKey?: string): void {
  storeGeneration += 1;
  if (repoKey) jobGenerations.set(repoKey, storeGeneration);
}

/** Fence a repository's renderer status to one captured native scope. */
export function setWikiJobScope(
  repoKey: string,
  scope: OwnerOperationScope,
): void {
  if (!activateWikiJobScope(scope)) return;
  const current = scopes.get(repoKey);
  if (current && sameOwnerOperationScope(current, scope)) return;
  scopes = new Map(scopes);
  scopes.set(repoKey, scope);
  jobs = new Map(jobs);
  jobs.delete(repoKey);
  markChanged(repoKey);
  notify();
}

export function setWikiJob(next: WikiJobState): void {
  if (next.scope && !activateWikiJobScope(next.scope)) return;
  const current = scopes.get(next.repoKey);
  if (current) {
    if (!next.scope || !sameOwnerOperationScope(current, next.scope)) return;
  } else if (next.scope) {
    // Production callers must claim a scope before publishing status. This
    // keeps a stale completion from establishing a new scope implicitly.
    return;
  }
  const currentJob = jobs.get(next.repoKey);
  // A late callback for the same durable operation must never regress the
  // renderer to an older native revision: an error callback holding the
  // revision it dispatched cannot undo a newer journal projection. Rows with
  // no durable identity are local pending status and keep replace semantics.
  if (
    currentJob &&
    currentJob.operationId !== undefined &&
    currentJob.operationId === next.operationId &&
    (currentJob.operationRevision ?? 0) > (next.operationRevision ?? 0)
  ) {
    return;
  }
  jobs = new Map(jobs);
  jobs.set(next.repoKey, next);
  markChanged(next.repoKey);
  notify();
}

/** Apply a bounded native journal projection after startup or scope refresh. */
export function hydrateWikiJobs(
  nextJobs: WikiJobState[],
  scope: OwnerOperationScope,
  startedGeneration = storeGeneration,
): boolean {
  if (!activateWikiJobScope(scope)) return false;
  const incoming = new Map(nextJobs.map((job) => [job.repoKey, job]));
  // Start from the authoritative maps. A late list for scope A must be
  // applied per repository; rebuilding the whole map from that response can
  // erase a newer A2 claim that was created after the request started.
  const nextScopes = new Map(scopes);
  const next = new Map(jobs);
  const seen = new Set(incoming.keys());
  for (const [repoKey, job] of incoming) {
    const current = jobs.get(repoKey);
    const claim = scopes.get(repoKey);
    if (claim && !sameOwnerOperationScope(claim, scope)) continue;
    if (current?.scope && !sameOwnerOperationScope(current.scope, scope)) {
      continue;
    }
    const changedSinceRead =
      (jobGenerations.get(repoKey) ?? 0) > startedGeneration;
    // A list started before prepare/CAS may return an empty or older row. A
    // per-repository store generation fences that response; an incoming row
    // can replace it only when its operation revision is provably newer.
    const projected = changedSinceRead
      ? current &&
        (shouldKeepCurrent(current, job) ||
          current.operationId !== job.operationId)
        ? current
        : undefined
      : shouldKeepCurrent(current, job)
        ? current
        : job;
    if (!projected) continue;
    nextScopes.set(repoKey, scope);
    next.set(repoKey, { ...projected, scope });
  }
  // A list response may have been captured before a local prepare or a newer
  // native CAS. Preserve that newer in-flight projection until a later poll
  // observes its durable row; completed stale rows are safely removed.
  for (const [repoKey, current] of jobs) {
    if (seen.has(repoKey)) continue;
    const claim = scopes.get(repoKey);
    if (claim && !sameOwnerOperationScope(claim, scope)) continue;
    if (current.scope && !sameOwnerOperationScope(current.scope, scope)) {
      continue;
    }
    if (
      (jobGenerations.get(repoKey) ?? 0) > startedGeneration ||
      !current.operationId ||
      current.status === "generating"
    ) {
      nextScopes.set(repoKey, scope);
      next.set(repoKey, { ...current, scope });
    } else {
      // The response is authoritative for this repository and did not race a
      // newer local write. Remove only this scope's old row; keep every other
      // scope claim intact.
      next.delete(repoKey);
      if (claim && sameOwnerOperationScope(claim, scope)) {
        nextScopes.delete(repoKey);
      }
    }
  }
  scopes = nextScopes;
  jobs = next;
  markChanged();
  notify();
  return true;
}

function shouldKeepCurrent(
  current: WikiJobState | undefined,
  incoming: WikiJobState,
): boolean {
  if (!current) return false;
  if (current.operationId === incoming.operationId) {
    return (
      (current.operationRevision ?? 0) >= (incoming.operationRevision ?? 0)
    );
  }
  return current.status === "generating";
}

export function subscribeWikiJobs(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function resetWikiStore(): void {
  jobs = new Map();
  scopes = new Map();
  activeScope = null;
  jobGenerations.clear();
  markChanged();
  notify();
}
