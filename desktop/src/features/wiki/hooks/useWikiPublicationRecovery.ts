import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

import { wikiEventsQueryKey } from "@/features/wiki/hooks/useWikiEventsQuery";
import {
  captureOwnerOperationScope,
  sameOwnerOperationScope,
  type OwnerOperationScope,
  type ScopedOwnerOperation,
} from "@/shared/api/ownerOperations";
import {
  activateWikiJobScope,
  getWikiJobs,
  getWikiStoreGeneration,
  hydrateWikiJobs,
  setWikiJob,
} from "@/features/wiki/lib/wikiStore";
import {
  wikiPublicationJobState,
  type WikiPublicationJob,
} from "@/features/wiki/hooks/useWikiGenerate";
import type { WikiJobState } from "@/features/wiki/lib/wikiEvents";

type ScopedJobs = ScopedOwnerOperation<WikiPublicationJob[]>;
type ScopedJob = ScopedOwnerOperation<WikiPublicationJob>;
type ScopedPrepare = ScopedOwnerOperation<
  | { result: "created"; job: WikiPublicationJob }
  | { result: "noop"; headId: string; sourceRevision: string }
>;
export type RecoveryAction = "retry" | "reconcile" | "cancel" | "regenerate";

/** Shared query prefix; the scope token is part of the cache identity. */
export const wikiPublicationQueryKey = [
  "crew-wiki-publication-status",
] as const;

type WikiPublicationQueryData = {
  jobs: WikiJobState[];
  scope: OwnerOperationScope;
  /** Token returned with this list; adoption is fenced on it in the effect. */
  token: OwnerOperationScope;
  /** Store generation observed before the native list read began. */
  startedGeneration: number;
};

function scopeQueryKey(scope: OwnerOperationScope | undefined) {
  return [
    ...wikiPublicationQueryKey,
    scope?.scope.owner ?? "",
    scope?.scope.community ?? "",
    scope?.workspace_generation ?? -1,
    scope?.identity_generation ?? -1,
  ] as const;
}

function ensureScope(
  actual: OwnerOperationScope,
  expected: OwnerOperationScope,
): void {
  if (!sameOwnerOperationScope(actual, expected)) {
    throw new Error(
      "The active owner or community changed during Wiki recovery.",
    );
  }
}

function jobForRepoKey(repoKey: string): WikiJobState | undefined {
  return getWikiJobs().get(repoKey);
}

function abortedError(): Error {
  const error = new Error("Wiki recovery status read was canceled.");
  error.name = "AbortError";
  return error;
}

/**
 * Write a native row only where this response still owns the projection.
 *
 * A status poll can hydrate a different, newer operation for the repository
 * while a recovery command is in flight. This response knows two identities:
 * the row the user acted on (`replaces`) and its own operation. Anything else
 * on the row is a newer projection this callback cannot speak for, and a
 * newer revision of its own operation must not be regressed either.
 */
function setRecoveryJob(
  next: WikiJobState,
  expected: OwnerOperationScope,
  replaces: string,
): void {
  const current = jobForRepoKey(next.repoKey);
  if (!current?.scope || !sameOwnerOperationScope(current.scope, expected)) {
    setWikiJob(next);
    return;
  }
  if (current.operationId === next.operationId) {
    if ((current.operationRevision ?? 0) > (next.operationRevision ?? 0)) {
      return;
    }
    setWikiJob(next);
    return;
  }
  if (current.operationId !== replaces) return;
  setWikiJob(next);
}

/**
 * Project a dispatch failure onto the successor row, scope-fenced.
 *
 * The dispatch token proves the scope the request ran under, not the scope
 * now, so re-read the native scope and match the original click-time scope
 * before writing. A newer revision of the same successor that native produced
 * while this dispatch was in flight is authoritative in full: a failure from
 * the older attempt cannot demote a completed row to failed.
 */
