import * as React from "react";
import { Loader2 } from "lucide-react";

import { subscribeAgentLiveness } from "@/features/agents/activeAgentTurnsLiveness";
import {
  getActiveTurnActivityBounds,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import { deriveAgentAttention } from "@/features/agents/agentAttention";
import { managedAgentWakingStatusLabel } from "@/features/agents/managedAgentRuntimeStatus";
import {
  getAgentAttentionSnoozeGeneration,
  getAgentAttentionSnoozedUntil,
  snoozeAgentAttention,
  subscribeAgentAttentionSnoozes,
} from "@/features/agents/agentAttentionSnoozeStore";
import { useAgentObserverConnectionState } from "@/features/agents/useAgentObserverConnectionState";
import {
  getRetryingTurn,
  getRetryingTurnsForChannel,
  subscribeRetryingTurns,
  type RetryingTurn,
} from "@/features/agents/retryingTurnsStore";
import {
  ACTIVITY_SILENCE_MS,
  AGENT_ACTIVITY_CHROME,
} from "@/features/agents/ui/agentActivityChrome";
import { formatElapsed } from "@/features/agents/ui/agentSessionUtils";
import { useComposerAgentStop } from "@/features/channels/ui/useComposerAgentStop";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  DEFAULT_POPOVER_HOVER_OPEN_DELAY_MS,
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/shared/ui/popover";
import { Shimmer } from "@/shared/ui/Shimmer";
import { UserAvatar } from "@/shared/ui/UserAvatar";

export type BotActivityAgent = Pick<ManagedAgent, "pubkey" | "name">;

type BotActivityBarProps = {
  agents: BotActivityAgent[];
  channelId?: string | null;
  conversationId?: string | null;
  onOpenAgentSession: (pubkey: string, channelId?: string | null) => void;
  openAgentSessionPubkey: string | null;
  profiles?: UserProfileLookup;
  wakingBotPubkeys?: string[];
  workingBotPubkeys: string[];
  variant?: "toolbar" | "inline";
};

const HOVER_CLOSE_DELAY_MS = 180;
const TICK_MS = 1_000;

export function BotActivityComposerAction({
  agents,
  channelId = null,
  conversationId = null,
  onOpenAgentSession,
  openAgentSessionPubkey,
  profiles,
  wakingBotPubkeys = [],
  workingBotPubkeys,
  variant = "toolbar",
}: BotActivityBarProps) {
  const [open, setOpen] = React.useState(false);
  const [now, setNow] = React.useState(() => Date.now());
  const [stoppingPubkey, setStoppingPubkey] = React.useState<string | null>(
    null,
  );
  const hoverTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const { hasStoppableWork, stopAgent } = useComposerAgentStop({
    channelId,
    conversationId,
  });

  const retryingTurns = useRetryingTurns(channelId, conversationId);
  const retryingPubkeys = React.useMemo(
    () => new Set(retryingTurns.map((entry) => entry.agentPubkey)),
    [retryingTurns],
  );
  const wakingPubkeys = React.useMemo(
    () => new Set(wakingBotPubkeys.map((pubkey) => pubkey.toLowerCase())),
    [wakingBotPubkeys],
  );

  const workingAgents = React.useMemo(() => {
    const workingSet = new Set(
      workingBotPubkeys.map((pubkey) => pubkey.toLowerCase()),
    );
    for (const pubkey of retryingPubkeys) workingSet.add(pubkey);

    return agents.filter((agent) => workingSet.has(agent.pubkey.toLowerCase()));
  }, [agents, retryingPubkeys, workingBotPubkeys]);
  const singleWorkingAgent =
    workingAgents.length === 1 ? (workingAgents[0] ?? null) : null;
  const wakingLabel =
    singleWorkingAgent &&
    wakingPubkeys.has(singleWorkingAgent.pubkey.toLowerCase())
      ? managedAgentWakingStatusLabel(singleWorkingAgent.name)
      : null;
  const primaryRetrying =
    retryingTurns.length === 1 ? (retryingTurns[0] ?? null) : null;

  const activityBounds = useActiveTurnActivityBounds(
    workingBotPubkeys,
    channelId,
    conversationId,
  );
  const connectionState = useAgentObserverConnectionState(workingBotPubkeys);
  React.useSyncExternalStore(
    subscribeAgentAttentionSnoozes,
    getAgentAttentionSnoozeGeneration,
    getAgentAttentionSnoozeGeneration,
  );

  React.useEffect(() => {
    if (workingAgents.length === 0) return;
    const interval = window.setInterval(() => setNow(Date.now()), TICK_MS);
    return () => window.clearInterval(interval);
  }, [workingAgents.length]);

  const clearHoverTimer = React.useCallback(() => {
    if (hoverTimerRef.current !== null) {
      clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
  }, []);

  const openWithDelay = React.useCallback(() => {
    clearHoverTimer();
    hoverTimerRef.current = setTimeout(() => {
      setOpen(true);
    }, DEFAULT_POPOVER_HOVER_OPEN_DELAY_MS);
  }, [clearHoverTimer]);

  const closeWithDelay = React.useCallback(() => {
    clearHoverTimer();
    hoverTimerRef.current = setTimeout(() => {
      setOpen(false);
    }, HOVER_CLOSE_DELAY_MS);
  }, [clearHoverTimer]);

  const keepOpen = React.useCallback(() => {
    clearHoverTimer();
  }, [clearHoverTimer]);

  React.useEffect(() => {
    return () => clearHoverTimer();
  }, [clearHoverTimer]);

  const handleStop = React.useCallback(
    async (agent: BotActivityAgent, event?: React.MouseEvent) => {
      event?.preventDefault();
      event?.stopPropagation();
      const key = agent.pubkey.toLowerCase();
      setStoppingPubkey(key);
      try {
        await stopAgent(agent.pubkey, agent.name);
      } finally {
        setStoppingPubkey((current) => (current === key ? null : current));
      }
    },
    [stopAgent],
  );

  if (workingAgents.length === 0) {
    return null;
  }

  const elapsedMs = activityBounds
    ? Math.max(0, now - activityBounds.anchorAt)
    : 0;
  // Silence only applies to in-flight turns. Queued/held work and automatic
  // retries have no live bounds, so the chrome stays visible for those.
  if (
    activityBounds &&
    elapsedMs < ACTIVITY_SILENCE_MS &&
    retryingTurns.length === 0
  ) {
    return null;
  }

  const attention = activityBounds
    ? deriveAgentAttention({
        connectionState,
        needsYou: false,
        now,
        outcome: null,
        receipt: null,
        snoozedUntil: conversationId
          ? getAgentAttentionSnoozedUntil(conversationId)
          : 0,
        turns: [
          {
            agentPubkey: singleWorkingAgent?.pubkey ?? "",
            ...activityBounds,
          },
        ],
      })
    : null;
  const stuck = attention?.state === "possibly-stalled";
  const attentionLabel =
    attention?.state === "lost-contact"
      ? "Lost contact"
      : attention?.state === "telemetry-unavailable"
        ? "Telemetry unavailable"
        : stuck
          ? "Possibly stalled"
          : null;
  const agentAvatarUrl = (agent: BotActivityAgent) =>
    profiles?.[agent.pubkey.toLowerCase()]?.avatarUrl ?? null;
  const selectedPubkey = openAgentSessionPubkey?.toLowerCase() ?? null;
  const triggerLabel =
    workingAgents.length === 1
      ? `${workingAgents[0]?.name ?? "Agent"} ${AGENT_ACTIVITY_CHROME.isWorking}`
      : AGENT_ACTIVITY_CHROME.agentsWorking(workingAgents.length);
  const retryingLabel = primaryRetrying
    ? AGENT_ACTIVITY_CHROME.retrying(
        primaryRetrying.attempt,
        primaryRetrying.maxAttempts,
      )
    : retryingTurns.length > 1
      ? AGENT_ACTIVITY_CHROME.retrying(
          retryingTurns[0]?.attempt ?? 1,
          retryingTurns[0]?.maxAttempts ?? 10,
        )
      : null;
  const statusLabel = retryingLabel
    ? workingAgents.length === 1
      ? `${workingAgents[0]?.name ?? "Agent"} · ${retryingLabel}`
      : retryingLabel
    : wakingLabel
      ? wakingLabel
      : attentionLabel
        ? `${triggerLabel} · ${attentionLabel}`
        : activityBounds
          ? `${triggerLabel} · ${formatElapsed(elapsedMs)}`
          : triggerLabel;
  const isInline = variant === "inline";
  const inlineWait =
    isInline && stuck && conversationId ? (
      <button
        className="shrink-0 rounded-md px-1.5 py-0.5 text-2xs font-medium text-attention transition-colors hover:bg-accent hover:text-attention"
        data-testid="bot-activity-wait"
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          snoozeAgentAttention(conversationId);
        }}
        type="button"
      >
        Wait 10m
      </button>
    ) : null;
  const inlineStop =
    isInline &&
    singleWorkingAgent &&
    hasStoppableWork(singleWorkingAgent.pubkey) ? (
      <button
        aria-label={`${AGENT_ACTIVITY_CHROME.stop} ${singleWorkingAgent.name}`}
        className={cn(
          "shrink-0 rounded-md px-1.5 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50",
          stuck && "text-attention hover:text-attention",
        )}
        data-testid="bot-activity-stop"
        disabled={stoppingPubkey === singleWorkingAgent.pubkey.toLowerCase()}
        onClick={(event) => {
          void handleStop(singleWorkingAgent, event);
        }}
        type="button"
      >
        {AGENT_ACTIVITY_CHROME.stop}
      </button>
    ) : null;

  return (
    <div
      className={cn(
        "inline-flex min-w-0 items-center",
        isInline ? "w-full gap-2" : "gap-1.5",
      )}
    >
      <Popover onOpenChange={setOpen} open={open}>
        <PopoverTrigger asChild>
          <button
            aria-label={`${triggerLabel}. View activity.`}
            className={cn(
              "inline-flex items-center justify-center rounded-full border border-border/60 bg-background font-medium text-muted-foreground transition-colors hover:border-primary/30 hover:bg-primary/5 hover:text-foreground focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring data-[state=open]:border-primary/40 data-[state=open]:bg-primary/10 data-[state=open]:text-primary",
              isInline
                ? "min-w-0 flex-1 gap-1.5 overflow-visible border-transparent bg-transparent px-0 text-xs font-normal leading-normal shadow-none hover:border-transparent hover:bg-transparent data-[state=open]:border-transparent data-[state=open]:bg-transparent"
                : "h-9 min-w-9 gap-1.5 px-2 text-xs",
              stuck && "text-attention hover:text-attention",
            )}
            data-testid="bot-activity-composer-trigger"
            onBlur={closeWithDelay}
            onClick={() => {
              clearHoverTimer();
              setOpen((current) => !current);
            }}
            onFocus={() => setOpen(true)}
            onMouseEnter={openWithDelay}
            onMouseLeave={closeWithDelay}
            type="button"
          >
            <span className="flex h-4.5 items-center overflow-visible -space-x-1">
              {workingAgents.slice(0, 2).map((agent) => (
                <UserAvatar
                  avatarUrl={agentAvatarUrl(agent)}
                  className={cn(
                    "border border-background",
                    isInline ? "!h-4.5 !w-4.5 text-3xs" : "shrink-0",
                  )}
                  displayName={agent.name}
                  fallbackDelayMs={isInline ? 0 : undefined}
                  key={agent.pubkey}
                  size="xs"
                />
              ))}
            </span>
            {workingAgents.length > 2 ? (
              <span className="text-2xs leading-none">
                +{workingAgents.length - 2}
              </span>
            ) : null}
            <span
              className={cn(
                isInline
                  ? "flex h-4.5 min-w-0 flex-1 items-center overflow-visible leading-none"
                  : "sr-only",
              )}
            >
              {isInline ? (
                <Shimmer className="-my-px truncate py-px">
                  {statusLabel}
                </Shimmer>
              ) : (
                AGENT_ACTIVITY_CHROME.workingFallback
              )}
            </span>
            {isInline ? null : (
              <Loader2 className="h-4 w-4 shrink-0 animate-spin opacity-70" />
            )}
          </button>
        </PopoverTrigger>
        <PopoverContent
          align={isInline ? "start" : "end"}
          className="w-72 p-1"
          onMouseEnter={keepOpen}
          onMouseLeave={closeWithDelay}
          onOpenAutoFocus={(event) => event.preventDefault()}
          side="top"
          sideOffset={8}
        >
          <div className="px-2 py-1 text-xs font-medium text-muted-foreground">
            {AGENT_ACTIVITY_CHROME.agentsWorkingLabel}
          </div>
          <div className="mt-1 flex flex-col gap-1">
            {workingAgents.map((agent) => {
              const isSelected = selectedPubkey === agent.pubkey.toLowerCase();
              const canStop = hasStoppableWork(agent.pubkey);
              const isStopping = stoppingPubkey === agent.pubkey.toLowerCase();

              return (
                <div
                  className={cn(
                    "flex w-full items-center gap-1 rounded-lg pr-1",
                    isSelected
                      ? "bg-primary/10 text-primary"
                      : "text-foreground",
                  )}
                  key={agent.pubkey}
                >
                  <button
                    className={cn(
                      "flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors",
                      isSelected
                        ? "text-primary"
                        : "hover:bg-accent hover:text-accent-foreground",
                    )}
                    data-testid={`bot-activity-composer-item-${agent.pubkey}`}
                    onClick={() => {
                      clearHoverTimer();
                      setOpen(false);
                      onOpenAgentSession(agent.pubkey, channelId);
                    }}
                    type="button"
                  >
                    <UserAvatar
                      avatarUrl={agentAvatarUrl(agent)}
                      className="shrink-0"
                      displayName={agent.name}
                      size="sm"
                    />
                    <span className="min-w-0 flex-1 truncate">
                      {agent.name}
                    </span>
                    <span className="shrink-0 whitespace-nowrap text-xs font-medium opacity-80">
                      {AGENT_ACTIVITY_CHROME.viewActivity}
                    </span>
                    <Loader2 className="h-4 w-4 shrink-0 animate-spin text-muted-foreground/70" />
                  </button>
                  {canStop ? (
                    <button
                      aria-label={`${AGENT_ACTIVITY_CHROME.stop} ${agent.name}`}
                      className="inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                      data-testid={`bot-activity-composer-stop-${agent.pubkey}`}
                      disabled={isStopping}
                      onClick={(event) => {
                        void handleStop(agent, event);
                      }}
                      type="button"
                    >
                      {AGENT_ACTIVITY_CHROME.stop}
                    </button>
                  ) : null}
                </div>
              );
            })}
          </div>
        </PopoverContent>
      </Popover>
      {inlineWait}
      {inlineStop}
    </div>
  );
}

