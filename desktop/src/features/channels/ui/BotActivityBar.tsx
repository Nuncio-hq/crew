import * as React from "react";
import { Loader2, Octagon } from "lucide-react";

import { useAgentTranscript } from "@/features/agents/ui/useObserverEvents";
import {
  getActivityHeadline,
  isMeaningfulItem,
  isSpineItem,
} from "@/features/agents/ui/agentSessionTranscriptPresentation";
import { useComposerAgentStop } from "@/features/channels/ui/useComposerAgentStop";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { ManagedAgent } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Shimmer } from "@/shared/ui/Shimmer";
import { UserAvatar } from "@/shared/ui/UserAvatar";

export type BotActivityAgent = Pick<ManagedAgent, "pubkey" | "name">;

type BotActivityBarProps = {
  agents: BotActivityAgent[];
  channelId?: string | null;
  /** Thread-scoped conversation; omit for channel-wide activity. */
  conversationId?: string | null;
  onOpenAgentSession: (pubkey: string, channelId?: string | null) => void;
  openAgentSessionPubkey: string | null;
  profiles?: UserProfileLookup;
  workingBotPubkeys: string[];
  variant?: "toolbar" | "inline";
};

const HOVER_OPEN_DELAY_MS = 150;
const HOVER_CLOSE_DELAY_MS = 180;
const HEADLINE_ROTATION_MS = 2200;

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

  const workingAgents = React.useMemo(() => {
    const workingSet = new Set(
      workingBotPubkeys.map((pubkey) => pubkey.toLowerCase()),
    );

    return agents.filter((agent) => workingSet.has(agent.pubkey.toLowerCase()));
  }, [agents, workingBotPubkeys]);
  const singleWorkingAgent =
    workingAgents.length === 1 ? (workingAgents[0] ?? null) : null;

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
  const transcript = useAgentTranscript(
    Boolean(singleWorkingAgent),
    singleWorkingAgent?.pubkey,
  );
  const activityHeadlines = React.useMemo(() => {
    if (!singleWorkingAgent) {
      return [];
    }

    const seen = new Set<string>();
    const headlines: string[] = [];
    const scopedTranscript = channelId
      ? transcript.filter((item) => item.channelId === channelId)
      : transcript;

    // Two-tier scan: spine items first (reads recede when real work is present).
    // If no spine headlines are found (session start / idle), fall back to all
    // meaningful items so the bar isn't left empty.
    const passFilter: (item: (typeof scopedTranscript)[number]) => boolean =
      scopedTranscript.some(isSpineItem) ? isSpineItem : isMeaningfulItem;

    for (let i = scopedTranscript.length - 1; i >= 0; i--) {
      const item = scopedTranscript[i];
      if (!passFilter(item)) {
        continue;
      }
      const headline = getActivityHeadline(item);
      if (!headline || seen.has(headline)) {
        continue;
      }

      seen.add(headline);
      headlines.unshift(headline);
      if (headlines.length >= 5) {
        break;
      }
    }

    return headlines;
  }, [channelId, singleWorkingAgent, transcript]);
  const [headlineIndex, setHeadlineIndex] = React.useState(0);

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

  React.useEffect(() => {
    if (activityHeadlines.length <= 1) {
      return;
    }

    const interval = window.setInterval(() => {
      setHeadlineIndex((current) => (current + 1) % activityHeadlines.length);
    }, HEADLINE_ROTATION_MS);

    return () => window.clearInterval(interval);
  }, [activityHeadlines.length]);

  if (workingAgents.length === 0) {
    return null;
  }

  const agentAvatarUrl = (agent: BotActivityAgent) =>
    profiles?.[agent.pubkey.toLowerCase()]?.avatarUrl ?? null;
  const selectedPubkey = openAgentSessionPubkey?.toLowerCase() ?? null;
  const triggerLabel =
    workingAgents.length === 1
      ? `${workingAgents[0]?.name ?? "Agent"} is working`
      : `${workingAgents.length} agents working`;
  const isInline = variant === "inline";
  const visibleStatusLabel =
    workingAgents.length === 1
      ? `${workingAgents[0]?.name ?? "Agent"}: ${
          activityHeadlines[headlineIndex % activityHeadlines.length] ??
          "Working"
        }`
      : `${workingAgents[0]?.name ?? "Agent"} +${workingAgents.length - 1}`;

  const inlineStop =
    isInline &&
    singleWorkingAgent &&
    hasStoppableWork(singleWorkingAgent.pubkey) ? (
      <button
        aria-label={`Stop ${singleWorkingAgent.name}`}
        className="ml-1 inline-flex h-5 shrink-0 items-center gap-1 rounded-md px-1.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
        data-testid="bot-activity-composer-stop"
        disabled={stoppingPubkey === singleWorkingAgent.pubkey.toLowerCase()}
        onClick={(event) => {
          void handleStop(singleWorkingAgent, event);
        }}
        type="button"
      >
        <Octagon className="h-3 w-3" />
        Stop
      </button>
    ) : null;

  return (
    <div className="flex min-w-0 items-center">
      <Popover onOpenChange={setOpen} open={open}>
        <PopoverTrigger asChild>
          <button
            aria-label={`${triggerLabel}. View activity.`}
            className={cn(
              "inline-flex items-center justify-center rounded-full border border-border/60 bg-background font-medium text-muted-foreground transition-colors hover:border-primary/30 hover:bg-primary/5 hover:text-foreground focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring data-[state=open]:border-primary/40 data-[state=open]:bg-primary/10 data-[state=open]:text-primary",
              isInline
                ? "min-w-0 gap-1.5 overflow-visible border-transparent bg-transparent px-0 text-xs font-normal leading-normal shadow-none hover:border-transparent hover:bg-transparent data-[state=open]:border-transparent data-[state=open]:bg-transparent"
                : "h-9 min-w-9 gap-1.5 px-2 text-xs",
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
                  {visibleStatusLabel}
                </Shimmer>
              ) : (
                "working"
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
            Agents working
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
                      View activity
                    </span>
                    <Loader2 className="h-4 w-4 shrink-0 animate-spin text-muted-foreground/70" />
                  </button>
                  {canStop ? (
                    <button
                      aria-label={`Stop ${agent.name}`}
                      className="inline-flex h-7 shrink-0 items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                      data-testid={`bot-activity-composer-stop-${agent.pubkey}`}
                      disabled={isStopping}
                      onClick={(event) => {
                        void handleStop(agent, event);
                      }}
                      type="button"
                    >
                      <Octagon className="h-3.5 w-3.5" />
                      Stop
                    </button>
                  ) : null}
                </div>
              );
            })}
          </div>
        </PopoverContent>
      </Popover>
      {inlineStop}
    </div>
  );
}
