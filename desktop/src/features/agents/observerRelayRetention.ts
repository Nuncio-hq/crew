import { normalizePubkey } from "@/shared/lib/pubkey";
import { observerEventIdentity } from "./observerEventIdentity";
import type { ObserverEvent } from "./ui/agentSessionTypes";

export const MAX_ARCHIVE_EVENTS_PER_CHANNEL = 3000;
export const MAX_ARCHIVE_CHANNELS = 12;
export const OBSERVER_AGENT_IDLE_RETENTION_MS = 5 * 60_000;

type ObserverOrderFields = Pick<
  ObserverEvent,
  "agentIndex" | "seq" | "sessionId" | "sourceEventId" | "timestamp" | "turnId"
>;

function compareObserverCausalOrder(
  left: ObserverOrderFields,
  right: ObserverOrderFields,
) {
  const sameAgentIndex = left.agentIndex === right.agentIndex;
  const sameSession =
    sameAgentIndex &&
    left.sessionId != null &&
    left.sessionId === right.sessionId;
  const sameTurn =
    sameAgentIndex && left.turnId != null && left.turnId === right.turnId;
  if ((sameSession || sameTurn) && left.seq !== right.seq) {
    return left.seq - right.seq;
  }
  const leftTime = Date.parse(left.timestamp);
  const rightTime = Date.parse(right.timestamp);
  if (Number.isFinite(leftTime) && Number.isFinite(rightTime)) {
    const timeDiff = leftTime - rightTime;
    if (timeDiff !== 0) return timeDiff;
  }

  return (
    (left.agentIndex ?? -1) - (right.agentIndex ?? -1) ||
    (left.sessionId ?? "").localeCompare(right.sessionId ?? "") ||
    (left.turnId ?? "").localeCompare(right.turnId ?? "") ||
    (left.sourceEventId ?? "").localeCompare(right.sourceEventId ?? "") ||
    left.seq - right.seq
  );
}

/** Sort observer frames in stable producer-causal order. */
export function compareObserverEvents(
  left: ObserverEvent,
  right: ObserverEvent,
) {
  const causalOrder = compareObserverCausalOrder(left, right);
  if (causalOrder !== 0) return causalOrder;
  return observerEventIdentity(left).localeCompare(
    observerEventIdentity(right),
  );
}

/** Whether a candidate frame advances beyond a stored frame. */
export function isObserverEventAfter(
  candidate: ObserverOrderFields,
  stored: ObserverOrderFields,
): boolean {
  return compareObserverCausalOrder(candidate, stored) > 0;
}

const archiveEventsByChannel = new Map<string, ObserverEvent[]>();
const archiveReadTickByChannel = new Map<string, number>();
let archiveReadTick = 0;

function archiveChannelKey(agentPubkey: string, channelId: string): string {
  return `${normalizePubkey(agentPubkey)}:${channelId}`;
}

function touchArchiveKey(key: string) {
  archiveReadTick += 1;
  archiveReadTickByChannel.set(key, archiveReadTick);
}

function evictArchiveKeysOverBudget() {
  while (archiveEventsByChannel.size > MAX_ARCHIVE_CHANNELS) {
    let leastRecentKey: string | null = null;
    let leastRecentTick = Number.POSITIVE_INFINITY;
    for (const key of archiveEventsByChannel.keys()) {
      const tick = archiveReadTickByChannel.get(key) ?? 0;
      if (tick < leastRecentTick) {
        leastRecentKey = key;
        leastRecentTick = tick;
      }
    }
    if (!leastRecentKey) return;
    archiveEventsByChannel.delete(leastRecentKey);
    archiveReadTickByChannel.delete(leastRecentKey);
  }
}

