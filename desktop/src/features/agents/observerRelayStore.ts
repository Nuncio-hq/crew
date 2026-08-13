import * as React from "react";

import { subscribeToAgentObserverFrames } from "@/shared/api/observerRelay";
import { relayClient } from "@/shared/api/relayClient";
import type { RelayLiveEventContext } from "@/shared/api/relayClientShared";
import type { RelayEvent, ManagedAgent } from "@/shared/api/types";
import { putAgentSessionConfig } from "@/shared/api/tauri";
import { putManagedAgentRuntimeLifecycle } from "@/shared/api/tauriManagedAgents";
import { getIdentity } from "@/shared/api/tauriIdentity";
import { decryptObserverEvent } from "@/shared/api/tauriObserver";
import {
  parseAgentManagementRequest,
  type AgentManagementRequest,
} from "./agentManagement";
import { normalizePubkey } from "@/shared/lib/pubkey";
import {
  getLatestAuthorizedLiveSessionId,
  resetLiveSessionAuthority,
} from "./observerLiveSessionAuthority";
import { useQueryClient } from "@tanstack/react-query";
import { agentConfigSurfaceQueryKey } from "@/features/agents/hooks";
import type {
  ConnectionState,
  ObserverEvent,
  TranscriptItem,
} from "./ui/agentSessionTypes";
import {
  type TranscriptState,
  buildTranscriptState,
  createEmptyTranscriptState,
  processTranscriptEvent,
} from "./ui/agentSessionTranscript";
import {
  prunePendingAgentRequests,
  resetDispatchedEventIdsStore,
} from "./dispatchedEventIds";
import { resetProjectThreadWorkspaceStore } from "./projectThreadWorkspaceStore";
import { logObserverDrop, resetObserverDropLogger } from "./observerDropLogger";
import {
  observerEventIdentity,
  unwrapObserverBatch,
} from "./observerEventIdentity";
import {
  applyCrewAppendBatchSideEffects,
  applyCrewE2EInjectSideEffects,
  applyCrewLiveFrameSideEffects,
  filterLiveObserverEventsForCrew,
} from "./observerRelayStoreCrew";
import {
  clearControlResultListeners,
  subscribeControlResults,
} from "./controlResultDispatch";

const MAX_OBSERVER_EVENTS = 3000;
const MAX_PENDING_UNKNOWN_AGENT_FRAMES = 100;

export type ObserverSnapshot = {
  connectionState: ConnectionState;
  errorMessage: string | null;
  events: ObserverEvent[];
};

export { getObserverDropCountsForTest as _testGetObserverDropCounts } from "./observerDropLogger";
export { subscribeControlResults };

const IDLE_SNAPSHOT: ObserverSnapshot = {
  connectionState: "idle",
  errorMessage: null,
  events: [],
};

const EMPTY_EVENTS: ObserverEvent[] = [];
const EMPTY_TRANSCRIPT: TranscriptItem[] = [];

const listeners = new Set<() => void>();
const eventsByAgent = new Map<string, ObserverEvent[]>();
const transcriptByAgent = new Map<string, TranscriptState>();
const snapshotByAgent = new Map<string, ObserverSnapshot>();
const connectionErrorByAgent = new Map<string, string>();
const agentsWithCurrentLiveContact = new Set<string>();

// Channel-scoped archive event journal — holds paged history loaded from the local
// SQLite archive without the MAX_OBSERVER_EVENTS live-relay cap. Keyed by
// `${normalizedAgentPubkey}:${channelId}`. The live relay path writes to
// `eventsByAgent` (per-agent, capped) and this map is NEVER written by live
// events — separation is strict so loading deep history can never evict live frames
// or vice versa. UI consumers merge the raw events from both sources, then derive
// TranscriptState once over the combined window.
const archiveEventsByChannel = new Map<string, ObserverEvent[]>();

