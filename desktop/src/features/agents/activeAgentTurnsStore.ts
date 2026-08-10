import { getAgentObserverSnapshot } from "@/features/agents/observerRelayStore";
import {
  clearTurnsWatermarks,
  gateEventByWatermark,
  recordEventProcessed,
  restoreTurnsWatermarks,
  snapshotTurnsWatermarks,
} from "@/features/agents/turnsWatermarkStore";
import {
  buildSignedConversationOutcome,
  clearConversationOutcomeLedger,
  cloneConversationOutcomeLedger,
  conversationOutcomeLedgerSize,
  pruneExpiredConversationOutcomes,
  recordConversationOutcome,
  retireConversationOutcomeAgent,
  restoreConversationOutcomeLedger,
  type ConversationOutcomeEntry,
} from "@/features/agents/conversationOutcomeLedger";
import {
  clearTurnRetrying,
  recordTurnRetrying,
} from "@/features/agents/retryingTurnsStore";
import {
  type AgentSessionGenerationSnapshot,
  observeAgentSession,
  resetAgentSessionGenerations,
  restoreAgentSessionGenerations,
  snapshotAgentSessionGenerations,
} from "@/features/agents/activeAgentSessionGeneration";
import {
  applyObserverFrame,
  createActiveTurn,
  MAX_TERMINAL_TOMBSTONES,
  MAX_TURNS_PER_AGENT,
  PRUNE_INTERVAL_MS,
  REMOVE_AFTER_MS,
  shouldPausePrune,
  triggeringEventIds,
  type ActiveTurn,
} from "@/features/agents/activeAgentTurnModel";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { ObserverEvent } from "./ui/agentSessionTypes";

export {
  CONVERSATION_OUTCOME_TTL_MS,
  getConversationOutcomeEntry,
  walkConversationOutcomes,
  type ConversationOutcomeEntry,
} from "@/features/agents/conversationOutcomeLedger";

export {
  useActiveAgentTurns,
  useActiveAgentTurnControlTargets,
  useActiveAgentTurnsByChannel,
  useActiveTurnsByConversation,
  useActiveAgentsForConversation,
  useActiveAgentTurnsBridge,
} from "@/features/agents/activeAgentTurnsHooks";

/** One working channel surfaced to the UI, anchored to the desktop clock. */
export type ActiveTurnSummary = {
  channelId: string;
  anchorAt: number;
};

/** Exact target for observer controls; never collapsed by real channel. */
export type ActiveTurnControlTarget = {
  channelId: string;
  conversationId: string;
  turnId: string;
};

/** One channel with active agent work, aggregated across agents. */
export type ActiveChannelTurnSummary = {
  channelId: string;
  anchorAt: number;
  agentCount: number;
  agentPubkeys: string[];
  agentNames?: string[];
};

// Module-level state: agentPubkey → turnId → ActiveTurn
const activeTurnsByAgent = new Map<string, Map<string, ActiveTurn>>();
const listeners = new Set<() => void>();

// Desktop minus agent-host clock. The running minimum rejects network jitter;
// read-time anchors retroactively tighten as better samples arrive.
const clockOffsetByAgent = new Map<string, number>();

// Cached snapshots for useSyncExternalStore reference stability.
// Only regenerated when the underlying turn map for an agent actually changes.
const cachedTurnSummaries = new Map<string, ActiveTurnSummary[]>();
const cachedControlTargets = new Map<string, ActiveTurnControlTarget[]>();
const cachedAgentsByConversation = new Map<string, string[]>();
let cachedChannelTurnSummaries: ActiveChannelTurnSummary[] | null = null;
/** Bumps on every turn-map mutation so conversation-scoped caches can drop. */
let activeTurnsGeneration = 0;

// Per-agent record of when each turn terminally ended (turnId →
// terminal-event timestamp, in agent-host clock ms). endTurn hard-deletes a
// turn with no surviving record, so without this a late liveness frame for an
// already-completed turn would resurrect a dead badge. Resurrection (A) checks
// this: a turn is revived only if the recovered liveness is strictly newer
// than its recorded terminal timestamp.
const terminalAtByAgent = new Map<string, Map<string, number>>();

let pruneInterval: ReturnType<typeof setInterval> | null = null;

function bumpActiveTurnsGeneration() {
  cachedAgentsByConversation.clear();
  cachedChannelTurnSummaries = null;
  activeTurnsGeneration += 1;
}