async function failSuccessor(
  repoKey: string,
  prepared: WikiJobState,
  expected: OwnerOperationScope,
  message: string,
): Promise<void> {
  try {
    ensureScope(await captureOwnerOperationScope(), expected);
  } catch {
    // The scope moved while dispatch ran; this failure is not ours to project.
    return;
  }
  const current = jobForRepoKey(repoKey);
  if (current?.scope && sameOwnerOperationScope(current.scope, expected)) {
    // Another operation already owns this row, so it is not this dispatch's
    // to fail.
    if (current.operationId !== prepared.operationId) return;
    // Native moved this successor past the revision the dispatch used. That
    // whole projection wins: a stale failure is not authoritative for it.
    if ((current.operationRevision ?? 0) > (prepared.operationRevision ?? 0)) {
      return;
    }
  }
  setWikiJob({ ...prepared, status: "failed", error: message });
}

/** Hydrate and operate the native Wiki journal through one scoped query. */
export function useWikiPublicationRecovery(
  scope: OwnerOperationScope | undefined,
) {
  const queryClient = useQueryClient();
  const scopeKey = scopeQueryKey(scope);
  const query = useQuery<WikiPublicationQueryData>({
    queryKey: scopeKey,
    enabled: scope !== undefined,
    queryFn: async ({ signal }) => {
      if (!scope) throw new Error("Wiki recovery scope is not ready.");
      const startedGeneration = getWikiStoreGeneration();
      const result = await invoke<ScopedJobs>("wiki_publication_list", {
        expected: scope,
      });
      if (signal.aborted) throw abortedError();
      ensureScope(result.token, scope);
      const jobs = result.value
        .map((job) => wikiPublicationJobState(job, scope))
        .filter((job): job is WikiJobState => job !== null);
      // The query function is deliberately pure: it adopts no scope and writes
      // no store. A canceled or stale list must never clear the active jobs or
      // scope claims; the effect below owns adoption and fences it.
      return { jobs, scope, token: result.token, startedGeneration };
    },
    retry: false,
    staleTime: 4_000,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
    refetchOnWindowFocus: true,
    // Recovery rows are scoped durable projections. Do not retain a previous
    // owner's journal projection after the last status observer unmounts.
    gcTime: 0,
  });

  const hydrationGeneration = React.useRef(0);
  React.useEffect(() => {
    const started = ++hydrationGeneration.current;
    if (!scope || !query.data) return;
    const data = query.data;
    let active = true;
    void captureOwnerOperationScope()
      .then((current) => {
        if (
          !active ||
          started !== hydrationGeneration.current ||
          !sameOwnerOperationScope(current, scope) ||
          // The token this list actually ran under, not just the rendered
          // scope, has to match the freshly captured native scope.
          !sameOwnerOperationScope(data.token, scope) ||
          !sameOwnerOperationScope(data.scope, scope)
        ) {
          return;
        }
        // hydrateWikiJobs adopts the scope atomically and rejects a token
        // older than the active one, so this is the only store write a status
        // read performs.
        hydrateWikiJobs(data.jobs, scope, data.startedGeneration);
      })
      .catch(() => {
        // A scope read that fails or changes makes this result unusable. The
        // query will refetch under its canonical key on the next interval.
      });
    return () => {
      active = false;
    };
  }, [query.data, scope]);

  const mutation = useMutation({
    mutationFn: async (input: {
      action: RecoveryAction;
      repoKey: string;
      operationId: string;
      operationRevision: number;
      /** Scope captured with the row that the user clicked. */
      expectedScope: OwnerOperationScope;
      /** Fresh source selection required by the explicit successor action. */
      repoPath?: string | null;
      workspaceMode?: "git" | "folder";
      /** UI must only expose this action for typed immutable-retired proof. */
      retiredDependencyId?: string;
    }) => {
      if (input.action === "regenerate" && !input.retiredDependencyId) {
        throw new Error(
          "Wiki regeneration requires native immutable-dependency retirement proof.",
        );
      }
      const current = await captureOwnerOperationScope();
      // Match the clicked row's scope before adopting anything: a scope the
      // user did not act in must not clear this store's jobs and claims.
      ensureScope(current, input.expectedScope);
      if (!activateWikiJobScope(current)) {
        throw new Error(
          "The captured owner or workspace scope is older than the active Wiki scope.",
        );
      }
      const command = `wiki_publication_${input.action}`;
      let resultToken: OwnerOperationScope;
      let nextJob: WikiPublicationJob;
      if (input.action === "regenerate") {
        const result = await invoke<ScopedPrepare>(command, {
          expected: input.expectedScope,
          id: input.operationId,
          revision: input.operationRevision,
          repoPath: input.repoPath ?? null,
          workspaceMode: input.workspaceMode ?? "git",
        });
        ensureScope(result.token, input.expectedScope);
        if (result.value.result !== "created") {
          throw new Error("Wiki regeneration did not create a successor job.");
        }
        resultToken = result.token;
        nextJob = result.value.job;

        // Regeneration atomically creates a fresh successor, but native keeps
        // that row reserved for only a short foreground-dispatch window. Put
        // the known successor in the renderer store before dispatch so a
        // dispatch error cannot fall back to the retired predecessor or erase
        // the new operation identity. The returned token proves the scope the
        // request ran under, not the current one: another window may have
        // changed identity meanwhile, so re-read it before the first write.
        const prepared = wikiPublicationJobState(nextJob, input.expectedScope);
        ensureScope(await captureOwnerOperationScope(), input.expectedScope);
        if (prepared) {
          setRecoveryJob(prepared, input.expectedScope, input.operationId);
        }
        try {
          const dispatched = await invoke<ScopedJob>(
            "wiki_publication_dispatch",
            {
              expected: input.expectedScope,
              id: nextJob.id,
              revision: nextJob.revision,
              explicitRetry: false,
            },
          );
          ensureScope(dispatched.token, input.expectedScope);
          resultToken = dispatched.token;
          nextJob = dispatched.value;
        } catch (error) {
          const message =
            error instanceof Error ? error.message : String(error);
          if (prepared) {
            await failSuccessor(
              input.repoKey,
              prepared,
              input.expectedScope,
              message,
            );
          }
          throw error;
        }
      } else {
        const result = await invoke<ScopedJob>(command, {
          expected: input.expectedScope,
          id: input.operationId,
          revision: input.operationRevision,
        });
        ensureScope(result.token, input.expectedScope);
        resultToken = result.token;
        nextJob = result.value;
      }
      const after = await captureOwnerOperationScope();
      ensureScope(after, input.expectedScope);
      const next = wikiPublicationJobState(nextJob, input.expectedScope);
      if (next) setRecoveryJob(next, input.expectedScope, input.operationId);
      return { token: resultToken, value: nextJob };
    },
    onSuccess: async (_result, input) => {
      try {
        ensureScope(await captureOwnerOperationScope(), input.expectedScope);
      } catch {
        return;
      }
      await queryClient.invalidateQueries({ queryKey: wikiEventsQueryKey });
      await queryClient.invalidateQueries({
        queryKey: wikiPublicationQueryKey,
      });
    },
    onError: (error, input) => {
      // Keep the original click-time token. A later A -> B -> A scope can
      // reuse the same persisted operation id/revision, so current scope plus
      // id/revision alone is not an adequate stale-result fence.
      void captureOwnerOperationScope()
        .then((currentScope) => {
          if (!sameOwnerOperationScope(currentScope, input.expectedScope)) {
            return;
          }
          const current = jobForRepoKey(input.repoKey);
          if (
            !current?.scope ||
            !sameOwnerOperationScope(current.scope, input.expectedScope) ||
            current.operationId !== input.operationId ||
            current.operationRevision !== input.operationRevision
          ) {
            return;
          }
          const message =
            error instanceof Error ? error.message : String(error);
          setWikiJob({ ...current, status: "failed", error: message });
        })
        .catch(() => {
          // A failed scope read cannot establish ownership of this error.
        });
    },
  });

  return {
    error: (query.error as Error | null) ?? (mutation.error as Error | null),
    isPending: mutation.isPending,
    refresh: query.refetch,
    recover: mutation.mutate,
    recoverAsync: mutation.mutateAsync,
  };
}
