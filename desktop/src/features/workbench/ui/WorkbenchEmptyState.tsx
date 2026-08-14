import { Hash } from "lucide-react";

import { PaneEmptyState } from "@/shared/ui/PaneEmptyState";

export function WorkbenchEmptyState() {
  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col items-center justify-center px-6"
      data-testid="workbench-empty"
    >
      <PaneEmptyState
        className="border-0 bg-transparent"
        description="Pick a thread from the rail. Same session the channel already holds — full screen, with a door back to the office."
        icon={<Hash className="h-8 w-8" />}
        narrowTitle="Pick a thread"
        title="Thread Workbench"
      />
    </div>
  );
}
