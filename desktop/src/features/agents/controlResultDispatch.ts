/**
 * `control_result` observer fan-out + Crew #173 handover aging clear.
 * Extracted from observerRelayStore to keep that file under the 1000-line budget.
 */

import type { ControlResultFrame } from "@/shared/api/types";
import { clearPendingAgentRequestsForConversation } from "./dispatchedEventIds";
import { clearSessionAgingAfterHandoverControl } from "./sessionAgingObserverEffects";
import { normalizePubkey } from "@/shared/lib/pubkey";

const controlResultListeners = new Map<
  string,
  Set<(frame: ControlResultFrame) => void>
>();

function isControlResultFrame(payload: unknown): payload is ControlResultFrame {
  return (
    typeof payload === "object" &&
    payload !== null &&
    typeof (payload as { type?: unknown }).type === "string" &&
    typeof (payload as { status?: unknown }).status === "string"
  );
}

export function dispatchControlResult(
  agentPubkey: string,
  payload: unknown,
): void {
  if (!isControlResultFrame(payload)) {
    return;
  }
  if (
    payload.type === "cancel_turn" &&
    payload.status === "cancelled_queued" &&
    typeof payload.conversationId === "string" &&
    payload.conversationId.length > 0
  ) {
    clearPendingAgentRequestsForConversation(payload.conversationId);
  }
  clearSessionAgingAfterHandoverControl(agentPubkey, payload);
  const subscribers = controlResultListeners.get(normalizePubkey(agentPubkey));
  if (!subscribers) {
    return;
  }
  for (const subscriber of subscribers) {
    subscriber(payload);
  }
}

/**
 * Subscribe to `control_result` frames for a single agent. Returns an
 * unsubscribe function. Used by the ModelPicker to learn the async outcome of
 * a `switch_model` frame.
 */
export function subscribeControlResults(
  agentPubkey: string,
  listener: (frame: ControlResultFrame) => void,
): () => void {
  const key = normalizePubkey(agentPubkey);
  const subscribers = controlResultListeners.get(key) ?? new Set();
  subscribers.add(listener);
  controlResultListeners.set(key, subscribers);
  return () => {
    const current = controlResultListeners.get(key);
    if (!current) {
      return;
    }
    current.delete(listener);
    if (current.size === 0) {
      controlResultListeners.delete(key);
    }
  };
}

/** Test / community-reset helper. */
export function clearControlResultListeners(): void {
  controlResultListeners.clear();
}
