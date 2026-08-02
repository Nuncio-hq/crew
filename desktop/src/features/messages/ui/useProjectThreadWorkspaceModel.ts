import * as React from "react";

import { useActiveAgentsForConversation } from "@/features/agents/activeAgentTurnsStore";
import { useProjectThreadWorkspace } from "@/features/agents/useProjectThreadWorkspace";
import { useProjectThreadGitHub } from "@/features/messages/lib/projectThreadGitHubStore";
import {
  buildProjectThreadAgentSteps,
  parseProjectThreadContext,
  type ProjectThreadAgentMention,
  type ProjectThreadAgentStep,
  type ProjectThreadContext,
} from "@/features/messages/lib/projectThreadWorkspace";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import type { ThreadPullRequest } from "@/shared/api/thread-workspace-types";
import type { ProjectThreadWorkspaceSnapshot } from "@/features/agents/projectThreadWorkspaceStore";

export type ProjectThreadWorkspaceModel = {
  activeName: string;
  context: ProjectThreadContext;
  conversationId: string | null;
  counts: { done: number; queued: number; working: number };
  pullRequest: ThreadPullRequest | null;
  refreshGitHub: () => Promise<void> | void;
  steps: ProjectThreadAgentStep[];
  target: {
    branch: string;
    repositoryPath: string;
    rootEventId: string;
  } | null;
  workspace: ProjectThreadWorkspaceSnapshot;
  workingPubkeys: string[];
};

/** Shared data for the sticky project-thread status bar and its drawers. */
export function useProjectThreadWorkspaceModel({
  agentMentions,
  profiles,
  replies,
  threadHead,
}: {
  agentMentions: readonly ProjectThreadAgentMention[];
  profiles?: UserProfileLookup;
  replies: readonly TimelineMessage[];
  threadHead: TimelineMessage;
}): ProjectThreadWorkspaceModel | null {
  const context = React.useMemo(
    () => parseProjectThreadContext(threadHead.body),
    [threadHead.body],
  );
  const workspace = useProjectThreadWorkspace(threadHead.id);
  const conversationId =
    workspace.status === "ready" || workspace.status === "error"
      ? workspace.conversationId
      : null;
  const activeAgentPubkeys = useActiveAgentsForConversation(conversationId);
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

  return {
    activeName,
    context,
    conversationId,
    counts,
    pullRequest,
    refreshGitHub,
    steps,
    target,
    workspace,
    workingPubkeys: activeAgentPubkeys.map(normalizePubkey),
  };
}
