import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const OrgScreen = React.lazy(async () => {
  const module = await import("@/features/org/ui/OrgScreen");
  return { default: module.OrgScreen };
});

export const Route = createFileRoute("/org")({
  component: OrgRouteComponent,
});

function OrgRouteComponent() {
  return (
    <React.Suspense fallback={<ViewLoadingFallback kind="projects" />}>
      <OrgScreen />
    </React.Suspense>
  );
}
