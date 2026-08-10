import type { ObserverEvent } from "./ui/agentSessionTypes";

/**
 * Terminal conversation-outcome ledger for the channel view.
 *
 * Lives beside `activeAgentTurnsStore`: the store records/clears entries on
 * turn lifecycle events; selectors (`recentConversationOutcomes`,
 * `agentThreadDigestForChannel`) read through the accessors below.
 */

/**
 * How long a terminal conversation outcome stays visible on the channel view
 * after the turn ends. Four hours covers a lunch break / context switch without
 * littering overnight sessions.
 */
export const CONVERSATION_OUTCOME_TTL_MS = 4 * 60 * 60 * 1000;

/** Hard cap on outcome-ledger entries (LRU by endedAt). */
const MAX_OUTCOME_ENTRIES = 512;

/** Terminal outcome for one conversation/thread (latest event wins). */
export type ConversationOutcomeEntry = {
  outcome: "completed" | "error" | "lost-contact";
  agentPubkey: string;
  sessionId?: string;
  turnId?: string;
  channelId: string;
  endedAt: number;
  /** Producer terminal time used for replay-stable causal ordering. */
  terminalAt?: number;
  /** Stable tie-breaker when distinct producers terminate at the same time. */
  terminalOrderKey?: string;
  terminalAgentIndex?: number | null;
  terminalSeq?: number;
  terminalSessionId?: string | null;
  terminalTurnId?: string | null;
  failedEventIds?: string[];
  triggeringEventIds?: string[];
  agentTriggerPairs?: Array<{
    agentPubkey: string;
    eventId: string;
    sessionId: string;
    turnId: string;
  }>;
};

const outcomeByConversation = new Map<string, ConversationOutcomeEntry>();

export function conversationOutcomeTerminalOrderKey(
  agentKey: string,
  event: ObserverEvent,
  resolvedTurnId: string | null,
): string {
  return [
    event.sessionId ?? "null-session",
    event.seq.toString().padStart(16, "0"),
    agentKey,
    event.agentIndex ?? "null-agent",
    resolvedTurnId ?? "null-turn",
    event.kind,
  ].join("\u0000");
}

/** Build one exact producer/run-scoped terminal outcome from a signed frame. */
export function buildSignedConversationOutcome(input: {
  agentKey: string;
  event: ObserverEvent;
  resolvedTurnId: string;
  channelId: string;
  endedAt: number;
  terminalAt: number;
  triggeringEventIds: string[];
  sessionId?: string;
}): ConversationOutcomeEntry {
  const triggers = [...input.triggeringEventIds];
  return {
    outcome: input.event.kind === "turn_completed" ? "completed" : "error",
    agentPubkey: input.agentKey,
    sessionId: input.sessionId,
    turnId: input.resolvedTurnId,
    channelId: input.channelId,
    endedAt: input.endedAt,
    terminalAt: input.terminalAt,
    terminalOrderKey: conversationOutcomeTerminalOrderKey(
      input.agentKey,
      input.event,
      input.resolvedTurnId,
    ),
    terminalAgentIndex: input.event.agentIndex,
    terminalSeq: input.event.seq,
    terminalSessionId: input.event.sessionId,
    terminalTurnId: input.event.turnId,
    failedEventIds: triggers,
    triggeringEventIds: triggers,
    agentTriggerPairs: input.sessionId
      ? triggers.map((eventId) => ({
          agentPubkey: input.agentKey,
          eventId,
          sessionId: input.sessionId ?? "",
          turnId: input.resolvedTurnId,
        }))
      : undefined,
  };
}

/** Drop oldest-by-endedAt entries once past the hard cap. */
function enforceOutcomeCap() {
  while (outcomeByConversation.size > MAX_OUTCOME_ENTRIES) {
    let oldestId: string | null = null;
    let oldestAt = Number.POSITIVE_INFINITY;
    for (const [id, entry] of outcomeByConversation) {
      if (entry.endedAt < oldestAt) {
        oldestAt = entry.endedAt;
        oldestId = id;
      }
    }
    if (oldestId === null) break;
    outcomeByConversation.delete(oldestId);
  }
}

/**
 * Clear the ledger entry for a conversation. Returns true when an entry was
 * removed (callers bump their generation / notify).
 */
export function clearConversationOutcome(conversationId: string): boolean {
  return outcomeByConversation.delete(conversationId);
}

export function retireConversationOutcomeAgent(
  conversationId: string,
  agentPubkey: string,
): boolean {
  const existing = outcomeByConversation.get(conversationId);
  if (!existing) return false;
  if (
    existing.outcome !== "completed" ||
    !existing.agentTriggerPairs ||
    existing.agentTriggerPairs.length === 0
  ) {
    return existing.agentPubkey === agentPubkey
      ? outcomeByConversation.delete(conversationId)
      : false;
  }
  const remaining = existing.agentTriggerPairs.filter(
    (pair) => pair.agentPubkey !== agentPubkey,
  );
  if (remaining.length === existing.agentTriggerPairs.length) return false;
  if (remaining.length === 0)
    return outcomeByConversation.delete(conversationId);
  outcomeByConversation.set(conversationId, {
    ...existing,
    agentPubkey: remaining.at(-1)?.agentPubkey ?? existing.agentPubkey,
    agentTriggerPairs: remaining,
  });
  return true;
}

