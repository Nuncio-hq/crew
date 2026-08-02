/**
 * Derived dispatch / edit-as-undo state for channel timeline messages.
 *
 * The harness already emits `turn_started.triggeringEventIds` and
 * `message_edit_applied` over the observer feed. This module holds the
 * edit-outcome side and pure selectors; live dispatch membership is read from
 * `collectTriggeringEventIds()` in observerRelayStore (no reverse import).
 *
 * Pending agent requests (sent, not yet in any `turn_started`) live here too
 * so Stop can appear during the dispatch-hold window before a turn exists.
 */

export type MessageEditAppliedOutcome = "patched" | "dropped";

export type MessageEditAppliedResult = {
  outcome: MessageEditAppliedOutcome;
  applied: boolean;
};

/**
 * Pending entries older than this are treated as never-dispatched and dropped
 * on read. Hold is ~2s and idle dispatch is near-instant, so anything still
 * queued after 30s will not become a turn.
 */
export const PENDING_AGENT_REQUEST_MAX_AGE_MS = 30_000;

/** A user-authored mention that is still waiting to be dispatched. */
export type PendingAgentRequest = {
  eventId: string;
  channelId: string;
  conversationId: string;
  agentPubkeys: string[];
  /** Wall-clock ms when the send was recorded locally. */
  sentAt: number;
};

const editOutcomesByEventId = new Map<string, MessageEditAppliedResult>();
const pendingByEventId = new Map<string, PendingAgentRequest>();
const outcomeListeners = new Set<() => void>();

const EMPTY_PUBKEYS: string[] = [];
const EMPTY_PENDING: PendingAgentRequest[] = [];
const pendingPubkeysByConversationCache = new Map<string, string[]>();
const pendingPubkeysByChannelCache = new Map<string, string[]>();
const pendingRequestsByConversationCache = new Map<
  string,
  PendingAgentRequest[]
>();
const pendingRequestsByChannelCache = new Map<string, PendingAgentRequest[]>();

function notifyOutcomeListeners(options?: { pendingChanged?: boolean }) {
  if (options?.pendingChanged) {
    pendingPubkeysByConversationCache.clear();
    pendingPubkeysByChannelCache.clear();
    pendingRequestsByConversationCache.clear();
    pendingRequestsByChannelCache.clear();
  }
  for (const listener of outcomeListeners) {
    listener();
  }
}

function normalizeEventId(eventId: string): string {
  return eventId.toLowerCase();
}

export function getMessageEditAppliedResult(
  eventId: string,
): MessageEditAppliedResult | null {
  return editOutcomesByEventId.get(normalizeEventId(eventId)) ?? null;
}

/**
 * Record a `message_edit_applied` observer frame. Harness is authoritative:
 * when `applied` is false the UI must surface too-late even if local state
 * still believed the message was queued.
 */
export function recordMessageEditApplied(
  targetEventId: string,
  outcome: MessageEditAppliedOutcome,
  applied: boolean,
): void {
  if (!/^[0-9a-fA-F]{64}$/.test(targetEventId)) {
    return;
  }
  const key = normalizeEventId(targetEventId);
  const previous = editOutcomesByEventId.get(key);
  if (
    previous &&
    previous.outcome === outcome &&
    previous.applied === applied
  ) {
    return;
  }
  editOutcomesByEventId.set(key, { outcome, applied });
  let pendingChanged = false;
  if (applied && outcome === "dropped" && pendingByEventId.delete(key)) {
    pendingChanged = true;
  }
  notifyOutcomeListeners({ pendingChanged });
}

/**
 * Record that a just-sent message mentioned agents and may still be held in
 * the harness queue. Cleared when the event is dispatched, withdrawn, or the
 * conversation is drained by Stop.
 */
export function recordPendingAgentRequest(input: {
  eventId: string;
  channelId: string;
  conversationId: string;
  agentPubkeys: readonly string[];
  /** Optional override for tests; defaults to Date.now(). */
  sentAt?: number;
}): void {
  if (!/^[0-9a-fA-F]{64}$/.test(input.eventId)) {
    return;
  }
  const agentPubkeys = [
    ...new Set(
      input.agentPubkeys
        .map((pubkey) => pubkey.trim().toLowerCase())
        .filter((pubkey) => /^[0-9a-f]{64}$/.test(pubkey)),
    ),
  ].sort();
  if (agentPubkeys.length === 0) {
    return;
  }
  const key = normalizeEventId(input.eventId);
  pendingByEventId.set(key, {
    eventId: key,
    channelId: input.channelId,
    conversationId: input.conversationId,
    agentPubkeys,
    sentAt: input.sentAt ?? Date.now(),
  });
  notifyOutcomeListeners({ pendingChanged: true });
}

