import { ChevronDown, ChevronUp, GitBranch, Route, Users } from "lucide-react";
import * as React from "react";

import {
  getActiveTurnActivityBounds,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import { deriveAgentAttention } from "@/features/agents/agentAttention";
import {
  getAgentAttentionSnoozeGeneration,
  getAgentAttentionSnoozedUntil,
  snoozeAgentAttention,
  subscribeAgentAttentionSnoozes,
} from "@/features/agents/agentAttentionSnoozeStore";
import { useAgentObserverConnectionState } from "@/features/agents/useAgentObserverConnectionState";
import { AGENT_ACTIVITY_CHROME } from "@/features/agents/ui/agentActivityChrome";
import { formatElapsed } from "@/features/agents/ui/agentSessionUtils";
import { useComposerAgentStop } from "@/features/channels/ui/useComposerAgentStop";
import { useNeedsYouForConversation } from "@/features/agents/needsYouStore";
import { useSharedNowWhen } from "@/features/agents/lib/sharedNow";

import {
  deriveProjectThreadPhaseStates,
  type ProjectThreadPhaseState,
} from "@/features/messages/lib/projectThreadMissionControl";

import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { useEscapeKey } from "@/shared/hooks/useEscapeKey";
import { cn } from "@/shared/lib/cn";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { ProjectThreadIntegrationCell } from "./ProjectThreadIntegrationCell";
import {
  ProjectThreadIntegrationDrawer,
  type ProjectThreadDrawer,
} from "./ProjectThreadIntegrationDrawer";
import { ProjectThreadGitHubRow } from "./ProjectThreadGitHubRow";
import { ProjectThreadPhaseDot } from "./ProjectThreadPhaseDot";
import type { ProjectThreadWorkspaceModel } from "./useProjectThreadWorkspaceModel";

type Props = {
  channelId: string | null;
  /** When true, the expanded grid uses the wide 3-column layout. */
  isFocusMode?: boolean;
  profiles?: UserProfileLookup;
  model: ProjectThreadWorkspaceModel | null;
};

function ChipButton({
  label,
  onClick,
  phase,
  title,
}: {
  label: string;
  onClick: () => void;
  phase?: ProjectThreadPhaseState;
  title?: string;
}) {
  return (
    <button
      className="inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
      onClick={onClick}
      title={title}
      type="button"
    >
      {phase ? <ProjectThreadPhaseDot phase={phase} /> : null}
      {label}
    </button>
  );
}

function githubDegradedTitle(
  availability: "cli-missing" | "cli-failed",
): string {
  return availability === "cli-missing"
    ? "GitHub CLI (gh) not found"
    : "GitHub CLI could not read this repo";
}

/**
 * Sticky project-thread status bar. Mount **outside** the scroll region
 * (between header and message list). Collapsed by default; expand reveals the
 * Task/Workspace/Handoff grid (stacked in the side panel, 3-col in focus).
 * Owns the agent working signal for project threads.
 */
export function ProjectThreadWorkspacePanel({
  channelId,
  isFocusMode = false,
  profiles,
  model,
}: Props) {
  const [activeDrawer, setActiveDrawer] =
    React.useState<ProjectThreadDrawer | null>(null);
  const [expanded, setExpanded] = React.useState(false);
  const [stopping, setStopping] = React.useState(false);

  const closeDrawer = React.useCallback(() => setActiveDrawer(null), []);
  useEscapeKey(closeDrawer, activeDrawer !== null);

  const refreshGitHub = model?.refreshGitHub;
  React.useEffect(() => {
    if (
      activeDrawer === "issue" ||
      activeDrawer === "pr" ||
      activeDrawer === "ci"
    ) {
      void refreshGitHub?.();
    }
  }, [activeDrawer, refreshGitHub]);

  const workingPubkeys = model?.workingPubkeys ?? [];
  const now = useSharedNowWhen(workingPubkeys.length > 0);
  const conversationId = model?.conversationId ?? null;
  // Real source for the amber "waiting on user" handoff phase: pending
  // approval requests (kind 46010) blocking this conversation (#74 store).
  const needsYou = useNeedsYouForConversation(conversationId);
  const { hasStoppableWork, stopAgent } = useComposerAgentStop({
    channelId,
    conversationId,
  });
  const stoppableCrew = React.useMemo(() => {
    const seen = new Set<string>();
    const crew: Array<{ name: string; pubkey: string }> = [];
    for (const pubkey of workingPubkeys) {
      const key = normalizePubkey(pubkey);
      if (seen.has(key) || !hasStoppableWork(pubkey)) continue;
      seen.add(key);
      const profile = profiles?.[key];
      crew.push({
        pubkey,
        name: profile?.displayName ?? profile?.name ?? truncatePubkey(pubkey),
      });
    }
    for (const step of model?.steps ?? []) {
      if (step.status !== "working" && step.status !== "queued") continue;
      const key = normalizePubkey(step.pubkey);
      if (seen.has(key) || !hasStoppableWork(step.pubkey)) continue;
      seen.add(key);
      const profile = profiles?.[key];
      crew.push({
        pubkey: step.pubkey,
        name:
          profile?.displayName ?? profile?.name ?? truncatePubkey(step.pubkey),
      });
    }
    return crew;
  }, [hasStoppableWork, model?.steps, profiles, workingPubkeys]);
  const activityBounds = useActiveTurnActivityBounds(
    workingPubkeys,
    channelId,
    conversationId,
  );
  const connectionState = useAgentObserverConnectionState(workingPubkeys);
  React.useSyncExternalStore(
    subscribeAgentAttentionSnoozes,
    getAgentAttentionSnoozeGeneration,
    getAgentAttentionSnoozeGeneration,
  );

  if (!model) return null;

  const {
    activeName,
    context,
    counts,
    githubAvailability,
    pullRequest,
    steps,
    target,
    workspace,
  } = model;
  const showExpanded = expanded;
  const elapsedMs = activityBounds
    ? Math.max(0, now - activityBounds.anchorAt)
    : 0;
  const attention = activityBounds
    ? deriveAgentAttention({
        connectionState,
        needsYou: needsYou.length > 0,
        now,
        outcome: null,
        receipt: null,
        snoozedUntil: conversationId
          ? getAgentAttentionSnoozedUntil(conversationId)
          : 0,
        turns: [
          {
            agentPubkey: workingPubkeys[0] ?? "",
            ...activityBounds,
          },
        ],
      })
    : null;
  const stuck = attention?.state === "possibly-stalled";
  const stepLabel = `${counts.done + counts.working}/${steps.length}`;
  const agentLabel =
    counts.working > 1
      ? AGENT_ACTIVITY_CHROME.agentsWorking(counts.working)
      : counts.working === 1
        ? `${activeName} · ${stepLabel}`
        : `${activeName} · ${stepLabel}`;
  const timeLabel = activityBounds
    ? attention?.state === "needs-you"
      ? "Needs you"
      : attention?.state === "lost-contact"
        ? "Lost contact"
        : attention?.state === "telemetry-unavailable"
          ? "Telemetry unavailable"
          : stuck
            ? "Possibly stalled"
            : formatElapsed(elapsedMs)
    : null;

  const phases = deriveProjectThreadPhaseStates({
    hasThread: true,
    pullRequest,
    steps,
    // Real source for the amber handoff state: pending approval requests
    // (kind 46010) blocking this conversation, from #74's needs-you store.
    waitingOnUser: needsYou.length > 0,
    workspace,
  });

  const toggle = (drawer: ProjectThreadDrawer) =>
    setActiveDrawer((current) => (current === drawer ? null : drawer));

  const handleStop = async (event: React.MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    if (stopping || stoppableCrew.length === 0) return;
    setStopping(true);
    try {
      // Same cancel path as the composer activity rail (#24): in-flight turns
      // and queued/held requests, with toast feedback per agent.
      await Promise.all(
        stoppableCrew.map((agent) => stopAgent(agent.pubkey, agent.name)),
      );
    } finally {
      setStopping(false);
    }
  };

  return (
    <section
      className="shrink-0 border-b border-border/50 bg-background px-3 py-1.5"
      data-expanded={showExpanded ? "true" : "false"}
      data-testid="project-thread-workspace-panel"
    >
      <div className="flex min-w-0 items-center gap-1.5">
        <div
          className={cn(
            "flex min-w-0 flex-1 items-center gap-1.5 text-xs",
            stuck ? "text-amber-400" : "text-muted-foreground",
          )}
          data-testid="project-thread-status-summary"
        >
          <span
            aria-hidden
            className={cn(
              "inline-block h-1.5 w-1.5 shrink-0 rounded-full",
              counts.working > 0
                ? stuck
                  ? "bg-amber-400"
                  : "animate-pulse bg-emerald-400 motion-reduce:animate-none"
                : "bg-muted-foreground/40",
            )}
          />
          <span className="min-w-0 truncate font-medium text-foreground/90">
            {agentLabel}
          </span>
          {timeLabel ? (
            <span className="shrink-0 tabular-nums">· {timeLabel}</span>
          ) : null}
        </div>

        <div className="hidden min-w-0 items-center gap-0.5 sm:flex">
          <ChipButton
            label="Task"
            onClick={() => toggle("task")}
            phase={phases.task}
          />
          <ChipButton
            label="Workspace"
            onClick={() => toggle("workspace")}
            phase={phases.workspace}
          />
          <ChipButton
            label="Handoff"
            onClick={() => toggle("handoff")}
            phase={phases.handoff}
          />
          {githubAvailability === "cli-missing" ||
          githubAvailability === "cli-failed" ? (
            <ChipButton
              label="GitHub"
              onClick={() => undefined}
              title={githubDegradedTitle(githubAvailability)}
            />
          ) : pullRequest ? (
            <>
              <ChipButton
                label="PR"
                onClick={() => toggle("pr")}
                phase={phases.pr}
              />
              <ChipButton
                label="CI"
                onClick={() => toggle("ci")}
                phase={phases.ci}
              />
            </>
          ) : null}
        </div>

        {stuck && conversationId ? (
          <button
            className="shrink-0 rounded-md px-1.5 py-0.5 text-2xs font-medium text-amber-400 transition-colors hover:bg-accent hover:text-amber-300"
            data-testid="project-thread-status-wait"
            onClick={() => snoozeAgentAttention(conversationId)}
            type="button"
          >
            Wait 10m
          </button>
        ) : null}

        {stoppableCrew.length > 0 ? (
          <button
            className={cn(
              "shrink-0 rounded-md px-1.5 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
              stuck && "text-amber-400 hover:text-amber-300",
            )}
            data-testid="project-thread-status-stop"
            disabled={stopping}
            onClick={handleStop}
            type="button"
          >
            {AGENT_ACTIVITY_CHROME.stop}
          </button>
        ) : null}

        <button
          aria-expanded={showExpanded}
          aria-label={showExpanded ? "Collapse status" : "Expand status"}
          className="shrink-0 rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
          data-testid="project-thread-status-expand"
          onClick={() => setExpanded((value) => !value)}
          type="button"
        >
          {showExpanded ? (
            <ChevronUp className="h-3.5 w-3.5" />
          ) : (
            <ChevronDown className="h-3.5 w-3.5" />
          )}
        </button>
      </div>

      {showExpanded ? (
        <div
          className="mt-2 space-y-2"
          data-testid="project-thread-status-expanded"
        >
          <div
            className={cn(
              "grid overflow-hidden rounded-xl border border-border/60 bg-muted/20",
              isFocusMode
                ? "grid-cols-3 [&>*:not(:last-child)]:border-r"
                : "grid-cols-1 [&>*:not(:last-child)]:border-b",
            )}
          >
            <ProjectThreadIntegrationCell
              active={activeDrawer === "task"}
              detail={`${context.repoAddress} · shared branch`}
              icon={<Users className="h-3.5 w-3.5" />}
              label="Task"
              onClick={() => toggle("task")}
              phase={phases.task}
              title={`${steps.length}-agent task`}
            />
            <ProjectThreadIntegrationCell
              active={activeDrawer === "workspace"}
              detail={
                workspace.status === "ready"
                  ? workspace.baseSource === "remote"
                    ? workspace.commitsBehindRemote &&
                      workspace.commitsBehindRemote > 0
                      ? `${workspace.commitsBehindRemote} behind origin/${workspace.remoteDefaultBranch ?? "default"}`
                      : "Remote tip · ready"
                    : "Local fallback · remote unavailable"
                  : workspace.status === "derived"
                    ? "Restored from disk"
                    : workspace.status === "error"
                      ? "Setup failed"
                      : "Preparing"
              }
              icon={<GitBranch className="h-3.5 w-3.5" />}
              label="Workspace"
              onClick={() => toggle("workspace")}
              phase={phases.workspace}
              title={
                workspace.status === "ready" || workspace.status === "derived"
                  ? workspace.branch
                  : "Shared worktree"
              }
            />
            <ProjectThreadIntegrationCell
              active={activeDrawer === "handoff"}
              detail={`${counts.done} done · ${counts.working} working · ${counts.queued} queued`}
              icon={<Route className="h-3.5 w-3.5" />}
              label="Handoff"
              onClick={() => toggle("handoff")}
              phase={phases.handoff}
              title={counts.working ? `${activeName} working` : "Thread crew"}
            />
          </div>

          {pullRequest ? (
            <ProjectThreadGitHubRow
              activeDrawer={activeDrawer}
              onToggle={toggle}
              phases={phases}
              pullRequest={pullRequest}
            />
          ) : null}
        </div>
      ) : null}

      {activeDrawer ? (
        <div className="mt-2">
          <ProjectThreadIntegrationDrawer
            active={activeDrawer}
            context={context}
            onClose={closeDrawer}
            onGitHubRefresh={model.refreshGitHub}
            profiles={profiles}
            pullRequest={pullRequest}
            steps={steps}
            target={target}
            workspace={workspace}
          />
        </div>
      ) : null}
    </section>
  );
}

function useActiveTurnActivityBounds(
  agentPubkeys: readonly string[],
  channelId: string | null,
  conversationId: string | null,
) {
  const agentKey = agentPubkeys.map((pubkey) => pubkey.toLowerCase()).join(",");
  const cacheRef = React.useRef<{
    agentKey: string;
    channelId: string | null;
    conversationId: string | null;
    value: ReturnType<typeof getActiveTurnActivityBounds>;
  } | null>(null);

  const getSnapshot = React.useCallback(() => {
    const next = getActiveTurnActivityBounds({
      agentPubkeys,
      channelId,
      conversationId,
    });
    const prev = cacheRef.current;
    if (
      prev &&
      prev.agentKey === agentKey &&
      prev.channelId === channelId &&
      prev.conversationId === conversationId &&
      ((prev.value === null && next === null) ||
        (prev.value != null &&
          next != null &&
          prev.value.anchorAt === next.anchorAt &&
          prev.value.lastSeenAt === next.lastSeenAt &&
          prev.value.lastSubstantiveProgressAt ===
            next.lastSubstantiveProgressAt &&
          prev.value.progressKind === next.progressKind &&
          prev.value.progressLabel === next.progressLabel))
    ) {
      return prev.value;
    }
    cacheRef.current = {
      agentKey,
      channelId,
      conversationId,
      value: next,
    };
    return next;
  }, [agentKey, agentPubkeys, channelId, conversationId]);

  return React.useSyncExternalStore(subscribeActiveAgentTurns, getSnapshot);
}