function invalidateCache(agentKey: string) {
  cachedTurnSummaries.delete(agentKey);
  cachedControlTargets.delete(agentKey);
  bumpActiveTurnsGeneration();
}

function recordOutcomeAndBump(
  conversationId: string,
  entry: ConversationOutcomeEntry,
) {
  if (!recordConversationOutcome(conversationId, entry)) return;
  bumpActiveTurnsGeneration();
}

function notifyListeners() {
  for (const listener of listeners) {
    listener();
  }
}

/**
 * Refine this agent's clock-offset estimate from one observer event. Samples
 * Date.now() - Date.parse(timestamp) and keeps the running minimum. When the
 * minimum tightens, every live anchor for the agent shifts, so the cache is
 * invalidated. Events with an unparseable timestamp contribute no sample.
 * Returns true when the offset changed.
 */
function sampleClockOffset(agentKey: string, timestamp: string): boolean {
  const sample = Date.now() - Date.parse(timestamp);
  if (Number.isNaN(sample)) return false;
  const prior = clockOffsetByAgent.get(agentKey);
  if (prior !== undefined && sample >= prior) return false;
  clockOffsetByAgent.set(agentKey, sample);
  invalidateCache(agentKey);
  return true;
}

function parseTimestamp(timestamp: string): number | null {
  const parsed = Date.parse(timestamp);
  return Number.isFinite(parsed) ? parsed : null;
}

function eventObservedAt(agentKey: string, event: ObserverEvent): number {
  if (!event.replayed) return Date.now();
  const eventAt = parseTimestamp(event.timestamp);
  if (eventAt === null) return 0;
  const corrected = eventAt + (clockOffsetByAgent.get(agentKey) ?? 0);
  // Replay is historical evidence, never fresh contact. Invalid or future
  // timestamps remain unverified until an actual live frame calibrates them.
  return corrected <= Date.now() ? corrected : 0;
}

function resolveTerminalTurn(
  agentKey: string,
  event: ObserverEvent,
  conversationId: string | null,
): ActiveTurn | null {
  const turns = activeTurnsByAgent.get(agentKey);
  if (!turns) return null;
  if (event.turnId) return turns.get(event.turnId) ?? null;
  if (!event.channelId || !conversationId) return null;

  const terminalTriggers = triggeringEventIds(event);
  const candidates = [...turns.values()].filter(
    (turn) =>
      turn.channelId === event.channelId &&
      turn.conversationId === conversationId &&
      (terminalTriggers.length === 0 ||
        terminalTriggers.every((trigger) =>
          turn.triggeringEventIds.includes(trigger),
        )),
  );
  return candidates.length === 1 ? (candidates[0] ?? null) : null;
}

function startTurn(
  agentPubkey: string,
  channelId: string,
  conversationId: string,
  turnId: string,
  sessionId: string | null,
  timestamp: string,
  observedAt: number,
  triggerIds: string[] = [],
) {
  const key = normalizePubkey(agentPubkey);
  if (retireConversationOutcomeAgent(conversationId, key))
    bumpActiveTurnsGeneration();
  let agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns) {
    agentTurns = new Map();
    activeTurnsByAgent.set(key, agentTurns);
  }

  // Cap at MAX_TURNS_PER_AGENT — evict oldest if exceeded
  if (agentTurns.size >= MAX_TURNS_PER_AGENT && !agentTurns.has(turnId)) {
    let oldestKey: string | null = null;
    let oldestTime = Number.POSITIVE_INFINITY;
    for (const [tid, turn] of agentTurns) {
      if (turn.startedAt < oldestTime) {
        oldestTime = turn.startedAt;
        oldestKey = tid;
      }
    }
    if (oldestKey) {
      agentTurns.delete(oldestKey);
    }
  }

  agentTurns.set(
    turnId,
    createActiveTurn({
      turnId,
      sessionId,
      channelId,
      conversationId,
      startedAt: parseTimestamp(timestamp) ?? observedAt,
      observedAt,
      triggeringEventIds: triggerIds,
    }),
  );
  invalidateCache(key);
}

function recordFrame(
  agentPubkey: string,
  event: ObserverEvent,
): { found: boolean; progressChanged: boolean } {
  if (!event.turnId) return { found: false, progressChanged: false };
  const key = normalizePubkey(agentPubkey);
  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns) return { found: false, progressChanged: false };
  const turn = agentTurns.get(event.turnId);
  if (!turn) return { found: false, progressChanged: false };
  return {
    found: true,
    progressChanged: applyObserverFrame(
      turn,
      event,
      eventObservedAt(key, event),
    ),
  };
}

