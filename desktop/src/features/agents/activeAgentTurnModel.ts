import { progressFromObserverEvent } from "@/features/agents/agentAttention";
import type { AgentProgressKind } from "@/features/agents/agentAttention";
import type { ObserverEvent } from "@/features/agents/ui/agentSessionTypes";

/** Harness emits turn_liveness every ~10s (BUZZ_ACP_TURN_LIVENESS_SECS). */
export const LIVENESS_INTERVAL_MS = 10_000;
/** Keep a missing live lease visible long enough to surface Lost contact. */
export const REMOVE_AFTER_MS = 3 * 60_000;
/** Engage the all-turn stream-gap pause before Lost contact is derived. */
export const FRAME_GAP_PAUSE_MS = LIVENESS_INTERVAL_MS * 2;
/** A silent agent is removed after this bounded prune pause. */
export const PRUNE_PAUSE_MAX_MS = REMOVE_AFTER_MS;
/** Harness hard upper bound for parallel agent subprocesses. */
export const MAX_TURNS_PER_AGENT = 32;
/** Bound terminal resurrection guards across a long session. */
export const MAX_TERMINAL_TOMBSTONES = MAX_TURNS_PER_AGENT * 4;
/** Interval for pruning stale/expired turns. */
export const PRUNE_INTERVAL_MS = 5_000;

export type ActiveTurn = {
  turnId: string;
  sessionId: string | null;
  channelId: string;
  conversationId: string;
  startedAt: number;
  lastSeenAt: number;
  lastSubstantiveProgressAt: number;
  progressFingerprint: string;
  progressKind: AgentProgressKind;
  progressLabel: string;
  triggeringEventIds: string[];
};

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

export function triggeringEventIds(event: ObserverEvent): string[] {
  const values = asRecord(event.payload).triggeringEventIds;
  if (!Array.isArray(values)) return [];
  return values.filter(
    (value): value is string => typeof value === "string" && value.length > 0,
  );
}

export function createActiveTurn(input: {
  channelId: string;
  conversationId: string;
  observedAt: number;
  startedAt: number;
  triggeringEventIds?: string[];
  turnId: string;
  sessionId?: string | null;
}): ActiveTurn {
  return {
    turnId: input.turnId,
    sessionId: input.sessionId ?? null,
    channelId: input.channelId,
    conversationId: input.conversationId,
    startedAt: input.startedAt,
    lastSeenAt: input.observedAt,
    lastSubstantiveProgressAt: input.observedAt,
    progressFingerprint: "turn_started",
    progressKind: "progress",
    progressLabel: "Turn started",
    triggeringEventIds: input.triggeringEventIds ?? [],
  };
}

/** Mutates one live turn and reports whether its substantive projection moved. */
export function applyObserverFrame(
  turn: ActiveTurn,
  event: ObserverEvent,
  observedAt: number,
): boolean {
  turn.lastSeenAt = Math.max(turn.lastSeenAt, observedAt);
  const progress = progressFromObserverEvent(event);
  if (!progress || progress.fingerprint === turn.progressFingerprint) {
    return false;
  }
  turn.lastSubstantiveProgressAt = Math.max(
    turn.lastSubstantiveProgressAt,
    observedAt,
  );
  turn.progressFingerprint = progress.fingerprint;
  turn.progressKind = progress.kind;
  turn.progressLabel = progress.label;
  return true;
}

/** True while every turn shares a transient observer-frame gap. */
export function shouldPausePrune(
  turns: ReadonlyMap<string, ActiveTurn>,
  now: number,
): boolean {
  let maxSeenAt = 0;
  for (const turn of turns.values()) {
    if (turn.lastSeenAt > maxSeenAt) maxSeenAt = turn.lastSeenAt;
  }
  const silentFor = now - maxSeenAt;
  return (
    maxSeenAt > 0 &&
    silentFor > FRAME_GAP_PAUSE_MS &&
    silentFor < PRUNE_PAUSE_MAX_MS
  );
}
