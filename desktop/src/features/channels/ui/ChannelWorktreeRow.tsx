import { ChevronDown, ChevronRight, GitBranch } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as React from "react";

import { useProjectWorktreeDetails } from "@/features/agents/projectWorktreeDetailsStore";
import type { WorktreeBucketItem } from "@/features/channels/lib/worktreeBuckets";
import { formatDiskBytes } from "@/features/channels/lib/worktreeDiskFormat";
import { projectThreadLabel } from "@/features/messages/lib/projectThreadLabel";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

type ChannelWorktreeRowProps = {
  item: WorktreeBucketItem;
  repositoryPath: string;
  readonly: boolean;
  rootBody?: string | null;
  selected: boolean;
  onToggleSelect: (path: string) => void;
  onOpenThread?: (rootEventId: string) => void;
  onRemove?: (worktreePath: string) => void;
  onPrune?: () => void;
};

export function ChannelWorktreeRow({
  item,
  repositoryPath,
  readonly,
  rootBody,
  selected,
  onToggleSelect,
  onOpenThread,
  onRemove,
  onPrune,
}: ChannelWorktreeRowProps) {
  const { entry, orphanReason } = item;
  const [expanded, setExpanded] = React.useState(false);
  const details = useProjectWorktreeDetails(
    repositoryPath,
    entry.worktreePath,
    expanded && entry.kind === "managed" && !entry.prunable,
  );
  const label =
    projectThreadLabel(rootBody) ??
    entry.worktreeName ??
    entry.branch ??
    "worktree";
  const openPr = entry.pullRequests.find(
    (pr) => pr.state.toUpperCase() === "OPEN" || pr.isDraft,
  );
  const size =
    details.status === "ready" ? formatDiskBytes(details.value.diskBytes) : "—";
  const canRemove =
    !readonly &&
    (item.bucket === "idle" || item.bucket === "orphan") &&
    !entry.prunable;
  const canSelect = canRemove;

  return (
    <div
      className={cn(
        "rounded-md border border-border/60 px-2.5 py-2",
        readonly && "opacity-70",
      )}
      data-testid="channel-worktree-row"
    >
      <div className="flex items-start gap-2">
        {canSelect ? (
          <input
            aria-label={`Select ${label}`}
            checked={selected}
            className="mt-1"
            onChange={() => onToggleSelect(entry.worktreePath)}
            type="checkbox"
          />
        ) : null}
        <button
          className="mt-0.5 text-muted-foreground"
          onClick={() => setExpanded((value) => !value)}
          type="button"
        >
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5" />
          )}
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <GitBranch className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            <span className="truncate text-sm font-medium" title={label}>
              {label}
            </span>
            <span className="ml-auto shrink-0 text-2xs text-muted-foreground">
              {size}
            </span>
          </div>
          <p className="mt-0.5 truncate text-2xs text-muted-foreground">
            {entry.branch ?? "detached"}
            {orphanReason === "other-channel" ? " · from another channel" : ""}
            {orphanReason === "unknown"
              ? " · no buzzThreadRoot · branch kept"
              : ""}
            {entry.prunable ? " · directory missing" : ""}
            {openPr ? ` · PR #${openPr.number}` : ""}
            {details.status === "ready" && details.value.dirty
              ? " · uncommitted changes"
              : ""}
          </p>
          {expanded ? (
            <div className="mt-2 space-y-1 text-2xs text-muted-foreground">
              <code className="block break-all rounded bg-muted/60 px-1.5 py-1">
                {entry.worktreePath}
              </code>
              {details.status === "ready" ? (
                <p>
                  {details.value.ahead} ahead · {details.value.behind} behind ·{" "}
                  {formatDiskBytes(details.value.diskBytes)}
                </p>
              ) : details.status === "pending" ? (
                <p>Measuring…</p>
              ) : details.status === "error" ? (
                <p className="text-destructive">{details.message}</p>
              ) : null}
            </div>
          ) : null}
          <div className="mt-2 flex flex-wrap gap-1.5">
            {entry.rootEventId && onOpenThread && orphanReason !== "unknown" ? (
              <Button
                onClick={() => {
                  const rootEventId = entry.rootEventId;
                  if (rootEventId) onOpenThread(rootEventId);
                }}
                size="sm"
                type="button"
                variant="outline"
                className="h-6 px-2 text-2xs"
              >
                Open thread
              </Button>
            ) : null}
            {openPr ? (
              <Button
                className="h-6 px-2 text-2xs"
                onClick={() => void openUrl(openPr.url)}
                size="sm"
                type="button"
                variant="outline"
              >
                Open PR
              </Button>
            ) : null}
            {item.bucket === "broken" && onPrune ? (
              <Button
                className="h-6 px-2 text-2xs"
                onClick={onPrune}
                size="sm"
                type="button"
                variant="outline"
              >
                Prune
              </Button>
            ) : null}
            {canRemove && onRemove ? (
              <Button
                className="h-6 px-2 text-2xs text-destructive"
                onClick={() => onRemove(entry.worktreePath)}
                size="sm"
                type="button"
                variant="ghost"
              >
                Remove worktree
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