/**
 * A — resurrect a badge that was pruned out from under a still-running turn.
 * A recovered liveness/acp frame for a turn no longer in the live map recreates
 * it, UNLESS C's tombstone shows the turn already terminally ended at or after
 * this frame's time (a stale frame must not revive a completed turn). The frame
 * may carry its original `startedAt` envelope field; when valid and not later
 * than the frame, preserve the elapsed timer by anchoring to that timestamp.
 * Old, malformed, or impossible future starts fall back to the recovery
 * timestamp. Returns true on revive.
 */
function resurrectTurn(agentPubkey: string, event: ObserverEvent): boolean {
  if (!event.turnId || !event.channelId) return false;
  const key = normalizePubkey(agentPubkey);
  const terminalAt = terminalAtByAgent.get(key)?.get(event.turnId);
  const frameAt = parseTimestamp(event.timestamp);
  // Only revive when this frame is strictly newer than the recorded terminal.
  if (terminalAt !== undefined && (frameAt === null || frameAt <= terminalAt)) {
    return false;
  }
  const startedAt =
    typeof event.startedAt === "string" &&
    parseTimestamp(event.startedAt) !== null
      ? event.startedAt
      : event.timestamp;
  const startedAtMs = parseTimestamp(startedAt);
  const safeStartedAt =
    frameAt !== null && startedAtMs !== null && startedAtMs <= frameAt
      ? startedAt
      : event.timestamp;
  startTurn(
    agentPubkey,
    event.channelId,
    event.conversationId ?? event.channelId,
    event.turnId,
    event.sessionId,
    safeStartedAt,
    eventObservedAt(key, event),
  );
  return true;
}

function reconcileDelayedTurnStart(
  agentKey: string,
  event: ObserverEvent,
): boolean {
  if (event.kind !== "turn_started" || !event.turnId) return false;
  const turn = activeTurnsByAgent.get(agentKey)?.get(event.turnId);
  if (!turn) return false;

  let changed = false;
  const startedAt = parseTimestamp(event.timestamp);
  if (startedAt !== null && startedAt < turn.startedAt) {
    turn.startedAt = startedAt;
    changed = true;
  }
  const triggerIds = triggeringEventIds(event);
  if (triggerIds.some((id) => !turn.triggeringEventIds.includes(id))) {
    turn.triggeringEventIds = [
      ...new Set([...turn.triggeringEventIds, ...triggerIds]),
    ];
    changed = true;
  }
  if (changed) invalidateCache(agentKey);
  return changed;
}

function recordTerminal(agentKey: string, turnId: string, terminalAt: number) {
  if (!Number.isFinite(terminalAt)) return;
  let terminals = terminalAtByAgent.get(agentKey);
  if (!terminals) {
    terminals = new Map();
    terminalAtByAgent.set(agentKey, terminals);
  }
  terminals.set(turnId, terminalAt);
  // Bound the tombstone map: only recently-completed turns can be the target of
  // a racing late liveness frame (older ones are already below the watermark).
  // Evict the oldest terminal once past the cap so the map can't grow unbounded
  // across a long session. Insertion order tracks completion order closely
  // enough; the first key is the oldest survivor.
  if (terminals.size > MAX_TERMINAL_TOMBSTONES) {
    const oldest = terminals.keys().next().value;
    if (oldest !== undefined) terminals.delete(oldest);
  }
}

function endTurn(
  agentPubkey: string,
  turnId: string | null,
  terminalAt: number,
) {
  const key = normalizePubkey(agentPubkey);
  // Tombstone the terminal time so a late liveness frame can't resurrect a
  // completed turn (A's guard). With an explicit turnId this is recorded even
  // when the turn was already pruned and the agent's live map is gone — the
  // completion is authoritative and must outlive the active record.
  if (turnId) {
    recordTerminal(key, turnId, terminalAt);
  }

  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns) return;

  if (turnId) agentTurns.delete(turnId);
  if (agentTurns.size === 0) {
    activeTurnsByAgent.delete(key);
  }
  invalidateCache(key);
}

