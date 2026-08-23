/**
 * Crew-owned coalesced listener hub for observerRelayStore (issue #287).
 * Kept out of the upstream envelope so the file-size ratchet does not grow.
 */

import type { ObserverEvent } from "./ui/agentSessionTypes";
import {
  createCoalescedHub,
  mergeUpdatesByAgentPubkey,
} from "./lib/coalescedNotify";

type ObserverNotifyUpdate = {
  agentPubkey: string;
  events: readonly ObserverEvent[];
};

const observerHub = createCoalescedHub<ObserverNotifyUpdate>({
  merge: mergeUpdatesByAgentPubkey,
});

export function notifyListeners(update?: ObserverNotifyUpdate): void {
  observerHub.notify(update);
}

export function subscribeAgentObserverStore(
  listener: (update?: ObserverNotifyUpdate) => void,
): () => void {
  return observerHub.subscribe(listener);
}

export function clearPendingObserverNotifications(): void {
  observerHub.clearPending();
}