function useRetryingTurns(
  channelId: string | null,
  conversationId: string | null,
): RetryingTurn[] {
  const cacheRef = React.useRef<{
    channelId: string | null;
    conversationId: string | null;
    value: RetryingTurn[];
  } | null>(null);

  const getSnapshot = React.useCallback(() => {
    const next = conversationId
      ? (() => {
          const single = getRetryingTurn(conversationId);
          return single ? [single] : [];
        })()
      : getRetryingTurnsForChannel(channelId);
    const prev = cacheRef.current;
    if (
      prev &&
      prev.channelId === channelId &&
      prev.conversationId === conversationId &&
      prev.value.length === next.length &&
      prev.value.every(
        (entry, index) =>
          entry.agentPubkey === next[index]?.agentPubkey &&
          entry.conversationId === next[index]?.conversationId &&
          entry.attempt === next[index]?.attempt &&
          entry.maxAttempts === next[index]?.maxAttempts,
      )
    ) {
      return prev.value;
    }
    cacheRef.current = { channelId, conversationId, value: next };
    return next;
  }, [channelId, conversationId]);

  return React.useSyncExternalStore(subscribeRetryingTurns, getSnapshot);
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

  // Membership changes arrive on the global subscription; lastSeenAt/progress
  // advance per liveness frame, delivered by each agent's own subscription.
  const subscribe = React.useCallback(
    (onStoreChange: () => void) => {
      const unsubscribers = [
        subscribeActiveAgentTurns(onStoreChange),
        ...agentKey
          .split(",")
          .filter((pubkey) => pubkey !== "")
          .map((pubkey) => subscribeAgentLiveness(pubkey, onStoreChange)),
      ];
      return () => {
        for (const unsubscribe of unsubscribers) unsubscribe();
      };
    },
    [agentKey],
  );

  return React.useSyncExternalStore(subscribe, getSnapshot);
}
