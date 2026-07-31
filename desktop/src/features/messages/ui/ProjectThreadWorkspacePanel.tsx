import {
  Check,
  Clock3,
  GitBranch,
  LoaderCircle,
  TriangleAlert,
} from "lucide-react";
import * as React from "react";

import { useActiveAgentsForConversation } from "@/features/agents/activeAgentTurnsStore";
import { useProjectThreadWorkspace } from "@/features/agents/useProjectThreadWorkspace";
import {
  buildProjectThreadAgentSteps,
  parseProjectThreadContext,
} from "@/features/messages/lib/projectThreadWorkspace";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import { ProjectThreadWorkspaceDetails } from "./ProjectThreadWorkspaceDetails";

type Props = {
  agentPubkeys: readonly string[];
  profiles?: UserProfileLookup;
  replies: readonly TimelineMessage[];
  threadHead: TimelineMessage;
};

function AgentStatus({ status }: { status: "queued" | "working" | "done" }) {
  if (status === "done") {
    return (
      <span className="flex items-center gap-1 text-xs text-emerald-600 dark:text-emerald-400">
        <Check className="h-3.5 w-3.5" /> Done
      </span>
    );
  }
  if (status === "working") {
    return (
      <span className="flex items-center gap-1 text-xs text-emerald-600 dark:text-emerald-400">
        <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-current" />
        Working
      </span>
    );
  }
  return (
    <span className="flex items-center gap-1 text-xs text-muted-foreground">
      <Clock3 className="h-3.5 w-3.5" /> Queued
    </span>
  );
}

export function ProjectThreadWorkspacePanel({
  agentPubkeys,
  profiles,
  replies,
  threadHead,
}: Props) {
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
        agentPubkeys,
        replies,
      }),
    [activeAgentPubkeys, agentPubkeys, replies],
  );
  if (!context || steps.length === 0) return null;

  const activeStep =
    steps.find((step) => step.status === "working") ?? steps[0];
  const activeProfile = profiles?.[normalizePubkey(activeStep.pubkey)];
  const activeName =
    activeProfile?.displayName ??
    activeProfile?.name ??
    truncatePubkey(activeStep.pubkey);

  return (
    <section
      className="my-2 space-y-2.5"
      data-testid="project-thread-workspace-panel"
    >
      <div className="flex items-center justify-between gap-3 rounded-xl border border-border/60 bg-muted/25 px-3 py-2">
        <div className="flex min-w-0 items-center gap-2">
          <div className="flex -space-x-1.5">
            {steps.slice(0, 3).map((step) => {
              const profile = profiles?.[step.pubkey];
              const name =
                profile?.displayName ??
                profile?.name ??
                truncatePubkey(step.pubkey);
              return (
                <UserAvatar
                  avatarUrl={profile?.avatarUrl ?? null}
                  className="ring-2 ring-background"
                  displayName={name}
                  key={step.pubkey}
                  size="sm"
                />
              );
            })}
          </div>
          <div className="min-w-0">
            <p className="text-sm font-semibold">{steps.length}-agent task</p>
            <p className="truncate text-xs text-muted-foreground">
              {workspace.status === "pending"
                ? `${activeName} · preparing workspace`
                : workspace.status === "error"
                  ? `${activeName} · workspace setup failed`
                  : `${activeName} · shared thread worktree`}
            </p>
          </div>
        </div>
        {workspace.status === "ready" ? (
          <ProjectThreadWorkspaceDetails workspace={workspace} />
        ) : workspace.status === "error" ? (
          <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-destructive/30 px-2.5 py-1 text-xs text-destructive">
            <TriangleAlert className="h-3.5 w-3.5" />
            Failed
          </span>
        ) : (
          <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-border/60 px-2.5 py-1 text-xs text-muted-foreground">
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            Preparing
          </span>
        )}
      </div>

      {workspace.status === "ready" ? (
        <div className="flex gap-2.5 rounded-xl border border-emerald-500/25 bg-emerald-500/10 px-3 py-2.5">
          <Check className="mt-0.5 h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
          <div className="min-w-0">
            <p className="text-sm font-semibold text-emerald-700 dark:text-emerald-300">
              Shared workspace ready
            </p>
            <p className="text-xs leading-relaxed text-muted-foreground">
              {workspace.branch} · all agents in this thread use this worktree
            </p>
          </div>
        </div>
      ) : workspace.status === "error" ? (
        <div className="flex gap-2.5 rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2.5">
          <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0 text-destructive" />
          <div>
            <p className="text-sm font-semibold">Workspace setup failed</p>
            <p className="text-xs text-muted-foreground">{workspace.message}</p>
          </div>
        </div>
      ) : (
        <div className="flex gap-2.5 rounded-xl border border-border/60 px-3 py-2.5">
          <GitBranch className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
          <div>
            <p className="text-sm font-semibold">
              Preparing isolated workspace
            </p>
            <p className="text-xs text-muted-foreground">
              Agents start after the harness creates and verifies this thread’s
              worktree.
            </p>
          </div>
        </div>
      )}

      <div className="overflow-hidden rounded-xl border border-border/60">
        <div className="flex items-center justify-between border-b border-border/50 px-3 py-2">
          <p className="text-sm font-semibold">Handoff in this thread</p>
          <span className="text-xs text-muted-foreground">1 worktree</span>
        </div>
        {steps.map((step, index) => {
          const profile = profiles?.[step.pubkey];
          const name =
            profile?.displayName ??
            profile?.name ??
            truncatePubkey(step.pubkey);
          return (
            <div
              className="flex items-center gap-2.5 border-b border-border/40 px-3 py-2 last:border-b-0"
              key={step.pubkey}
            >
              <span className="w-4 text-center text-xs text-muted-foreground">
                {index + 1}
              </span>
              <UserAvatar
                avatarUrl={profile?.avatarUrl ?? null}
                displayName={name}
                size="sm"
              />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {name}
              </span>
              <AgentStatus status={step.status} />
            </div>
          );
        })}
      </div>
    </section>
  );
}