/** Drop pending entries whose event ids appear in any `turn_started`. */
export function prunePendingAgentRequests(
  dispatchedIds: ReadonlySet<string>,
): void {
  if (pendingByEventId.size === 0 || dispatchedIds.size === 0) {
    return;
  }
  let changed = false;
  for (const eventId of pendingByEventId.keys()) {
    if (dispatchedIds.has(eventId)) {
      pendingByEventId.delete(eventId);
      changed = true;
    }
  }
  if (changed) {
    notifyOutcomeListeners({ pendingChanged: true });
  }
}

export function clearPendingAgentRequestsForConversation(
  conversationId: string,
): void {
  let changed = false;
  for (const [eventId, pending] of pendingByEventId) {
    if (pending.conversationId === conversationId) {
      pendingByEventId.delete(eventId);
      changed = true;
    }
  }
  if (changed) {
    notifyOutcomeListeners({ pendingChanged: true });
  }
}

/**
 * Drop pending entries past {@link PENDING_AGENT_REQUEST_MAX_AGE_MS}.
 * Prune-on-read only — no timer. Returns whether anything was removed.
 */
export function pruneStalePendingAgentRequests(now = Date.now()): boolean {
  if (pendingByEventId.size === 0) {
    return false;
  }
  let changed = false;
  for (const [eventId, pending] of pendingByEventId) {
    if (now - pending.sentAt > PENDING_AGENT_REQUEST_MAX_AGE_MS) {
      pendingByEventId.delete(eventId);
      changed = true;
    }
  }
  if (changed) {
    notifyOutcomeListeners({ pendingChanged: true });
  }
  return changed;
}

/**
 * Keep only pubkeys that are known community agents (managed ∪ relay).
 *
 * Pass the set from `useKnownAgentPubkeys` — identity, not observer liveness.
 * Humans are never in that set, so a human-only mention cannot light Stop
 * even when the observer registry is populated. Empty known set → [].
 */
export function filterPendingToKnownAgents(
  pending: readonly string[],
  knownAgents: ReadonlySet<string>,
): string[] {
  if (knownAgents.size === 0) {
    return [];
  }
  return pending.filter((pubkey) => knownAgents.has(pubkey));
}

function listPendingRequests(now = Date.now()): PendingAgentRequest[] {
  pruneStalePendingAgentRequests(now);
  if (pendingByEventId.size === 0) {
    return EMPTY_PENDING;
  }
  return [...pendingByEventId.values()];
}

export function getPendingAgentRequestsForConversation(
  conversationId: string | null | undefined,
): PendingAgentRequest[] {
  if (!conversationId) {
    return EMPTY_PENDING;
  }
  // Prune before cache lookup so age expiry cannot stick behind a hit.
  pruneStalePendingAgentRequests();
  const cached = pendingRequestsByConversationCache.get(conversationId);
  if (cached) {
    return cached;
  }
  const result = listPendingRequests().filter(
    (pending) => pending.conversationId === conversationId,
  );
  const stable = result.length === 0 ? EMPTY_PENDING : result;
  pendingRequestsByConversationCache.set(conversationId, stable);
  return stable;
}

export function getPendingAgentRequestsForChannel(
  channelId: string | null | undefined,
): PendingAgentRequest[] {
  if (!channelId) {
    return EMPTY_PENDING;
  }
  pruneStalePendingAgentRequests();
  const cached = pendingRequestsByChannelCache.get(channelId);
  if (cached) {
    return cached;
  }
  const result = listPendingRequests().filter(
    (pending) => pending.channelId === channelId,
  );
  const stable = result.length === 0 ? EMPTY_PENDING : result;
  pendingRequestsByChannelCache.set(channelId, stable);
  return stable;
}

export function getPendingAgentPubkeysForConversation(
  conversationId: string | null | undefined,
): string[] {
  if (!conversationId) {
    return EMPTY_PUBKEYS;
  }
  pruneStalePendingAgentRequests();
  const cached = pendingPubkeysByConversationCache.get(conversationId);
  if (cached) {
    return cached;
  }
  const merged = new Set<string>();
  for (const pending of getPendingAgentRequestsForConversation(
    conversationId,
  )) {
    for (const pubkey of pending.agentPubkeys) {
      merged.add(pubkey);
    }
  }
  const result = merged.size === 0 ? EMPTY_PUBKEYS : [...merged].sort();
  pendingPubkeysByConversationCache.set(conversationId, result);
  return result;
}

