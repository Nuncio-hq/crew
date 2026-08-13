import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import type { MissionInboxRow } from "@/features/home/lib/missionInbox";
import type { InboxItem } from "@/features/home/lib/inbox";
import { getThreadReference } from "@/features/messages/lib/threading";
import { normalizePubkey } from "@/shared/lib/pubkey";

export type WorkbenchAgentStatus =
  | "working"
  | "sleeping"
  | "needs-you"
  | "ready"
  | "idle"
  | "failed";

export type WorkbenchAgentChip = {
  name: string;
  pubkey: string;
  status: WorkbenchAgentStatus;
};

export type WorkbenchThreadRow = {
  agents: WorkbenchAgentChip[];
  channelId: string;
  channelName: string;
  conversationId: string;
  messageEventId: string | null;
  prNumber: number | null;
  status: WorkbenchAgentStatus;
  threadRootId: string;
  title: string;
  unread: boolean;
};

export type WorkbenchChannelGroup = {
  channelId: string;
  channelName: string;
  threads: WorkbenchThreadRow[];
};

export type WorkbenchAgentGroup = {
  name: string;
  pubkey: string;
  status: WorkbenchAgentStatus;
  threads: WorkbenchThreadRow[];
};

const STATUS_RANK: Record<WorkbenchAgentStatus, number> = {
  "needs-you": 6,
  failed: 5,
  working: 4,
  sleeping: 3,
  ready: 2,
  idle: 1,
};

export function missionStateToStatus(
  state: MissionInboxRow["state"],
  sleeping: boolean,
): WorkbenchAgentStatus {
  if (sleeping) return "sleeping";
  switch (state) {
    case "needsYou":
      return "needs-you";
    case "working":
    case "possiblyStalled":
      return "working";
    case "readyToReview":
      return "ready";
    case "failed":
    case "lostContact":
      return "failed";
    case "telemetryUnavailable":
      return "idle";
    default: {
      const _exhaustive: never = state;
      return _exhaustive;
    }
  }
}

export function rollupStatus(
  statuses: readonly WorkbenchAgentStatus[],
): WorkbenchAgentStatus {
  let best: WorkbenchAgentStatus = "idle";
  for (const status of statuses) {
    if (STATUS_RANK[status] > STATUS_RANK[best]) best = status;
  }
  return best;
}

function threadRootFromInboxItem(item: InboxItem): string | null {
  const thread = getThreadReference(item.item.tags);
  return thread.rootId ?? thread.parentId ?? item.item.id;
}

function mergeAgent(
  chips: WorkbenchAgentChip[],
  next: WorkbenchAgentChip,
): WorkbenchAgentChip[] {
  const key = normalizePubkey(next.pubkey);
  const index = chips.findIndex((chip) => normalizePubkey(chip.pubkey) === key);
  if (index < 0) return [...chips, next];
  const current = chips[index];
  const status =
    STATUS_RANK[next.status] > STATUS_RANK[current.status]
      ? next.status
      : current.status;
  const copy = [...chips];
  copy[index] = {
    name: next.name || current.name,
    pubkey: current.pubkey,
    status,
  };
  return copy;
}

