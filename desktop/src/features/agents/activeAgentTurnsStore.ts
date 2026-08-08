import { getAgentObserverSnapshot } from "@/features/agents/observerRelayStore";
import {
  clearTurnsWatermarks,
  gateEventByWatermark,
  recordEventProcessed,
  restoreTurnsWatermarks,
  snapshotTurnsWatermarks,
} from "@/features/agents/turnsWatermarkStore";
import {
  clearConversationOutcome,
  clearConversationOutcomeLedger,
  cloneConversationOutcomeLedger,
  conversationOutcomeLedgerSize,
  pruneExpiredConversationOutcomes,
  recordConversationOutcome,
  restoreConversationOutcomeLedger,
  type ConversationOutcomeEntry,
} from "@/features/agents/conversationOutcomeLedger";
import {
  clearTurnRetrying,
  recordTurnRetrying,
} from "@/features/agents/retryingTurnsStore";
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

/** Harness emits turn_liveness every ~10s (BUZZ_ACP_TURN_LIVENESS_SECS). */
const LIVENESS_INTERVAL_MS = 10_000;
/** Remove a turn after this long with no activity. Tolerates one fully dropped
 * liveness ping plus slack before pruning a turn whose host died without
 * unwinding (kill -9 / crash) — the only case that reaches this bound, since
 * graceful exits clear via turn_completed and working turns refresh on every
 * stream event. Derived from the interval so it tracks if the interval changes. */
const REMOVE_AFTER_MS = LIVENESS_INTERVAL_MS * 2.5;
/** Pause pruning for an agent once ALL of its tracked turns have gone this long
 * without activity — the "all at once" signature of that agent's frame stream
 * being down. Set below REMOVE_AFTER_MS so the pause engages before the 25s
 * prune would wipe badges. */
const FRAME_GAP_PAUSE_MS = LIVENESS_INTERVAL_MS * 2;
/** A silent agent is treated as dead after this bounded prune pause. */
const PRUNE_PAUSE_MAX_MS = 3 * 60_000;
/** Maximum concurrent active turns tracked per agent. Purely an unbounded-growth
 * guard, so it sits at the harness's hard upper bound for parallel agent
 * subprocesses (`--agents` / `BUZZ_ACP_AGENTS` accepts `1..=32`) rather than the
 * Desktop default of 24 — any lower value silently evicts a live turn, dropping
 * its working badge. */
const MAX_TURNS_PER_AGENT = 32;
/** Cap on per-agent terminal tombstones (A's resurrection guard). For
 * terminals that carry the turn's channelId (turn_completed / turn_error),
 * the (agent, channel) watermark already gates any liveness emitted before
 * the terminal — both gate on the same channel key and within-channel order
 * is preserved end-to-end — so those tombstones only matter briefly. The
 * tombstones doing durable work are the ones whose creation the channel
 * watermark does NOT witness: null-channel terminals (agent_panic gates on
 * the null bucket) and desktop-initiated clears (`clearActiveTurnsForAgent`).
 * Those are always the newest entries, so evicting the oldest past a small
 * multiple of the live cap preserves them. Worst case for an evicted
 * tombstone (a >128-completions-late liveness beating a quiet channel's
 * watermark) is a ghost badge that the prune reaps within its normal bounds —
 * bounded cosmetic staleness, not state corruption. */
const MAX_TERMINAL_TOMBSTONES = MAX_TURNS_PER_AGENT * 4;
/** Interval for pruning stale/expired turns. */
const PRUNE_INTERVAL_MS = 5_000;

type ActiveTurn = {
  turnId: string;
  channelId: string;
  conversationId: string;
  startedAt: number;
  lastActivityAt: number;
};

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

/** One conversation/thread with active agent work, aggregated across agents. */
export type ActiveConversationTurnSummary = {
  channelId: string;
  conversationId: string;
  anchorAt: number;
  agentCount: number;
  agentPubkeys: string[];
  agentNames?: string[];
};

// Module-level state: agentPubkey → turnId → ActiveTurn
const activeTurnsByAgent = new Map<string, Map<string, ActiveTurn>>();
const listeners = new Set<() => void>();

// Per-agent clock offset: the desktop clock minus the agent-host clock, in
// milliseconds. Estimated as the running minimum of
// (Date.now() - Date.parse(event.timestamp)) across that agent's events. The
// minimum converges on true skew minus the smallest network/processing delay
// seen — a monotonically tightening estimate immune to per-event jitter. While
// true skew is constant or shrinking it is conservative: elapsed under-reports
// by the minimum delay and never inflates. The minimum never loosens, so under
// GROWING skew (an NTP step forward, or the host clock drifting further behind
// mid-session) the stored estimate goes stale-too-small and elapsed can over-
// report — bounded by how far the skew grows, sub-second over a session. A
// turn's badge anchor is startedAt + offset: the agent's own start, translated
// into desktop-clock terms. Anchors are derived at read time so a later, tighter
// offset retroactively corrects every live turn — distinct agent starts then
// yield distinct anchors (no lockstep) and a turn started long ago anchors into
// the past (large elapsed) instead of resetting to Date.now().
const clockOffsetByAgent = new Map<string, number>();