// Per-agent, per-channel latest-live-session-id.
// Key: `${normalizePubkey(agentPubkey)}:${channelId}`.
// Set when a live relay observer event with a sessionId arrives.
// Cleared in resetAgentObserverStore.
//
// "Latest-live" means: the sessionId that most recently appeared via the
// live relay path (handleRelayObserverEvent). It is NOT derived from
// connectionState or an ever-live Set — an ever-live Set would incorrectly
// mark session A as "current" after session B has started (Thufir Pass 3).
//
// Payload timestamps and sequence numbers are producer-local and can reset or
// skew across sessions. A changed generation in one conversation or an exact
// turn_started frame advances channel authority; arbitrary late frames from a
// different conversation cannot roll it back.
/** Read the latest-live-session-id for a (agent, channel) pair. */
export function getLatestLiveSessionId(
  agentPubkey: string | null | undefined,
  channelId: string | null | undefined,
): string | null {
  return getLatestAuthorizedLiveSessionId(agentPubkey, channelId);
}

const agentManagementListeners = new Set<
  (agentPubkey: string, request: AgentManagementRequest) => void
>();

// Normalized pubkeys of agents we are actively managing. Only events whose
// "agent" tag matches an entry here will be decrypted (defense-in-depth).
//
// This set is the *union* of every active subscriber's contribution. Multiple
// callers of `useManagedAgentObserverBridge` (e.g. the channel screen and the
// profile panel) can be mounted at once, each tracking a different agent list.
// We key each subscriber's contribution in `knownAgentsBySubscription` and
// recompute the union, so co-mounted callers no longer clobber each other.
const knownAgentPubkeys = new Set<string>();
const knownAgentsBySubscription = new Map<string, Set<string>>();
const pendingUnknownAgentFrames: Array<{
  event: RelayEvent;
  context: RelayLiveEventContext;
}> = [];

// Callback invoked when session_config_captured is received, so React Query
// can invalidate the config-surface query for the affected agent. Wired up
// by useManagedAgentObserverBridge via setSessionConfigCapturedCallback.
let onSessionConfigCaptured: ((pubkey: string) => void) | null = null;

export function setSessionConfigCapturedCallback(
  cb: ((pubkey: string) => void) | null,
) {
  onSessionConfigCaptured = cb;
}

function recomputeKnownAgentPubkeys() {
  knownAgentPubkeys.clear();
  for (const subscriptionAgents of knownAgentsBySubscription.values()) {
    for (const pubkey of subscriptionAgents) {
      knownAgentPubkeys.add(pubkey);
    }
  }
}

function registerKnownAgents(
  subscriptionId: string,
  pubkeys: readonly string[],
) {
  knownAgentsBySubscription.set(
    subscriptionId,
    new Set(pubkeys.map((pubkey) => normalizePubkey(pubkey))),
  );
  recomputeKnownAgentPubkeys();
  if (knownAgentPubkeys.size > 0 && pendingUnknownAgentFrames.length > 0) {
    const pending = pendingUnknownAgentFrames.splice(0);
    for (const { event, context } of pending) {
      eventProcessingQueue = eventProcessingQueue.then(() =>
        handleRelayObserverEvent(event, generation, context),
      );
    }
  }
}

function unregisterKnownAgents(subscriptionId: string) {
  if (knownAgentsBySubscription.delete(subscriptionId)) {
    recomputeKnownAgentPubkeys();
  }
}

let connectionState: ConnectionState = "idle";
let errorMessage: string | null = null;
let unsubscribeRelay: (() => Promise<void>) | null = null;
let unsubscribeRelayState: (() => void) | null = null;
let startPromise: Promise<void> | null = null;
let eventProcessingQueue: Promise<void> = Promise.resolve();
let generation = 0;
let relayConnectionHealthy = false;
let observerSubscriptionReady = false;

function notifyListeners() {
  for (const listener of listeners) {
    listener();
  }
}

function invalidateSnapshot(key: string) {
  snapshotByAgent.delete(key);
}

/** Whether a relay-state transition invalidates current-session contact proof. */
export function shouldResetObserverLiveContacts(
  currentState: ConnectionState,
  nextState: ConnectionState,
) {
  return (
    (nextState === "connecting" && currentState !== "connecting") ||
    (nextState !== "open" && nextState !== "connecting")
  );
}

function setConnectionState(
  nextState: ConnectionState,
  nextErrorMessage: string | null = errorMessage,
) {
  if (shouldResetObserverLiveContacts(connectionState, nextState)) {
    agentsWithCurrentLiveContact.clear();
  }
  connectionState = nextState;
  errorMessage = nextErrorMessage;
  snapshotByAgent.clear();
  notifyListeners();
}

