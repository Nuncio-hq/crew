import type { SearchHit } from "@/shared/api/searchTypes";

export type DiscussionChannel = {
  id: string;
  name: string | null;
  messageCount: number;
  lastActivityAt: number;
  participants: string[];
};

export function entityDiscussionQuery(eventId: string): string {
  return eventId;
}

export function repositoryDiscussionQuery(repository: {
  owner: string;
  dtag: string;
}): string {
  return `${repository.owner} ${repository.dtag}`;
}

export function commitDiscussionQuery(commit: {
  hash: string;
  shortHash?: string | null;
}): string {
  const short = commit.shortHash ?? commit.hash.slice(0, 7);
  return !short || short === commit.hash
    ? commit.hash
    : `${commit.hash} OR ${short}`;
}

export function groupDiscussionChannels(
  hits: readonly Pick<
    SearchHit,
    "channelId" | "channelName" | "createdAt" | "pubkey"
  >[],
): DiscussionChannel[] {
  const byChannel = new Map<string, DiscussionChannel>();
  const ordered = [...hits].sort((a, b) => b.createdAt - a.createdAt);
  for (const hit of ordered) {
    if (!hit.channelId) continue;
    const pubkey = hit.pubkey.toLowerCase();
    const existing = byChannel.get(hit.channelId);
    if (existing) {
      existing.messageCount += 1;
      existing.lastActivityAt = Math.max(
        existing.lastActivityAt,
        hit.createdAt,
      );
      if (existing.name === null && hit.channelName) {
        existing.name = hit.channelName;
      }
      if (!existing.participants.includes(pubkey)) {
        existing.participants.push(pubkey);
      }
    } else {
      byChannel.set(hit.channelId, {
        id: hit.channelId,
        name: hit.channelName ?? null,
        messageCount: 1,
        lastActivityAt: hit.createdAt,
        participants: [pubkey],
      });
    }
  }
  return [...byChannel.values()].sort(
    (a, b) =>
      b.messageCount - a.messageCount || b.lastActivityAt - a.lastActivityAt,
  );
}

export function formatNameList(names: readonly string[], maxNames = 3): string {
  if (names.length === 0) return "";
  if (names.length === 1) return names[0];
  if (names.length <= maxNames) {
    return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  }
  const shown = names.slice(0, maxNames - 1);
  return `${shown.join(", ")} and ${names.length - shown.length} others`;
}

const SNIPPET_MAX_CHARS = 400;

export function discussionSnippet(content: string): string {
  const cleaned = content
    .replace(/buzz:\/\/\S+/g, "")
    .replace(/\b\d{5}:[0-9a-f]{64}:\S+/gi, "")
    .replace(/\s+/g, " ")
    .trim();
  if (cleaned.length === 0) return "Shared a link to this.";
  if (cleaned.length <= SNIPPET_MAX_CHARS) return cleaned;
  return `${cleaned.slice(0, SNIPPET_MAX_CHARS - 1).trimEnd()}…`;
}
