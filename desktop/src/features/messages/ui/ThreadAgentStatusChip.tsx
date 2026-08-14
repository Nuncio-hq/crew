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
import { deriveAgentAttention } from "@/features/agents/agentAttention";
import {
  type AgentReceiptSummary,
  useLatestOwnedAgentReceiptForActiveTurns,
} from "@/features/agents/agentReceiptStore";
import { mergeOwnedAgentPubkeys } from "@/features/agents/knownAgentPubkeys";
import { useAgentObserverConnectionState } from "@/features/agents/useAgentObserverConnectionState";
import type { ConnectionState } from "@/features/agents/ui/agentSessionTypes";

const MAX_CHIP_AVATARS = 2;

export function receiptForActiveTurns(
  receipt: AgentReceiptSummary | null,
  summaries: readonly Pick<
    ActiveConversationAgentTurnSummary,
    "agentPubkey" | "runs"
  >[],
): AgentReceiptSummary | null {
  if (!receipt || summaries.length === 0) return receipt;
  const activeReceiptAgentTurns = summaries.filter(
    (summary) =>
      normalizePubkey(summary.agentPubkey) ===
      normalizePubkey(receipt.agentPubkey),
  );
  if (activeReceiptAgentTurns.length === 0) return receipt;
  return activeReceiptAgentTurns.some((summary) =>
    summary.runs.some(
      (run) =>
        run.sessionId === receipt.sessionId &&
        run.turnId === receipt.turnId &&
        run.triggeringEventIds.some(
          (eventId) =>
            eventId.toLowerCase() === receipt.parentEventId.toLowerCase(),
        ),
    ),
  )
    ? receipt
    : null;
}

export type ThreadAgentStatusChipState =
  | "needs-you"
  | "running"
  | "possibly-stalled"
  | "lost-contact"
  | "telemetry-unavailable"
  | "ready-to-review"
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
  receipt: AgentReceiptSummary | null = null,
  connectionState: ConnectionState = "open",
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

  const pubkeys =
    summaries.length > 0
      ? summaries.map((summary) => summary.agentPubkey)
      : receipt
        ? [receipt.agentPubkey]
        : outcome
          ? [outcome.agentPubkey]
          : [];
  if (pubkeys.length === 0) return null;
  const { names, displayAgents } = buildAgentSlots(pubkeys, profiles);
  const name = names[0] ?? "Agent";
  const attention = deriveAgentAttention({
    connectionState,
    needsYou: false,
    now,
    outcome: summaries.length > 0 ? null : (outcome?.outcome ?? null),
    receipt,
    turns: summaries.map((summary) => ({
      agentPubkey: summary.agentPubkey,
      anchorAt: summary.anchorAt,
      lastSeenAt: summary.lastSeenAt ?? now,
      lastSubstantiveProgressAt: summary.lastSubstantiveProgressAt ?? now,
      progressKind: summary.progressKind ?? "progress",
      progressLabel: summary.progressLabel ?? "Working",
    })),
  });
  const activeElapsed = formatElapsed(
    Math.max(
      0,
      now -
        Math.min(
          ...summaries.map((summary) => summary.anchorAt),
          Number.POSITIVE_INFINITY,
        ),
    ),
  );

  if (attention.state === "failed") {
    const ago = formatCompactAgo(Math.max(0, now - (outcome?.endedAt ?? now)));
    return {
      state: "failed",
      displayAgents,
      label: "Failed",
      elapsedLabel: ago,
      title: `${name} failed ${ago}`,
    };
  }
  if (attention.state === "telemetry-unavailable") {
    return {
      state: "telemetry-unavailable",
      displayAgents,
      label: "Telemetry unavailable",
      elapsedLabel: activeElapsed,
      title: `${name} observer telemetry is unavailable · ${activeElapsed}`,
    };
  }
  if (attention.state === "lost-contact") {
    const elapsedLabel = formatElapsed(
      Math.max(0, now - (attention.lastSeenAt ?? now)),
    );
    return {
      state: "lost-contact",
      displayAgents,
      label: "Lost contact",
      elapsedLabel,
      title: `${name} lost contact · ${elapsedLabel}`,
    };
  }
  if (attention.state === "possibly-stalled") {
    const elapsedLabel = formatElapsed(
      Math.max(0, now - (attention.lastSubstantiveProgressAt ?? now)),
    );
    return {
      state: "possibly-stalled",
      displayAgents,
      label: "Possibly stalled",
      elapsedLabel,
      title: `${name} may be stalled · ${elapsedLabel}`,
    };
  }
  if (attention.state === "ready-to-review" && receipt) {
    const elapsedLabel = formatCompactAgo(Math.max(0, now - receipt.createdAt));
    return {
      state: "ready-to-review",
      displayAgents,
      label: "Ready to review",
      elapsedLabel,
      title: `${name} is ready to review ${elapsedLabel}`,
    };
  }
  if (attention.state === "done" && receipt?.reviewed) {
    const elapsedLabel = formatCompactAgo(Math.max(0, now - receipt.createdAt));
    return {
      state: "done",
      displayAgents,
      label: "Done",
      elapsedLabel,
      title: `${name} was reviewed ${elapsedLabel}`,
    };
  }
  if (summaries.length === 0) return null;
  const label =
    summaries.length === 1
      ? (names[0] ?? "Agent")
      : `${summaries.length} agents`;
  return {
    state: "running",
    displayAgents,
    label,
    elapsedLabel: activeElapsed,
    title: `${names.join(", ")} working · ${activeElapsed}`,
  };
}

