/**
 * #367 — the "From a private Wiki answer" backlink rendered on a dispatched
 * task kickoff.
 *
 * The author restores the exact origin: the `crew-wiki-task` client tag names
 * the durable operation whose journal payload carries the draft key and the
 * private attempt id — the chip stashes that attempt and opens the Wiki
 * surface it belongs to. Anyone else (same op record absent, another viewer,
 * another device) gets the authorized Wiki surface for the published
 * coordinate — a project wiki tab their own access lists, or the library —
 * never the author's private transcript.
 */
import * as React from "react";

import {
  captureOwnerOperationScope,
  loadOwnerOperation,
} from "@/shared/api/ownerOperations";
import { fetchProjects } from "@/features/projects/hooks";
import {
  readWikiTaskOrigin,
  stashWikiAskFocus,
} from "@/features/wiki/lib/wikiTaskOrigin";
import { parseWikiTaskDraftKey } from "@/features/wiki/lib/wikiTaskDispatch";

export type WikiOriginNavigateTarget =
  | { kind: "library" }
  | { kind: "project"; projectId: string; repositoryAddress: string };

/**
 * The app's router is a module singleton (`@/app/router`); imported lazily so
 * this chip — rendered inside MessageRow — never forces the route tree into
 * a bare-mount test.
 */
async function defaultNavigate(
  target: WikiOriginNavigateTarget,
): Promise<void> {
  const { router } = await import("@/app/router");
  if (target.kind === "project") {
    await router.navigate({
      to: "/projects/$projectId",
      params: { projectId: target.projectId },
      search: {
        repositoryAddress: target.repositoryAddress,
        tab: "wiki",
      } as never,
    });
  } else {
    await router.navigate({ to: "/wiki" });
  }
}

export function WikiTaskOriginLine({
  navigate = defaultNavigate,
  resolveProjects = fetchProjects,
  tags,
}: {
  navigate?: (target: WikiOriginNavigateTarget) => void;
  /** The non-author project's project lookup — injectable for tests. */
  resolveProjects?: () => Promise<
    { id: string; repositoryAddresses: string[] }[]
  >;
  tags?: string[][];
}) {
  const origin = readWikiTaskOrigin(tags);
  const [state, setState] = React.useState<"idle" | "busy" | "failed">("idle");

  const openOrigin = React.useCallback(async () => {
    if (!origin) return;
    setState("busy");
    try {
      // Author path: the durable record carries the draft key (project,
      // coordinate, attempt) — resolve it and restore the exact attempt.
      const scope = await captureOwnerOperationScope();
      const loaded = await loadOwnerOperation<{ draftKey?: string }>(
        scope,
        origin.dispatchId,
      );
      const parsed = loaded?.value?.payload?.draftKey
        ? parseWikiTaskDraftKey(loaded.value.payload.draftKey)
        : null;
      if (parsed) {
        stashWikiAskFocus(parsed.attemptId, parsed.repositoryCoordinate);
        if (parsed.projectId === "library") {
          navigate({ kind: "library" });
        } else {
          navigate({
            kind: "project",
            projectId: parsed.projectId,
            repositoryAddress: parsed.repositoryCoordinate,
          });
        }
        return;
      }
      throw new Error("not the dispatch author's record");
    } catch {
      // Other-viewer path: only the published coordinate can be followed.
      try {
        const projects = await resolveProjects();
        const project = projects.find((candidate) =>
          candidate.repositoryAddresses.includes(origin.coordinate),
        );
        if (project) {
          navigate({
            kind: "project",
            projectId: project.id,
            repositoryAddress: origin.coordinate,
          });
        } else {
          navigate({ kind: "library" });
        }
      } catch {
        navigate({ kind: "library" });
      }
    }
  }, [navigate, origin, resolveProjects]);

  if (!origin) return null;

  const repoD = origin.coordinate.split(":").slice(1).join(":");
  return (
    <div
      className="mb-1 flex min-w-0 items-center gap-1.5 pt-0.5 text-sm font-normal leading-4 text-muted-foreground/70"
      data-testid="wiki-task-origin"
    >
      <button
        className="min-w-0 truncate text-left text-primary hover:underline disabled:no-underline"
        data-testid="wiki-task-origin-link"
        disabled={state === "busy"}
        onClick={() => void openOrigin()}
        type="button"
      >
        From a private Wiki answer{repoD ? ` — ${repoD}` : ""}
      </button>
      {state === "failed" ? (
        <span className="text-2xs" data-testid="wiki-task-origin-failed">
          The origin is no longer readable.
        </span>
      ) : null}
    </div>
  );
}
