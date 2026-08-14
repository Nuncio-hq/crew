import * as React from "react";
import { toast } from "sonner";

import { setForgeFileViewed } from "@/shared/api/threadForge";
import { invalidateThreadForgePullRequestStore } from "@/features/messages/lib/threadForgePullRequestStore";
import type { ThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import type {
  ForgeChangedFile,
  ForgeDiffResult,
  ForgePullRequestDetail,
} from "@/shared/api/threadForgeTypes";
import type { ProjectRepoDiff } from "@/shared/api/projectGitTypes";
import { DiffViewer } from "@/features/messages/ui/DiffViewer";
import { cn } from "@/shared/lib/cn";

export function ThreadPrHubChanges({
  diff,
  files,
  onRefresh,
  pr,
  refIdentity,
}: {
  diff: ForgeDiffResult | null;
  files: ForgeChangedFile[];
  onRefresh: () => void;
  pr: ForgePullRequestDetail;
  refIdentity: Extract<ThreadForgeHubSubject, { kind: "pr" }>;
}) {
  const [selected, setSelected] = React.useState<string | null>(
    files[0]?.path ?? null,
  );
  const diffFiles = diff?.diff?.files ?? [];
  const selectedDiff =
    diffFiles.find((file) => file.path === selected) ?? diffFiles[0] ?? null;
  const selectedMeta =
    files.find((file) => file.path === selected) ?? files[0] ?? null;

  async function toggleViewed(file: ForgeChangedFile, viewed: boolean) {
    try {
      await setForgeFileViewed({
        owner: refIdentity.owner,
        name: refIdentity.name,
        number: refIdentity.number,
        pullRequestId: pr.id,
        path: file.path,
        viewed,
      });
      invalidateThreadForgePullRequestStore();
      onRefresh();
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : "Could not update viewed state.",
      );
    }
  }

  const repoDiff: ProjectRepoDiff | null = diff?.diff
    ? {
        files: diff.diff.files,
        additions: diff.diff.additions,
        deletions: diff.diff.deletions,
        commitBody: null,
      }
    : null;

  return (
    <div className="flex min-h-0 flex-1" data-testid="thread-pr-hub-changes">
      <div className="w-48 shrink-0 overflow-y-auto border-r border-border/60">
        {files.map((file) => (
          <button
            className={cn(
              "flex w-full items-start gap-1 px-2 py-1 text-left text-2xs hover:bg-muted/50",
              file.path === selected ? "bg-muted" : null,
              file.viewedState === "viewed" ? "text-muted-foreground" : null,
            )}
            key={file.path}
            onClick={() => setSelected(file.path)}
            type="button"
          >
            <input
              checked={file.viewedState === "viewed"}
              className="mt-0.5"
              onChange={(event) => {
                event.stopPropagation();
                void toggleViewed(file, event.target.checked);
              }}
              onClick={(event) => event.stopPropagation()}
              type="checkbox"
            />
            <span className="min-w-0 flex-1 truncate font-mono">
              {file.path}
            </span>
            <span className="shrink-0 text-success">+{file.additions}</span>
            <span className="shrink-0 text-destructive">−{file.deletions}</span>
          </button>
        ))}
      </div>
      <div className="min-w-0 flex-1 overflow-auto">
        {selectedMeta ? (
          <div className="flex items-center justify-between border-b border-border/60 px-3 py-1">
            <span className="truncate font-mono text-2xs">
              {selectedMeta.path}
            </span>
            <button
              className="text-2xs text-muted-foreground hover:text-foreground"
              onClick={() =>
                void toggleViewed(
                  selectedMeta,
                  selectedMeta.viewedState !== "viewed",
                )
              }
              type="button"
            >
              {selectedMeta.viewedState === "viewed"
                ? "Mark as unviewed"
                : "Mark as viewed"}
            </button>
          </div>
        ) : null}
        {selectedDiff?.patch ? (
          <DiffViewer
            content={selectedDiff.patch}
            fallbackFilePath={selectedDiff.path}
          />
        ) : repoDiff && !selectedDiff ? (
          <p className="p-3 text-sm text-muted-foreground">No diff loaded.</p>
        ) : (
          <p className="p-3 text-sm text-muted-foreground">
            {selectedDiff?.truncated
              ? "This patch was truncated."
              : "No patch for this file (binary or empty)."}
          </p>
        )}
      </div>
    </div>
  );
}