function pruneExpired() {
  const now = Date.now();
  let changed = false;
  for (const [agentKey, agentTurns] of activeTurnsByAgent) {
    // A single fresh tracked turn for this agent means a stale sibling is
    // genuinely dead and must still prune at 25s. Conversely, all of this
    // agent's turns going stale together identifies a per-agent frame-stream
    // gap, regardless of whether other agents keep reporting activity.
    if (shouldPausePrune(agentTurns, now)) continue;

    for (const [turnId, turn] of agentTurns) {
      if (now - turn.lastSeenAt >= REMOVE_AFTER_MS) {
        recordOutcomeAndBump(turn.conversationId, {
          outcome: "lost-contact",
          agentPubkey: agentKey,
          channelId: turn.channelId,
          endedAt: now,
          terminalAt: now,
          terminalOrderKey: `${agentKey}\u0000${turnId}\u0000lost-contact`,
          failedEventIds: [...turn.triggeringEventIds],
        });
        agentTurns.delete(turnId);
        invalidateCache(agentKey);
        changed = true;
      }
    }
    if (agentTurns.size === 0) {
      activeTurnsByAgent.delete(agentKey);
    }
  }
  if (pruneExpiredConversationOutcomes(now)) {
    bumpActiveTurnsGeneration();
    changed = true;
  }
  if (activeTurnsByAgent.size > 0) {
    // Attention states cross elapsed-time thresholds even when no new frame
    // mutates the turn. Refresh external-store snapshots on the prune cadence.
    bumpActiveTurnsGeneration();
    changed = true;
  }
  if (changed) {
    notifyListeners();
  }
}

// INVARIANT: events must be sorted by (timestamp, seq) ascending.
// syncAgentTurnsFromEvents receives sorted arrays from observerRelayStore.
// Calling with unsorted events will cause silent data loss.
function processEvent(agentPubkey: string, event: ObserverEvent) {
  const key = normalizePubkey(agentPubkey);

  const sessionObservation = observeAgentSession(key, event);
  if (sessionObservation === "retired") return;
  if (sessionObservation === "changed") {
    clockOffsetByAgent.delete(key);
    invalidateCache(key);
  }

  // Gate on the per-(agent, channel) watermark to keep sorted replays a no-op
  // and prevent stale liveness/eviction from killing live turns.
  if (gateEventByWatermark(key, event)) {
    if (reconcileDelayedTurnStart(key, event)) {
      notifyListeners();
    }
    return;
  }
  recordEventProcessed(key, event);

  // Refine the clock offset from every fresh event. A tighter offset shifts
  // every live anchor for this agent, so a change must reach the UI even when
  // the event itself surfaces no new turn.
  const offsetChanged = event.replayed
    ? false
    : sampleClockOffset(key, event.timestamp);
  const observedAt = eventObservedAt(key, event);

  switch (event.kind) {
    case "turn_started":
      if (event.channelId) {
        clearTurnRetrying(event.conversationId ?? event.channelId);
        startTurn(
          agentPubkey,
          event.channelId,
          event.conversationId ?? event.channelId,
          event.turnId ?? `seq-${event.seq}`,
          event.sessionId,
          event.timestamp,
          observedAt,
          triggeringEventIds(event),
        );
        notifyListeners();
        return;
      }
      break;
    case "turn_retrying": {
      const frame = recordFrame(agentPubkey, event);
      const payload = event.payload as {
        attempt?: unknown;
        maxAttempts?: unknown;
      } | null;
      const attempt =
        typeof payload?.attempt === "number" ? payload.attempt : null;
      const maxAttempts =
        typeof payload?.maxAttempts === "number" ? payload.maxAttempts : null;
      if (
        event.channelId &&
        (event.conversationId || event.channelId) &&
        attempt != null &&
        maxAttempts != null
      ) {
        recordTurnRetrying({
          agentPubkey,
          channelId: event.channelId,
          conversationId: event.conversationId ?? event.channelId,
          attempt,
          maxAttempts,
        });
        if (frame.progressChanged) invalidateCache(key);
        notifyListeners();
        return;
      }
      break;
    }
    case "turn_completed":
    case "turn_error":
    case "agent_panic": {
      // A failure may immediately emit `turn_retrying`; preserve it until the
      // next `turn_started` or retrying overwrite.
      // Reuse the event's already-resolved channel/conversation ids — do not
      // re-derive from the live turn map (the turn may already be pruned).
      const conversationId = event.conversationId ?? event.channelId;
      const terminalTurn = resolveTerminalTurn(key, event, conversationId);
      const resolvedTurnId = event.turnId ?? terminalTurn?.turnId ?? null;
      const terminalTriggers =
        terminalTurn?.triggeringEventIds ?? triggeringEventIds(event);
      const terminalAt = parseTimestamp(event.timestamp) ?? 0;
      if (event.channelId && conversationId && resolvedTurnId) {
        recordOutcomeAndBump(
          conversationId,
          buildSignedConversationOutcome({
            agentKey: key,
            event,
            resolvedTurnId,
            channelId: event.channelId,
            endedAt: observedAt,
            terminalAt,
            triggeringEventIds: terminalTriggers,
            sessionId: event.sessionId ?? terminalTurn?.sessionId ?? undefined,
          }),
        );
      }
      endTurn(agentPubkey, resolvedTurnId, terminalAt);
      notifyListeners();
      return;
    }
    case "acp_read":
    case "acp_write":
    // Liveness only advances lastSeenAt; ACP phase changes may also advance the
    // substantive-progress clock. If the turn was pruned under a live host,
    // resurrect it unless a terminal tombstone rejects the frame.
    case "turn_liveness": {
      const frame = recordFrame(agentPubkey, event);
      if (!frame.found && resurrectTurn(agentPubkey, event)) {
        notifyListeners();
        return;
      }
      if (frame.found) {
        // Every observer frame is liveness, even when it is not substantive
        // progress. Invalidate external-store projections for token, usage,
        // stdout, and raw ACP traffic without moving the progress clock.
        invalidateCache(key);
        notifyListeners();
        return;
      }
      break;
    }
  }

  if (offsetChanged) {
    notifyListeners();
  }
}

