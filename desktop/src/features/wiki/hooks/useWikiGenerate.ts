import { useMutation, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

import { wikiEventsQueryKey } from "@/features/wiki/hooks/useWikiEventsQuery";
import {
  activateWikiJobScope,
  getWikiJobs,
  setWikiJob,
  setWikiJobScope,
} from "@/features/wiki/lib/wikiStore";
import {
  captureOwnerOperationScope,
  sameOwnerOperationScope,
  type OwnerOperationScope,
  type ScopedOwnerOperation,
} from "@/shared/api/ownerOperations";
import {
  repoKey,
  type WikiJobState,
  type WikiCadence,
} from "@/features/wiki/lib/wikiEvents";

export type WikiPublicationJob = {
  id: string;
  revision: number;
  resourceKey: string;
  status: string;
  reconciled: boolean;
  snapshotId: string;
  sourceRevision: string;
  cadence: string;
  pages: number;
  attempts: number;
  progress: string;
  headAttempted: boolean;
  cancelRequested: boolean;
  reconcileOnly: boolean;
  retiredDependencyId?: string | null;
  retryAt: number;
  lastError: string | null;
};

type WikiPrepareResult =
  | { result: "created"; job: WikiPublicationJob }
  | { result: "noop"; headId: string; sourceRevision: string };

type ScopedPrepare = ScopedOwnerOperation<WikiPrepareResult>;
type ScopedJob = ScopedOwnerOperation<WikiPublicationJob>;

function repositoryKey(resourceKey: string): string | null {
  const prefix = "30617:";
  if (!resourceKey.startsWith(prefix)) return null;
  const rest = resourceKey.slice(prefix.length);
  const separator = rest.indexOf(":");
  if (separator <= 0 || separator === rest.length - 1) return null;
  return repoKey(rest.slice(0, separator), rest.slice(separator + 1));
}

/** Convert the native bounded projection into the renderer's recovery state. */
export function wikiPublicationJobState(
  job: WikiPublicationJob,
  scope: OwnerOperationScope,
  errorOverride?: string | null,
): WikiJobState | null {
  const key = repositoryKey(job.resourceKey);
  if (!key) return null;
  const terminalSuperseded = job.status === "superseded";
  const status: WikiJobState["status"] =
    job.status === "failed" || terminalSuperseded
      ? "failed"
      : job.reconciled
        ? "idle"
        : "generating";
  return {
    repoKey: key,
    scope,
    operationId: job.id,
    operationRevision: job.revision,
    nativeStatus: job.status,
    reconciled: job.reconciled,
    attempts: job.attempts,
    retryAt: job.retryAt,
    headAttempted: job.headAttempted,
    cancelRequested: job.cancelRequested,
    reconcileOnly: job.reconcileOnly,
    retiredDependencyId: job.retiredDependencyId ?? undefined,
    snapshotId: job.snapshotId,
    sourceRevision: job.sourceRevision,
    cadence: job.cadence as WikiCadence,
    status,
    done: job.reconciled || job.progress === "head" ? job.pages : 0,
    total: job.pages,
    error: errorOverride === undefined ? job.lastError : errorOverride,
    costNote: "Native governed Wiki publication",
  };
}

function ensureScope(
  result: ScopedOwnerOperation<unknown>,
  expected: OwnerOperationScope,
) {
  ensureScopeToken(result.token, expected);
}

function ensureScopeToken(
  actual: OwnerOperationScope,
  expected: OwnerOperationScope,
): void {
  if (!sameOwnerOperationScope(actual, expected)) {
    throw new Error(
      "The active owner or community changed during Wiki publication.",
    );
  }
}

async function ensureCurrentScope(
  expected: OwnerOperationScope,
): Promise<void> {
  ensureScopeToken(await captureOwnerOperationScope(), expected);
}

function updateJob(
  repoKey: string,
  job: WikiPublicationJob | null,
  error: string | null,
  status: "idle" | "generating" | "failed",
  scope: OwnerOperationScope,
): void {
  if (job) {
    const nativeState = wikiPublicationJobState(job, scope, error);
    if (nativeState) {
      setWikiJob({ ...nativeState, repoKey, status });
      return;
    }
  }
  setWikiJob({
    repoKey,
    scope,
    status,
    done: 0,
    total: job?.pages ?? 0,
    error,
    costNote: "Native governed Wiki publication",
  });
}

/**
 * Renderer status for a dispatch that native already settled.
 *
 * A superseded row is terminal and reconciled: a different head won. Keeping
 * the native failed projection is what shows that warning and the fresh
 * Generate affordance instead of a Complete-looking idle card.
 */
function finalJobStatus(
  job: WikiPublicationJob,
): "idle" | "generating" | "failed" {
  if (job.status === "superseded") return "failed";
  return job.reconciled ? "idle" : "generating";
}

export function useWikiGenerate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: {
      owner: string;
      repoD: string;
      repoKey: string;
      repoPath?: string | null;
      workspaceMode?: "git" | "folder";
      /** Scope captured with the rendered repository query. */
      expectedScope: OwnerOperationScope;
    }) => {
      let expected: OwnerOperationScope | null = null;
      let knownJob: WikiPublicationJob | null = null;
      try {
        expected = await captureOwnerOperationScope();
        if (!activateWikiJobScope(expected)) {
          throw new Error(
            "The captured owner or workspace scope is older than the active Wiki scope.",
          );
        }
        ensureScopeToken(input.expectedScope, expected);
        setWikiJobScope(input.repoKey, expected);
        updateJob(input.repoKey, null, null, "generating", expected);
        const prepared = await invoke<ScopedPrepare>(
          "wiki_publication_prepare",
          {
            expected,
            coordinate: `30617:${input.owner}:${input.repoD}`,
            repoPath: input.repoPath ?? null,
            workspaceMode: input.workspaceMode ?? "git",
          },
        );
        ensureScope(prepared, expected);
        if (prepared.value.result === "noop") {
          await ensureCurrentScope(expected);
          updateJob(input.repoKey, null, null, "idle", expected);
          await ensureCurrentScope(expected);
          await queryClient.invalidateQueries({ queryKey: wikiEventsQueryKey });
          return;
        }
        const job = prepared.value.job;
        knownJob = job;
        await ensureCurrentScope(expected);
        updateJob(input.repoKey, job, null, "generating", expected);
        const dispatched = await invoke<ScopedJob>(
          "wiki_publication_dispatch",
          {
            expected,
            id: job.id,
            revision: job.revision,
            explicitRetry: false,
          },
        );
        ensureScope(dispatched, expected);
        const finalJob = dispatched.value;
        knownJob = finalJob;
        if (finalJob.lastError && !finalJob.reconciled) {
          await ensureCurrentScope(expected);
          updateJob(
            input.repoKey,
            finalJob,
            finalJob.lastError,
            "failed",
            expected,
          );
          throw new Error(finalJob.lastError);
        }
        if (finalJob.status === "failed" && !finalJob.reconciled) {
          const error = "Native Wiki publication failed.";
          await ensureCurrentScope(expected);
          updateJob(input.repoKey, finalJob, error, "failed", expected);
          throw new Error(error);
        }
        await ensureCurrentScope(expected);
        updateJob(
          input.repoKey,
          finalJob,
          null,
          finalJobStatus(finalJob),
          expected,
        );
        await ensureCurrentScope(expected);
        await queryClient.invalidateQueries({ queryKey: wikiEventsQueryKey });
      } catch (error) {
        // A stale completion must never overwrite the new owner's status. The
        // re-capture also covers a native command that failed after a scope
        // change but before returning its scoped response.
        if (expected) {
          try {
            const current = await captureOwnerOperationScope();
            if (sameOwnerOperationScope(current, expected)) {
              const message =
                error instanceof Error ? error.message : String(error);
              if (knownJob) {
                updateJob(input.repoKey, knownJob, message, "failed", expected);
              } else {
                const existing = getWikiJobs().get(input.repoKey);
                if (
                  existing?.scope &&
                  sameOwnerOperationScope(existing.scope, expected) &&
                  existing.operationId
                ) {
                  // Keep the durable identity and revision visible when the
                  // native call failed after a recovery row was already
                  // projected but before it returned a fresh job envelope.
                  setWikiJob({
                    ...existing,
                    status: "failed",
                    error: message,
                  });
                } else {
                  updateJob(input.repoKey, null, message, "failed", expected);
                }
              }
            }
          } catch {
            // Scope capture failed; leave the existing native projection alone.
          }
        }
        throw error;
      }
    },
  });
}