function markAgentLiveContact(
  agentPubkey: string,
  options?: { notify?: boolean },
) {
  const key = normalizePubkey(agentPubkey);
  if (agentsWithCurrentLiveContact.has(key)) return;
  agentsWithCurrentLiveContact.add(key);
  invalidateSnapshot(key);
  if (options?.notify !== false) {
    notifyListeners();
  }
}

function setAgentConnectionError(agentPubkey: string, message: string | null) {
  const key = normalizePubkey(agentPubkey);
  const prior = connectionErrorByAgent.get(key) ?? null;
  if (prior === message) return;
  if (message) connectionErrorByAgent.set(key, message);
  else connectionErrorByAgent.delete(key);
  invalidateSnapshot(key);
  notifyListeners();
}

function observerTag(event: RelayEvent, tagName: string) {
  return event.tags.find((tag) => tag[0] === tagName)?.[1] ?? null;
}

function appendAgentEvents(
  agentPubkey: string,
  events: readonly ObserverEvent[],
): boolean {
  if (events.length === 0) return false;

  const key = normalizePubkey(agentPubkey);
  const current = eventsByAgent.get(key) ?? [];
  const seen = new Set(current.map((event) => observerEventIdentity(event)));
  const added: ObserverEvent[] = [];
  for (const event of events) {
    const identity = observerEventIdentity(event);
    if (seen.has(identity)) continue;
    seen.add(identity);
    added.push(event);
  }
  if (added.length === 0) return false;

  const sortedAdded = added.sort(compareObserverEvents);
  const sorted = [...current, ...sortedAdded].sort(compareObserverEvents);
  const trimmed = sorted.length > MAX_OBSERVER_EVENTS;
  const final = trimmed
    ? sorted.slice(sorted.length - MAX_OBSERVER_EVENTS)
    : sorted;
  eventsByAgent.set(key, final);

  // The common live path appends a sorted batch after the retained window. Fold
  // that batch through the transcript state once without rebuilding history.
  // Out-of-order arrivals and cap eviction rebuild from the final window so
  // stateful tool/permission relationships remain correct.
  const currentLast = current.at(-1);
  const allAtEnd =
    !currentLast ||
    sortedAdded.every((event) => compareObserverEvents(event, currentLast) > 0);
  if (allAtEnd && !trimmed) {
    let transcriptState =
      transcriptByAgent.get(key) ?? createEmptyTranscriptState();
    for (const event of sortedAdded) {
      transcriptState = processTranscriptEvent(transcriptState, event);
    }
    transcriptByAgent.set(key, transcriptState);
  } else {
    transcriptByAgent.set(key, buildTranscriptState(final));
  }

  applyCrewAppendBatchSideEffects(key, sortedAdded, () =>
    prunePendingAgentRequests(collectTriggeringEventIds()),
  );
  invalidateSnapshot(key);
  return true;
}

function appendAgentEvent(agentPubkey: string, event: ObserverEvent) {
  if (appendAgentEvents(agentPubkey, [event])) {
    notifyListeners();
  }
}

/**
 * Flatten every `turn_started.triggeringEventIds` across agent transcripts.
 * Used by the channel timeline to know which messages the agent has already
 * read (edit-as-undo window closed).
 */
export function collectTriggeringEventIds(): Set<string> {
  const ids = new Set<string>();
  for (const state of transcriptByAgent.values()) {
    for (const turnIds of state.triggeringEventIdsByTurn.values()) {
      for (const id of turnIds) {
        if (/^[0-9a-fA-F]{64}$/.test(id)) {
          ids.add(id.toLowerCase());
        }
      }
    }
  }
  return ids;
}

/**
 * Compose the map key for the channel-scoped archive transcript.
 * Separates agent identity from channel with `:` — the same delimiter used by
 * liveSessionKey so all composite keys in this module are consistently shaped.
 */
function archiveChannelKey(agentPubkey: string, channelId: string): string {
  return `${normalizePubkey(agentPubkey)}:${channelId}`;
}