export function deriveWorkbenchThreadIndex(args: {
  agentNamesByPubkey?: ReadonlyMap<string, string>;
  channels: ReadonlyArray<{ id: string; name: string }>;
  inboxItems: readonly InboxItem[];
  missionRows: readonly MissionInboxRow[];
  sleepingAgentPubkeys: ReadonlySet<string>;
  unreadRootIds?: ReadonlySet<string>;
}): WorkbenchThreadRow[] {
  const channelNameById = new Map(
    args.channels.map((channel) => [channel.id, channel.name]),
  );
  const byKey = new Map<string, WorkbenchThreadRow>();

  const upsert = (row: WorkbenchThreadRow) => {
    const existing = byKey.get(row.conversationId);
    if (!existing) {
      byKey.set(row.conversationId, row);
      return;
    }
    const agents = row.agents.reduce(mergeAgent, [...existing.agents]);
    byKey.set(row.conversationId, {
      ...existing,
      agents,
      messageEventId: row.messageEventId ?? existing.messageEventId,
      status: rollupStatus(agents.map((agent) => agent.status)),
      title: existing.title || row.title,
      unread: existing.unread || row.unread,
    });
  };

  for (const row of args.missionRows) {
    if (!row.channelId || !row.rootEventId) continue;
    const conversationId =
      row.conversationId ||
      deriveAgentConversationIdOrNull(row.channelId, row.rootEventId) ||
      `${row.channelId}:${row.rootEventId}`;
    const sleeping = args.sleepingAgentPubkeys.has(
      normalizePubkey(row.agentPubkey),
    );
    const status = missionStateToStatus(row.state, sleeping);
    const pubkey = normalizePubkey(row.agentPubkey);
    upsert({
      agents: pubkey
        ? [
            {
              name: args.agentNamesByPubkey?.get(pubkey) ?? row.agentPubkey,
              pubkey,
              status,
            },
          ]
        : [],
      channelId: row.channelId,
      channelName: channelNameById.get(row.channelId) ?? row.channelId,
      conversationId,
      messageEventId: row.messageEventId,
      prNumber: null,
      status,
      threadRootId: row.rootEventId,
      title: row.threadTitle,
      unread: args.unreadRootIds?.has(row.rootEventId) ?? false,
    });
  }

  for (const item of args.inboxItems) {
    const channelId = item.item.channelId;
    if (!channelId) continue;
    const threadRootId = threadRootFromInboxItem(item);
    if (!threadRootId) continue;
    const conversationId =
      deriveAgentConversationIdOrNull(channelId, threadRootId) ??
      item.conversationId;
    upsert({
      agents: [],
      channelId,
      channelName:
        channelNameById.get(channelId) ??
        item.channelLabel ??
        item.item.channelName,
      conversationId,
      messageEventId: item.id,
      prNumber: null,
      status: "idle",
      threadRootId,
      title: item.subject || item.preview,
      unread: item.unreadCount > 0,
    });
  }

  return [...byKey.values()].sort((left, right) => {
    const status = STATUS_RANK[right.status] - STATUS_RANK[left.status];
    if (status !== 0) return status;
    return left.title.localeCompare(right.title);
  });
}

export function groupWorkbenchByChannel(
  rows: readonly WorkbenchThreadRow[],
): WorkbenchChannelGroup[] {
  const groups = new Map<string, WorkbenchChannelGroup>();
  for (const row of rows) {
    const existing = groups.get(row.channelId);
    if (existing) {
      existing.threads.push(row);
      continue;
    }
    groups.set(row.channelId, {
      channelId: row.channelId,
      channelName: row.channelName,
      threads: [row],
    });
  }
  return [...groups.values()].sort((left, right) =>
    left.channelName.localeCompare(right.channelName),
  );
}

export function groupWorkbenchByAgent(
  rows: readonly WorkbenchThreadRow[],
): WorkbenchAgentGroup[] {
  const groups = new Map<string, WorkbenchAgentGroup>();
  for (const row of rows) {
    const agents =
      row.agents.length > 0
        ? row.agents
        : [
            {
              name: "Unassigned",
              pubkey: "",
              status: row.status,
            },
          ];
    for (const agent of agents) {
      const key = agent.pubkey || "unassigned";
      const existing = groups.get(key);
      if (existing) {
        existing.threads.push(row);
        existing.status = rollupStatus([existing.status, agent.status]);
        continue;
      }
      groups.set(key, {
        name: agent.name,
        pubkey: agent.pubkey,
        status: agent.status,
        threads: [row],
      });
    }
  }
  return [...groups.values()].sort((left, right) => {
    const status = STATUS_RANK[right.status] - STATUS_RANK[left.status];
    if (status !== 0) return status;
    return left.name.localeCompare(right.name);
  });
}

export function findWorkbenchRow(
  rows: readonly WorkbenchThreadRow[],
  channelId: string,
  threadRootId: string,
): WorkbenchThreadRow | null {
  return (
    rows.find(
      (row) => row.channelId === channelId && row.threadRootId === threadRootId,
    ) ?? null
  );
}

export function ensureSelectedWorkbenchRow(
  rows: readonly WorkbenchThreadRow[],
  selected: {
    channelId: string;
    threadRootId: string;
    channelName: string;
  } | null,
): WorkbenchThreadRow[] {
  if (!selected) return [...rows];
  if (findWorkbenchRow(rows, selected.channelId, selected.threadRootId)) {
    return [...rows];
  }
  const conversationId =
    deriveAgentConversationIdOrNull(
      selected.channelId,
      selected.threadRootId,
    ) ?? `${selected.channelId}:${selected.threadRootId}`;
  return [
    ...rows,
    {
      agents: [],
      channelId: selected.channelId,
      channelName: selected.channelName,
      conversationId,
      messageEventId: null,
      prNumber: null,
      status: "idle",
      threadRootId: selected.threadRootId,
      title: "Open thread",
      unread: false,
    },
  ];
}
