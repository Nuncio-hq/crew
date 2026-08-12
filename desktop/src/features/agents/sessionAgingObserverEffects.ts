/**
 * Crew #173 — session-aging side effects on observer frames.
 * Kept out of observerRelayStore so that upstream-heavy file stays ≤1000 lines.
 */

import type { ControlResultFrame } from "@/shared/api/types";
import {
  clearSessionAging,
  parseSessionAgingPayload,
  putSessionAging,
} from "@/features/messages/lib/sessionAgingStore";

/** Apply a `session_aging` observer payload into the aging store. */
export function applySessionAgingObserverPayload(
  agentPubkey: string,
  payload: unknown,
): void {
  const aging = parseSessionAgingPayload(agentPubkey, payload);
  if (aging) {
    putSessionAging(aging);
  }
}

/**
 * Clear aging banner after a successful owner handover / blind reset.
 * Returns true when this frame was a successful handover-family control.
 */
export function clearSessionAgingAfterHandoverControl(
  agentPubkey: string,
  payload: ControlResultFrame,
): boolean {
  if (
    (payload.type === "guided_handover" ||
      payload.type === "blind_session_reset") &&
    payload.status === "ok" &&
    typeof payload.conversationId === "string" &&
    payload.conversationId.length > 0
  ) {
    clearSessionAging(agentPubkey, payload.conversationId);
    return true;
  }
  return false;
}
