import * as React from "react";

import {
  type AgentThreadDigest,
  type ConversationRef,
  useAgentThreadDigestForChannel,
} from "@/features/agents/agentThreadDigestForChannel";
import {
  formatCompactAgo,
  formatElapsed,
} from "@/features/agents/ui/agentSessionUtils";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { UserAvatar } from "@/shared/ui/UserAvatar";

/** Shared 1s clock while any digest strip is mounted. */
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

function useSharedNow(): number {
  return React.useSyncExternalStore(
    subscribeSharedNow,
    getSharedNowSnapshot,
    getSharedNowSnapshot,
  );
}

type DigestBucket = "needsYou" | "running" | "failed" | "done";

const BUCKET_META: Record<
  DigestBucket,
  {
    glyph: string;
    word: string;
    pillClass: string;
    testId: string;
  }
> = {
  needsYou: {
    glyph: "⚠",
    word: "needs you",
    pillClass:
      "border-attention/35 bg-attention/10 text-attention hover:bg-attention/15 dark:text-attention",
    testId: "channel-agent-digest-pill-needs-you",
  },
  running: {
    glyph: "⟳",
    word: "running",
    pillClass:
      "border-primary/25 bg-primary/10 text-primary hover:bg-primary/15",
    testId: "channel-agent-digest-pill-running",
  },
  failed: {
    glyph: "✕",
    word: "failed",
    pillClass:
      "border-destructive/25 bg-destructive/10 text-destructive hover:bg-destructive/15",
    testId: "channel-agent-digest-pill-failed",
  },
  done: {
    glyph: "✓",
    word: "done",
    pillClass:
      "border-success/25 bg-success/10 text-success hover:bg-success/15 dark:text-success",
    testId: "channel-agent-digest-pill-done",
  },
};

function agentLabel(
  pubkey: string,
  profiles: UserProfileLookup | undefined,
): string {
  const profile = profiles?.[normalizePubkey(pubkey)];
  return profile?.displayName ?? profile?.name ?? truncatePubkey(pubkey);
}

type ChannelAgentDigestProps = {
  channelId: string | null | undefined;
  onOpenThread: (conversationId: string) => void;
  profiles?: UserProfileLookup;
};

export function ChannelAgentDigest({
  channelId,
  onOpenThread,
  profiles,
}: ChannelAgentDigestProps) {
  const digest = useAgentThreadDigestForChannel(channelId);
  if (!digest) return null;
  return (
    <ChannelAgentDigestView
      digest={digest}
      onOpenThread={onOpenThread}
      profiles={profiles}
    />
  );
}

/** Presentational strip — exported for empty-null tests without a store. */
export function ChannelAgentDigestView({
  digest,
  onOpenThread,
  profiles,
}: {
  digest: AgentThreadDigest;
  onOpenThread: (conversationId: string) => void;
  profiles?: UserProfileLookup;
}) {
  const now = useSharedNow();

  const buckets: Array<{ key: DigestBucket; refs: ConversationRef[] }> = [
    { key: "needsYou", refs: digest.needsYou ?? [] },
    { key: "running", refs: digest.running },
    { key: "failed", refs: digest.failed },
    { key: "done", refs: digest.done },
  ];
  const visible = buckets.filter((bucket) => bucket.refs.length > 0);
  if (visible.length === 0) return null;

  return (
    <div
      className="@container flex min-w-0 flex-wrap items-center gap-1.5 border-b border-border/40 px-3 py-1.5"
      data-testid="channel-agent-digest"
    >
      {visible.map((bucket) => (
        <DigestPill
          key={bucket.key}
          bucket={bucket.key}
          now={now}
          onOpenThread={onOpenThread}
          profiles={profiles}
          refs={bucket.refs}
        />
      ))}
    </div>
  );
}

function DigestPill({
  bucket,
  refs,
  now,
  profiles,
  onOpenThread,
}: {
  bucket: DigestBucket;
  refs: ConversationRef[];
  now: number;
  profiles: UserProfileLookup | undefined;
  onOpenThread: (conversationId: string) => void;
}) {
  const [open, setOpen] = React.useState(false);
  const meta = BUCKET_META[bucket];
  const count = refs.length;
  const ariaLabel = `${count} ${meta.word}`;

  return (
    <Popover onOpenChange={setOpen} open={open}>
      <PopoverTrigger asChild>
        <button
          aria-label={ariaLabel}
          className={cn(
            "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-2xs font-semibold tabular-nums transition-colors",
            meta.pillClass,
          )}
          data-testid={meta.testId}
          type="button"
        >
          <span aria-hidden>{meta.glyph}</span>
          <span>{count}</span>
          <span className="[@container(max-width:419.9px)]:hidden">
            {meta.word}
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-72 p-1"
        onOpenAutoFocus={(event) => event.preventDefault()}
        side="bottom"
        sideOffset={6}
      >
        <div className="px-2 py-1 text-xs font-medium text-muted-foreground">
          {ariaLabel}
        </div>
        <div className="mt-1 flex max-h-64 flex-col gap-0.5 overflow-y-auto">
          {refs.map((ref) => (
            <DigestItem
              bucket={bucket}
              key={ref.conversationId}
              now={now}
              onOpen={() => {
                setOpen(false);
                onOpenThread(ref.conversationId);
              }}
              profiles={profiles}
              refEntry={ref}
            />
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}

function DigestItem({
  bucket,
  refEntry,
  now,
  profiles,
  onOpen,
}: {
  bucket: DigestBucket;
  refEntry: ConversationRef;
  now: number;
  profiles: UserProfileLookup | undefined;
  onOpen: () => void;
}) {
  const meta = BUCKET_META[bucket];
  const pubkeys = refEntry.agentPubkeys;
  const names = pubkeys.map((pubkey) => agentLabel(pubkey, profiles));
  const timeLabel =
    bucket === "running" || bucket === "needsYou"
      ? formatElapsed(Math.max(0, now - refEntry.anchorAt))
      : formatCompactAgo(Math.max(0, now - refEntry.anchorAt));
  const title =
    bucket === "running"
      ? `${names.join(", ")} working · ${timeLabel}`
      : bucket === "needsYou"
        ? `${names.join(", ")} is waiting for your approval · ${timeLabel}`
        : bucket === "failed"
          ? `${names.join(", ")} failed ${timeLabel}`
          : `${names.join(", ")} finished ${timeLabel}`;

  return (
    <button
      aria-label={title}
      className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors hover:bg-accent hover:text-accent-foreground"
      data-testid="channel-agent-digest-item"
      onClick={onOpen}
      title={title}
      type="button"
    >
      <span aria-hidden className="shrink-0 text-2xs font-semibold">
        {meta.glyph}
      </span>
      <span className="flex shrink-0 items-center -space-x-1">
        {pubkeys.slice(0, 2).map((pubkey) => {
          const displayName = agentLabel(pubkey, profiles);
          return (
            <UserAvatar
              avatarUrl={profiles?.[normalizePubkey(pubkey)]?.avatarUrl ?? null}
              className="!h-5 !w-5 border border-background text-3xs"
              displayName={displayName}
              fallbackDelayMs={0}
              key={pubkey}
              size="xs"
            />
          );
        })}
      </span>
      <span className="min-w-0 flex-1 truncate">{names.join(", ")}</span>
      <span className="shrink-0 text-2xs tabular-nums text-muted-foreground">
        {timeLabel}
      </span>
    </button>
  );
}