/** Append one identity-deduplicated event to the uncapped archive window. */
function appendArchivedChannelEvent(
  agentPubkey: string,
  channelId: string,
  event: ObserverEvent,
): boolean {
  const key = archiveChannelKey(agentPubkey, channelId);
  const current = archiveEventsByChannel.get(key) ?? [];

  if (
    current.some(
      (existing) =>
        observerEventIdentity(existing) === observerEventIdentity(event),
    )
  ) {
    return false;
  }

  // Archive pages arrive newest-first from SQLite, so each new event sorts
  // BEFORE the existing entries. Sort the combined array to maintain ascending
  // order for consumers that call buildTranscriptState over the window.
  const sorted = [...current, event].sort(compareObserverEvents);
  archiveEventsByChannel.set(key, sorted);
  return true;
}

/**
 * Read the channel-scoped archive raw events for a given (agent, channel)
 * pair. Returns an empty array when no archive has been loaded yet.
 *
 * Called by `useArchivedChannelEvents` so UI components can reactively
 * subscribe to archive loads and derive transcript state from the combined
 * live + archive raw event window without touching the live-capped per-agent
 * store.
 */
export function getArchivedChannelEvents(
  agentPubkey: string | null | undefined,
  channelId: string | null | undefined,
): ObserverEvent[] {
  if (!agentPubkey || !channelId) return EMPTY_EVENTS;
  return (
    archiveEventsByChannel.get(archiveChannelKey(agentPubkey, channelId)) ??
    EMPTY_EVENTS
  );
}

export function compareObserverEvents(
  left: ObserverEvent,
  right: ObserverEvent,
) {
  const causalOrder = compareObserverCausalOrder(left, right);
  if (causalOrder !== 0) return causalOrder;
  return observerEventIdentity(left).localeCompare(
    observerEventIdentity(right),
  );
}

type ObserverOrderFields = Pick<
  ObserverEvent,
  "agentIndex" | "seq" | "sessionId" | "sourceEventId" | "timestamp" | "turnId"
>;

function compareObserverCausalOrder(
  left: ObserverOrderFields,
  right: ObserverOrderFields,
) {
  const sameAgentIndex = left.agentIndex === right.agentIndex;
  const sameSession =
    sameAgentIndex &&
    left.sessionId != null &&
    left.sessionId === right.sessionId;
  const sameTurn =
    sameAgentIndex && left.turnId != null && left.turnId === right.turnId;
  if ((sameSession || sameTurn) && left.seq !== right.seq) {
    return left.seq - right.seq;
  }
  const leftTime = Date.parse(left.timestamp);
  const rightTime = Date.parse(right.timestamp);
  if (Number.isFinite(leftTime) && Number.isFinite(rightTime)) {
    const timeDiff = leftTime - rightTime;
    if (timeDiff !== 0) {
      return timeDiff;
    }
  }

  return (
    (left.agentIndex ?? -1) - (right.agentIndex ?? -1) ||
    (left.sessionId ?? "").localeCompare(right.sessionId ?? "") ||
    (left.turnId ?? "").localeCompare(right.turnId ?? "") ||
    (left.sourceEventId ?? "").localeCompare(right.sourceEventId ?? "") ||
    left.seq - right.seq
  );
}

/**
 * Returns true if `candidate` sorts strictly after `stored` using the same
 * two-key ordering as `compareObserverEvents`: later timestamp wins; equal
 * timestamp falls back to higher seq.  Extracted so latest-live advancement
 * cannot drift from transcript ordering.
 */
export function isObserverEventAfter(
  candidate: ObserverOrderFields,
  stored: ObserverOrderFields,
): boolean {
  return compareObserverCausalOrder(candidate, stored) > 0;
}

