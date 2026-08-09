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
  channelId: string;
  endedAt: number;
  failedEventIds?: string[];
  triggeringEventIds?: string[];
};

const outcomeByConversation = new Map<string, ConversationOutcomeEntry>();

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

/**
 * Record (or overwrite) the latest terminal outcome for a conversation.
 * Enforces the LRU cap. Always mutates — callers bump generation.
 */
export function recordConversationOutcome(
  conversationId: string,
  entry: ConversationOutcomeEntry,
): void {
  outcomeByConversation.set(conversationId, entry);
  enforceOutcomeCap();
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
    });
  }
}
