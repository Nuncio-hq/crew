import { Moon } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { formatCompactAge, truncateMiddle } from "../lib/workTreeFormat";
import type {
  WorkThreadRowModel,
  WorkThreadStatus,
} from "../lib/workTreeTypes";

const STATUS_LABEL: Record<WorkThreadStatus, string> = {
  "needs-you": "Needs you",
  sleeping: "Sleeping",
  working: "Working",
};

function StatusGlyph({ status }: { status: WorkThreadStatus }) {
  switch (status) {
    case "sleeping":
      return (
        <span
          className="inline-flex h-3 w-3 shrink-0 items-center justify-center text-3xs leading-none"
          data-testid="work-thread-status-sleeping"
          title={STATUS_LABEL[status]}
        >
          <Moon className="h-3 w-3 text-indigo-400" />
        </span>
      );
    case "needs-you":
    case "working": {
      const tone = status === "needs-you" ? "bg-amber-400" : "bg-emerald-500";
      return (
        <span
          className={cn("inline-flex h-2 w-2 shrink-0 rounded-full", tone)}
          data-testid={`work-thread-status-${status}`}
          title={STATUS_LABEL[status]}
        />
      );
    }
    default: {
      const _exhaustive: never = status;
      return _exhaustive;
    }
  }
}

function ciGlyphLabel(glyph: WorkThreadRowModel["ciGlyph"]): string {
  switch (glyph) {
    case "pass":
      return "✓";
    case "fail":
      return "✗";
    case "pending":
      return "●";
    case null:
      return "";
    default: {
      const _exhaustive: never = glyph;
      return _exhaustive;
    }
  }
}

function prToneClass(tone: WorkThreadRowModel["prTone"]): string {
  switch (tone) {
    case "merged":
      return "text-purple-600 dark:text-purple-400";
    case "closed":
      return "text-destructive";
    case "draft":
      return "text-muted-foreground";
    case "open":
      return "text-emerald-600 dark:text-emerald-400";
    case null:
      return "text-muted-foreground";
    default: {
      const _exhaustive: never = tone;
      return _exhaustive;
    }
  }
}

export function WorkThreadRow({
  now,
  onSelect,
  row,
  selected,
  testId,
}: {
  now: number;
  onSelect: (row: WorkThreadRowModel) => void;
  row: WorkThreadRowModel;
  selected: boolean;
  testId?: string;
}) {
  const age = formatCompactAge(now - row.lastActivityAt);
  return (
    <button
      className={cn(
        "flex w-full min-w-0 flex-col gap-0.5 rounded-md px-2 py-1 text-left hover:bg-sidebar-accent motion-safe:duration-150 motion-safe:animate-in motion-safe:fade-in motion-safe:slide-in-from-top-1",
        selected && "bg-sidebar-accent",
      )}
      data-testid={testId ?? `work-thread-row-${row.threadRootId}`}
      onClick={() => onSelect(row)}
      title={row.title}
      type="button"
    >
      <span className="flex min-w-0 items-center gap-1.5">
        <span className="min-w-0 flex-1 truncate font-medium text-sm text-sidebar-foreground">
          {truncateMiddle(row.title, 28)}
        </span>
        <StatusGlyph status={row.status} />
        {row.lastActivityAt > 0 ? (
          <span className="shrink-0 text-2xs tabular-nums text-muted-foreground">
            {age}
          </span>
        ) : null}
        {row.unread ? (
          <span
            className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary"
            data-testid={`work-thread-unread-${row.threadRootId}`}
          />
        ) : null}
      </span>
      {row.hasWorkspaceBinding ? (
        <span className="flex min-w-0 items-center gap-1 font-mono text-2xs text-muted-foreground">
          <span className="min-w-0 truncate">{row.branch ?? "—"}</span>
          {row.prNumber ? (
            <span className={cn("shrink-0", prToneClass(row.prTone))}>
              PR #{row.prNumber}
            </span>
          ) : null}
          {row.ciGlyph ? (
            <span className="shrink-0" data-testid="work-thread-ci">
              {ciGlyphLabel(row.ciGlyph)}
            </span>
          ) : null}
        </span>
      ) : null}
    </button>
  );
}