// Per-event processing shared by every event a live frame carries (one for a
// plain frame, many for a batch envelope).
function processLiveObserverEvents(
  agentPubkey: string,
  events: readonly ObserverEvent[],
): boolean {
  if (events.length === 0) return false;

  const { acceptedEvents, authorityUnavailable } =
    filterLiveObserverEventsForCrew(agentPubkey, events);
  if (authorityUnavailable) {
    setAgentConnectionError(
      agentPubkey,
      "Observer session authority exceeded its safe recovery bound",
    );
  }
  if (acceptedEvents.length === 0) return false;

  for (const parsed of acceptedEvents) {
    if (!parsed.replayed) {
      markAgentLiveContact(agentPubkey, { notify: false });
    }
  }

  // Commit the full envelope before dispatching synchronous specialized
  // callbacks. Those callbacks historically observed their triggering frame
  // in the raw/transcript stores; batching must preserve that visibility while
  // deferring only the global external-store publication.
  const observerChanged = appendAgentEvents(agentPubkey, acceptedEvents);

  for (const parsed of acceptedEvents) {
    const managementRequest = parseAgentManagementRequest(parsed.payload);
    if (managementRequest) {
      for (const listener of agentManagementListeners) {
        listener(agentPubkey, managementRequest);
      }
    }
    if (parsed.kind === "session_config_captured") {
      void putAgentSessionConfig(agentPubkey, parsed.payload);
      onSessionConfigCaptured?.(agentPubkey);
    } else if (parsed.kind === "managed_agent_runtime_lifecycle") {
      void putManagedAgentRuntimeLifecycle(agentPubkey, parsed.payload).catch(
        (error) => {
          console.debug("Late/untracked lifecycle frame dropped:", error);
        },
      );
    } else {
      applyCrewLiveFrameSideEffects(agentPubkey, parsed);
    }
  }

  if (observerChanged) {
    notifyListeners();
  }
  return true;
}

function processLiveObserverEvent(
  agentPubkey: string,
  parsed: ObserverEvent,
): boolean {
  return processLiveObserverEvents(agentPubkey, [parsed]);
}

export { processLiveObserverEvent as _testProcessLiveObserverEvent };

function processDecryptedObserverFrame(
  agentPubkey: string,
  parsed: ObserverEvent,
  context: RelayLiveEventContext,
  sourceEventId?: string,
) {
  const events = unwrapObserverBatch(parsed).map((inner) => ({
    ...inner,
    sourceEventId,
    replayed: context.replay,
  }));
  const accepted = processLiveObserverEvents(agentPubkey, events);
  if (!accepted) return false;
  setAgentConnectionError(agentPubkey, null);
  if (relayConnectionHealthy && observerSubscriptionReady) {
    setConnectionState("open", null);
  }
  return true;
}

export {
  processDecryptedObserverFrame as _testProcessDecryptedObserverFrame,
  setAgentConnectionError as _testSetAgentConnectionError,
};

async function handleRelayObserverEvent(
  event: RelayEvent,
  activeGeneration: number,
  context: RelayLiveEventContext = { replay: false },
) {
  const agentPubkey = observerTag(event, "agent");
  const frame = observerTag(event, "frame");
  if (!agentPubkey || frame == null) {
    logObserverDrop("missing_telemetry_tag", event, activeGeneration);
    return;
  }
  if (frame !== "telemetry") return;

  // Ownership data arrives asynchronously during startup. Buffer raw signed
  // frames until the first trusted-agent set is registered, then re-run this
  // same gate. Once initialized, unknown agents are rejected immediately.
  if (!knownAgentPubkeys.has(normalizePubkey(agentPubkey))) {
    if (knownAgentsBySubscription.size === 0 || knownAgentPubkeys.size === 0) {
      pendingUnknownAgentFrames.push({ event, context });
      if (pendingUnknownAgentFrames.length > MAX_PENDING_UNKNOWN_AGENT_FRAMES) {
        pendingUnknownAgentFrames.shift();
      }
    } else {
      logObserverDrop("unknown_agent", event, activeGeneration);
    }
    return;
  }

  // Defense-in-depth: verify the event sender matches the claimed agent pubkey.
  // The relay gates on is_agent_owner, but a compromised relay could misroute.
  if (normalizePubkey(event.pubkey) !== normalizePubkey(agentPubkey)) {
    logObserverDrop("sender_agent_mismatch", event, activeGeneration);
    return;
  }

  try {
    const parsed = (await decryptObserverEvent(event)) as ObserverEvent;
    if (activeGeneration !== generation) {
      logObserverDrop("stale_generation", event, activeGeneration);
      return;
    }
    processDecryptedObserverFrame(agentPubkey, parsed, context, event.id);
  } catch (error) {
    if (activeGeneration !== generation) {
      logObserverDrop("stale_generation", event, activeGeneration);
      return;
    }
    logObserverDrop("decrypt_failed", event, activeGeneration);
    setAgentConnectionError(
      agentPubkey,
      error instanceof Error
        ? `Observer event decrypt failed: ${error.message}`
        : "Observer event decrypt failed.",
    );
  }
}