/**
 * Record the causally latest terminal outcome for a conversation.
 * Returns whether the ledger changed so callers can bump their generation.
 */
export function recordConversationOutcome(
  conversationId: string,
  incoming: ConversationOutcomeEntry,
): boolean {
  const prior = outcomeByConversation.get(conversationId);
  const entry =
    prior?.outcome === "completed" &&
    incoming.outcome === "completed" &&
    prior.agentTriggerPairs?.length
      ? {
          ...incoming,
          agentTriggerPairs: [
            ...new Map(
              [
                ...prior.agentTriggerPairs,
                ...(incoming.agentTriggerPairs ?? []),
              ].map((pair) => [
                [pair.agentPubkey, pair.eventId].join("\u0000"),
                pair,
              ]),
            ).values(),
          ],
        }
      : incoming;
  if (prior) {
    const priorIsInferred = prior.outcome === "lost-contact";
    const entryIsInferred = entry.outcome === "lost-contact";
    if (!priorIsInferred && entryIsInferred) return false;
    if (priorIsInferred && !entryIsInferred) {
      outcomeByConversation.set(conversationId, entry);
      enforceOutcomeCap();
      return true;
    }
    const priorHasOrder = Number.isFinite(prior.terminalAt);
    const entryHasOrder = Number.isFinite(entry.terminalAt);
    if (priorHasOrder && !entryHasOrder) return false;
    if (priorHasOrder && entryHasOrder) {
      const sameProducerGeneration =
        prior.agentPubkey === entry.agentPubkey &&
        prior.terminalAgentIndex === entry.terminalAgentIndex &&
        ((prior.terminalSessionId &&
          prior.terminalSessionId === entry.terminalSessionId) ||
          (prior.terminalTurnId &&
            prior.terminalTurnId === entry.terminalTurnId));
      if (sameProducerGeneration) {
        if ((entry.terminalSeq ?? -1) <= (prior.terminalSeq ?? -1)) {
          return false;
        }
      } else {
        const priorAt = prior.terminalAt ?? 0;
        const entryAt = entry.terminalAt ?? 0;
        if (entryAt < priorAt) return false;
        if (
          entryAt === priorAt &&
          (entry.terminalOrderKey ?? "") <= (prior.terminalOrderKey ?? "")
        ) {
          return false;
        }
      }
    }
  }
  outcomeByConversation.set(conversationId, entry);
  enforceOutcomeCap();
  return true;
}

/** Drop entries past the TTL. Returns true when anything was removed. */
export function pruneExpiredConversationOutcomes(now: number): boolean {
  let changed = false;
  for (const [conversationId, entry] of outcomeByConversation) {
    if (now - entry.endedAt > CONVERSATION_OUTCOME_TTL_MS) {
      outcomeByConversation.delete(conversationId);
      changed = true;
    }
  }
  return changed;
}

/**
 * Raw ledger lookup (no active-turn suppression, no TTL filter). Prefer
 * `getRecentOutcomeForConversation` for UI.
 */
export function getConversationOutcomeEntry(
  conversationId: string | null | undefined,
): ConversationOutcomeEntry | null {
  if (!conversationId) return null;
  return outcomeByConversation.get(conversationId) ?? null;
}

/** Walk every conversation outcome entry currently in the ledger. */
export function walkConversationOutcomes(
  visit: (conversationId: string, entry: ConversationOutcomeEntry) => void,
): void {
  for (const [conversationId, entry] of outcomeByConversation) {
    visit(conversationId, entry);
  }
}

export function conversationOutcomeLedgerSize(): number {
  return outcomeByConversation.size;
}

export function clearConversationOutcomeLedger(): void {
  outcomeByConversation.clear();
}

/** Deep-clone for community-switch snapshots. */
export function cloneConversationOutcomeLedger(): Map<
  string,
  ConversationOutcomeEntry
> {
  const outcomes = new Map<string, ConversationOutcomeEntry>();
  for (const [conversationId, entry] of outcomeByConversation) {
    outcomes.set(conversationId, {
      ...entry,
      failedEventIds: entry.failedEventIds
        ? [...entry.failedEventIds]
        : undefined,
      triggeringEventIds: entry.triggeringEventIds
        ? [...entry.triggeringEventIds]
        : undefined,
      agentTriggerPairs: entry.agentTriggerPairs?.map((pair) => ({ ...pair })),
    });
  }
  return outcomes;
}

/** Replace the live ledger with a previously cloned snapshot. */
export function restoreConversationOutcomeLedger(
  outcomes: ReadonlyMap<string, ConversationOutcomeEntry>,
): void {
  outcomeByConversation.clear();
  for (const [conversationId, entry] of outcomes) {
    outcomeByConversation.set(conversationId, {
      ...entry,
      failedEventIds: entry.failedEventIds
        ? [...entry.failedEventIds]
        : undefined,
      triggeringEventIds: entry.triggeringEventIds
        ? [...entry.triggeringEventIds]
        : undefined,
      agentTriggerPairs: entry.agentTriggerPairs?.map((pair) => ({ ...pair })),
    });
  }
}
