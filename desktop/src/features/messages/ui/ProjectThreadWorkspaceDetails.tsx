import { Check, Copy, FolderGit2, Trash2, TriangleAlert } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import type { ProjectThreadWorkspaceSnapshot } from "@/features/agents/projectThreadWorkspaceStore";
import {
  deleteThreadBranch,
  getThreadWorkspaceLifecycle,
  removeThreadWorktree,
} from "@/shared/api/agentControl";
import type {
  ThreadWorkspaceActionResult,
  ThreadWorkspaceLifecycle,
} from "@/shared/api/thread-workspace-types";
import { writeTextToClipboard } from "@/shared/lib/clipboard";
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

type ReadyWorkspace = Extract<
  ProjectThreadWorkspaceSnapshot,
  { status: "ready" }
>;

export function ProjectThreadWorkspaceDetails({
  workspace,
}: {
  workspace: ReadyWorkspace;
}) {
  const [lifecycle, setLifecycle] =
    React.useState<ThreadWorkspaceLifecycle | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [pendingAction, setPendingAction] = React.useState<{
    action: () => Promise<ThreadWorkspaceActionResult>;
    description: string;
    title: string;
  } | null>(null);
  const target = React.useMemo(
    () =>
      workspace.repositoryPath
        ? {
            branch: workspace.branch,
            repositoryPath: workspace.repositoryPath,
            rootEventId: workspace.rootEventId,
          }
        : null,
    [workspace],
  );
  const refresh = React.useCallback(async () => {
    if (!target) return;
    try {
      setLifecycle(
        await getThreadWorkspaceLifecycle({
          ...target,
          worktreePath: workspace.worktreePath,
        }),
      );
    } catch {
      setLifecycle(null);
    }
  }, [target, workspace.worktreePath]);
  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  const copyPath = async () => {
    try {
      await writeTextToClipboard(workspace.worktreePath);
      toast.success("Workspace path copied");
    } catch {
      toast.error("Could not copy workspace path");
    }
  };
  const run = async (action: () => Promise<ThreadWorkspaceActionResult>) => {
    setBusy(true);
    try {
      const result = await action();
      result.status === "completed"
        ? toast.success(result.message)
        : toast.error(result.message);
      await refresh();
    } catch {
      toast.error("The workspace action could not be completed.");
    } finally {
      setBusy(false);
    }
  };
  const behind =
    workspace.commitsBehindRemote && workspace.commitsBehindRemote > 0
      ? `${workspace.commitsBehindRemote} behind origin/${workspace.remoteDefaultBranch ?? "default"}`
      : null;
  const sourceLabel =
    workspace.baseSource === "local-fallback"
      ? "Local fallback"
      : behind
        ? "Remote base"
        : "Remote tip";

  return (
    <div className="space-y-3" data-testid="project-thread-workspace-details">
      <div className="flex items-center gap-2">
        <FolderGit2 className="h-4 w-4 text-muted-foreground" />
        <div className="min-w-0">
          <p className="text-sm font-semibold">Thread worktree</p>
          <p className="truncate text-xs text-muted-foreground">
            {workspace.branch}
          </p>
        </div>
      </div>
      <code className="block break-all rounded-lg bg-muted/70 p-2 text-xs">
        {workspace.worktreePath}
      </code>
      <dl className="grid grid-cols-[4.5rem_1fr] gap-x-2 gap-y-1.5 text-xs">
        <dt className="text-muted-foreground">Base</dt>
        <dd className="truncate font-mono">{workspace.baseRevision}</dd>
        <dt className="text-muted-foreground">Source</dt>
        <dd>{sourceLabel}</dd>
        <dt className="text-muted-foreground">Status</dt>
        <dd className="flex items-center gap-1 text-emerald-600 dark:text-emerald-400">
          <Check className="h-3.5 w-3.5" /> Ready
        </dd>
        <dt className="text-muted-foreground">Changes</dt>
        <dd>
          {lifecycle?.dirty === true
            ? "Uncommitted changes"
            : lifecycle?.dirty === false
              ? "Clean"
              : "Unavailable"}
        </dd>
      </dl>
      {behind ? (
        <p className="flex items-center gap-1.5 text-xs text-destructive">
          <TriangleAlert className="h-3.5 w-3.5" />
          {behind}
        </p>
      ) : null}
      <div className="grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-3">
        <Button
          className="min-w-0 w-full"
          onClick={copyPath}
          size="sm"
          type="button"
          variant="outline"
        >
          <Copy className="h-4 w-4 shrink-0" />
          <span className="truncate">Copy path</span>
        </Button>
        <Button
          className="min-w-0 w-full"
          disabled={!target || busy || lifecycle?.dirty !== false}
          onClick={() =>
            target &&
            setPendingAction({
              action: () =>
                removeThreadWorktree({
                  ...target,
                  worktreePath: workspace.worktreePath,
                }),
              description:
                "Remove this clean thread worktree? The branch will remain.",
              title: "Remove worktree?",
            })
          }
          size="sm"
          type="button"
          variant="destructive"
        >
          <Trash2 className="h-4 w-4 shrink-0" />
          <span className="truncate">Remove worktree</span>
        </Button>
        <Button
          className="min-w-0 w-full"
          disabled={
            !target ||
            busy ||
            lifecycle?.branchExists !== true ||
            lifecycle.branchCheckedOut
          }
          onClick={() =>
            target &&
            setPendingAction({
              action: () => deleteThreadBranch(target),
              description: "Delete this local and remote thread branch?",
              title: "Delete branch?",
            })
          }
          size="sm"
          type="button"
          variant="destructive"
        >
          <Trash2 className="h-4 w-4 shrink-0" />
          <span className="truncate">Delete branch</span>
        </Button>
      </div>
      {lifecycle?.dirty ? (
        <p className="text-2xs text-muted-foreground">
          Remove worktree stays disabled until every change is committed or
          discarded.
        </p>
      ) : null}
      <AlertDialog
        onOpenChange={(open) => {
          if (!open && !busy) setPendingAction(null);
        }}
        open={pendingAction !== null}
      >
        <AlertDialogContent data-testid="project-thread-workspace-confirm">
          <AlertDialogHeader>
            <AlertDialogTitle>{pendingAction?.title}</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingAction?.description}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
            <AlertDialogAction asChild>
              <Button
                data-testid="project-thread-workspace-confirm-action"
                disabled={busy}
                onClick={(event) => {
                  event.preventDefault();
                  const action = pendingAction?.action;
                  setPendingAction(null);
                  if (action) void run(action);
                }}
                type="button"
                variant="destructive"
              >
                {busy ? "Working…" : "Confirm"}
              </Button>
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
