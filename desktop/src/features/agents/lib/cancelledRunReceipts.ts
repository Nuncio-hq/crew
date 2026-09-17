/**
 * Relating an agent receipt to a cancelled conversation outcome.
 *
 * A stop cancels specific runs, not the whole thread. When a later,
 * non-cancelled run leaves a receipt, that receipt is still reviewable work —
 * the two predicates here are the single place that decides whether a receipt
 * belongs to a cancelled run or demonstrably to another one.
 */

/** The receipt fields needed to place a receipt against a cancelled run. */
export type CancelledRunReceipt = Readonly<{
  agentPubkey: string;
  sessionId: string;
  turnId: string;
  parentEventId: string;
}>;

/** One cancelled run recorded on a conversation outcome. */
export type CancelledRunSlot = Readonly<{
  agentPubkey: string;
  triggeringEventIds: string[];
  sessionId?: string;
  turnId?: string;
}>;

/** The cancelled-outcome fields needed by both predicates. */
export type CancelledRunOutcome = Readonly<{
  outcome: string;
  agentPubkey: string;
  triggeringEventIds?: string[];
  sessionId?: string;
  turnId?: string;
  cancelledAgentSlots?: readonly CancelledRunSlot[];
}>;

/** The cancelled runs an outcome names, including the pre-slot legacy shape. */
export function cancelledRunSlots(
  outcome: CancelledRunOutcome,
): readonly CancelledRunSlot[] {
  if (outcome.outcome !== "cancelled") return [];
  return (
    outcome.cancelledAgentSlots ?? [
      {
        agentPubkey: outcome.agentPubkey,
        triggeringEventIds: outcome.triggeringEventIds ?? [],
        sessionId: outcome.sessionId,
        turnId: outcome.turnId,
      },
    ]
  );
}

/** Whether this exact receipt was produced by a run the owner cancelled. */
export function receiptMatchesCancelledRun(
  receipt: CancelledRunReceipt,
  outcome: CancelledRunOutcome,
): boolean {
  return cancelledRunSlots(outcome).some(
    (slot) =>
      receipt.agentPubkey === slot.agentPubkey &&
      Boolean(slot.sessionId) &&
      Boolean(slot.turnId) &&
      receipt.sessionId === slot.sessionId &&
      receipt.turnId === slot.turnId &&
      slot.triggeringEventIds.length > 0 &&
      slot.triggeringEventIds.includes(receipt.parentEventId),
  );
}

/**
 * Whether the receipt provably comes from a run the cancel did not cover.
 *
 * Deliberately conservative: a slot that does not name its own session and
 * turn cannot prove anything about a receipt from the same agent, so such a
 * receipt is not treated as a successor. Guessing wrong here would present
 * cancelled work as reviewable.
 */
export function receiptIsFromUncancelledRun(
  receipt: CancelledRunReceipt,
  outcome: CancelledRunOutcome,
): boolean {
  const slots = cancelledRunSlots(outcome);
  if (slots.length === 0) return false;
  // A receipt that does not name its own run cannot be placed against one.
  if (!receipt.sessionId || !receipt.turnId) return false;
  return slots.every((slot) => {
    if (receipt.agentPubkey !== slot.agentPubkey) return true;
    if (!slot.sessionId || !slot.turnId) return false;
    return (
      receipt.sessionId !== slot.sessionId || receipt.turnId !== slot.turnId
    );
  });
}