export function getPendingAgentPubkeysForChannel(
  channelId: string | null | undefined,
): string[] {
  if (!channelId) {
    return EMPTY_PUBKEYS;
  }
  pruneStalePendingAgentRequests();
  const cached = pendingPubkeysByChannelCache.get(channelId);
  if (cached) {
    return cached;
  }
  const merged = new Set<string>();
  for (const pending of getPendingAgentRequestsForChannel(channelId)) {
    for (const pubkey of pending.agentPubkeys) {
      merged.add(pubkey);
    }
  }
  const result = merged.size === 0 ? EMPTY_PUBKEYS : [...merged].sort();
  pendingPubkeysByChannelCache.set(channelId, result);
  return result;
}

/**
 * True when the conversation has a queued (not-yet-dispatched) request and/or
 * the caller already knows there is a running turn.
 */
export function conversationHasStoppableWork(args: {
  hasRunningTurn: boolean;
  pendingRequests: readonly PendingAgentRequest[];
}): boolean {
  return args.hasRunningTurn || args.pendingRequests.length > 0;
}

export function subscribeMessageEditApplied(listener: () => void): () => void {
  outcomeListeners.add(listener);
  return () => {
    outcomeListeners.delete(listener);
  };
}

export function resetDispatchedEventIdsStore(): void {
  if (editOutcomesByEventId.size === 0 && pendingByEventId.size === 0) {
    return;
  }
  editOutcomesByEventId.clear();
  pendingByEventId.clear();
  notifyOutcomeListeners({ pendingChanged: true });
}

function parseMessageEditAppliedPayload(payload: unknown): {
  targetEventId: string;
  outcome: MessageEditAppliedOutcome;
  applied: boolean;
} | null {
  if (typeof payload !== "object" || payload === null) {
    return null;
  }
  const record = payload as {
    targetEventId?: unknown;
    outcome?: unknown;
    applied?: unknown;
  };
  if (
    typeof record.targetEventId !== "string" ||
    typeof record.applied !== "boolean"
  ) {
    return null;
  }
  if (record.outcome !== "patched" && record.outcome !== "dropped") {
    return null;
  }
  return {
    targetEventId: record.targetEventId,
    outcome: record.outcome,
    applied: record.applied,
  };
}

/** Ingest a decoded observer frame; no-op unless kind is `message_edit_applied`. */
export function ingestObserverFrameForEditAsUndo(frame: {
  kind: string;
  payload: unknown;
}): void {
  if (frame.kind !== "message_edit_applied") {
    return;
  }
  const parsed = parseMessageEditAppliedPayload(frame.payload);
  if (!parsed) {
    return;
  }
  recordMessageEditApplied(
    parsed.targetEventId,
    parsed.outcome,
    parsed.applied,
  );
}

/**
 * Affordance state for editing a message that mentioned an agent.
 *
 * - `queued` — still editable-as-undo (not in any turn_started)
 * - `dispatched` — ordinary edit (agent already read it)
 * - `too-late` — harness rejected a race (`applied: false`)
 * - `withdrawn` — harness dropped the queued request (`outcome: dropped`)
 */
export type EditAsUndoUiState =
  | "queued"
  | "dispatched"
  | "too-late"
  | "withdrawn";

export function deriveEditAsUndoUiState(args: {
  mentionsAgent: boolean;
  eventId: string;
  dispatchedIds: ReadonlySet<string>;
  editResult: MessageEditAppliedResult | null;
}): EditAsUndoUiState | null {
  if (!args.mentionsAgent) {
    return null;
  }
  if (args.editResult) {
    if (!args.editResult.applied) {
      return "too-late";
    }
    if (args.editResult.outcome === "dropped") {
      return "withdrawn";
    }
  }
  if (args.dispatchedIds.has(normalizeEventId(args.eventId))) {
    return "dispatched";
  }
  return "queued";
}

/** @internal test helper */
export function _testEditOutcomeCount(): number {
  return editOutcomesByEventId.size;
}

/** @internal test helper */
export function _testPendingRequestCount(): number {
  return pendingByEventId.size;
}