// Cached snapshots for useSyncExternalStore reference stability.
// Only regenerated when the underlying turn map for an agent actually changes.
const cachedTurnSummaries = new Map<string, ActiveTurnSummary[]>();
const cachedControlTargets = new Map<string, ActiveTurnControlTarget[]>();
const cachedAgentsByConversation = new Map<string, string[]>();
let cachedChannelTurnSummaries: ActiveChannelTurnSummary[] | null = null;
let cachedConversationTurnSummaries: ActiveConversationTurnSummary[] | null =
  null;
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
  cachedConversationTurnSummaries = null;
  activeTurnsGeneration += 1;
}

function invalidateCache(agentKey: string) {
  cachedTurnSummaries.delete(agentKey);
  cachedControlTargets.delete(agentKey);
  bumpActiveTurnsGeneration();
}

function clearOutcomeAndBump(conversationId: string) {
  if (!clearConversationOutcome(conversationId)) return;
  bumpActiveTurnsGeneration();
}

function recordOutcomeAndBump(
  conversationId: string,
  entry: ConversationOutcomeEntry,
) {
  recordConversationOutcome(conversationId, entry);
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

function startTurn(
  agentPubkey: string,
  channelId: string,
  conversationId: string,
  turnId: string,
  timestamp: string,
) {
  const key = normalizePubkey(agentPubkey);
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

  const startedAt = parseTimestamp(timestamp) ?? Date.now();
  agentTurns.set(turnId, {
    turnId,
    channelId,
    conversationId,
    startedAt,
    lastActivityAt: Date.now(),
  });
  // A new turn for this conversation supersedes any prior terminal outcome.
  clearOutcomeAndBump(conversationId);
  invalidateCache(key);
}

function recordActivity(agentPubkey: string, turnId: string | null): boolean {
  if (!turnId) return false;
  const key = normalizePubkey(agentPubkey);
  const agentTurns = activeTurnsByAgent.get(key);
  if (!agentTurns) return false;
  const turn = agentTurns.get(turnId);
  if (turn) {
    turn.lastActivityAt = Date.now();
    return true;
  }
  return false;
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
    safeStartedAt,
  );
  return true;
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
  channelId: string | null,
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

  if (turnId) {
    agentTurns.delete(turnId);
  } else if (channelId) {
    // Fallback: remove by channelId if turnId not available. Tombstone the
    // resolved turn so a later stale liveness for it can't resurrect a badge.
    for (const [tid, turn] of agentTurns) {
      if (turn.channelId === channelId) {
        agentTurns.delete(tid);
        recordTerminal(key, tid, terminalAt);
        break;
      }
    }
  }
  if (agentTurns.size === 0) {
    activeTurnsByAgent.delete(key);
  }
  invalidateCache(key);
}

/** True when every tracked turn for one agent is stale, but only until the
 * bounded backstop expires. Other agents' activity intentionally has no effect. */
