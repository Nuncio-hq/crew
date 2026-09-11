import * as React from "react";

import { useWikiGenerate } from "@/features/wiki/hooks/useWikiGenerate";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import { useProjectsQuery } from "@/features/projects/hooks";
import {
  debounce_due,
  next_cadence_due,
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

  React.useEffect(() => {
    if (!scope) return;
    const tocs = eventsQuery.data?.tocs ?? [];
    const states = eventsQuery.data?.states ?? [];
    const repos = (projectsQuery.data ?? []).flatMap(
      (project) => project.repositories,
    );
    const now = Math.floor(Date.now() / 1000);
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
      if (!due) continue;
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
  }, [eventsQuery.data, jobs, mutateGenerate, projectsQuery.data, scope]);
}

function debounceDue(lastFiredUnix: number, lastPushUnix: number, now: number) {
  return debounce_due(lastFiredUnix, lastPushUnix, now);
}

function nextCadenceDue(toc: WikiToc, now: number) {
  return next_cadence_due(toc.cadence, toc.generatedAt, now);
}
