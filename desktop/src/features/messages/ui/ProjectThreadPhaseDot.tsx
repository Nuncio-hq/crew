import type { ProjectThreadPhaseState } from "@/features/messages/lib/projectThreadMissionControl";
import { cn } from "@/shared/lib/cn";

export function ProjectThreadPhaseDot({
  phase,
}: {
  phase: ProjectThreadPhaseState;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-block h-1.5 w-1.5 shrink-0 rounded-full",
        phase === "complete" && "bg-success",
        phase === "active" &&
          "animate-pulse bg-blue-500 motion-reduce:animate-none",
        phase === "failed" && "bg-destructive",
        phase === "waiting-on-user" && "bg-attention",
        phase === "pending" && "bg-muted-foreground/40",
      )}
      data-phase={phase}
      data-testid="project-thread-phase-dot"
    />
  );
}