export function ensureRelayObserverSubscription() {
  if (unsubscribeRelay) {
    return Promise.resolve();
  }
  if (startPromise) {
    return startPromise;
  }

  const activeGeneration = generation;
  setConnectionState("connecting", null);
  startPromise = (async () => {
    unsubscribeRelayState ??= relayClient.subscribeToConnectionState(
      (state) => {
        if (activeGeneration !== generation) return;
        relayConnectionHealthy = state === "connected";
        if (state === "connected") {
          if (observerSubscriptionReady) setConnectionState("open", null);
          return;
        }
        observerSubscriptionReady = false;
        if (state === "idle" || state === "connecting") {
          setConnectionState("connecting", null);
          return;
        }
        setConnectionState("error", `Observer relay is ${state}.`);
      },
    );
    const identity = await getIdentity();
    const unsubscribe = await subscribeToAgentObserverFrames(
      identity.pubkey,
      (event, context) => {
        eventProcessingQueue = eventProcessingQueue
          .then(() =>
            handleRelayObserverEvent(event, activeGeneration, context),
          )
          .catch((error) => {
            if (activeGeneration !== generation) {
              return;
            }
            setConnectionState(
              "error",
              error instanceof Error
                ? `Observer event handling failed: ${error.message}`
                : "Observer event handling failed.",
            );
          });
      },
      (status) => {
        if (activeGeneration !== generation) return;
        observerSubscriptionReady = status.state === "open";
        if (status.state === "open" && relayConnectionHealthy) {
          setConnectionState("open", null);
          return;
        }
        if (status.state !== "open") {
          setConnectionState("error", status.message);
        }
      },
    );
    if (activeGeneration !== generation) {
      await unsubscribe();
      return;
    }
    unsubscribeRelay = unsubscribe;
  })()
    .catch((error) => {
      if (activeGeneration === generation) {
        setConnectionState(
          "error",
          error instanceof Error
            ? error.message
            : "Observer relay subscription failed.",
        );
      }
    })
    .finally(() => {
      if (activeGeneration === generation) {
        startPromise = null;
      }
    });

  return startPromise;
}

