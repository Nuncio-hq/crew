import { Hash } from "lucide-react";

export function WorkbenchEmptyState() {
  return (
    <div
      className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center"
      data-testid="workbench-empty"
    >
      <Hash className="h-8 w-8 text-muted-foreground/60" />
      <p className="text-base font-medium text-foreground">Thread Workbench</p>
      <p className="max-w-sm text-sm text-muted-foreground">
        Pick a thread from the rail. Same session the channel already holds —
        full screen, with a door back to the office.
      </p>
    </div>
  );
}
