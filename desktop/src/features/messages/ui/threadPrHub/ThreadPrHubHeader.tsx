import { RefreshCw } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import { mergeForgePr } from "@/shared/api/threadForge";
import { invalidateThreadForgePullRequestStore } from "@/features/messages/lib/threadForgePullRequestStore";
import type {
  ForgeDiffSource,
  ForgeMergeStrategy,
  ForgePullRequestDetail,
} from "@/shared/api/threadForgeTypes";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { cn } from "@/shared/lib/cn";

import {
  forgeStateChipClass,
  forgeStateLabel,
  truncateMiddle,
} from "./forgeHubCopy";

export function ThreadPrHubHeader({
  diffSource,
  onRefresh,
  owner,
  name,
  pr,
  refreshDisabled,
  refreshing,
  refreshedLabel,
}: {
  diffSource: ForgeDiffSource | null;
  onRefresh: () => void;
  owner: string;
  name: string;
  pr: ForgePullRequestDetail;
  refreshDisabled: boolean;
  refreshing: boolean;
  refreshedLabel: string;
}) {
  const [merging, setMerging] = React.useState(false);
  const canMerge = pr.state === "open" && pr.mergeStrategies.length > 0;

  async function merge(strategy: ForgeMergeStrategy) {
    setMerging(true);
    try {
      await mergeForgePr({
        owner,
        name,
        number: pr.number,
        strategy,
      });
      invalidateThreadForgePullRequestStore();
      onRefresh();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Could not merge.");
    } finally {
      setMerging(false);
    }
  }

  return (
    <div className="shrink-0 border-b border-border/60 px-3 py-2">
      <div className="flex items-start gap-2">
        <span
          className={cn(
            "mt-0.5 inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-2xs font-semibold",
            forgeStateChipClass(pr.state),
          )}
        >
          {forgeStateLabel(pr.state)}
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium" title={pr.title}>
            #{pr.number} {truncateMiddle(pr.title, 64)}
          </div>
          <div className="font-mono text-2xs text-muted-foreground">
            {pr.headRefName} → {pr.baseRefName}
            <span className="ml-2">
              +{pr.additions} −{pr.deletions} · {pr.changedFiles} files
            </span>
          </div>
        </div>
        {refreshing ? (
          <span
            className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-2xs text-muted-foreground"
            data-testid="thread-pr-hub-updating"
          >
            Updating…
          </span>
        ) : null}
        {canMerge ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                data-testid="thread-pr-hub-merge"
                disabled={merging}
                size="xs"
                type="button"
                variant="outline"
              >
                Merge
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {pr.mergeStrategies.map((strategy) => (
                <DropdownMenuItem
                  key={strategy}
                  onSelect={() => void merge(strategy)}
                >
                  {mergeLabel(strategy)}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
        <Button
          disabled={refreshDisabled || refreshing}
          onClick={onRefresh}
          size="xs"
          title={refreshedLabel}
          type="button"
          variant="ghost"
        >
          <RefreshCw
            className={cn("h-3.5 w-3.5", refreshing && "animate-spin")}
          />
          Refresh
        </Button>
      </div>
      {diffSource === "api" ? (
        <p
          className="mt-1 text-2xs text-muted-foreground"
          data-testid="thread-pr-hub-api-diff-banner"
        >
          Showing API diff — the local worktree is missing or this pull request
          is not checked out here.
        </p>
      ) : null}
    </div>
  );
}

function mergeLabel(strategy: ForgeMergeStrategy): string {
  switch (strategy) {
    case "merge":
      return "Create a merge commit";
    case "squash":
      return "Squash and merge";
    case "rebase":
      return "Rebase and merge";
    default: {
      const _exhaustive: never = strategy;
      return _exhaustive;
    }
  }
}
