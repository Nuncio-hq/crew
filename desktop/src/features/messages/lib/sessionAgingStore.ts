/**
 * Per-thread session aging projection (#173).
 *
 * Aging is a property of the session-on-thread, not of the agent — the UI
 * home is a thin banner under the thread header (never Mission Inbox /
 * Lost contact / Possibly stalled).
 */

import { normalizePubkey } from "@/shared/lib/pubkey";

export type SessionAgingSignal = "known" | "unknown" | "unavailable";

export type SessionAgingEntry = {
  agentPubkey: string;
  agentName?: string;
  channelId: string;
  conversationId: string;
  aging: boolean;
  reason: "compaction_threshold" | "turn_count_net" | string | null;
  compactionCount: number;
  compactionSignal: SessionAgingSignal;
  sessionTurnCount: number;
  compactionThreshold: number;
  turnThreshold: number;
};

type AgingKey = string;

const entries = new Map<AgingKey, SessionAgingEntry>();
const listeners = new Set<() => void>();

function agingKey(agentPubkey: string, conversationId: string): AgingKey {
  return `${normalizePubkey(agentPubkey)}:${conversationId}`;
}

function emit() {
  for (const listener of listeners) {
    listener();
  }
}

export function putSessionAging(entry: SessionAgingEntry): void {
  const key = agingKey(entry.agentPubkey, entry.conversationId);
  if (!entry.aging) {
    if (entries.delete(key)) {
      emit();
    }
    return;
  }
  entries.set(key, {
    ...entry,
    agentPubkey: normalizePubkey(entry.agentPubkey),
    // Honesty: never surface a fabricated compaction number.
    compactionCount:
      entry.compactionSignal === "known" ? entry.compactionCount : 0,
  });
  emit();
}

export function clearSessionAging(
  agentPubkey: string,
  conversationId: string,
): void {
  if (entries.delete(agingKey(agentPubkey, conversationId))) {
    emit();
  }
}

export function clearAllSessionAging(): void {
  if (entries.size === 0) {
    return;
  }
  entries.clear();
  emit();
}

export function getSessionAging(
  agentPubkey: string,
  conversationId: string,
): SessionAgingEntry | undefined {
  return entries.get(agingKey(agentPubkey, conversationId));
}

export function listSessionAgingForConversation(
  conversationId: string,
): SessionAgingEntry[] {
  const out: SessionAgingEntry[] = [];
  for (const entry of entries.values()) {
    if (
      entry.conversationId === conversationId ||
      entry.channelId === conversationId
    ) {
      out.push(entry);
    }
  }
  return out;
}

export function subscribeSessionAging(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getSessionAgingSnapshot(): ReadonlyMap<
  AgingKey,
  SessionAgingEntry
> {
  return entries;
}

/** Parse a harness `session_aging` observer payload. */
export function parseSessionAgingPayload(
  agentPubkey: string,
  payload: unknown,
): SessionAgingEntry | null {
  if (!payload || typeof payload !== "object") {
    return null;
  }
  const value = payload as Record<string, unknown>;
  const channelId =
    typeof value.channelId === "string" ? value.channelId : null;
  const conversationId =
    typeof value.conversationId === "string" ? value.conversationId : channelId;
  if (!channelId || !conversationId) {
    return null;
  }
  const signalRaw =
    typeof value.compactionSignal === "string"
      ? value.compactionSignal
      : "unknown";
  const compactionSignal: SessionAgingSignal =
    signalRaw === "known"
      ? "known"
      : signalRaw === "unavailable"
        ? "unavailable"
        : "unknown";
  return {
    agentPubkey: normalizePubkey(
      typeof value.pubkey === "string" ? value.pubkey : agentPubkey,
    ),
    channelId,
    conversationId,
    aging: value.aging === true,
    reason:
      typeof value.reason === "string"
        ? value.reason
        : value.aging === true
          ? "compaction_threshold"
          : null,
    compactionCount:
      typeof value.compactionCount === "number" ? value.compactionCount : 0,
    compactionSignal,
    sessionTurnCount:
      typeof value.sessionTurnCount === "number" ? value.sessionTurnCount : 0,
    compactionThreshold:
      typeof value.compactionThreshold === "number"
        ? value.compactionThreshold
        : 3,
    turnThreshold:
      typeof value.turnThreshold === "number" ? value.turnThreshold : 100,
  };
}

export function sessionAgingBannerText(
  entry: SessionAgingEntry,
  agentLabel: string,
): string {
  if (entry.reason === "turn_count_net") {
    return `${agentLabel}'s long session (${entry.sessionTurnCount}+ turns)`;
  }
  if (entry.compactionSignal === "known" && entry.compactionCount > 0) {
    return `${agentLabel}'s session compacted ${entry.compactionCount}× — memory may be degraded`;
  }
  return `${agentLabel}'s session is aging — consider a fresh session`;
}