export function subscribeAgentObserverStore(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Subscribe to agent-management request frames. Returns an unsubscribe
 * function.
 */
export function subscribeAgentManagementRequests(
  listener: (agentPubkey: string, request: AgentManagementRequest) => void,
) {
  agentManagementListeners.add(listener);
  return () => {
    agentManagementListeners.delete(listener);
  };
}

export function getAgentObserverSnapshot(
  agentPubkey?: string | null,
  // `_enabled` only gates the relay subscription in useObserverEvents.
  _enabled?: boolean,
): ObserverSnapshot {
  // Serve stored data when present — archived frames stay readable offline.
  if (!agentPubkey) {
    return IDLE_SNAPSHOT;
  }
  const key = normalizePubkey(agentPubkey);
  const agentError = connectionErrorByAgent.get(key) ?? null;
  const agentEvents = eventsByAgent.get(key) ?? [];
  // connecting only when restored events lack live contact; empty is idle open.
  const effectiveConnectionState =
    connectionState === "open" && agentError
      ? "error"
      : connectionState === "open" &&
          !agentsWithCurrentLiveContact.has(key) &&
          agentEvents.length > 0
        ? "connecting"
        : connectionState;
  const effectiveErrorMessage = agentError ?? errorMessage;
  const cached = snapshotByAgent.get(key);
  if (
    cached &&
    cached.connectionState === effectiveConnectionState &&
    cached.errorMessage === effectiveErrorMessage
  ) {
    return cached;
  }
  const snapshot: ObserverSnapshot = {
    connectionState: effectiveConnectionState,
    errorMessage: effectiveErrorMessage,
    events: agentEvents,
  };
  snapshotByAgent.set(key, snapshot);
  return snapshot;
}

export function getAgentTranscript(
  agentPubkey?: string | null,
  // `_enabled` previously gated store reads — now only gates the relay
  // subscription in useObserverEvents. Kept for call-site compatibility.
  _enabled?: boolean,
): TranscriptItem[] {
  // Same decoupling as getAgentObserverSnapshot: `_enabled` gates relay
  // subscription, not store reads. Archived items are in transcriptByAgent
  // and must be readable regardless of live status.
  if (!agentPubkey) {
    return EMPTY_TRANSCRIPT;
  }
  const key = normalizePubkey(agentPubkey);
  const state = transcriptByAgent.get(key);
  return state?.items ?? EMPTY_TRANSCRIPT;
}

export function shouldObserveManagedAgents(
  agents: readonly Pick<ManagedAgent, "pubkey">[],
): boolean {
  return agents.length > 0;
}

export function useManagedAgentObserverBridge(
  agents: readonly Pick<ManagedAgent, "pubkey" | "status">[],
) {
  const subscriptionId = React.useId();
  const hasManagedAgent = shouldObserveManagedAgents(agents);

  const agentPubkeys = React.useMemo(
    () => agents.map((agent) => agent.pubkey),
    [agents],
  );

  // Keep this subscriber's slice of the trusted-pubkey set in sync with its
  // own agent list. The store recomputes the union across all subscribers, so
  // a co-mounted caller no longer wipes out this caller's agents.
  React.useEffect(() => {
    registerKnownAgents(subscriptionId, agentPubkeys);
    return () => {
      unregisterKnownAgents(subscriptionId);
    };
  }, [subscriptionId, agentPubkeys]);

  React.useEffect(() => {
    if (!hasManagedAgent) {
      return;
    }
    void ensureRelayObserverSubscription();
  }, [hasManagedAgent]);

  // Wire up config-surface query invalidation when session_config_captured fires.
  const queryClient = useQueryClient();
  React.useEffect(() => {
    setSessionConfigCapturedCallback((pubkey) => {
      void queryClient.invalidateQueries({
        queryKey: agentConfigSurfaceQueryKey(pubkey),
      });
    });
    return () => setSessionConfigCapturedCallback(null);
  }, [queryClient]);
}

/**
 * Ingest a batch of raw archived observer events from the local archive into
 * the store. Applies the same security guards as the live relay path:
 *
 * - Event must have an `agent` tag pointing to a known/trusted pubkey
 *   (registered via `useManagedAgentObserverBridge`).
 * - The event sender (`pubkey`) must match the `agent` tag value.
 * - Event must decrypt successfully via `decryptObserverEvent`.
 *
 * Routes through `appendAgentEvent` so dedup on `(seq, timestamp)` and
 * sort are reused — archived events that are already present (live-delivered)
 * are silently skipped. Failed decryptions are silently dropped (same as
 * live path error handling).
 *
 * Note: events for agents not currently registered in `knownAgentPubkeys`
 * (e.g. an agent that is stopped but has archived history) are dropped.
 * The caller should ensure the agent is registered before calling.
 *
 * `_decryptFn` is only used by tests to inject a mock decryption function.
 * Production callers must always omit it.
 */
export async function ingestArchivedObserverEvents(
  rawEvents: RelayEvent[],
  _decryptFn: (event: RelayEvent) => Promise<unknown> = decryptObserverEvent,
): Promise<void> {
  let archiveChanged = false;
  for (const event of rawEvents) {
    const agentPubkey = observerTag(event, "agent");
    const frame = observerTag(event, "frame");
    if (!agentPubkey || frame == null) {
      logObserverDrop("missing_telemetry_tag", event, generation);
      continue;
    }
    if (frame !== "telemetry") continue;
    if (!knownAgentPubkeys.has(normalizePubkey(agentPubkey))) {
      if (knownAgentPubkeys.size > 0) {
        logObserverDrop("unknown_agent", event, generation);
      }
      continue;
    }
    if (normalizePubkey(event.pubkey) !== normalizePubkey(agentPubkey)) {
      logObserverDrop("sender_agent_mismatch", event, generation);
      continue;
    }
    try {
      const parsed = (await _decryptFn(event)) as ObserverEvent;
      for (const inner of unwrapObserverBatch(parsed)) {
        const archived = { ...inner, sourceEventId: event.id };
        // Route archived events to the channel-scoped archive window (no cap)
        // rather than the per-agent live-relay store (MAX_OBSERVER_EVENTS cap).
        // Events without a channelId fall through to the live store so they
        // remain visible in the agent's general transcript.
        if (archived.channelId) {
          const added = appendArchivedChannelEvent(
            agentPubkey,
            archived.channelId,
            archived,
          );
          if (added) archiveChanged = true;
        } else {
          // Live path already calls notifyListeners() inside appendAgentEvent.
          appendAgentEvent(agentPubkey, archived);
        }
      }
    } catch {
      logObserverDrop("decrypt_failed", event, generation);
    }
  }
  // Batch-notify once for the whole page of archive events. appendAgentEvent
  // already notifies individually for live/no-channelId events above, so we
  // only need one extra notify here for the archive path.
  if (archiveChanged) {
    notifyListeners();
  }
}

/**
 * E2E-only: inject synthetic observer events directly into the store, bypassing
 * the relay-security knownAgentPubkeys filter. Exercises the real
 * appendAgentEvent → processTranscriptEvent ingestion path so screenshot specs
 * prove the production render, not a stub.
 *
 * Never call this from production code — it is intentionally not re-exported
 * from the public agent feature barrel.
 */
export function injectObserverEventsForE2E(
  agentPubkey: string,
  events: ObserverEvent[],
) {
  setConnectionState("open", null);
  for (const event of events) {
    if (!event.replayed) {
      markAgentLiveContact(agentPubkey, { notify: false });
    }
  }
  if (appendAgentEvents(agentPubkey, events)) {
    applyCrewE2EInjectSideEffects(agentPubkey, events);
    notifyListeners();
  }
}

/** E2E-only: drive observer telemetry health through the production store. */
export function setObserverConnectionStateForE2E(state: ConnectionState) {
  setConnectionState(state, state === "error" ? "Mock observer error" : null);
  notifyListeners();
}

/**
 * Synchronize the observer store with a sorted buffer of events for one agent.
 * Used by test harnesses and replay bridges that already hold decoded frames.
 */
export function syncAgentObserverEvents(
  agentPubkey: string,
  events: ObserverEvent[],
) {
  if (appendAgentEvents(agentPubkey, events)) {
    notifyListeners();
  }
}

export function resetAgentObserverStore() {
  generation += 1;
  const unsubscribe = unsubscribeRelay;
  unsubscribeRelay = null;
  unsubscribeRelayState?.();
  unsubscribeRelayState = null;
  relayConnectionHealthy = false;
  observerSubscriptionReady = false;
  startPromise = null;
  eventProcessingQueue = Promise.resolve();
  eventsByAgent.clear();
  transcriptByAgent.clear();
  snapshotByAgent.clear();
  connectionErrorByAgent.clear();
  agentsWithCurrentLiveContact.clear();
  archiveEventsByChannel.clear();
  resetProjectThreadWorkspaceStore();
  resetDispatchedEventIdsStore();
  knownAgentPubkeys.clear();
  knownAgentsBySubscription.clear();
  pendingUnknownAgentFrames.length = 0;
  resetObserverDropLogger();
  resetLiveSessionAuthority();
  clearControlResultListeners();
  agentManagementListeners.clear();
  onSessionConfigCaptured = null;
  connectionState = "idle";
  errorMessage = null;
  notifyListeners();
  void unsubscribe?.();
}

/** E2E-only: remove live frames while retaining the hydrated archive journal. */
export function resetAgentObserverLiveEventsForE2E() {
  eventsByAgent.clear();
  transcriptByAgent.clear();
  snapshotByAgent.clear();
  connectionErrorByAgent.clear();
  agentsWithCurrentLiveContact.clear();
  notifyListeners();
}

/**
 * Test-only: register a set of agent pubkeys as trusted for a given
 * subscription id. Mirrors the effect of mounting `useManagedAgentObserverBridge`
 * in a React tree. Only call from tests — never from production code.
 */
export function _testRegisterKnownAgents(
  subscriptionId: string,
  pubkeys: readonly string[],
): void {
  registerKnownAgents(subscriptionId, pubkeys);
}

/** Test-only: exercise live envelope ordering without relay/decryption setup. */
export function _testProcessLiveObserverEvents(
  agentPubkey: string,
  events: readonly ObserverEvent[],
): void {
  processLiveObserverEvents(agentPubkey, events);
}

/**
 * Test-only: read the raw archived observer events for a (agent, channel) pair.
 * Production callers should use `getArchivedChannelEvents`.
 * Only call from tests — never from production code.
 */
export function _testGetArchivedChannelEvents(
  agentPubkey: string,
  channelId: string,
): ObserverEvent[] {
  return (
    archiveEventsByChannel.get(archiveChannelKey(agentPubkey, channelId)) ?? []
  );
}
