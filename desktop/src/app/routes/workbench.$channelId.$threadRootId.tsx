import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";
import { parseWorkbenchLens } from "@/features/workbench/lib/workbenchRoutes";

const WorkbenchScreen = React.lazy(async () => {
  const module = await import("@/features/workbench/ui/WorkbenchScreen");
  return { default: module.WorkbenchScreen };
});

export const Route = createFileRoute("/workbench/$channelId/$threadRootId")({
  component: WorkbenchThreadRouteComponent,
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

function WorkbenchThreadRouteComponent() {
  const { channelId, threadRootId } = Route.useParams();
  const { lens, office, messageId } = Route.useSearch();
  return (
    <React.Suspense fallback={<ViewLoadingFallback kind="projects" />}>
      <WorkbenchScreen
        channelId={channelId}
        lens={lens}
        messageId={messageId}
        office={office}
        threadRootId={threadRootId}
      />
    </React.Suspense>
  );
}
