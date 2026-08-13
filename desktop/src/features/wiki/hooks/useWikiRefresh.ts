import * as React from "react";

import { useWikiGenerate } from "@/features/wiki/hooks/useWikiGenerate";
import { useWikiEventsQuery } from "@/features/wiki/hooks/useWikiEventsQuery";
import { useProjectsQuery } from "@/features/projects/hooks";
import {
  debounce_due,
  next_cadence_due,
} from "@/features/wiki/lib/wikiCadenceJs";
import { repoKey, type WikiToc } from "@/features/wiki/lib/wikiEvents";

/**
 * Desktop-governed cadence + on-push debounce. Subscribes via TOC freshness
 * and kind 30618 state events already fetched by `useWikiEventsQuery`.
 */
export function useWikiRefresh() {
  const eventsQuery = useWikiEventsQuery();
  const projectsQuery = useProjectsQuery();
  const generate = useWikiGenerate();
  const mutateGenerate = generate.mutate;
  const lastFired = React.useRef(new Map<string, number>());

  React.useEffect(() => {
    const tocs = eventsQuery.data?.tocs ?? [];
    const states = eventsQuery.data?.states ?? [];
    const repos = (projectsQuery.data ?? []).flatMap(
      (project) => project.repositories,
    );
    const now = Math.floor(Date.now() / 1000);
    for (const toc of tocs) {
      const key = repoKey(toc.owner, toc.repoD);
      const fired = lastFired.current.get(key) ?? 0;
      const state = states.find((event) =>
        event.tags.some((tag) => tag[0] === "d" && tag[1] === toc.repoD),
      );
      const pushAt = state?.created_at ?? 0;
      const due =
        toc.cadence === "on-push"
          ? debounceDue(fired, pushAt, now) &&
            toc.commit !==
              (state?.tags.find((tag) => tag[0] === "refs/heads/main")?.[1] ??
                toc.commit)
          : nextCadenceDue(toc, now);
      if (!due) continue;
      lastFired.current.set(key, now);
      const repo = repos.find((item) => item.dtag === toc.repoD);
      mutateGenerate({
        owner: toc.owner,
        repoD: toc.repoD,
        repoKey: key,
        repoPath: repo?.localWorkspacePath,
      });
    }
  }, [eventsQuery.data, mutateGenerate, projectsQuery.data]);
}

function debounceDue(lastFiredUnix: number, lastPushUnix: number, now: number) {
  return debounce_due(lastFiredUnix, lastPushUnix, now);
}

function nextCadenceDue(toc: WikiToc, now: number) {
  return next_cadence_due(toc.cadence, toc.generatedAt, now);
}
