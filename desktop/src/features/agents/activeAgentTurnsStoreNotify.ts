/**
 * Crew-owned coalesced listener hub for activeAgentTurnsStore (issue #287).
 * Kept out of the upstream envelope so the file-size ratchet does not grow.
 */

import { createCoalescedHub } from "./lib/coalescedNotify";

const turnsHub = createCoalescedHub();

export function notifyListeners(): void {
  turnsHub.notify();
}

export function subscribeActiveAgentTurnsListeners(
  listener: () => void,
): () => void {
  return turnsHub.subscribe(listener);
}

export function activeAgentTurnsListenerCount(): number {
  return turnsHub.listenerCount;
}
