import * as React from "react";

import { useWikiGenerate } from "@/features/wiki/hooks/useWikiGenerate";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import { useWikiRepoStateLive } from "./useWikiRepoStateLive";
import { useProjectsQuery } from "@/features/projects/hooks";
import {
  debounce_due,
  next_cadence_due,
  ON_PUSH_DEBOUNCE_SECS,
} from "@/features/wiki/lib/wikiCadenceJs";
import {
  defaultBranchCommit,
  jobForRepo,
  repoKey,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import { getWikiJobs, subscribeWikiJobs } from "@/features/wiki/lib/wikiStore";
import { wikiRepositoryCoordinate } from "@/shared/api/wikiSnapshot";

/**
 * Desktop-governed cadence + on-push debounce. Subscribes via TOC freshness
 * and kind 30618 state events already fetched by `useWikiEventsQuery`.
 */
export function useWikiRefresh() {
  const eventsQuery = useWikiEventsQuery();
  const projectsQuery = useProjectsQuery();
  const generate = useWikiGenerate();
  const mutateGenerate = generate.mutate;
  const jobs = React.useSyncExternalStore(
    subscribeWikiJobs,
    getWikiJobs,
    getWikiJobs,
  );
  const lastFired = React.useRef(new Map<string, number>());
  // Use the exact scope that produced the displayed Wiki projection. A
  // separate recapture can pair a fresh B token with stale A events and turn
  // a read-only view into an owner write.
  const scope = eventsQuery.scopeQuery.error
    ? null
    : (eventsQuery.scopeQuery.data ?? null);
  const repositorySignature = JSON.stringify([
    ...new Map(
      (projectsQuery.data ?? []).flatMap((project) =>
        project.repositories.map(
          ({ owner, dtag }) => [`${owner}:${dtag}`, { owner, dtag }] as const,
        ),
      ),
    ).values(),
  ]);
  const live = useWikiRepoStateLive(
    scope,
    repositorySignature,
    eventsQuery.refetch,
  );

  React.useEffect(() => {
    if (!scope || live.error) return;
    const tocs = eventsQuery.data?.tocs ?? [];
    const states = eventsQuery.data?.states ?? [];
    const repos = (projectsQuery.data ?? []).flatMap(
      (project) => project.repositories,
    );
    // Equal query results retain their references. A deadline must wake this
    // evaluator even when no relay event or UI interaction changes its inputs.
    let timer: ReturnType<typeof window.setTimeout> | undefined;
    const checkCadence = () => {
      const now = Math.floor(Date.now() / 1000);
      let nextWakeAt = Number.POSITIVE_INFINITY;
      for (const toc of tocs) {
        const key = repoKey(toc.owner, toc.repoD);
        const repo = repos.find(
          (item) =>
            item.owner.toLowerCase() === toc.owner.toLowerCase() &&
            item.dtag === toc.repoD,
        );
        const coordinate = wikiRepositoryCoordinate(toc.owner, toc.repoD);
        const readStatus = eventsQuery.data?.repositoryStatuses[coordinate];
        // Cadence is an owner write. Require the exact current-scope read and a
        // writable local source before creating a native publication job.
        if (
          toc.owner.toLowerCase() !== scope.scope.owner.toLowerCase() ||
          !repo?.localWorkspacePath ||
          repo.localWorkspaceStatus !== "linked" ||
          !readStatus ||
          readStatus.stale ||
          readStatus.unavailable
        ) {
          continue;
        }
        const publicationJob = jobForRepo(jobs, toc.owner, toc.repoD);
        // Polling and cadence refresh may never silently replace an unresolved
        // durable claim. Recovery or an explicit Generate/Regenerate action must
        // resolve that row first.
        if (publicationJob?.operationId && !publicationJob.reconciled) continue;
        const state = states.find(
          (event) =>
            event.tags.some((tag) => tag[0] === "d" && tag[1] === toc.repoD) &&
            (event.pubkey.toLowerCase() === toc.owner.toLowerCase() ||
              event.tags.some(
                (tag) => tag[0] === "a" && tag[1] === repo.repoAddress,
              )),
        );
        const tip = defaultBranchCommit(state);
        // Push cadence needs a verified repository tip. Time cadence can still
        // run from the immutable TOC schedule when relay push state is absent or
        // unavailable; do not turn that missing advisory into a hard stop.
        if (toc.cadence === "on-push" && !tip?.commit) continue;
        const scheduleKey =
          `${scope.scope.owner}:${scope.scope.community}:${scope.workspace_generation}:${scope.identity_generation}:${key}` +
          ":" +
          repo.localWorkspacePath +
          ":" +
          (repo.workspaceMode ?? "git");
        const fired = lastFired.current.get(scheduleKey) ?? 0;
        const pushAt = state?.created_at ?? 0;
        const due =
          toc.cadence === "on-push"
            ? debounceDue(fired, pushAt, now) && toc.commit !== tip?.commit
            : nextCadenceDue(toc, now);
        if (!due) {
          const dueAt =
            toc.cadence === "on-push"
              ? pushAt > 0 && fired < pushAt && toc.commit !== tip?.commit
                ? pushAt + ON_PUSH_DEBOUNCE_SECS
                : Number.POSITIVE_INFINITY
              : toc.cadence === "daily"
                ? toc.generatedAt + 86_400
                : toc.cadence === "weekly"
                  ? toc.generatedAt + 86_400 * 7
                  : Number.POSITIVE_INFINITY;
          if (dueAt > now) nextWakeAt = Math.min(nextWakeAt, dueAt);
          continue;
        }
        lastFired.current.set(scheduleKey, now);
        mutateGenerate({
          owner: toc.owner,
          repoD: toc.repoD,
          repoKey: key,
          repoPath: repo?.localWorkspacePath,
          workspaceMode: repo?.workspaceMode,
          expectedScope: scope,
        });
      }
      if (Number.isFinite(nextWakeAt)) {
        // Browser timers overflow past signed 32-bit delays; cap distant
        // timestamps so they cannot turn into an immediate retry loop.
        timer = window.setTimeout(
          checkCadence,
          Math.min((nextWakeAt - now) * 1_000, 2_147_483_647),
        );
      }
    };
    checkCadence();
    return () => window.clearTimeout(timer);
  }, [
    eventsQuery.data,
    jobs,
    mutateGenerate,
    projectsQuery.data,
    scope,
    live.error,
  ]);
  return live;
}

function debounceDue(lastFiredUnix: number, lastPushUnix: number, now: number) {
  return debounce_due(lastFiredUnix, lastPushUnix, now);
}

function nextCadenceDue(toc: WikiToc, now: number) {
  return next_cadence_due(toc.cadence, toc.generatedAt, now);
}
