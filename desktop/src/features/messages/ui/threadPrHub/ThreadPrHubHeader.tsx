import { ExternalLink, LoaderCircle, RefreshCw } from "lucide-react";
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

import { forgeStateChipClass, forgeStateLabel } from "./forgeHubCopy";

/**
 * Cursor-like PR chrome: Open badge · branch → base · title · Squash & Merge.
 */
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
  const preferred =
    pr.mergeStrategies.find((strategy) => strategy === "squash") ??
    pr.mergeStrategies[0] ??
    null;

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
    <div className="shrink-0 border-b border-border/60 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <span
          className={cn(
            "inline-flex shrink-0 items-center rounded-md px-1.5 py-0.5 text-2xs font-semibold",
            forgeStateChipClass(pr.state),
          )}
        >
          {forgeStateLabel(pr.state)}
        </span>
        <span
          className="min-w-0 flex-1 truncate font-mono text-2xs text-muted-foreground"
          title={`${owner}/${name}`}
        >
          <span className="text-foreground/80">{pr.headRefName}</span>
          <span className="mx-1 text-muted-foreground/50">→</span>
          <span>{pr.baseRefName}</span>
        </span>
        <a
          className="shrink-0 text-muted-foreground hover:text-foreground"
          href={pr.url}
          rel="noreferrer"
          target="_blank"
          title="Open on GitHub"
        >
          <ExternalLink className="h-3.5 w-3.5" />
        </a>
        {canMerge && preferred ? (
          <div className="flex shrink-0 items-center">
            <Button
              className="rounded-r-none"
              data-testid="thread-pr-hub-merge"
              disabled={merging}
              onClick={() => void merge(preferred)}
              size="xs"
              type="button"
            >
              {mergeLabel(preferred)}
            </Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  className="rounded-l-none border-l border-primary-foreground/20 px-1.5"
                  disabled={merging}
                  size="xs"
                  type="button"
                >
                  ▾
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
          </div>
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
        </Button>
      </div>
      <h2 className="mt-1.5 text-base font-semibold leading-snug text-foreground">
        {pr.title}{" "}
        <span className="font-normal text-muted-foreground">#{pr.number}</span>
      </h2>
      <p className="mt-0.5 text-2xs text-muted-foreground">
        <span className="text-success">+{pr.additions}</span>{" "}
        <span className="text-destructive">−{pr.deletions}</span>
        <span className="mx-1.5 text-muted-foreground/40">·</span>
        {pr.changedFiles} files
      </p>
      {diffSource === "api" ? (
        <p
          className="mt-1 text-2xs text-muted-foreground"
          data-testid="thread-pr-hub-api-diff-banner"
        >
          Showing API diff — worktree missing or PR not checked out here.
        </p>
      ) : null}
      {refreshing ? (
        <p
          className="mt-1 inline-flex items-center gap-1 text-2xs text-muted-foreground"
          data-testid="thread-pr-hub-updating"
        >
          <LoaderCircle className="h-3 w-3 animate-spin" />
          Updating…
        </p>
      ) : null}
    </div>
  );
}

function mergeLabel(strategy: ForgeMergeStrategy): string {
  switch (strategy) {
    case "merge":
      return "Merge";
    case "squash":
      return "Squash & Merge";
    case "rebase":
      return "Rebase & Merge";
    default: {
      const _exhaustive: never = strategy;
      return _exhaustive;
    }
  }
}