function ensurePruneInterval() {
  if (pruneInterval) return;
  pruneInterval = setInterval(pruneExpired, PRUNE_INTERVAL_MS);
}

function stopPruneInterval() {
  if (pruneInterval) {
    clearInterval(pruneInterval);
    pruneInterval = null;
  }
}

export function subscribeActiveAgentTurns(listener: () => void) {
  listeners.add(listener);
  if (listeners.size === 1) {
    ensurePruneInterval();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      stopPruneInterval();
    }
  };
}

/**
 * Returns the channels where the given agent has active turns, sorted by
 * channelId, each anchored to the earliest `anchorAt` for that channel.
 * The array reference is cached and stable until the turn map mutates — a
 * requirement for `useSyncExternalStore`.
 */
export function getActiveTurnsForAgent(
  agentPubkey: string | null | undefined,
): ActiveTurnSummary[] {
  if (!agentPubkey) return EMPTY_TURNS;
  const key = normalizePubkey(agentPubkey);
  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns || agentTurns.size === 0) return EMPTY_TURNS;

  const cached = cachedTurnSummaries.get(key);
  if (cached) return cached;

  const offset = clockOffsetByAgent.get(key) ?? 0;

  // Collapse multiple turns in one channel to the earliest start — the badge
  // should count from when the channel's oldest live turn began. Anchors are
  // derived here (startedAt + offset) so the latest skew estimate applies.
  const earliestByChannel = new Map<string, number>();
  for (const turn of agentTurns.values()) {
    const prior = earliestByChannel.get(turn.channelId);
    if (prior === undefined || turn.startedAt < prior) {
      earliestByChannel.set(turn.channelId, turn.startedAt);
    }
  }

  const result = [...earliestByChannel.entries()]
    .map(([channelId, startedAt]) => ({
      channelId,
      anchorAt: startedAt + offset,
    }))
    .sort((a, b) => a.channelId.localeCompare(b.channelId));
  cachedTurnSummaries.set(key, result);
  return result;
}

export function getActiveTurnControlTargetsForAgent(
  agentPubkey: string | null | undefined,
): ActiveTurnControlTarget[] {
  if (!agentPubkey) return EMPTY_CONTROL_TARGETS;
  const key = normalizePubkey(agentPubkey);
  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns || agentTurns.size === 0) return EMPTY_CONTROL_TARGETS;

  const cached = cachedControlTargets.get(key);
  if (cached) return cached;

  const result = [...agentTurns.values()]
    .map(({ channelId, conversationId, turnId }) => ({
      channelId,
      conversationId,
      turnId,
    }))
    .sort(
      (a, b) =>
        a.channelId.localeCompare(b.channelId) ||
        a.conversationId.localeCompare(b.conversationId) ||
        a.turnId.localeCompare(b.turnId),
    );
  cachedControlTargets.set(key, result);
  return result;
}

