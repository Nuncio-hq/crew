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
import type {
  ThreadGitHubAvailability,
  ThreadPullRequest,
} from "@/shared/api/thread-workspace-types";
import type { ProjectThreadWorkspaceSnapshot } from "@/features/agents/projectThreadWorkspaceStore";

export type ProjectThreadWorkspaceModel = {
  activeName: string;
  activePubkey: string;
  context: ProjectThreadContext;
  conversationId: string | null;
  counts: { done: number; queued: number; working: number };
  githubAvailability: ThreadGitHubAvailability | null;
  githubDetail: string | null;
  pullRequest: ThreadPullRequest | null;
  refreshGitHub: () => Promise<void>;
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
  threadHead: TimelineMessage | null;
}): ProjectThreadWorkspaceModel | null {
  const context = React.useMemo(
    () => parseProjectThreadContext(threadHead?.body),
    [threadHead?.body],
  );
  // The repository path is what lets the hook fall back to the worktree
  // registry and report `derived`. It is an optional parameter, so omitting it
  // still compiles and still passes tests — it just silently disables that
  // fallback entirely. Keep it wired.
  const workspace = useProjectThreadWorkspace(
    threadHead?.id ?? null,
    context?.localPath ?? null,
  );
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
      (workspace.status === "ready" || workspace.status === "derived") &&
      workspace.repositoryPath
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

  const workingPubkeys = React.useMemo(
    () => activeAgentPubkeys.map(normalizePubkey),
    [activeAgentPubkeys],
  );

  const activeStep =
    steps.length > 0
      ? (steps.find((step) => step.status === "working") ?? steps[0])
      : null;
  const activeProfile = activeStep
    ? profiles?.[normalizePubkey(activeStep.pubkey)]
    : undefined;
  const activeName = activeStep
    ? (activeProfile?.displayName ??
      activeProfile?.name ??
      truncatePubkey(activeStep.pubkey))
    : "";
  const counts = React.useMemo(
    () => ({
      done: steps.filter((step) => step.status === "done").length,
      queued: steps.filter((step) => step.status === "queued").length,
      working: steps.filter((step) => step.status === "working").length,
    }),
    [steps],
  );
  const pullRequest =
    githubSnapshot.status === "ready" ? githubSnapshot.value.pullRequest : null;
  const githubAvailability =
    githubSnapshot.status === "ready"
      ? githubSnapshot.value.availability
      : null;
  const githubDetail =
    githubSnapshot.status === "ready"
      ? (githubSnapshot.value.detail ?? null)
      : null;

  // Fresh `{}` each render defeats every effect keyed on `model` (CLAUDE.md
  // gotcha #7). Memoize over the real inputs so drawer refresh effects stay
  // stable — see #34.
  return React.useMemo(() => {
    if (!context || !activeStep || steps.length === 0) return null;
    return {
      activeName,
      activePubkey: activeStep.pubkey,
      context,
      conversationId,
      counts,
      githubAvailability,
      githubDetail,
      pullRequest,
      refreshGitHub,
      steps,
      target,
      workspace,
      workingPubkeys,
    };
  }, [
    activeName,
    activeStep,
    context,
    conversationId,
    counts,
    githubAvailability,
    githubDetail,
    pullRequest,
    refreshGitHub,
    steps,
    target,
    workspace,
    workingPubkeys,
  ]);
}
