import { GitBranch, Route, Users } from "lucide-react";
import * as React from "react";

import { useActiveAgentsForConversation } from "@/features/agents/activeAgentTurnsStore";
import { useProjectThreadWorkspace } from "@/features/agents/useProjectThreadWorkspace";
import { useProjectThreadGitHub } from "@/features/messages/lib/projectThreadGitHubStore";
import {
  buildProjectThreadAgentSteps,
  parseProjectThreadContext,
  type ProjectThreadAgentMention,
} from "@/features/messages/lib/projectThreadWorkspace";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { useEscapeKey } from "@/shared/hooks/useEscapeKey";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { ProjectThreadIntegrationCell } from "./ProjectThreadIntegrationCell";
import {
  ProjectThreadIntegrationDrawer,
  type ProjectThreadDrawer,
} from "./ProjectThreadIntegrationDrawer";
import { ProjectThreadGitHubRow } from "./ProjectThreadGitHubRow";

type Props = {
  agentMentions: readonly ProjectThreadAgentMention[];
  profiles?: UserProfileLookup;
  replies: readonly TimelineMessage[];
  threadHead: TimelineMessage;
};

export function ProjectThreadWorkspacePanel({
  agentMentions,
  profiles,
  replies,
  threadHead,
}: Props) {
  const [activeDrawer, setActiveDrawer] =
    React.useState<ProjectThreadDrawer | null>(null);
  const context = React.useMemo(
    () => parseProjectThreadContext(threadHead.body),
    [threadHead.body],
  );
  const workspace = useProjectThreadWorkspace(threadHead.id);
  const activeAgentPubkeys = useActiveAgentsForConversation(
    workspace.status === "ready" || workspace.status === "error"
      ? workspace.conversationId
      : null,
  );
  const steps = React.useMemo(
    () =>
      buildProjectThreadAgentSteps({
        activeAgentPubkeys,
        agentMentions,
        replies,
      }),
    [activeAgentPubkeys, agentMentions, replies],
  );
  const target = React.useMemo(
    () =>
      workspace.status === "ready" && workspace.repositoryPath
        ? {
            branch: workspace.branch,
            repositoryPath: workspace.repositoryPath,
            rootEventId: workspace.rootEventId,
          }
        : null,
    [workspace],
  );
  const { refresh: refreshGitHub, snapshot: githubSnapshot } =
    useProjectThreadGitHub(target);
  const closeDrawer = React.useCallback(() => setActiveDrawer(null), []);
  useEscapeKey(closeDrawer, activeDrawer !== null);
  React.useEffect(() => {
    if (
      activeDrawer === "issue" ||
      activeDrawer === "pr" ||
      activeDrawer === "ci"
    ) {
      void refreshGitHub();
    }
  }, [activeDrawer, refreshGitHub]);

  if (!context || steps.length === 0) return null;
  const activeStep =
    steps.find((step) => step.status === "working") ?? steps[0];
  const activeProfile = profiles?.[normalizePubkey(activeStep.pubkey)];
  const activeName =
    activeProfile?.displayName ??
    activeProfile?.name ??
    truncatePubkey(activeStep.pubkey);
  const counts = {
    done: steps.filter((step) => step.status === "done").length,
    queued: steps.filter((step) => step.status === "queued").length,
    working: steps.filter((step) => step.status === "working").length,
  };
  const pullRequest =
    githubSnapshot.status === "ready" ? githubSnapshot.value.pullRequest : null;
  const toggle = (drawer: ProjectThreadDrawer) =>
    setActiveDrawer((current) => (current === drawer ? null : drawer));

  return (
    <section
      className="my-2 space-y-2"
      data-testid="project-thread-workspace-panel"
    >
      <div className="grid grid-cols-3 overflow-hidden rounded-xl border border-border/60 bg-muted/20 [&>*:not(:last-child)]:border-r">
        <ProjectThreadIntegrationCell
          active={activeDrawer === "task"}
          detail={`${context.repoAddress} · shared branch`}
          icon={<Users className="h-3.5 w-3.5" />}
          label="Task"
          onClick={() => toggle("task")}
          title={`${steps.length}-agent task`}
        />
        <ProjectThreadIntegrationCell
          active={activeDrawer === "workspace"}
          detail={
            workspace.status === "ready"
              ? workspace.baseSource === "remote"
                ? "Remote tip · ready"
                : `${workspace.commitsBehindRemote ?? "?"} behind remote`
              : workspace.status === "error"
                ? "Setup failed"
                : "Preparing"
          }
          icon={<GitBranch className="h-3.5 w-3.5" />}
          label="Workspace"
          onClick={() => toggle("workspace")}
          title={
            workspace.status === "ready" ? workspace.branch : "Shared worktree"
          }
        />
        <ProjectThreadIntegrationCell
          active={activeDrawer === "handoff"}
          detail={`${counts.done} done · ${counts.working} working · ${counts.queued} queued`}
          icon={<Route className="h-3.5 w-3.5" />}
          label="Handoff"
          onClick={() => toggle("handoff")}
          title={counts.working ? `${activeName} working` : "Thread crew"}
        />
      </div>

      {pullRequest ? (
        <ProjectThreadGitHubRow
          activeDrawer={activeDrawer}
          onToggle={toggle}
          pullRequest={pullRequest}
        />
      ) : null}

      {activeDrawer ? (
        <ProjectThreadIntegrationDrawer
          active={activeDrawer}
          context={context}
          onClose={closeDrawer}
          onGitHubRefresh={refreshGitHub}
          profiles={profiles}
          pullRequest={pullRequest}
          steps={steps}
          target={target}
          workspace={workspace}
        />
      ) : null}
    </section>
  );
}
