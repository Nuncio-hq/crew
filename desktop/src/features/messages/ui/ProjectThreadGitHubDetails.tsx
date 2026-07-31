import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Check,
  CircleDot,
  ExternalLink,
  GitPullRequest,
  LoaderCircle,
  MessageSquare,
  X,
} from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import { closeThreadPullRequest } from "@/shared/api/agentControl";
import type {
  ThreadPullRequest,
  ThreadPullRequestCheck,
} from "@/shared/api/thread-workspace-types";
import { Button } from "@/shared/ui/button";

export type ProjectThreadGitHubDrawer = "issue" | "pr" | "ci";

function checkTone(check: ThreadPullRequestCheck) {
  const state = check.state.toUpperCase();
  if (["SUCCESS", "NEUTRAL", "SKIPPED"].includes(state)) {
    return "text-emerald-600 dark:text-emerald-400";
  }
  if (["FAILURE", "ERROR", "CANCELLED", "TIMED_OUT"].includes(state)) {
    return "text-destructive";
  }
  return "text-muted-foreground";
}

function IssueDetails({ pullRequest }: { pullRequest: ThreadPullRequest }) {
  const issues = pullRequest.closingIssuesReferences;
  if (issues.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        This pull request does not close a linked issue.
      </p>
    );
  }
  return (
    <div className="space-y-2">
      {issues.map((issue) => (
        <button
          className="flex w-full items-start gap-2 rounded-lg border border-border/60 p-2 text-left hover:bg-muted/50"
          key={issue.number}
          onClick={() => void openUrl(issue.url)}
          type="button"
        >
          <CircleDot className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1">
            <span className="block text-xs font-semibold">
              Issue #{issue.number}
            </span>
            <span className="block truncate text-xs text-muted-foreground">
              {issue.title}
            </span>
          </span>
          <ExternalLink className="h-3.5 w-3.5" />
        </button>
      ))}
    </div>
  );
}

function CiDetails({ checks }: { checks: ThreadPullRequestCheck[] }) {
  if (checks.length === 0) {
    return <p className="text-xs text-muted-foreground">No checks reported.</p>;
  }
  return (
    <div className="space-y-1.5">
      {checks.map((check) => (
        <button
          className="flex w-full items-center gap-2 rounded-lg border border-border/60 px-2 py-1.5 text-left disabled:cursor-default"
          disabled={!check.url}
          key={`${check.workflow ?? "check"}:${check.name}:${check.url ?? check.state}`}
          onClick={() => check.url && void openUrl(check.url)}
          type="button"
        >
          {checkTone(check).includes("emerald") ? (
            <Check className={`h-4 w-4 ${checkTone(check)}`} />
          ) : checkTone(check) === "text-destructive" ? (
            <X className="h-4 w-4 text-destructive" />
          ) : (
            <LoaderCircle className="h-4 w-4 text-muted-foreground" />
          )}
          <span className="min-w-0 flex-1 truncate text-xs font-medium">
            {check.name}
          </span>
          <span className={`text-2xs ${checkTone(check)}`}>{check.state}</span>
        </button>
      ))}
    </div>
  );
}

export function ProjectThreadGitHubDetails({
  drawer,
  onRefresh,
  pullRequest,
  target,
}: {
  drawer: ProjectThreadGitHubDrawer;
  onRefresh: () => Promise<void>;
  pullRequest: ThreadPullRequest;
  target: { branch: string; repositoryPath: string; rootEventId: string };
}) {
  const [busy, setBusy] = React.useState(false);
  if (drawer === "issue") return <IssueDetails pullRequest={pullRequest} />;
  if (drawer === "ci") return <CiDetails checks={pullRequest.checks} />;

  const close = async () => {
    if (!window.confirm(`Close pull request #${pullRequest.number}?`)) return;
    setBusy(true);
    try {
      const result = await closeThreadPullRequest(target);
      result.status === "completed"
        ? toast.success(result.message)
        : toast.error(result.message);
      await onRefresh();
    } catch {
      toast.error("The pull request could not be closed.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="space-y-3">
      <div className="flex items-start gap-2">
        <GitPullRequest className="mt-0.5 h-4 w-4 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-semibold">
            #{pullRequest.number} {pullRequest.title}
          </p>
          <p className="text-xs text-muted-foreground">
            {pullRequest.headRefName} → {pullRequest.baseRefName} · +
            {pullRequest.additions} −{pullRequest.deletions} ·{" "}
            {pullRequest.changedFiles} files
          </p>
        </div>
        <Button
          onClick={() => void openUrl(pullRequest.url)}
          size="sm"
          type="button"
          variant="outline"
        >
          <ExternalLink className="h-4 w-4" /> GitHub
        </Button>
      </div>
      <div className="space-y-2">
        <p className="flex items-center gap-1.5 text-xs font-semibold">
          <MessageSquare className="h-3.5 w-3.5" />
          Recent comments
        </p>
        {pullRequest.comments.length ? (
          pullRequest.comments.map((comment) => (
            <button
              className="block w-full rounded-lg border border-border/60 p-2 text-left hover:bg-muted/50"
              key={comment.url}
              onClick={() => void openUrl(comment.url)}
              type="button"
            >
              <span className="block text-2xs font-semibold">
                {comment.author?.login ?? "GitHub user"} · {comment.createdAt}
              </span>
              <span className="mt-1 block whitespace-pre-wrap text-xs text-muted-foreground">
                {comment.body}
              </span>
            </button>
          ))
        ) : (
          <p className="text-xs text-muted-foreground">No PR comments yet.</p>
        )}
      </div>
      <Button
        disabled={busy || pullRequest.state.toUpperCase() !== "OPEN"}
        onClick={() => void close()}
        size="sm"
        type="button"
        variant="destructive"
      >
        Close PR
      </Button>
    </div>
  );
}
