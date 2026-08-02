import * as React from "react";
import { Loader2 } from "lucide-react";

import {
  getActiveTurnActivityBounds,
  getActiveTurnControlTargetsForAgent,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import {
  ACTIVITY_SILENCE_MS,
  ACTIVITY_STUCK_MS,
  AGENT_ACTIVITY_CHROME,
} from "@/features/agents/ui/agentActivityChrome";
import { formatElapsed } from "@/features/agents/ui/agentSessionUtils";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { cancelManagedAgentTurn } from "@/shared/api/agentControl";
import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
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
  workingBotPubkeys: string[];
  variant?: "toolbar" | "inline";
};

const HOVER_OPEN_DELAY_MS = 150;
const HOVER_CLOSE_DELAY_MS = 180;
const TICK_MS = 1_000;

export function BotActivityComposerAction({
  agents,
  channelId = null,
  conversationId = null,
  onOpenAgentSession,
  openAgentSessionPubkey,
  profiles,
  workingBotPubkeys,
  variant = "toolbar",
}: BotActivityBarProps) {
  const [open, setOpen] = React.useState(false);
  const [now, setNow] = React.useState(() => Date.now());
  const [stopping, setStopping] = React.useState(false);
  const hoverTimerRef = React.useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );

  const workingAgents = React.useMemo(() => {
    const workingSet = new Set(
      workingBotPubkeys.map((pubkey) => pubkey.toLowerCase()),
    );

    return agents.filter((agent) => workingSet.has(agent.pubkey.toLowerCase()));
  }, [agents, workingBotPubkeys]);

  const activityBounds = useActiveTurnActivityBounds(
    workingBotPubkeys,
    channelId,
    conversationId,
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
    }, HOVER_OPEN_DELAY_MS);
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
    async (event: React.MouseEvent<HTMLButtonElement>) => {
      event.preventDefault();
      event.stopPropagation();
      if (stopping || workingAgents.length === 0) return;
      setStopping(true);
      try {
        await Promise.all(
          workingAgents.flatMap((agent) => {
            const targets = getActiveTurnControlTargetsForAgent(agent.pubkey);
            return targets
              .filter((target) => {
                if (
                  conversationId &&
                  target.conversationId !== conversationId
                ) {
                  return false;
                }
                if (channelId && target.channelId !== channelId) {
                  return false;
                }
                return true;
              })
              .map((target) =>
                cancelManagedAgentTurn(
                  agent.pubkey,
                  target.channelId,
                  target.conversationId,
                  target.turnId,
                ),
              );
          }),
        );
      } finally {
        setStopping(false);
      }
    },
    [channelId, conversationId, stopping, workingAgents],
  );

  if (workingAgents.length === 0) {
    return null;
  }

  const elapsedMs = activityBounds
    ? Math.max(0, now - activityBounds.anchorAt)
    : 0;
  if (activityBounds && elapsedMs < ACTIVITY_SILENCE_MS) {
    return null;
  }

  const stuck =
    activityBounds != null &&
    now - activityBounds.lastActivityAt >= ACTIVITY_STUCK_MS;
  const agentAvatarUrl = (agent: BotActivityAgent) =>
    profiles?.[agent.pubkey.toLowerCase()]?.avatarUrl ?? null;
  const selectedPubkey = openAgentSessionPubkey?.toLowerCase() ?? null;
  const triggerLabel =
    workingAgents.length === 1
      ? `${workingAgents[0]?.name ?? "Agent"} ${AGENT_ACTIVITY_CHROME.isWorking}`
      : AGENT_ACTIVITY_CHROME.agentsWorking(workingAgents.length);
  const statusLabel = stuck
    ? `${triggerLabel} · ${AGENT_ACTIVITY_CHROME.seemsStuck}`
    : activityBounds
      ? `${triggerLabel} · ${formatElapsed(elapsedMs)}`
      : triggerLabel;
  const isInline = variant === "inline";

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
              stuck && "text-amber-400 hover:text-amber-300",
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
          className="w-64 p-1"
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

              return (
                <button
                  className={cn(
                    "flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors",
                    isSelected
                      ? "bg-primary/10 text-primary"
                      : "text-foreground hover:bg-accent hover:text-accent-foreground",
                  )}
                  data-testid={`bot-activity-composer-item-${agent.pubkey}`}
                  key={agent.pubkey}
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
                  <span className="min-w-0 flex-1 truncate">{agent.name}</span>
                  <span className="shrink-0 whitespace-nowrap text-xs font-medium opacity-80">
                    {AGENT_ACTIVITY_CHROME.viewActivity}
                  </span>
                  <Loader2 className="h-4 w-4 shrink-0 animate-spin text-muted-foreground/70" />
                </button>
              );
            })}
          </div>
        </PopoverContent>
      </Popover>
      {isInline ? (
        <button
          className={cn(
            "shrink-0 rounded-md px-1.5 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
            stuck && "text-amber-400 hover:text-amber-300",
          )}
          data-testid="bot-activity-stop"
          disabled={stopping}
          onClick={handleStop}
          type="button"
        >
          {AGENT_ACTIVITY_CHROME.stop}
        </button>
      ) : null}
    </div>
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