/** Append one archive page with one deduplication pass and one sort. */
export function appendArchivedChannelEvents(
  agentPubkey: string,
  channelId: string,
  events: readonly ObserverEvent[],
): boolean {
  if (events.length === 0) return false;
  const key = archiveChannelKey(agentPubkey, channelId);
  const current = archiveEventsByChannel.get(key) ?? [];
  const seen = new Set(current.map(observerEventIdentity));
  const added: ObserverEvent[] = [];
  for (const event of events) {
    const identity = observerEventIdentity(event);
    if (seen.has(identity)) continue;
    seen.add(identity);
    added.push(event);
  }
  if (added.length === 0) return false;

  const sorted = [...current, ...added].sort(compareObserverEvents);
  const retained =
    sorted.length > MAX_ARCHIVE_EVENTS_PER_CHANNEL
      ? sorted.slice(sorted.length - MAX_ARCHIVE_EVENTS_PER_CHANNEL)
      : sorted;
  const changed =
    retained.length !== current.length ||
    retained.some(
      (event, index) =>
        observerEventIdentity(event) !==
        observerEventIdentity(current[index] as ObserverEvent),
    );
  if (!changed) return false;

  const isNewKey = !archiveEventsByChannel.has(key);
  archiveEventsByChannel.set(key, retained);
  if (isNewKey) touchArchiveKey(key);
  evictArchiveKeysOverBudget();
  return true;
}

/** Append one event through the bounded archive-channel path. */
export function appendArchivedChannelEvent(
  agentPubkey: string,
  channelId: string,
  event: ObserverEvent,
): boolean {
  return appendArchivedChannelEvents(agentPubkey, channelId, [event]);
}

/** Read and touch one archive key, or return the provided empty sentinel. */
export function readArchivedChannelEvents(
  agentPubkey: string,
  channelId: string,
  emptyEvents: ObserverEvent[],
): ObserverEvent[] {
  const key = archiveChannelKey(agentPubkey, channelId);
  const events = archiveEventsByChannel.get(key);
  if (!events) return emptyEvents;
  touchArchiveKey(key);
  return events;
}

/** Read one archive key without changing its LRU position. */
export function peekArchivedChannelEvents(
  agentPubkey: string,
  channelId: string,
): ObserverEvent[] {
  return (
    archiveEventsByChannel.get(archiveChannelKey(agentPubkey, channelId)) ?? []
  );
}

/** Reset all bounded archive state. */
export function resetArchivedChannelEvents() {
  archiveEventsByChannel.clear();
  archiveReadTickByChannel.clear();
  archiveReadTick = 0;
}

const agentLastActivityAt = new Map<string, number>();
const agentProjectionSubscribers = new Map<string, number>();

/** Record that an agent's retained observer state changed. */
export function markObserverAgentActivity(
  agentPubkey: string,
  now = Date.now(),
) {
  agentLastActivityAt.set(normalizePubkey(agentPubkey), now);
}

/** Retain one agent's observer projection until the returned release runs. */
export function retainObserverAgentProjection(agentPubkey: string): () => void {
  const key = normalizePubkey(agentPubkey);
  agentProjectionSubscribers.set(
    key,
    (agentProjectionSubscribers.get(key) ?? 0) + 1,
  );
  markObserverAgentActivity(key);
  return () => {
    const remaining = (agentProjectionSubscribers.get(key) ?? 1) - 1;
    if (remaining > 0) agentProjectionSubscribers.set(key, remaining);
    else agentProjectionSubscribers.delete(key);
    markObserverAgentActivity(key);
  };
}

/** Select observer agents whose inactive, unmounted grace period expired. */
export function selectIdleObserverAgents(
  candidateAgentPubkeys: Iterable<string>,
  activeAgentPubkeys: Iterable<string>,
  now = Date.now(),
): string[] {
  const active = new Set(
    [...activeAgentPubkeys].map((pubkey) => normalizePubkey(pubkey)),
  );
  const idle: string[] = [];
  for (const pubkey of candidateAgentPubkeys) {
    const key = normalizePubkey(pubkey);
    if (active.has(key) || (agentProjectionSubscribers.get(key) ?? 0) > 0) {
      agentLastActivityAt.set(key, now);
      continue;
    }
    const lastActivityAt = agentLastActivityAt.get(key);
    if (lastActivityAt === undefined) {
      agentLastActivityAt.set(key, now);
      continue;
    }
    if (now - lastActivityAt < OBSERVER_AGENT_IDLE_RETENTION_MS) continue;
    idle.push(key);
    agentLastActivityAt.delete(key);
  }
  return idle;
}

/** Reset all per-agent retention bookkeeping. */
export function resetObserverAgentRetention() {
  agentLastActivityAt.clear();
  agentProjectionSubscribers.clear();
}
