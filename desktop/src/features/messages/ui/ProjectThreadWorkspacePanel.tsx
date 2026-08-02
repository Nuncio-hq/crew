import { ChevronDown, ChevronUp, GitBranch, Route, Users } from "lucide-react";
import * as React from "react";

import {
  getActiveTurnActivityBounds,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import {
  ACTIVITY_STUCK_MS,
  AGENT_ACTIVITY_CHROME,
} from "@/features/agents/ui/agentActivityChrome";
import { formatElapsed } from "@/features/agents/ui/agentSessionUtils";
import { useComposerAgentStop } from "@/features/channels/ui/useComposerAgentStop";
import type { ProjectThreadAgentMention } from "@/features/messages/lib/projectThreadWorkspace";
import type { TimelineMessage } from "@/features/messages/types";
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
import { ciStatus, pullRequestStatus } from "./projectThreadGitHubStatus";
import { useProjectThreadWorkspaceModel } from "./useProjectThreadWorkspaceModel";

type Props = {
  agentMentions: readonly ProjectThreadAgentMention[];
  channelId: string | null;
  /** When true, the bar may expand to the full Task/Workspace/Handoff grid. */
  isFocusMode?: boolean;
  profiles?: UserProfileLookup;
  replies: readonly TimelineMessage[];
  threadHead: TimelineMessage;
};

type ChipTone = "idle" | "active" | "success" | "danger";

function StatusDot({ tone }: { tone: ChipTone }) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-block h-1.5 w-1.5 rounded-full",
        tone === "active" && "bg-amber-400",
        tone === "success" && "bg-emerald-500",
        tone === "danger" && "bg-destructive",
        tone === "idle" && "bg-muted-foreground/40",
      )}
    />
  );
}

function ChipButton({
  label,
  onClick,
  tone,
}: {
  label: string;
  onClick: () => void;
  tone: ChipTone;
}) {
  return (
    <button
      className="inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
      onClick={onClick}
      type="button"
    >
      <StatusDot tone={tone} />
      {label}
    </button>
  );
}

/**
 * Sticky project-thread status bar. Mount **outside** the scroll region
 * (between header and message list). Collapsed by default; full grid only in
 * focus mode. Owns the agent working signal for project threads.
 */
export function ProjectThreadWorkspacePanel({
  agentMentions,
  channelId,
  isFocusMode = false,
  profiles,
  replies,
  threadHead,
}: Props) {
  const model = useProjectThreadWorkspaceModel({
    agentMentions,
    profiles,
    replies,
    threadHead,
  });
  const [activeDrawer, setActiveDrawer] =
    React.useState<ProjectThreadDrawer | null>(null);
  const [expanded, setExpanded] = React.useState(false);
  const [stopping, setStopping] = React.useState(false);
  const [now, setNow] = React.useState(() => Date.now());

  const closeDrawer = React.useCallback(() => setActiveDrawer(null), []);
  useEscapeKey(closeDrawer, activeDrawer !== null);

  React.useEffect(() => {
    if (!isFocusMode) setExpanded(false);
  }, [isFocusMode]);

  React.useEffect(() => {
    if (
      activeDrawer === "issue" ||
      activeDrawer === "pr" ||
      activeDrawer === "ci"
    ) {
      void model?.refreshGitHub();
    }
  }, [activeDrawer, model]);

  const workingPubkeys = model?.workingPubkeys ?? [];
  const conversationId = model?.conversationId ?? null;
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
        name:
          profile?.displayName ??
          profile?.name ??
          truncatePubkey(pubkey),
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
          profile?.displayName ??
          profile?.name ??
          truncatePubkey(step.pubkey),
      });
    }
    return crew;
  }, [hasStoppableWork, model?.steps, profiles, workingPubkeys]);
  const activityBounds = useActiveTurnActivityBounds(
    workingPubkeys,
    channelId,
    conversationId,
  );

  React.useEffect(() => {
    if (workingPubkeys.length === 0) return;
    const interval = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(interval);
  }, [workingPubkeys.length]);

  if (!model) return null;

  const { activeName, context, counts, pullRequest, steps, target, workspace } =
    model;
  const showExpanded = isFocusMode && expanded;
  const elapsedMs = activityBounds
    ? Math.max(0, now - activityBounds.anchorAt)
    : 0;
  const stuck =
    activityBounds != null &&
    now - activityBounds.lastActivityAt >= ACTIVITY_STUCK_MS;
  const stepLabel = `${counts.done + counts.working}/${steps.length}`;
  const agentLabel =
    counts.working > 1
      ? AGENT_ACTIVITY_CHROME.agentsWorking(counts.working)
      : counts.working === 1
        ? `${activeName} · ${stepLabel}`
        : `${activeName} · ${stepLabel}`;
  const timeLabel = activityBounds
    ? stuck
      ? AGENT_ACTIVITY_CHROME.seemsStuck
      : formatElapsed(elapsedMs)
    : null;

  const workspaceTone: ChipTone =
    workspace.status === "error"
      ? "danger"
      : workspace.status === "ready"
        ? "success"
        : "active";
  const taskTone: ChipTone =
    counts.working > 0
      ? "active"
      : counts.done === steps.length
        ? "success"
        : "idle";
  const handoffTone: ChipTone =
    counts.working > 0 ? "active" : counts.done > 0 ? "success" : "idle";
  const prTone: ChipTone = pullRequest
    ? pullRequestStatus(pullRequest).tone === "merged" ||
      pullRequestStatus(pullRequest).tone === "open"
      ? "success"
      : pullRequestStatus(pullRequest).tone === "closed"
        ? "danger"
        : "active"
    : "idle";
  const ciTone: ChipTone = pullRequest
    ? ciStatus(pullRequest.checks).tone === "success"
      ? "success"
      : ciStatus(pullRequest.checks).tone === "failure"
        ? "danger"
        : "active"
    : "idle";

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
                  : "animate-pulse bg-emerald-400"
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
            tone={taskTone}
          />
          <ChipButton
            label="Workspace"
            onClick={() => toggle("workspace")}
            tone={workspaceTone}
          />
          <ChipButton
            label="Handoff"
            onClick={() => toggle("handoff")}
            tone={handoffTone}
          />
          {pullRequest ? (
            <>
              <ChipButton
                label="PR"
                onClick={() => toggle("pr")}
                tone={prTone}
              />
              <ChipButton
                label="CI"
                onClick={() => toggle("ci")}
                tone={ciTone}
              />
            </>
          ) : null}
        </div>

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

        {isFocusMode ? (
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
        ) : null}
      </div>

      {showExpanded ? (
        <div
          className="mt-2 space-y-2"
          data-testid="project-thread-status-expanded"
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
                    ? workspace.commitsBehindRemote &&
                      workspace.commitsBehindRemote > 0
                      ? `${workspace.commitsBehindRemote} behind origin/${workspace.remoteDefaultBranch ?? "default"}`
                      : "Remote tip · ready"
                    : "Local fallback · remote unavailable"
                  : workspace.status === "error"
                    ? "Setup failed"
                    : "Preparing"
              }
              icon={<GitBranch className="h-3.5 w-3.5" />}
              label="Workspace"
              onClick={() => toggle("workspace")}
              title={
                workspace.status === "ready"
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
    value: { anchorAt: number; lastActivityAt: number } | null;
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
          prev.value.lastActivityAt === next.lastActivityAt))
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
