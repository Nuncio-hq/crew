import { Loader2 } from "lucide-react";

import {
  type NeedsYouRequest,
  useNeedsYouForConversation,
} from "@/features/agents/needsYouStore";
import {
  type ActiveConversationAgentTurnSummary,
  useActiveTurnSummariesForConversation,
} from "@/features/agents/activeConversationAgentTurnSummaries";
import {
  type RecentConversationOutcome,
  useRecentOutcomeForConversation,
} from "@/features/agents/recentConversationOutcomes";
import {
  formatCompactAgo,
  formatElapsed,
} from "@/features/agents/ui/agentSessionUtils";
import { ThreadAgentActivityHeadline } from "@/features/messages/ui/ThreadAgentActivityHeadline";
import { useThreadAgentActivityHeadline } from "@/features/messages/ui/conversationActivityHeadline";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { cn } from "@/shared/lib/cn";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import { useSharedNowWhen } from "@/features/agents/lib/sharedNow";

const MAX_CHIP_AVATARS = 2;

export type ThreadAgentStatusChipState =
  | "needs-you"
  | "running"
  | "done"
  | "failed";

export type ThreadAgentStatusChipView = {
  state: ThreadAgentStatusChipState;
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

function buildAgentSlots(
  pubkeys: readonly string[],
  profiles: UserProfileLookup | undefined,
) {
  const names = pubkeys.map((pubkey) => agentDisplayName(pubkey, profiles));
  const displayAgents = pubkeys.slice(0, MAX_CHIP_AVATARS).map((pubkey) => {
    const displayName = agentDisplayName(pubkey, profiles);
    return {
      pubkey,
      displayName,
      avatarUrl: profiles?.[normalizePubkey(pubkey)]?.avatarUrl ?? null,
    };
  });
  return { names, displayAgents };
}

/** Pure view model for the chip — unit-tested without a React tree. */
export function buildThreadAgentStatusChipView(
  summaries: readonly ActiveConversationAgentTurnSummary[],
  outcome: RecentConversationOutcome | null,
  profiles: UserProfileLookup | undefined,
  now: number,
  needsYou: readonly NeedsYouRequest[] = [],
): ThreadAgentStatusChipView | null {
  if (needsYou.length > 0) {
    const earliest = needsYou.reduce((oldest, request) =>
      request.createdAt < oldest.createdAt ? request : oldest,
    );
    const { names, displayAgents } = buildAgentSlots(
      needsYou.map((request) => request.agentPubkey),
      profiles,
    );
    const elapsedLabel = formatElapsed(Math.max(0, now - earliest.createdAt));
    const name = names[0] ?? "Agent";
    return {
      state: "needs-you",
      displayAgents,
      label: "Needs you",
      elapsedLabel,
      title: `${name} is waiting for your approval · ${elapsedLabel}`,
    };
  }

  // Priority: running > failed > done.
  if (summaries.length > 0) {
    let earliestAnchorAt = Number.POSITIVE_INFINITY;
    for (const summary of summaries) {
      if (summary.anchorAt < earliestAnchorAt) {
        earliestAnchorAt = summary.anchorAt;
      }
    }
    const { names, displayAgents } = buildAgentSlots(
      summaries.map((summary) => summary.agentPubkey),
      profiles,
    );
    const label =
      summaries.length === 1
        ? (names[0] ?? "Agent")
        : `${summaries.length} agents`;
    const elapsedLabel = formatElapsed(Math.max(0, now - earliestAnchorAt));
    const title = `${names.join(", ")} working · ${elapsedLabel}`;
    return {
      state: "running",
      displayAgents,
      label,
      elapsedLabel,
      title,
    };
  }

  if (!outcome) return null;

  const { names, displayAgents } = buildAgentSlots(
    [outcome.agentPubkey],
    profiles,
  );
  const name = names[0] ?? "Agent";
  const ago = formatCompactAgo(Math.max(0, now - outcome.endedAt));

  if (outcome.outcome === "error") {
    return {
      state: "failed",
      displayAgents,
      label: "Failed",
      elapsedLabel: ago,
      title: `${name} failed ${ago}`,
    };
  }

  return {
    state: "done",
    displayAgents,
    label: "Done",
    elapsedLabel: ago,
    title: `${name} finished ${ago}`,
  };
}

const STATE_CHROME: Record<
  ThreadAgentStatusChipState,
  { className: string; glyph: string }
> = {
  running: {
    className: "border-primary/25 bg-primary/10 text-primary",
    glyph: "",
  },
  "needs-you": {
    className:
      "border-amber-500/35 bg-amber-500/10 text-amber-600 dark:text-amber-400",
    glyph: "⚠",
  },
  done: {
    className:
      "border-emerald-500/25 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
    glyph: "✓",
  },
  failed: {
    className: "border-destructive/25 bg-destructive/10 text-destructive",
    glyph: "✕",
  },
};

export function ThreadAgentStatusChip({
  conversationId,
  profiles,
}: {
  conversationId: string | null | undefined;
  profiles?: UserProfileLookup;
}) {
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  const needsYou = useNeedsYouForConversation(conversationId);
  const outcome = useRecentOutcomeForConversation(conversationId);
  const enabled =
    needsYou.length > 0 || summaries.length > 0 || outcome !== null;
  const now = useSharedNowWhen(enabled);
  const view = buildThreadAgentStatusChipView(
    summaries,
    outcome,
    profiles,
    now,
    needsYou,
  );
  const isRunning = view?.state === "running";
  // Transcript subscription only while ≥1 running chip is mounted.
  const activity = useThreadAgentActivityHeadline(
    conversationId,
    Boolean(isRunning),
    profiles,
  );
  if (!view) return null;

  const chrome = STATE_CHROME[view.state];
  const title =
    isRunning && activity?.latest
      ? `${view.title} · ${activity.latest}`
      : view.title;

  return (
    <>
      <span
        aria-label={title}
        className={cn(
          "ml-1 inline-flex shrink-0 items-center gap-1 rounded-full border px-1.5 py-0.5 text-2xs font-semibold tabular-nums",
          chrome.className,
          view.state === "needs-you" && "motion-safe:animate-pulse",
        )}
        data-state={view.state}
        data-testid="thread-agent-status-chip"
        role="status"
        title={title}
      >
        {view.state === "running" ? (
          <Loader2
            aria-hidden
            className="h-3 w-3 shrink-0 animate-spin opacity-80"
          />
        ) : (
          <span aria-hidden className="shrink-0 text-2xs leading-none">
            {chrome.glyph}
          </span>
        )}
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
      {isRunning ? <ThreadAgentActivityHeadline selection={activity} /> : null}
    </>
  );
}
