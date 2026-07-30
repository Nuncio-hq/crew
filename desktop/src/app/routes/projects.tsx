import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { usePreviewFeatureWarning } from "@/shared/features";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const ProjectsScreen = React.lazy(async () => {
  const module = await import("@/features/projects/ui/crew-projects-screen");
  return { default: module.CrewProjectsScreen };
});

export const Route = createFileRoute("/projects")({
  component: ProjectsRouteComponent,
});

function ProjectsRouteComponent() {
  usePreviewFeatureWarning("projects");
  return (
    <React.Suspense fallback={<ViewLoadingFallback kind="projects" />}>
      <ProjectsScreen />
    </React.Suspense>
  );
}
