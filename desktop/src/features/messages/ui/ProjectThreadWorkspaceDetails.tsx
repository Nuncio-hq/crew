import { Check, Copy, FolderGit2, GitBranch } from "lucide-react";
import { toast } from "sonner";

import type { ProjectThreadWorkspaceSnapshot } from "@/features/agents/projectThreadWorkspaceStore";
import { writeTextToClipboard } from "@/shared/lib/clipboard";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

type ReadyWorkspace = Extract<
  ProjectThreadWorkspaceSnapshot,
  { status: "ready" }
>;

export function ProjectThreadWorkspaceDetails({
  workspace,
}: {
  workspace: ReadyWorkspace;
}) {
  const copyPath = async () => {
    try {
      await writeTextToClipboard(workspace.worktreePath);
      toast.success("Workspace path copied");
    } catch {
      toast.error("Could not copy workspace path");
    }
  };

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          className="h-7 max-w-56 gap-1.5 rounded-full px-2.5 text-xs"
          data-testid="project-thread-workspace-chip"
          size="sm"
          type="button"
          variant="outline"
        >
          <GitBranch className="h-3.5 w-3.5" />
          <span className="truncate">{workspace.branch}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="w-80 space-y-3"
        data-testid="project-thread-workspace-details"
      >
        <div className="flex items-center gap-2">
          <FolderGit2 className="h-4 w-4 text-muted-foreground" />
          <div className="min-w-0">
            <p className="text-sm font-semibold">Thread worktree</p>
            <p className="text-xs text-muted-foreground">
              Created and verified by the agent harness
            </p>
          </div>
        </div>
        <code className="block break-all rounded-lg bg-muted/70 p-2 text-xs">
          {workspace.worktreePath}
        </code>
        <dl className="grid grid-cols-[4.5rem_1fr] gap-x-2 gap-y-1.5 text-xs">
          <dt className="text-muted-foreground">Name</dt>
          <dd className="truncate">{workspace.worktreeName}</dd>
          <dt className="text-muted-foreground">Base</dt>
          <dd className="truncate font-mono">{workspace.baseRevision}</dd>
          <dt className="text-muted-foreground">Branch</dt>
          <dd className="truncate font-mono">{workspace.branch}</dd>
          <dt className="text-muted-foreground">Status</dt>
          <dd className="flex items-center gap-1 text-emerald-600 dark:text-emerald-400">
            <Check className="h-3.5 w-3.5" />
            Ready
          </dd>
        </dl>
        <Button className="w-full gap-2" onClick={copyPath} type="button">
          <Copy className="h-4 w-4" />
          Copy path
        </Button>
      </PopoverContent>
    </Popover>
  );
}