const EMPTY_TURNS: ActiveTurnSummary[] = [];
const EMPTY_CONTROL_TARGETS: ActiveTurnControlTarget[] = [];
const EMPTY_CHANNEL_TURNS: ActiveChannelTurnSummary[] = [];
const EMPTY_CONVERSATION_AGENTS: string[] = [];

/** Walk every live turn with its agent clock offset (desktop-clock anchors). */
export function walkActiveAgentTurns(
  visit: (agentKey: string, turn: ActiveTurn, offset: number) => void,
): void {
  for (const [agentKey, agentTurns] of activeTurnsByAgent) {
    if (agentTurns.size === 0) continue;
    const offset = clockOffsetByAgent.get(agentKey) ?? 0;
    for (const turn of agentTurns.values()) {
      visit(agentKey, turn, offset);
    }
  }
}

/** Generation counter for conversation-scoped derived caches. */
export function getActiveTurnsGeneration(): number {
  return activeTurnsGeneration;
}

export function getActiveAgentsForConversation(
  conversationId: string | null | undefined,
): string[] {
  if (!conversationId) return EMPTY_CONVERSATION_AGENTS;
  const cached = cachedAgentsByConversation.get(conversationId);
  if (cached) return cached;
  const agentPubkeys: string[] = [];
  for (const [agentPubkey, turns] of activeTurnsByAgent) {
    if (
      [...turns.values()].some((turn) => turn.conversationId === conversationId)
    ) {
      agentPubkeys.push(agentPubkey);
    }
  }
  const result = agentPubkeys.sort();
  cachedAgentsByConversation.set(conversationId, result);
  return result;
}

/**
 * Returns active working channels across all tracked agents, sorted by
 * channelId and anchored to the earliest live turn in each channel.
 */
export function getActiveTurnsByChannel(): ActiveChannelTurnSummary[] {
  if (cachedChannelTurnSummaries) return cachedChannelTurnSummaries;
  if (activeTurnsByAgent.size === 0) return EMPTY_CHANNEL_TURNS;

  const summaries = new Map<
    string,
    { anchorAt: number; channelId: string; agentPubkeys: Set<string> }
  >();
  for (const [agentKey, agentTurns] of activeTurnsByAgent) {
    if (agentTurns.size === 0) continue;
    const offset = clockOffsetByAgent.get(agentKey) ?? 0;

    for (const turn of agentTurns.values()) {
      const anchorAt = turn.startedAt + offset;
      const summary = summaries.get(turn.channelId);
      if (!summary) {
        summaries.set(turn.channelId, {
          anchorAt,
          channelId: turn.channelId,
          agentPubkeys: new Set([agentKey]),
        });
        continue;
      }
      summary.agentPubkeys.add(agentKey);
      if (anchorAt < summary.anchorAt) {
        summary.anchorAt = anchorAt;
      }
    }
  }

  const result = [...summaries.entries()]
    .map(([channelId, summary]) => ({
      channelId,
      anchorAt: summary.anchorAt,
      agentCount: summary.agentPubkeys.size,
      agentPubkeys: [...summary.agentPubkeys].sort(),
    }))
    .sort((a, b) => a.channelId.localeCompare(b.channelId));
  cachedChannelTurnSummaries = result;
  return result;
}

