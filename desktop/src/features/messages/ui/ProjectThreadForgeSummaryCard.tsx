import { setThreadViewMode } from "@/features/channels/lib/threadViewModePreference";
import { parseForgePullRequestUrl } from "@/features/messages/lib/parseForgePullRequestUrl";
import { setThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import type { ThreadPullRequest } from "@/shared/api/thread-workspace-types";
import { cn } from "@/shared/lib/cn";

import { CiCheckSummary, DiffStatSummary } from "./ci/CiPresentation";
import {
  forgeStateChipClass,
  forgeStateLabel,
  summarizeChecks,
  summaryStateFromThreadPullRequest,
  threadReviewDecisionLabel,
} from "./threadPrHub/forgeHubCopy";

export function openThreadForgeHubFromPullRequest(input: {
  branch: string | null;
  channelId: string | null;
  pullRequest: ThreadPullRequest;
  repositoryPath: string | null;
  rootEventId: string | null;
  worktreePath?: string | null;
}): void {
  const ref = parseForgePullRequestUrl(input.pullRequest.url);
  if (ref) {
    setThreadForgeHubSubject({
      kind: "pr",
      ...ref,
      repositoryPath: input.repositoryPath,
      worktreePath: input.worktreePath,
      branch: input.branch,
      channelId: input.channelId,
      rootEventId: input.rootEventId,
      source: "thread",
    });
  }
  setThreadViewMode("focus");
}

export function ProjectThreadForgeSummaryCard({
  channelId,
  pullRequest,
  repositoryPath,
  rootEventId,
  worktreePath,
  branch,
}: {
  channelId: string | null;
  pullRequest: ThreadPullRequest;
  repositoryPath: string | null;
  rootEventId: string | null;
  worktreePath?: string | null;
  branch: string | null;
}) {
  const state = summaryStateFromThreadPullRequest(pullRequest);
  const checks = summarizeChecks(
    pullRequest.checks.map((check) => check.state),
  );
  function openHub() {
    openThreadForgeHubFromPullRequest({
      branch,
      channelId,
      pullRequest,
      repositoryPath,
      rootEventId,
      worktreePath,
    });
  }
  return (
    <button
      className={cn(
        "w-full rounded-xl border border-border/60 bg-muted/20 px-3 py-2 text-left transition-colors hover:bg-muted/40",
      )}
      data-testid="thread-forge-summary-card"
      onClick={openHub}
      title="Open PR in Crew — Changes, Checks, merge"
      type="button"
    >
      <div className="flex items-center gap-2">
        <span
          className={cn(
            "inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-2xs font-semibold",
            forgeStateChipClass(state),
          )}
        >
          {forgeStateLabel(state)}
        </span>
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          #{pullRequest.number} {pullRequest.title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 font-mono text-2xs text-muted-foreground">
        <span>
          {pullRequest.headRefName} → {pullRequest.baseRefName}
        </span>
        <DiffStatSummary
          additions={pullRequest.additions}
          className="text-2xs"
          deletions={pullRequest.deletions}
          files={pullRequest.changedFiles}
        />
      </div>
      <div className="mt-1.5 flex flex-wrap items-center justify-between gap-2">
        <CiCheckSummary
          failed={checks.failed}
          passed={checks.passed}
          running={checks.running}
          total={pullRequest.checks.length}
        />
        <span className="text-2xs text-muted-foreground">
          {threadReviewDecisionLabel(pullRequest.reviewDecision)}
        </span>
      </div>
    </button>
  );
}
