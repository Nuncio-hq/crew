import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const WikiLibraryScreen = React.lazy(async () => {
  const module = await import("@/features/wiki/ui/WikiLibraryScreen");
  return { default: module.WikiLibraryScreen };
});

export const Route = createFileRoute("/wiki")({
  component: WikiRouteComponent,
});

function WikiRouteComponent() {
  return (
    <React.Suspense fallback={<ViewLoadingFallback kind="projects" />}>
      <WikiLibraryScreen />
    </React.Suspense>
  );
}
