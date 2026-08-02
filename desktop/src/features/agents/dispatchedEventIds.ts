/**
 * Derived dispatch / edit-as-undo state for channel timeline messages.
 *
 * The harness already emits `turn_started.triggeringEventIds` and
 * `message_edit_applied` over the observer feed. This module holds the
 * edit-outcome side and pure selectors; live dispatch membership is read from
 * `collectTriggeringEventIds()` in observerRelayStore (no reverse import).
 */

export type MessageEditAppliedOutcome = "patched" | "dropped";

export type MessageEditAppliedResult = {
  outcome: MessageEditAppliedOutcome;
  applied: boolean;
};

const editOutcomesByEventId = new Map<string, MessageEditAppliedResult>();
const outcomeListeners = new Set<() => void>();

function notifyOutcomeListeners() {
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
  notifyOutcomeListeners();
}

export function subscribeMessageEditApplied(listener: () => void): () => void {
  outcomeListeners.add(listener);
  return () => {
    outcomeListeners.delete(listener);
  };
}

export function resetDispatchedEventIdsStore(): void {
  if (editOutcomesByEventId.size === 0) {
    return;
  }
  editOutcomesByEventId.clear();
  notifyOutcomeListeners();
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