function shouldPausePrune(
  agentTurns: Map<string, ActiveTurn>,
  now: number,
): boolean {
  let maxActivity = 0;
  for (const turn of agentTurns.values()) {
    if (turn.lastActivityAt > maxActivity) maxActivity = turn.lastActivityAt;
  }
  const silentFor = now - maxActivity;
  return (
    maxActivity > 0 &&
    silentFor > FRAME_GAP_PAUSE_MS &&
    silentFor < PRUNE_PAUSE_MAX_MS
  );
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
      if (now - turn.lastActivityAt > REMOVE_AFTER_MS) {
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
  if (changed) {
    notifyListeners();
  }
}

// INVARIANT: events must be sorted by (timestamp, seq) ascending.
// syncAgentTurnsFromEvents receives sorted arrays from observerRelayStore.
// Calling with unsorted events will cause silent data loss.
function processEvent(agentPubkey: string, event: ObserverEvent) {
  const key = normalizePubkey(agentPubkey);

  // Gate on the per-(agent, channel) watermark to keep sorted replays a no-op
  // and prevent stale liveness/eviction from killing live turns.
  if (gateEventByWatermark(key, event)) {
    return;
  }
  recordEventProcessed(key, event);

  // Refine the clock offset from every fresh event. A tighter offset shifts
  // every live anchor for this agent, so a change must reach the UI even when
  // the event itself surfaces no new turn.
  const offsetChanged = sampleClockOffset(key, event.timestamp);

  switch (event.kind) {
    case "turn_started":
      if (event.channelId) {
        clearTurnRetrying(event.conversationId ?? event.channelId);
        startTurn(
          agentPubkey,
          event.channelId,
          event.conversationId ?? event.channelId,
          event.turnId ?? `seq-${event.seq}`,
          event.timestamp,
        );
        notifyListeners();
        return;
      }
      break;
    case "turn_retrying": {
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
        notifyListeners();
        return;
      }
      break;
    }
    case "turn_completed":
    case "turn_error":
    case "agent_panic": {
      // Do not clear retrying here: automatic requeue emits `turn_retrying`
      // in the same failure path, and clearing on turn_error would wipe it.
      // Retrying state clears on the next `turn_started` (or a later
      // turn_retrying overwrite).
      // Reuse the event's already-resolved channel/conversation ids — do not
      // re-derive from the live turn map (the turn may already be pruned).
      const conversationId = event.conversationId ?? event.channelId;
      if (event.channelId && conversationId) {
        recordOutcomeAndBump(conversationId, {
          outcome: event.kind === "turn_completed" ? "completed" : "error",
          agentPubkey: key,
          channelId: event.channelId,
          endedAt: Date.now(),
        });
      }
      endTurn(
        agentPubkey,
        event.turnId ?? null,
        event.channelId ?? null,
        Date.parse(event.timestamp),
      );
      notifyListeners();
      return;
    }
    case "acp_read":
    case "acp_write":
    // turn_liveness keeps a quiet-but-alive turn from being pruned; same
    // refresh-only path as stream activity — no surfaced summary change on its
    // own, so it only notifies when the offset above actually moved. If the
    // turn was pruned out from under a still-running host (a transient drop
    // raced the pause, or the lone-crash residual self-healed), resurrect it.
    case "turn_liveness": {
      const refreshed = recordActivity(agentPubkey, event.turnId ?? null);
      if (!refreshed && resurrectTurn(agentPubkey, event)) {
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
const EMPTY_CONVERSATION_TURNS: ActiveConversationTurnSummary[] = [];
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

/**
 * Returns active working conversations/threads across all tracked agents,
 * sorted by conversationId and anchored to the earliest live turn in each.
 */
export function getActiveTurnsByConversation(): ActiveConversationTurnSummary[] {
  if (cachedConversationTurnSummaries) return cachedConversationTurnSummaries;
  if (activeTurnsByAgent.size === 0) return EMPTY_CONVERSATION_TURNS;

  const summaries = new Map<
    string,
    { anchorAt: number; channelId: string; agentPubkeys: Set<string> }
  >();
  for (const [agentKey, agentTurns] of activeTurnsByAgent) {
    if (agentTurns.size === 0) continue;
    const offset = clockOffsetByAgent.get(agentKey) ?? 0;

    for (const turn of agentTurns.values()) {
      const conversationId = turn.conversationId;
      if (!conversationId) continue;
      const anchorAt = turn.startedAt + offset;
      const summary = summaries.get(conversationId);
      if (!summary) {
        summaries.set(conversationId, {
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
    .map(([conversationId, summary]) => ({
      channelId: summary.channelId,
      conversationId,
      anchorAt: summary.anchorAt,
      agentCount: summary.agentPubkeys.size,
      agentPubkeys: [...summary.agentPubkeys].sort(),
    }))
    .sort((a, b) => a.conversationId.localeCompare(b.conversationId));
  cachedConversationTurnSummaries = result;
  return result;
}

/** Desktop-clock activity bounds for the given agents, optionally scoped. */
export function getActiveTurnActivityBounds(options: {
  agentPubkeys: readonly string[];
  channelId?: string | null;
  conversationId?: string | null;
}): { anchorAt: number; lastActivityAt: number } | null {
  const channelId = options.channelId?.trim() || null;
  const conversationId = options.conversationId?.trim() || null;
  let anchorAt = Number.POSITIVE_INFINITY;
  let lastActivityAt = 0;

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
      if (turn.lastActivityAt > lastActivityAt) {
        lastActivityAt = turn.lastActivityAt;
      }
    }
  }

  if (!Number.isFinite(anchorAt) || lastActivityAt === 0) return null;
  return { anchorAt, lastActivityAt };
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
  cachedTurnSummaries.clear();
  cachedControlTargets.clear();
  cachedAgentsByConversation.clear();
  cachedChannelTurnSummaries = null;
  cachedConversationTurnSummaries = null;
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
 * Refreshes `lastActivityAt` on every restored turn so the prune interval
 * doesn't immediately kill turns that were saved more than 25 s ago (the prune
 * threshold).  New observer events arriving after restore will update
 * `lastActivityAt` normally via `recordActivity`.
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
  clearTurnsWatermarks();
  terminalAtByAgent.clear();
  clearConversationOutcomeLedger();

  const now = Date.now();

  for (const [agentKey, agentTurns] of snap.turns) {
    const restored = new Map<string, ActiveTurn>();
    for (const [turnId, turn] of agentTurns) {
      restored.set(turnId, { ...turn, lastActivityAt: now });
    }
    activeTurnsByAgent.set(agentKey, restored);
  }

  for (const [agentKey, offset] of snap.offsets) {
    clockOffsetByAgent.set(agentKey, offset);
  }

  restoreTurnsWatermarks(snap.watermarks);

  for (const [agentKey, tombstones] of snap.terminals) {
    terminalAtByAgent.set(agentKey, new Map(tombstones));
  }

  restoreConversationOutcomeLedger(snap.outcomes);

  cachedTurnSummaries.clear();
  cachedControlTargets.clear();
  cachedAgentsByConversation.clear();
  cachedChannelTurnSummaries = null;
  cachedConversationTurnSummaries = null;
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
