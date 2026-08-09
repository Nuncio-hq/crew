import type { RelaySubscriptionFilter } from "@/shared/api/relayClientShared";
import type { RelayEvent } from "@/shared/api/types";
import { getThreadReference } from "@/features/messages/lib/threading";

/** Exact ids authorize a producer-kind-independent relationship lookup. */
export function buildReceiptParentFilter(
  ids: string[],
): RelaySubscriptionFilter {
  return { ids, limit: ids.length };
}

export async function fetchRelayEventAncestry(
  fetchEvents: (filter: RelaySubscriptionFilter) => Promise<RelayEvent[]>,
  seedIds: readonly string[],
  batchSize = 100,
): Promise<Map<string, RelayEvent>> {
  const eventsById = new Map<string, RelayEvent>();
  let frontier = [...new Set(seedIds.filter(Boolean))];
  let depth = 0;
  while (frontier.length > 0) {
    if (depth >= 1_000) throw new Error("relay ancestry exceeds safe depth");
    depth += 1;
    const next = new Set<string>();
    for (let offset = 0; offset < frontier.length; offset += batchSize) {
      const ids = frontier.slice(offset, offset + batchSize);
      const events = await fetchEvents(buildReceiptParentFilter(ids));
      const requested = new Set(ids);
      if (events.some((event) => !requested.has(event.id))) {
        throw new Error(
          "history unavailable: relay returned unsolicited ancestry",
        );
      }
      const found = new Set(events.map((event) => event.id));
      const missing = ids.filter((id) => !found.has(id));
      if (missing.length > 0) {
        throw new Error(
          `history unavailable: ${missing.length} ancestry event(s) missing`,
        );
      }
      for (const event of events) {
        eventsById.set(event.id, event);
        const thread = getThreadReference(event.tags);
        for (const relatedId of [thread.parentId, thread.rootId]) {
          if (
            relatedId &&
            relatedId !== event.id &&
            !eventsById.has(relatedId)
          ) {
            next.add(relatedId);
          }
        }
      }
    }
    frontier = [...next];
  }
  return eventsById;
}

const LOWER_HEX_64 = /^[0-9a-f]{64}$/;

function singleTag(event: RelayEvent, name: string): string | null {
  const tags = event.tags.filter((tag) => tag[0] === name);
  return tags.length === 1 && tags[0]?.length === 2
    ? (tags[0][1] ?? null)
    : null;
}

function canonicalThread(event: RelayEvent) {
  const tags = event.tags.filter((tag) => tag[0] === "e");
  if (tags.length === 0) return { rootId: null, parentId: null };
  if (tags.length > 2) return null;
  let rootId: string | null = null;
  let parentId: string | null = null;
  for (const tag of tags) {
    if (tag.length !== 4 || !LOWER_HEX_64.test(tag[1] ?? "")) return null;
    if (tag[3] === "root" && rootId === null) rootId = tag[1] ?? null;
    else if (tag[3] === "reply" && parentId === null) {
      parentId = tag[1] ?? null;
    } else return null;
  }
  if (parentId !== null && rootId === null) rootId = parentId;
  if (rootId !== null && parentId === null) parentId = rootId;
  return { rootId, parentId };
}

export function validatesCanonicalEventAncestry(
  start: RelayEvent,
  rootEventId: string,
  channelId: string,
  eventsById: ReadonlyMap<string, RelayEvent>,
): boolean {
  let current: RelayEvent | undefined = start;
  const visited = new Set<string>();
  for (let depth = 0; current && depth <= 1_000; depth += 1) {
    if (visited.has(current.id) || singleTag(current, "h") !== channelId) {
      return false;
    }
    visited.add(current.id);
    const thread = canonicalThread(current);
    if (!thread) return false;
    if (current.id === rootEventId) {
      return thread.rootId === null && thread.parentId === null;
    }
    if (thread.rootId !== rootEventId || thread.parentId === null) return false;
    current = eventsById.get(thread.parentId);
  }
  return false;
}