export function useWikiSetCadence() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: {
      owner: string;
      repoD: string;
      cadence: string;
      repoKey?: string;
      /** Scope captured with the rendered repository query. */
      expectedScope: OwnerOperationScope;
    }) => {
      const expected = await captureOwnerOperationScope();
      try {
        if (!activateWikiJobScope(expected)) {
          throw new Error(
            "The captured owner or workspace scope is older than the active Wiki scope.",
          );
        }
        ensureScopeToken(input.expectedScope, expected);
        const prepared = await invoke<ScopedPrepare>(
          "wiki_publication_set_cadence",
          {
            expected,
            coordinate: `30617:${input.owner}:${input.repoD}`,
            cadence: input.cadence,
          },
        );
        ensureScope(prepared, expected);
        if (prepared.value.result !== "noop") {
          const job = prepared.value.job;
          const dispatched = await invoke<ScopedJob>(
            "wiki_publication_dispatch",
            {
              expected,
              id: job.id,
              revision: job.revision,
              explicitRetry: false,
            },
          );
          ensureScope(dispatched, expected);
          if (dispatched.value.lastError && !dispatched.value.reconciled) {
            throw new Error(dispatched.value.lastError);
          }
        }
        await ensureCurrentScope(expected);
        await queryClient.invalidateQueries({ queryKey: wikiEventsQueryKey });
      } catch (error) {
        await ensureCurrentScope(expected);
        throw error;
      }
    },
  });
}
