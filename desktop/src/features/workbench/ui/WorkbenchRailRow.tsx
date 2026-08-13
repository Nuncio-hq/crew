import { Moon } from "lucide-react";

import { UserAvatar } from "@/shared/ui/UserAvatar";
import { cn } from "@/shared/lib/cn";
import type {
  WorkbenchAgentStatus,
  WorkbenchThreadRow,
} from "../lib/workbenchThreadIndex";

function StatusDot({ status }: { status: WorkbenchAgentStatus }) {
  const tone =
    status === "needs-you"
      ? "bg-amber-400"
      : status === "working"
        ? "bg-emerald-500"
        : status === "failed"
          ? "bg-destructive"
          : status === "ready"
            ? "bg-sky-500"
            : status === "sleeping"
              ? "bg-indigo-400"
              : "bg-muted-foreground/40";
  return (
    <span
      className={cn("inline-flex h-2 w-2 shrink-0 rounded-full", tone)}
      data-testid={`workbench-status-${status}`}
    >
      {status === "sleeping" ? (
        <Moon className="h-2 w-2 text-indigo-700" />
      ) : null}
    </span>
  );
}

export function WorkbenchRailRow({
  onSelect,
  row,
  selected,
}: {
  onSelect: (row: WorkbenchThreadRow) => void;
  row: WorkbenchThreadRow;
  selected: boolean;
}) {
  return (
    <button
      className={cn(
        "flex w-full min-w-0 flex-col gap-0.5 rounded-md px-2 py-1.5 text-left hover:bg-muted/60",
        selected && "bg-muted",
      )}
      data-testid={`workbench-rail-row-${row.threadRootId}`}
      onClick={() => onSelect(row)}
      type="button"
    >
      <span className="flex min-w-0 items-center gap-1.5">
        <StatusDot status={row.status} />
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
          {row.title}
        </span>
        {row.unread ? (
          <span
            className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary"
            data-testid={`workbench-unread-${row.threadRootId}`}
          />
        ) : null}
      </span>
      <span className="flex min-w-0 items-center gap-1 pl-3.5">
        {row.agents.slice(0, 3).map((agent) => (
          <UserAvatar
            avatarUrl={null}
            className="h-4 w-4"
            displayName={agent.name}
            key={agent.pubkey}
            size="xs"
          />
        ))}
        <span className="truncate text-2xs text-muted-foreground">
          {row.agents.map((agent) => agent.name[0] ?? "?").join(" ") || "—"}
          {row.prNumber ? ` · PR #${row.prNumber}` : ""}
        </span>
      </span>
    </button>
  );
}
