import * as React from "react";

import type {
  ForgeCommit,
  ForgeDiffResult,
} from "@/shared/api/threadForgeTypes";
import type {
  ProjectRepoCommit,
  ProjectRepoDiff,
} from "@/shared/api/projectGitTypes";
import { ProjectCommitDetailPanel } from "@/features/projects/ui/ProjectCommitDetailPanel";
import { cn } from "@/shared/lib/cn";

import { formatIsoRelative } from "./forgeHubCopy";

export function ThreadPrHubCommits({
  commits,
  diff,
}: {
  commits: ForgeCommit[];
  diff: ForgeDiffResult | null;
  worktreePath?: string | null;
}) {
  const [selected, setSelected] = React.useState<string | null>(
    commits[0]?.oid ?? null,
  );
  const commit = commits.find((entry) => entry.oid === selected) ?? null;
  const repoDiff: ProjectRepoDiff | null = diff?.diff
    ? {
        files: diff.diff.files,
        additions: diff.diff.additions,
        deletions: diff.diff.deletions,
        commitBody: null,
      }
    : null;
  const mapped = commit ? toRepoCommit(commit) : null;

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1"
      data-testid="thread-pr-hub-commits"
    >
      <div className="w-56 shrink-0 overflow-y-auto border-r border-border/60">
        {commits.length === 0 ? (
          <p className="p-3 text-sm text-muted-foreground">No commits.</p>
        ) : (
          commits.map((entry) => (
            <button
              className={cn(
                "flex w-full flex-col items-start gap-0.5 px-2 py-1.5 text-left hover:bg-muted/50",
                entry.oid === selected ? "bg-muted" : null,
              )}
              key={entry.oid}
              onClick={() => setSelected(entry.oid)}
              type="button"
            >
              <span className="line-clamp-2 text-2xs font-medium">
                {entry.messageHeadline}
              </span>
              <span className="font-mono text-2xs text-muted-foreground">
                {entry.oid.slice(0, 7)} · {formatIsoRelative(entry.committedAt)}
              </span>
            </button>
          ))
        )}
      </div>
      <div className="min-w-0 flex-1 overflow-auto p-3">
        {mapped ? (
          <ProjectCommitDetailPanel
            commit={mapped}
            commitHash={mapped.hash}
            diff={repoDiff}
            diffError={diff?.message ?? null}
            diffLoading={false}
          />
        ) : (
          <p className="text-sm text-muted-foreground">Select a commit.</p>
        )}
      </div>
    </div>
  );
}

function toRepoCommit(commit: ForgeCommit): ProjectRepoCommit {
  const timestamp = Date.parse(commit.committedAt);
  return {
    hash: commit.oid,
    shortHash: commit.oid.slice(0, 7),
    authorName: commit.authorName ?? "",
    authorEmail: commit.authorEmail ?? "",
    timestamp: Number.isNaN(timestamp) ? 0 : Math.floor(timestamp / 1000),
    subject: commit.messageHeadline,
  };
}
