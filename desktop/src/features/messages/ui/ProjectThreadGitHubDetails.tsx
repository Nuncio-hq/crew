import { openUrl } from "@tauri-apps/plugin-opener";
import {
  CircleDot,
  ExternalLink,
  GitPullRequest,
  MessageSquare,
} from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import { closeThreadPullRequest } from "@/shared/api/agentControl";
import type {
  ThreadPullRequest,
  ThreadPullRequestCheck,
} from "@/shared/api/thread-workspace-types";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";
import { Badge } from "@/shared/ui/badge";
import {
  ciStatus,
  projectThreadStatusClassName,
  pullRequestStatus,
} from "@/features/messages/lib/projectThreadGitHubStatus";
import {
  CheckStatusDot,
  CheckStatusLabel,
  CiCheckSummary,
  DiffStatSummary,
} from "./ci/CiPresentation";
import { summarizeThreadChecks } from "./ci/checkPresentation";

export type ProjectThreadGitHubDrawer = "issue" | "pr" | "ci";

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
  const status = ciStatus(checks);
  const summary = summarizeThreadChecks(checks);
  if (checks.length === 0) {
    return (
      <div className="space-y-2">
        <Badge
          className={projectThreadStatusClassName(status.tone)}
          variant="secondary"
        >
          {status.label}
        </Badge>
        <p className="text-xs text-muted-foreground">No checks reported.</p>
      </div>
    );
  }
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <Badge
          className={projectThreadStatusClassName(status.tone)}
          variant="secondary"
        >
          {status.label}
        </Badge>
        <CiCheckSummary
          failed={summary.failed}
          passed={summary.passed}
          running={summary.running}
          total={checks.length}
        />
      </div>
      <p className="text-2xs text-muted-foreground">
        Each row is one GitHub check. Click a row to open its log on GitHub.
      </p>
      {checks.map((check) => (
        <button
          className="flex w-full items-center gap-2 rounded-lg border border-border/60 px-2 py-1.5 text-left disabled:cursor-default"
          disabled={!check.url}
          key={`${check.workflow ?? "check"}:${check.name}:${check.url ?? check.state}`}
          onClick={() => check.url && void openUrl(check.url)}
          title={check.url ? "Open check on GitHub" : undefined}
          type="button"
        >
          <CheckStatusDot state={check.state} />
          <span className="min-w-0 flex-1 truncate text-xs font-medium">
            {check.name}
          </span>
          {check.workflow ? (
            <span className="hidden max-w-24 truncate text-2xs text-muted-foreground [@container(min-width:24rem)]:inline">
              {check.workflow}
            </span>
          ) : null}
          <CheckStatusLabel state={check.state} />
          {check.url ? (
            <ExternalLink className="h-3 w-3 shrink-0 text-muted-foreground" />
          ) : null}
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
  const [confirmOpen, setConfirmOpen] = React.useState(false);
  if (drawer === "issue") return <IssueDetails pullRequest={pullRequest} />;
  if (drawer === "ci") return <CiDetails checks={pullRequest.checks} />;

  const close = async () => {
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
  const status = pullRequestStatus(pullRequest);
  return (
    <div className="space-y-3">
      <div className="flex items-start gap-2">
        <GitPullRequest className="mt-0.5 h-4 w-4 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <p className="min-w-0 truncate text-sm font-semibold">
              #{pullRequest.number} {pullRequest.title}
            </p>
            <Badge
              className={projectThreadStatusClassName(status.tone)}
              variant="secondary"
            >
              {status.label}
            </Badge>
          </div>
          <p className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
            <span>
              {pullRequest.headRefName} → {pullRequest.baseRefName}
            </span>
            <DiffStatSummary
              additions={pullRequest.additions}
              className="text-xs"
              deletions={pullRequest.deletions}
              files={pullRequest.changedFiles}
            />
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
        onClick={() => setConfirmOpen(true)}
        size="sm"
        type="button"
        variant="destructive"
      >
        Close PR
      </Button>
      <AlertDialog
        onOpenChange={(open) => {
          if (!busy) setConfirmOpen(open);
        }}
        open={confirmOpen}
      >
        <AlertDialogContent data-testid="project-thread-close-pr-confirm">
          <AlertDialogHeader>
            <AlertDialogTitle>Close pull request?</AlertDialogTitle>
            <AlertDialogDescription>
              Close pull request #{pullRequest.number}?
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
            <AlertDialogAction asChild>
              <Button
                data-testid="project-thread-close-pr-confirm-action"
                disabled={busy}
                onClick={(event) => {
                  event.preventDefault();
                  setConfirmOpen(false);
                  void close();
                }}
                type="button"
                variant="destructive"
              >
                {busy ? "Closing…" : "Close PR"}
              </Button>
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