/** Desktop-clock activity bounds for the given agents, optionally scoped. */
export function getActiveTurnActivityBounds(options: {
  agentPubkeys: readonly string[];
  channelId?: string | null;
  conversationId?: string | null;
}): {
  anchorAt: number;
  lastSeenAt: number;
  lastSubstantiveProgressAt: number;
  progressKind: ActiveTurn["progressKind"];
  progressLabel: string;
} | null {
  const channelId = options.channelId?.trim() || null;
  const conversationId = options.conversationId?.trim() || null;
  let anchorAt = Number.POSITIVE_INFINITY;
  let lastSeenAt = Number.POSITIVE_INFINITY;
  let lastSubstantiveProgressAt = Number.POSITIVE_INFINITY;
  let progressKind: ActiveTurn["progressKind"] = "progress";
  let progressLabel = "Turn started";

  for (const pubkey of options.agentPubkeys) {
    const key = normalizePubkey(pubkey);
    const agentTurns = activeTurnsByAgent.get(key);
    if (!agentTurns || agentTurns.size === 0) continue;
    const offset = clockOffsetByAgent.get(key) ?? 0;

    for (const turn of agentTurns.values()) {
      if (channelId && turn.channelId !== channelId) continue;
      if (conversationId && turn.conversationId !== conversationId) continue;
      const turnAnchor = turn.startedAt + offset;
      if (turnAnchor < anchorAt) anchorAt = turnAnchor;
      if (turn.lastSeenAt < lastSeenAt) lastSeenAt = turn.lastSeenAt;
      if (turn.lastSubstantiveProgressAt < lastSubstantiveProgressAt) {
        lastSubstantiveProgressAt = turn.lastSubstantiveProgressAt;
        progressKind = turn.progressKind;
        progressLabel = turn.progressLabel;
      }
    }
  }

  if (!Number.isFinite(anchorAt) || !Number.isFinite(lastSeenAt)) return null;
  return {
    anchorAt,
    lastSeenAt,
    lastSubstantiveProgressAt,
    progressKind,
    progressLabel,
  };
}

/**
 * Synchronize the active-turns store with the latest observer events for a
 * given agent.
 */
export function syncAgentTurnsFromEvents(
  agentPubkey: string,
  events: ObserverEvent[],
) {
  for (const event of events) {
    processEvent(agentPubkey, event);
  }
}

/**
 * Sync every running/deployed agent's observer events into the active-turns
 * store. Extracted from the bridge hook so a regression can drive the exact
 * observer→derived-liveness path without a React renderer.
 */
export function syncActiveAgentTurnsFromObserver(
  agents: readonly { pubkey: string; status: string }[],
) {
  for (const agent of agents) {
    if (agent.status !== "running" && agent.status !== "deployed") continue;
    const snapshot = getAgentObserverSnapshot(agent.pubkey, true);
    syncAgentTurnsFromEvents(agent.pubkey, snapshot.events);
  }
}

/**
 * Immediately clear all active turns for a specific agent — called when
 * Desktop itself stops or restarts the agent, so the turn store doesn't
 * have to wait for the 3-minute prune-pause backstop.
 *
 * Preserves the watermark so a full-buffer replay after
 * the clear is still a no-op — without the watermark a replayed
 * `turn_started` would immediately resurrect the badge.  Preserves
 * `clockOffsetByAgent` — the offset remains valid and harmless.
 *
 * Tombstones every cleared turn (C) so an in-flight `turn_liveness` frame
 * already on the wire at kill time cannot resurrect the badge via
 * `resurrectTurn`.  A restarted agent's genuinely new turns carry new
 * turnIds / newer timestamps, so the tombstones don't block them.
 */
export function clearActiveTurnsForAgent(agentPubkey: string): void {
  const key = normalizePubkey(agentPubkey);
  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns || agentTurns.size === 0) return;

  const agentClockNow = Date.now() - (clockOffsetByAgent.get(key) ?? 0);
  for (const turnId of agentTurns.keys()) {
    recordTerminal(key, turnId, agentClockNow);
  }

  activeTurnsByAgent.delete(key);
  invalidateCache(key);
  notifyListeners();
}

/**
 * Clears all live turn state (active turns, offsets, watermarks, tombstones).
 * Intentionally preserves `savedByCommunity` — community-switch snapshots
 * must survive the reset that runs between save and restore.
 */
export function resetActiveAgentTurnsStore() {
  activeTurnsByAgent.clear();
  clearTurnsWatermarks();
  clockOffsetByAgent.clear();
  resetAgentSessionGenerations();
  cachedTurnSummaries.clear();
  cachedControlTargets.clear();
  cachedAgentsByConversation.clear();
  cachedChannelTurnSummaries = null;
  activeTurnsGeneration += 1;
  terminalAtByAgent.clear();
  clearConversationOutcomeLedger();
  notifyListeners();
}

// ---------------------------------------------------------------------------
// Community-switch save / restore
// ---------------------------------------------------------------------------

type TurnsStoreSnapshot = {
  turns: Map<string, Map<string, ActiveTurn>>;
  offsets: Map<string, number>;
  sessions: AgentSessionGenerationSnapshot;
  watermarks: Map<string, Map<string, ObserverEvent>>;
  terminals: Map<string, Map<string, number>>;
  outcomes: Map<string, ConversationOutcomeEntry>;
};

