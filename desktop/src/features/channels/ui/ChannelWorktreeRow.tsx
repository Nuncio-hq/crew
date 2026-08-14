import { ChevronDown, ChevronRight, GitBranch } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as React from "react";
import { toast } from "sonner";

import { useProjectWorktreeDetails } from "@/features/agents/projectWorktreeDetailsStore";
import {
  canClearCacheWorktree,
  canReclaimWorktree,
  type WorktreeBucketItem,
} from "@/features/channels/lib/worktreeBuckets";
import { formatDiskBytes } from "@/features/channels/lib/worktreeDiskFormat";
import { projectThreadLabel } from "@/features/messages/lib/projectThreadLabel";
import { projectThreadStatusClassName } from "@/features/messages/lib/projectThreadGitHubStatus";
import {
  clearProjectWorktreeCache,
  previewProjectWorktreeReclaim,
} from "@/shared/api/agentControl";
import type {
  ProjectWorktreeReclaimPreview,
  RegistryPullRequest,
} from "@/shared/api/thread-workspace-types";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

type ChannelWorktreeRowProps = {
  item: WorktreeBucketItem;
  repositoryPath: string;
  channelId?: string | null;
  readonly: boolean;
  rootBody?: string | null;
  selected: boolean;
  activeRootIds: ReadonlySet<string>;
  onToggleSelect: (path: string) => void;
  onOpenThread?: (rootEventId: string) => void;
  onRemove?: (worktreePath: string) => void;
  onPrune?: () => void;
  onCacheCleared?: () => void;
};

