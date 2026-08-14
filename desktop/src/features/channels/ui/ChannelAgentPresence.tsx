import * as React from "react";
import { MoreHorizontal } from "lucide-react";

import {
  useChannelAgentPresence,
  type ChannelAgentPresence as ChannelAgentPresenceEntry,
  type ChannelAgentRosterEntry,
} from "@/features/agents/channelAgentPresence";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import {
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import {
  formatCompactAgo,
  formatElapsed,
} from "@/features/agents/ui/agentSessionUtils";
import type { TimelineMessage } from "@/features/messages/types";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { useNow } from "@/shared/lib/useNow";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { UserAvatar } from "@/shared/ui/UserAvatar";

const MAX_VISIBLE_AGENTS = 4;

const STATE_META: Record<
  ChannelAgentPresenceEntry["state"],
  { label: string; dotClass: string; animationClass: string }
> = {
  "needs-you": {
    label: "needs you",
    dotClass: "bg-attention",
    animationClass: "motion-safe:animate-pulse",
  },
  working: {
    label: "working",
    dotClass: "bg-blue-500",
    animationClass: "animate-pulse",
  },
  "done-recent": {
    label: "finished",
    dotClass: "bg-success",
    animationClass: "",
  },
  idle: {
    label: "idle",
    dotClass: "bg-muted-foreground/50",
    animationClass: "",
  },
};

type ChannelAgentPresenceProps = {
  channelId: string;
  onOpenThread: (message: TimelineMessage) => void;
  timelineMessages: readonly TimelineMessage[];
};

type AgentRoster = ChannelAgentRosterEntry & {
  displayName: string;
  avatarUrl: string | null;
};

export function ChannelAgentPresence({
  channelId,
  onOpenThread,
  timelineMessages,
}: ChannelAgentPresenceProps) {
  const membersQuery = useChannelMembersQuery(channelId);
  const managedAgentsQuery = useManagedAgentsQuery();
  const relayAgentsQuery = useRelayAgentsQuery();
  const roster = React.useMemo<AgentRoster[]>(() => {
    const managedByPubkey = new Map(
      (managedAgentsQuery.data ?? []).map((agent) => [
        normalizePubkey(agent.pubkey),
        agent,
      ]),
    );
    const relayByPubkey = new Map(
      (relayAgentsQuery.data ?? [])
        .filter((agent) => agent.channelIds.includes(channelId))
        .map((agent) => [normalizePubkey(agent.pubkey), agent]),
    );
    return (membersQuery.data ?? [])
      .filter((member) => member.isAgent || member.role === "bot")
      .map((member) => {
        const pubkey = normalizePubkey(member.pubkey);
        const agent = managedByPubkey.get(pubkey) ?? relayByPubkey.get(pubkey);
        return {
          agentPubkey: pubkey,
          displayName:
            member.displayName ?? agent?.name ?? truncatePubkey(pubkey),
          avatarUrl: agent && "avatarUrl" in agent ? agent.avatarUrl : null,
        };
      })
      .sort((left, right) => left.agentPubkey.localeCompare(right.agentPubkey));
  }, [
    channelId,
    membersQuery.data,
    managedAgentsQuery.data,
    relayAgentsQuery.data,
  ]);
  const now = useNow(1_000);
  const presence = useChannelAgentPresence(channelId, roster, now);
  if (presence.length === 0) return null;

  const visible = presence.slice(0, MAX_VISIBLE_AGENTS);
  const hidden = presence.slice(MAX_VISIBLE_AGENTS);
  const rosterByPubkey = new Map(
    roster.map((entry) => [entry.agentPubkey, entry]),
  );

  return (
    <div
      className="flex min-w-0 items-center gap-0.5"
      data-testid="channel-agent-presence"
    >
      {visible.map((entry) => (
        <PresenceAvatar
          entry={entry}
          key={entry.agentPubkey}
          onOpenThread={onOpenThread}
          rosterEntry={rosterByPubkey.get(entry.agentPubkey)}
          conversationMessage={getConversationMessage(
            entry.conversationId,
            channelId,
            timelineMessages,
          )}
        />
      ))}
      {hidden.length > 0 ? (
        <Popover>
          <PopoverTrigger asChild>
            <button
              aria-label={`Show ${hidden.length} more agents`}
              className="inline-flex h-7 items-center gap-0.5 rounded-full px-1.5 text-2xs font-semibold text-muted-foreground hover:bg-accent hover:text-accent-foreground"
              data-testid="channel-agent-presence-more"
              type="button"
            >
              <MoreHorizontal className="h-3.5 w-3.5" />+{hidden.length}
            </button>
          </PopoverTrigger>
          <PopoverContent
            align="end"
            className="w-72 p-1"
            side="bottom"
            sideOffset={6}
          >
            <div className="flex flex-col gap-0.5">
              {hidden.map((entry) => (
                <PresenceListItem
                  entry={entry}
                  key={entry.agentPubkey}
                  onOpenThread={onOpenThread}
                  rosterEntry={rosterByPubkey.get(entry.agentPubkey)}
                  conversationMessage={getConversationMessage(
                    entry.conversationId,
                    channelId,
                    timelineMessages,
                  )}
                />
              ))}
            </div>
          </PopoverContent>
        </Popover>
      ) : null}
    </div>
  );
}

function getConversationMessage(
  conversationId: string | null,
  channelId: string,
  messages: readonly TimelineMessage[],
): TimelineMessage | null {
  if (!conversationId) return null;
  return (
    messages.find(
      (entry) =>
        deriveAgentConversationIdOrNull(channelId, entry.id) === conversationId,
    ) ?? null
  );
}

function activityLabel(
  entry: ChannelAgentPresenceEntry,
  message: TimelineMessage | null,
) {
  const meta = STATE_META[entry.state];
  const elapsed =
    entry.since === null
      ? ""
      : entry.state === "working" || entry.state === "needs-you"
        ? formatElapsed(Math.max(0, Date.now() - entry.since))
        : formatCompactAgo(Math.max(0, Date.now() - entry.since));
  const title = message?.body?.split("\n")[0]?.trim() || null;
  return [meta.label, elapsed, title].filter(Boolean).join(" · ");
}

function PresenceAvatar({
  entry,
  rosterEntry,
  conversationMessage,
  onOpenThread,
}: {
  entry: ChannelAgentPresenceEntry;
  rosterEntry?: AgentRoster;
  conversationMessage: TimelineMessage | null;
  onOpenThread: (message: TimelineMessage) => void;
}) {
  const label = activityLabel(entry, conversationMessage);
  return (
    <div className="group relative flex items-center" title={label}>
      <button
        aria-label={label}
        className={cn(
          "relative rounded-full p-0.5 transition-[padding,background-color] hover:bg-accent",
          entry.state === "needs-you" && "pr-2.5",
        )}
        data-testid={`channel-agent-presence-${entry.agentPubkey}`}
        onClick={() => {
          if (conversationMessage) onOpenThread(conversationMessage);
        }}
        type="button"
      >
        <UserAvatar
          avatarUrl={rosterEntry?.avatarUrl ?? null}
          className="h-6 w-6 text-3xs"
          displayName={
            rosterEntry?.displayName ?? truncatePubkey(entry.agentPubkey)
          }
          fallbackDelayMs={0}
          size="xs"
        />
        <StatusDot entry={entry} />
        <span className="sr-only">{label}</span>
      </button>
      <span
        className={cn(
          "pointer-events-none absolute left-full z-10 ml-1 max-w-48 truncate whitespace-nowrap rounded bg-popover px-1.5 py-0.5 text-2xs text-popover-foreground opacity-0 shadow-sm transition-opacity group-hover:opacity-100",
          entry.state === "needs-you" && "static opacity-100",
        )}
      >
        {label}
      </span>
    </div>
  );
}

function PresenceListItem({
  entry,
  rosterEntry,
  conversationMessage,
  onOpenThread,
}: {
  entry: ChannelAgentPresenceEntry;
  rosterEntry?: AgentRoster;
  conversationMessage: TimelineMessage | null;
  onOpenThread: (message: TimelineMessage) => void;
}) {
  const label = activityLabel(entry, conversationMessage);
  return (
    <button
      aria-label={label}
      className="flex items-center gap-2 rounded-lg px-2 py-1.5 text-left hover:bg-accent"
      onClick={() => {
        if (conversationMessage) onOpenThread(conversationMessage);
      }}
      type="button"
    >
      <span className="relative shrink-0">
        <UserAvatar
          avatarUrl={rosterEntry?.avatarUrl ?? null}
          className="h-6 w-6 text-3xs"
          displayName={
            rosterEntry?.displayName ?? truncatePubkey(entry.agentPubkey)
          }
          fallbackDelayMs={0}
          size="xs"
        />
        <StatusDot entry={entry} />
      </span>
      <span className="min-w-0 truncate text-xs">{label}</span>
    </button>
  );
}

function StatusDot({ entry }: { entry: ChannelAgentPresenceEntry }) {
  const meta = STATE_META[entry.state];
  return (
    <span
      aria-hidden
      className={cn(
        "absolute bottom-0 right-0 h-2 w-2 rounded-full border-2 border-background motion-reduce:animate-none",
        meta.dotClass,
        meta.animationClass,
      )}
      data-testid={`channel-agent-presence-dot-${entry.state}`}
    />
  );
}
