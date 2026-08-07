import { ChevronDown } from "lucide-react";
import type * as React from "react";

import type { ProjectThreadPhaseState } from "@/features/messages/lib/projectThreadMissionControl";
import { cn } from "@/shared/lib/cn";
import { ProjectThreadPhaseDot } from "./ProjectThreadPhaseDot";

export function ProjectThreadIntegrationCell({
  active,
  detail,
  icon,
  label,
  onClick,
  phase,
  statusClassName,
  title,
}: {
  active: boolean;
  detail: string;
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  phase?: ProjectThreadPhaseState;
  statusClassName?: string;
  title: string;
}) {
  return (
    <button
      aria-expanded={active}
      className={cn(
        "group min-h-16 min-w-0 border-border/50 px-2.5 py-2 text-left transition-colors hover:bg-muted/50",
        active && "bg-muted/60",
      )}
      onClick={onClick}
      type="button"
    >
      <span className="flex items-center gap-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground">
        {phase ? <ProjectThreadPhaseDot phase={phase} /> : null}
        {icon}
        {label}
        <ChevronDown
          className={cn(
            "ml-auto h-3 w-3 transition-transform motion-reduce:transition-none",
            active && "rotate-180",
          )}
        />
      </span>
      <span
        className={cn(
          "mt-1 block truncate text-xs font-semibold",
          statusClassName,
        )}
      >
        {title}
      </span>
      <span
        className={cn(
          "mt-0.5 block truncate text-2xs text-muted-foreground",
          statusClassName,
        )}
      >
        {detail}
      </span>
    </button>
  );
}
