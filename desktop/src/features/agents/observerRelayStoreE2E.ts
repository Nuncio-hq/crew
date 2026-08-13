import type { ConnectionState, ObserverEvent } from "./ui/agentSessionTypes";

import { applyCrewE2EInjectSideEffects } from "./observerRelayStoreCrew";

/**
 * Crew-owned E2E / test harness for the observer relay store.
 *
 * Kept out of `observerRelayStore.ts` so upstream sync growth stays under the
 * desktop file-size ratchet (D-022). Bound once at store-module load.
 */

export type ObserverRelayStoreE2EApi = {
  getConnectionState: () => ConnectionState;
  getErrorMessage: () => string | null;
  openSilently: () => void;
  markLiveContactQuiet: (agentPubkey: string) => void;
  appendAgentEvents: (
    agentPubkey: string,
    events: readonly ObserverEvent[],
  ) => boolean;
  notifyListeners: () => void;
  setConnectionState: (
    nextState: ConnectionState,
    nextErrorMessage?: string | null,
  ) => void;
  clearLiveEvents: () => void;
  registerKnownAgents: (
    subscriptionId: string,
    pubkeys: readonly string[],
  ) => void;
  processLiveObserverEvents: (
    agentPubkey: string,
    events: readonly ObserverEvent[],
  ) => boolean;
  getArchivedChannelEvents: (
    agentPubkey: string,
    channelId: string,
  ) => ObserverEvent[];
};

let api: ObserverRelayStoreE2EApi | null = null;

export function bindObserverRelayStoreE2E(
  next: ObserverRelayStoreE2EApi,
): void {
  api = next;
}

function requireApi(): ObserverRelayStoreE2EApi {
  if (!api) {
    throw new Error("observerRelayStoreE2E not bound");
  }
  return api;
}

/**
 * E2E-only: inject synthetic observer events, bypassing knownAgentPubkeys.
 * Opens the store silently and publishes at most once (upstream #5680).
 */
export function injectObserverEventsForE2E(
  agentPubkey: string,
  events: ObserverEvent[],
): void {
  const store = requireApi();
  let opened = false;
  if (
    store.getConnectionState() !== "open" ||
    store.getErrorMessage() !== null
  ) {
    store.openSilently();
    opened = true;
  }
  for (const event of events) {
    if (!event.replayed) {
      store.markLiveContactQuiet(agentPubkey);
    }
  }
  const appended = store.appendAgentEvents(agentPubkey, events);
  if (appended) {
    applyCrewE2EInjectSideEffects(agentPubkey, events);
  }
  if (opened || appended) {
    store.notifyListeners();
  }
}

/** E2E-only: drive observer telemetry health through the production store. */
export function setObserverConnectionStateForE2E(state: ConnectionState): void {
  const store = requireApi();
  store.setConnectionState(
    state,
    state === "error" ? "Mock observer error" : null,
  );
  store.notifyListeners();
}

/** E2E-only: remove live frames while retaining the hydrated archive journal. */
export function resetAgentObserverLiveEventsForE2E(): void {
  requireApi().clearLiveEvents();
}

/** Test-only: register trusted agent pubkeys for a subscription id. */
export function _testRegisterKnownAgents(
  subscriptionId: string,
  pubkeys: readonly string[],
): void {
  requireApi().registerKnownAgents(subscriptionId, pubkeys);
}

/** Test-only: exercise live envelope ordering without relay/decryption setup. */
export function _testProcessLiveObserverEvents(
  agentPubkey: string,
  events: readonly ObserverEvent[],
): void {
  requireApi().processLiveObserverEvents(agentPubkey, events);
}

/** Test-only: read raw archived observer events for a (agent, channel) pair. */
export function _testGetArchivedChannelEvents(
  agentPubkey: string,
  channelId: string,
): ObserverEvent[] {
  return requireApi().getArchivedChannelEvents(agentPubkey, channelId);
}
