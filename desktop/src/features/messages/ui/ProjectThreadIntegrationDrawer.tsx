import { LoaderCircle, TriangleAlert, X } from "lucide-react";
import type { ReactNode } from "react";

import type { ProjectThreadWorkspaceSnapshot } from "@/features/agents/projectThreadWorkspaceStore";
import {
  isMissingFolderWorkspaceError,
  type ProjectThreadAgentStep,
  type ProjectThreadContext,
} from "@/features/messages/lib/projectThreadWorkspace";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { ThreadPullRequest } from "@/shared/api/thread-workspace-types";
import { Button } from "@/shared/ui/button";
import {
  ProjectThreadGitHubDetails,
  type ProjectThreadGitHubDrawer,
} from "./ProjectThreadGitHubDetails";
import { ProjectThreadHandoffDetails } from "./ProjectThreadHandoffDetails";
import { ProjectThreadTaskDetails } from "./ProjectThreadTaskDetails";
import { ProjectThreadWorkspaceDetails } from "./ProjectThreadWorkspaceDetails";

export type ProjectThreadDrawer =
  | "task"
  | "workspace"
  | "handoff"
  | ProjectThreadGitHubDrawer;

const TITLES: Record<ProjectThreadDrawer, string> = {
  task: "Task",
  workspace: "Workspace",
  handoff: "Handoff in this thread",
  issue: "Linked issue",
  pr: "Pull request",
  ci: "CI checks",
};

export function ProjectThreadIntegrationDrawer({
  active,
  context,
  onClose,
  onGitHubRefresh,
  onPickFolder,
  profiles,
  pullRequest,
  steps,
  target,
  workspace,
}: {
  active: ProjectThreadDrawer;
  context: ProjectThreadContext;
  onClose: () => void;
  onGitHubRefresh: () => Promise<void>;
  onPickFolder?: () => void;
  profiles?: UserProfileLookup;
  pullRequest: ThreadPullRequest | null;
  steps: readonly ProjectThreadAgentStep[];
  target: {
    branch: string;
    repositoryPath: string;
    rootEventId: string;
  } | null;
  workspace: ProjectThreadWorkspaceSnapshot;
}) {
  let content: ReactNode;
  if (active === "task") {
    content = (
      <ProjectThreadTaskDetails
        context={context}
        profiles={profiles}
        steps={steps}
      />
    );
  } else if (active === "handoff") {
    content = <ProjectThreadHandoffDetails profiles={profiles} steps={steps} />;
  } else if (active === "workspace") {
    content =
      workspace.status === "ready" ? (
        <ProjectThreadWorkspaceDetails workspace={workspace} />
      ) : workspace.status === "derived" ? (
        <div className="space-y-1 text-xs text-muted-foreground">
          <p className="font-machine text-foreground">{workspace.branch}</p>
          <p className="truncate font-machine" title={workspace.worktreePath}>
            {workspace.worktreePath}
          </p>
          <p>Restored from disk</p>
        </div>
      ) : workspace.status === "error" ? (
        <div className="space-y-2">
          <p className="flex items-center gap-2 text-xs text-destructive">
            <TriangleAlert className="h-4 w-4" /> {workspace.message}
          </p>
          {isMissingFolderWorkspaceError(workspace) ? (
            <>
              <p
                className="truncate font-machine text-xs text-muted-foreground"
                title={context.localPath}
              >
                {context.localPath}
              </p>
              {onPickFolder ? (
                <Button
                  onClick={onPickFolder}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  Pick folder
                </Button>
              ) : null}
            </>
          ) : null}
        </div>
      ) : (
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="h-4 w-4 animate-spin" />
          The harness is preparing this isolated worktree.
        </p>
      );
  } else {
    content =
      pullRequest && target ? (
        <ProjectThreadGitHubDetails
          drawer={active}
          onRefresh={onGitHubRefresh}
          pullRequest={pullRequest}
          target={target}
        />
      ) : null;
  }

  return (
    <div
      className="rounded-xl border border-border/60 bg-muted/20 p-3"
      data-testid="project-thread-integration-drawer"
    >
      <div className="mb-3 flex items-center justify-between">
        <p className="text-sm font-semibold">{TITLES[active]}</p>
        <Button
          aria-label="Close details"
          className="h-7 w-7"
          onClick={onClose}
          size="icon"
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
      {content}
    </div>
  );
}
