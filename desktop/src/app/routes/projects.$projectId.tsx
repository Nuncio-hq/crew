import * as React from "react";
import { createFileRoute, useLocation } from "@tanstack/react-router";

import { parseProjectDetailSearch } from "@/features/projects/lib/projectDetailSearch";
import { usePreviewFeatureWarning } from "@/shared/features";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const ProjectDetailScreen = React.lazy(async () => {
  const module = await import("@/features/projects/ui/CrewProjectDetailGate");
  return { default: module.CrewProjectDetailGate };
});

export const Route = createFileRoute("/projects/$projectId")({
  component: ProjectDetailRouteComponent,
  validateSearch: (search: Record<string, unknown>) => ({
    ...parseProjectDetailSearch(search),
    tab: typeof search.tab === "string" ? search.tab : undefined,
    thread: typeof search.thread === "string" ? search.thread : undefined,
  }),
});

function ProjectDetailRouteComponent() {
  usePreviewFeatureWarning("projects");
  const { projectId } = Route.useParams();
  const {
    commitHash,
    filePath,
    pullRequestId,
    issueId,
    repositoryId,
    tab,
    thread,
  } = Route.useSearch();
  const entityNavigationId = useLocation({
    select: (location) => {
      const value = (
        location.state as { entityNavigationId?: unknown } | undefined
      )?.entityNavigationId;
      return typeof value === "string" ? value : undefined;
    },
  });

  return (
    <React.Suspense fallback={<ViewLoadingFallback kind="projects" />}>
      <ProjectDetailScreen
        commitHash={commitHash}
        entityNavigationId={entityNavigationId}
        filePath={filePath}
        issueId={issueId}
        projectId={projectId}
        pullRequestId={pullRequestId}
        repositoryId={repositoryId}
        tab={tab}
        thread={thread}
      />
    </React.Suspense>
  );
}