/** Per-community snapshots. Keyed by community ID. */
const savedByCommunity = new Map<string, TurnsStoreSnapshot>();

/**
 * Snapshot the current active-turns state under `communityId` so it can be
 * restored when the user switches back.  If turns, tombstones, and outcomes
 * are all empty there is nothing worth restoring — discard any
 * previously-saved snapshot instead.
 *
 * Deep-clones all maps so subsequent mutations on the live maps do not
 * corrupt the snapshot.
 */
export function saveActiveAgentTurnsForCommunity(communityId: string): void {
  if (
    activeTurnsByAgent.size === 0 &&
    terminalAtByAgent.size === 0 &&
    conversationOutcomeLedgerSize() === 0
  ) {
    savedByCommunity.delete(communityId);
    return;
  }

  // Deep-clone activeTurnsByAgent: outer map + inner per-agent maps + turn
  // objects (plain structs, no nested references beyond primitives).
  const turns = new Map<string, Map<string, ActiveTurn>>();
  for (const [agentKey, agentTurns] of activeTurnsByAgent) {
    const clonedAgent = new Map<string, ActiveTurn>();
    for (const [turnId, turn] of agentTurns) {
      clonedAgent.set(turnId, { ...turn });
    }
    turns.set(agentKey, clonedAgent);
  }

  // Shallow-clone the offsets map (primitives as values).
  const offsets = new Map(clockOffsetByAgent);

  const watermarks = snapshotTurnsWatermarks();

  // Deep-clone terminalAtByAgent: outer map + inner per-agent maps.
  const terminals = new Map<string, Map<string, number>>();
  for (const [agentKey, tombstones] of terminalAtByAgent) {
    terminals.set(agentKey, new Map(tombstones));
  }

  savedByCommunity.set(communityId, {
    turns,
    offsets,
    sessions: snapshotAgentSessionGenerations(),
    watermarks,
    terminals,
    outcomes: cloneConversationOutcomeLedger(),
  });
}

/**
 * Restore a previously saved active-turns snapshot for `communityId` into the
 * module maps.  No-op when no snapshot exists.
 *
 * Clears all four module maps before writing so the function is
 * self-contained — it replaces rather than merging, regardless of whether the
 * caller pre-cleared.  At the primary call site (`useCommunityInit`) the maps
 * are already empty after `resetCommunityState()`, but this guard makes the
 * contract explicit.
 *
 * Preserves both activity clocks. A community switch is not evidence of
 * liveness or substantive progress; connection health decides whether stale
 * restored turns surface as telemetry failure or lost contact.
 *
 * Consumes the snapshot (deletes it from `savedByCommunity`) — a given
 * community's snapshot is only usable once per round-trip.
 */
export function restoreActiveAgentTurnsForCommunity(communityId: string): void {
  const snap = savedByCommunity.get(communityId);
  if (!snap) return;
  savedByCommunity.delete(communityId);

  // Clear before writing so this is a replace, not a merge.
  activeTurnsByAgent.clear();
  clockOffsetByAgent.clear();
  resetAgentSessionGenerations();
  clearTurnsWatermarks();
  terminalAtByAgent.clear();
  clearConversationOutcomeLedger();

  for (const [agentKey, agentTurns] of snap.turns) {
    const restored = new Map<string, ActiveTurn>();
    for (const [turnId, turn] of agentTurns) {
      restored.set(turnId, { ...turn });
    }
    activeTurnsByAgent.set(agentKey, restored);
  }

  for (const [agentKey, offset] of snap.offsets) {
    clockOffsetByAgent.set(agentKey, offset);
  }

  restoreAgentSessionGenerations(snap.sessions);

  restoreTurnsWatermarks(snap.watermarks);

  for (const [agentKey, tombstones] of snap.terminals) {
    terminalAtByAgent.set(agentKey, new Map(tombstones));
  }

  restoreConversationOutcomeLedger(snap.outcomes);

  cachedTurnSummaries.clear();
  cachedControlTargets.clear();
  cachedAgentsByConversation.clear();
  cachedChannelTurnSummaries = null;
  activeTurnsGeneration += 1;
  notifyListeners();
}

/**
 * Discard the saved turn-state snapshot for a community that has been
 * permanently deleted so the entry doesn't sit in memory indefinitely.
 * Call this alongside the other relay-specific GC in `removeCommunity`.
 */
export function clearSavedCommunitySnapshot(communityId: string): void {
  savedByCommunity.delete(communityId);
}
