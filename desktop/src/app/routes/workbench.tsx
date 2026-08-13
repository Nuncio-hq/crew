import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";
import { parseWorkbenchLens } from "@/features/workbench/lib/workbenchRoutes";

const WorkbenchScreen = React.lazy(async () => {
  const module = await import("@/features/workbench/ui/WorkbenchScreen");
  return { default: module.WorkbenchScreen };
});

export const Route = createFileRoute("/workbench")({
  component: WorkbenchRouteComponent,
  validateSearch: (search: Record<string, unknown>) => ({
    lens: parseWorkbenchLens(search.lens),
    office:
      search.office === "1" ||
      search.office === true ||
      search.office === "true",
    messageId:
      typeof search.messageId === "string" ? search.messageId : undefined,
  }),
});

function WorkbenchRouteComponent() {
  const { lens, office, messageId } = Route.useSearch();
  return (
    <React.Suspense fallback={<ViewLoadingFallback kind="projects" />}>
      <WorkbenchScreen lens={lens} messageId={messageId} office={office} />
    </React.Suspense>
  );
}