export function ChannelWorktreeRow({
  item,
  repositoryPath,
  channelId = null,
  readonly,
  rootBody,
  selected,
  activeRootIds,
  onToggleSelect,
  onOpenThread,
  onRemove,
  onPrune,
  onCacheCleared,
}: ChannelWorktreeRowProps) {
  const { entry, orphanReason } = item;
  const [expanded, setExpanded] = React.useState(false);
  const [preview, setPreview] =
    React.useState<ProjectWorktreeReclaimPreview | null>(null);
  const [previewError, setPreviewError] = React.useState<string | null>(null);
  const [cacheBusy, setCacheBusy] = React.useState(false);
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
  // Presentation only — Rust revalidates before any destructive mutation.
  const canReclaim = canReclaimWorktree(item, { activeRootIds });
  const canClearCache = canClearCacheWorktree(item, { activeRootIds });
  const detailsBlockEvict =
    details.status === "ready" && details.value.hasIgnoredLocalState === true;
  const previewBlocksEvict = preview?.canEvict === false;
  const canSelect =
    canReclaim && !readonly && !detailsBlockEvict && !previewBlocksEvict;
  const cacheBytes =
    preview?.cacheCategories.reduce(
      (sum, category) => sum + (category.present ? category.bytes : 0),
      0,
    ) ?? 0;
  const presentCacheIds =
    preview?.cacheCategories
      .filter((category) => category.present)
      .map((category) => category.id) ?? [];

  React.useEffect(() => {
    if (!expanded || entry.kind !== "managed" || entry.prunable) {
      setPreview(null);
      setPreviewError(null);
      return;
    }
    let cancelled = false;
    void previewProjectWorktreeReclaim(
      repositoryPath,
      entry.worktreePath,
      channelId,
    )
      .then((value) => {
        if (!cancelled) {
          setPreview(value);
          setPreviewError(null);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setPreview(null);
          setPreviewError(
            error instanceof Error ? error.message : "Preview failed",
          );
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    expanded,
    entry.kind,
    entry.prunable,
    entry.worktreePath,
    repositoryPath,
    channelId,
  ]);

  const runClearCache = async () => {
    if (presentCacheIds.length === 0 || !channelId) return;
    setCacheBusy(true);
    try {
      const result = await clearProjectWorktreeCache(
        repositoryPath,
        entry.worktreePath,
        presentCacheIds,
        channelId,
      );
      const cleared = result.results.filter(
        (row) => row.status === "completed",
      ).length;
      const refused = result.results.length - cleared;
      toast.message(`${cleared} cache categories cleared · ${refused} refused`);
      onCacheCleared?.();
      const refreshed = await previewProjectWorktreeReclaim(
        repositoryPath,
        entry.worktreePath,
        channelId,
      );
      setPreview(refreshed);
    } catch {
      toast.error("Cache clear failed.");
    } finally {
      setCacheBusy(false);
    }
  };

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
            {item.bucket === "other-channel" ? " · from another channel" : ""}
            {orphanReason === "channel-unknown" ? " · channel unknown" : ""}
            {orphanReason === "unknown"
              ? " · no buzzThreadRoot · branch kept"
              : ""}
            {entry.prunable ? " · directory missing" : ""}
            {openPr ? ` · PR #${openPr.number}` : ""}
            {details.status === "ready" && details.value.dirty
              ? " · uncommitted changes"
              : ""}
            {details.status === "ready" && details.value.hasIgnoredLocalState
              ? " · ignored local files present"
              : ""}
          </p>
          {entry.pullRequests.length > 0 ? (
            <ul
              className="mt-1.5 space-y-0.5"
              data-testid="channel-worktree-pr-list"
            >
              {entry.pullRequests.map((pr) => (
                <li key={pr.number}>
                  <button
                    className={cn(
                      "inline-flex max-w-full items-center gap-1 truncate text-left text-2xs tabular-nums",
                      projectThreadStatusClassName(drawerPrTone(pr)),
                    )}
                    onClick={() => void openUrl(pr.url)}
                    title={pr.title}
                    type="button"
                  >
                    <span>#{pr.number}</span>
                    <span className="truncate font-normal text-muted-foreground">
                      {pr.title}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          {(entry.linkedIssues ?? []).length > 0 ? (
            <ul
              className="mt-1 space-y-0.5"
              data-testid="channel-worktree-issue-list"
            >
              {entry.linkedIssues.map((issue) => (
                <li key={issue.number}>
                  <button
                    className={cn(
                      "inline-flex max-w-full items-center gap-1 truncate text-left text-2xs tabular-nums",
                      issue.state.toLowerCase() === "open"
                        ? "text-success"
                        : "text-muted-foreground",
                    )}
                    onClick={() => void openUrl(issue.url)}
                    title={issue.title}
                    type="button"
                  >
                    <span aria-hidden="true">◉</span>
                    <span>#{issue.number}</span>
                    <span className="truncate font-normal text-muted-foreground">
                      {issue.title}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          {expanded ? (
            <div className="mt-2 space-y-1 text-2xs text-muted-foreground">
              <code className="block break-all rounded bg-muted/60 px-1.5 py-1">
                {entry.worktreePath}
              </code>
              {details.status === "ready" ? (
                <p>
                  {details.value.ahead} ahead · {details.value.behind} behind ·{" "}
                  {formatDiskBytes(details.value.diskBytes)}
                  {cacheBytes > 0
                    ? ` · ${formatDiskBytes(cacheBytes)} reclaimable cache`
                    : ""}
                </p>
              ) : details.status === "pending" ? (
                <p>Measuring…</p>
              ) : details.status === "error" ? (
                <p className="text-destructive">{details.message}</p>
              ) : null}
              {previewError ? (
                <p className="text-destructive">{previewError}</p>
              ) : null}
              {preview?.busy ? (
                <p>Busy — an agent holds this worktree.</p>
              ) : null}
              {preview?.canEvict === false && preview.evictionRefusal ? (
                <p>{preview.evictionRefusal}</p>
              ) : details.status === "ready" &&
                details.value.hasIgnoredLocalState ? (
                <p>
                  Ignored local files block Free local space. Clear generated
                  cache first, or review/remove local files before freeing the
                  checkout.
                </p>
              ) : null}
            </div>
          ) : null}
          <div className="mt-2 flex flex-wrap gap-1.5">
            {entry.rootEventId &&
            onOpenThread &&
            orphanReason !== "unknown" &&
            item.bucket !== "channel-unknown" &&
            item.bucket !== "other-channel" ? (
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
            {canClearCache && presentCacheIds.length > 0 && channelId ? (
              <Button
                className="h-6 px-2 text-2xs"
                disabled={
                  cacheBusy ||
                  Boolean(preview?.busy) ||
                  preview?.canClearCache === false
                }
                onClick={() => void runClearCache()}
                size="sm"
                type="button"
                variant="outline"
              >
                Clear generated cache
              </Button>
            ) : null}
            {canSelect && onRemove ? (
              <Button
                className="h-6 px-2 text-2xs text-destructive"
                onClick={() => onRemove(entry.worktreePath)}
                size="sm"
                type="button"
                variant="ghost"
              >
                Free local space
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

function drawerPrTone(
  pr: RegistryPullRequest,
): "open" | "draft" | "merged" | "closed" {
  const state = pr.state.toUpperCase();
  if (state === "MERGED") return "merged";
  if (state === "CLOSED") return "closed";
  if (pr.isDraft) return "draft";
  return "open";
}