const STATE_CHROME: Record<
  ThreadAgentStatusChipState,
  { className: string; glyph: string }
> = {
  running: {
    className: "border-success/25 bg-success/10 text-success",
    glyph: "",
  },
  "needs-you": {
    className: "border-attention/35 bg-attention/10 text-attention",
    glyph: "⚠",
  },
  done: {
    className: "border-success/25 bg-success/10 text-success",
    glyph: "✓",
  },
  failed: {
    className: "border-destructive/25 bg-destructive/10 text-destructive",
    glyph: "✕",
  },
  "possibly-stalled": {
    className: "border-attention/35 bg-attention/10 text-attention",
    glyph: "!",
  },
  "lost-contact": {
    className: "border-destructive/25 bg-destructive/10 text-destructive",
    glyph: "!",
  },
  "telemetry-unavailable": {
    className: "border-muted-foreground/30 bg-muted text-muted-foreground",
    glyph: "?",
  },
  "ready-to-review": {
    className: "border-success/25 bg-success/10 text-success",
    glyph: "✓",
  },
};

export function ThreadAgentStatusChip({
  conversationId,
  currentPubkey,
  profiles,
}: {
  conversationId: string | null | undefined;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
}) {
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  const needsYou = useNeedsYouForConversation(conversationId);
  const outcome = useRecentOutcomeForConversation(conversationId);
  const ownedAgentPubkeys = mergeOwnedAgentPubkeys(profiles, currentPubkey);
  const receipt = useLatestOwnedAgentReceiptForActiveTurns(
    conversationId,
    ownedAgentPubkeys,
    summaries,
  );
  const connectionAgentPubkeys = summaries.map(
    (summary) => summary.agentPubkey,
  );
  if (outcome) connectionAgentPubkeys.push(outcome.agentPubkey);
  if (receipt) connectionAgentPubkeys.push(receipt.agentPubkey);
  const connectionState = useAgentObserverConnectionState(
    connectionAgentPubkeys,
  );
  const enabled =
    needsYou.length > 0 ||
    summaries.length > 0 ||
    outcome !== null ||
    receipt !== null;
  const now = useSharedNowWhen(enabled);
  const view = buildThreadAgentStatusChipView(
    summaries,
    outcome,
    profiles,
    now,
    needsYou,
    receipt,
    connectionState,
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
