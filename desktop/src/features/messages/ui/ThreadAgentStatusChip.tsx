import * as React from "react";
import { Loader2 } from "lucide-react";

import {
  type ActiveConversationAgentTurnSummary,
  useActiveTurnSummariesForConversation,
} from "@/features/agents/activeConversationAgentTurnSummaries";
import { formatElapsed } from "@/features/agents/ui/agentSessionUtils";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { cn } from "@/shared/lib/cn";
import { UserAvatar } from "@/shared/ui/UserAvatar";

const MAX_CHIP_AVATARS = 2;

/** Shared 1s clock for every mounted chip — one interval, not one per row. */
let sharedNow = Date.now();
const sharedNowListeners = new Set<() => void>();
let sharedNowInterval: ReturnType<typeof setInterval> | null = null;

function subscribeSharedNow(listener: () => void) {
  sharedNowListeners.add(listener);
  if (sharedNowListeners.size === 1) {
    sharedNowInterval = setInterval(() => {
      sharedNow = Date.now();
      for (const notify of sharedNowListeners) {
        notify();
      }
    }, 1_000);
  }
  return () => {
    sharedNowListeners.delete(listener);
    if (sharedNowListeners.size === 0 && sharedNowInterval) {
      clearInterval(sharedNowInterval);
      sharedNowInterval = null;
    }
  };
}

function getSharedNowSnapshot() {
  return sharedNow;
}

function useSharedNowWhen(enabled: boolean): number {
  const subscribe = React.useCallback(
    (listener: () => void) => {
      if (!enabled) return () => {};
      return subscribeSharedNow(listener);
    },
    [enabled],
  );
  return React.useSyncExternalStore(
    subscribe,
    getSharedNowSnapshot,
    getSharedNowSnapshot,
  );
}

export type ThreadAgentStatusChipView = {
  displayAgents: ReadonlyArray<{
    pubkey: string;
    displayName: string;
    avatarUrl: string | null;
  }>;
  label: string;
  elapsedLabel: string;
  title: string;
};

function agentDisplayName(
  pubkey: string,
  profiles: UserProfileLookup | undefined,
): string {
  const profile = profiles?.[normalizePubkey(pubkey)];
  return profile?.displayName ?? profile?.name ?? truncatePubkey(pubkey);
}

/** Pure view model for the chip — unit-tested without a React tree. */
export function buildThreadAgentStatusChipView(
  summaries: readonly ActiveConversationAgentTurnSummary[],
  profiles: UserProfileLookup | undefined,
  now: number,
): ThreadAgentStatusChipView | null {
  if (summaries.length === 0) return null;

  let earliestAnchorAt = Number.POSITIVE_INFINITY;
  for (const summary of summaries) {
    if (summary.anchorAt < earliestAnchorAt) {
      earliestAnchorAt = summary.anchorAt;
    }
  }

  const names = summaries.map((summary) =>
    agentDisplayName(summary.agentPubkey, profiles),
  );
  const displayAgents = summaries.slice(0, MAX_CHIP_AVATARS).map((summary) => {
    const displayName = agentDisplayName(summary.agentPubkey, profiles);
    return {
      pubkey: summary.agentPubkey,
      displayName,
      avatarUrl:
        profiles?.[normalizePubkey(summary.agentPubkey)]?.avatarUrl ?? null,
    };
  });
  const label =
    summaries.length === 1
      ? (names[0] ?? "Agent")
      : `${summaries.length} agents`;
  const elapsedLabel = formatElapsed(Math.max(0, now - earliestAnchorAt));
  const title = `${names.join(", ")} working · ${elapsedLabel}`;

  return { displayAgents, label, elapsedLabel, title };
}

export function ThreadAgentStatusChip({
  conversationId,
  profiles,
}: {
  conversationId: string | null | undefined;
  profiles?: UserProfileLookup;
}) {
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  const now = useSharedNowWhen(summaries.length > 0);
  const view = buildThreadAgentStatusChipView(summaries, profiles, now);
  if (!view) return null;

  return (
    <span
      aria-label={view.title}
      className="ml-1 inline-flex shrink-0 items-center gap-1 rounded-full border border-primary/25 bg-primary/10 px-1.5 py-0.5 text-2xs font-semibold text-primary tabular-nums"
      data-testid="thread-agent-status-chip"
      role="status"
      title={view.title}
    >
      <Loader2
        aria-hidden
        className="h-3 w-3 shrink-0 animate-spin opacity-80"
      />
      <span className="flex items-center -space-x-1">
        {view.displayAgents.map((agent) => (
          <UserAvatar
            avatarUrl={agent.avatarUrl}
            className="!h-3.5 !w-3.5 border border-background text-3xs"
            displayName={agent.displayName}
            fallbackDelayMs={0}
            key={agent.pubkey}
            size="xs"
          />
        ))}
      </span>
      <span
        className={cn(
          "min-w-0 truncate",
          "[@container(max-width:519.9px)]:hidden",
        )}
        data-testid="thread-agent-status-chip-label"
      >
        {view.label}
      </span>
      <span data-testid="thread-agent-status-chip-elapsed">
        {view.elapsedLabel}
      </span>
    </span>
  );
}
