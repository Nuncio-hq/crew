import { GitBranch } from "lucide-react";

import { useProjectWorktreeRegistry } from "@/features/agents/projectWorktreeRegistryStore";
import {
  aggregateGithubRollup,
  countManagedWorktrees,
  type GithubRollupCounts,
} from "@/features/channels/lib/worktreeBuckets";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

type ChannelWorktreesPillProps = {
  repositoryPath: string | null;
  onOpen: () => void;
};

type RollupSegment = {
  key: string;
  text: string;
  className: string;
};

export function ChannelWorktreesPill({
  repositoryPath,
  onOpen,
}: ChannelWorktreesPillProps) {
  const { snapshot } = useProjectWorktreeRegistry(repositoryPath);
  if (!repositoryPath || snapshot.status !== "ready") return null;

  const managed = countManagedWorktrees(snapshot.value.entries);
  if (managed === 0) return null;
  const github = snapshot.value.github;
  const rollup = aggregateGithubRollup(snapshot.value.entries);
  const segments =
    github === "available" ? rollupSegments(rollup) : ([] as RollupSegment[]);

  return (
    <Button
      className="h-6 gap-1 px-2 text-2xs font-medium text-muted-foreground"
      data-testid="channel-worktrees-pill"
      onClick={onOpen}
      size="sm"
      title="Manage project worktrees"
      type="button"
      variant="ghost"
    >
      <GitBranch className="h-3 w-3" />
      <span className="inline-flex min-w-0 flex-wrap items-center gap-x-1">
        <span>{managed} worktrees</span>
        {github !== "available" ? (
          <span>· PRs unavailable</span>
        ) : (
          segments.map((segment, index) => (
            <span
              className="inline-flex items-center gap-x-1"
              key={segment.key}
            >
              {index === 0 || segment.key === "issues-open" ? (
                <span className="text-muted-foreground/50">
                  {segment.key === "issues-open" &&
                  segments.some((s) => s.key.startsWith("pr-"))
                    ? "|"
                    : "·"}
                </span>
              ) : (
                <span className="text-muted-foreground/50">·</span>
              )}
              <span className={cn("tabular-nums", segment.className)}>
                {segment.text}
              </span>
            </span>
          ))
        )}
      </span>
    </Button>
  );
}

function rollupSegments(rollup: GithubRollupCounts): RollupSegment[] {
  const segments: RollupSegment[] = [];
  if (rollup.prOpen > 0) {
    segments.push({
      key: "pr-open",
      text: `${rollup.prOpen} open`,
      className: "text-blue-600 dark:text-blue-400",
    });
  }
  if (rollup.prDraft > 0) {
    segments.push({
      key: "pr-draft",
      text: `${rollup.prDraft} draft`,
      className: "text-muted-foreground",
    });
  }
  if (rollup.prMerged > 0) {
    segments.push({
      key: "pr-merged",
      text: `${rollup.prMerged} merged`,
      className: "text-merged",
    });
  }
  if (rollup.prClosed > 0) {
    segments.push({
      key: "pr-closed",
      text: `${rollup.prClosed} closed`,
      className: "text-muted-foreground",
    });
  }
  if (rollup.issuesOpen > 0) {
    segments.push({
      key: "issues-open",
      text: `◉ ${rollup.issuesOpen}`,
      className: "text-success",
    });
  }
  return segments;
}
