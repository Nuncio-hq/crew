import * as React from "react";

import { normalizePubkey } from "@/shared/lib/pubkey";
import type {
  ManagedAgent,
  ManagedAgentRuntimeStatus,
} from "@/shared/api/types";
import type { ObserverEvent } from "../ui/agentSessionTypes";
import { canonicalRelayUrl } from "../managedAgentRuntimeStatus";

type Membership = {
  generation: string;
  startedAt: number;
  seq: number;
  count: number | null;
};
type MembershipRuntime = Pick<
  ManagedAgentRuntimeStatus,
  "pubkey" | "relayUrl" | "startNonce" | "transport" | "transportRetired"
>;
type AgentMembershipProjection = {
  byGeneration: Map<string, Membership>;
  generationOrder: string[];
  retiredGenerations: Set<string>;
  /** Generation selected by the latest connected native runtime row. */
  activeGeneration?: string;
  /** First unbound generation seen since the native row was last observed. */
  pendingGeneration?: string;
  /** Canonical relay paired with the native runtime that selected this view. */
  relayUrl?: string;
};
const MAX_GENERATIONS_PER_AGENT = 128;
const MAX_RETIRED_GENERATIONS_PER_AGENT = 128;
/** Current projection state; absence and an explicit null count are unknown. */
export type ChannelMembershipState = "unknown" | "zero" | "nonzero";
const membershipByAgent = new Map<string, AgentMembershipProjection>();
const listeners = new Set<() => void>();
const notify = () => {
  for (const listener of listeners) listener();
};

function stateForCount(
  count: number | null | undefined,
): ChannelMembershipState {
  if (count === null || count === undefined) return "unknown";
  return count === 0 ? "zero" : "nonzero";
}

function stateForMembership(
  membership: Membership | undefined,
): ChannelMembershipState {
  return stateForCount(membership?.count);
}

function ensureProjection(agentPubkey: string): AgentMembershipProjection {
  const key = normalizePubkey(agentPubkey);
  const existing = membershipByAgent.get(key);
  if (existing) return existing;
  const projection: AgentMembershipProjection = {
    byGeneration: new Map(),
    generationOrder: [],
    retiredGenerations: new Set(),
  };
  membershipByAgent.set(key, projection);
  return projection;
}

function removeGeneration(
  projection: AgentMembershipProjection,
  generation: string,
): void {
  projection.byGeneration.delete(generation);
  const index = projection.generationOrder.indexOf(generation);
  if (index >= 0) projection.generationOrder.splice(index, 1);
}

function retireGeneration(
  projection: AgentMembershipProjection,
  generation: string,
): void {
  if (projection.activeGeneration === generation) {
    projection.activeGeneration = undefined;
  }
  if (projection.pendingGeneration === generation) {
    projection.pendingGeneration = undefined;
  }
  removeGeneration(projection, generation);
  projection.retiredGenerations.add(generation);
  while (
    projection.retiredGenerations.size > MAX_RETIRED_GENERATIONS_PER_AGENT
  ) {
    const oldest = projection.retiredGenerations.values().next().value;
    if (oldest === undefined) break;
    projection.retiredGenerations.delete(oldest);
  }
}

function trimProjection(projection: AgentMembershipProjection): void {
  while (projection.generationOrder.length > MAX_GENERATIONS_PER_AGENT) {
    const index = projection.generationOrder.findIndex(
      (generation) =>
        generation !== projection.activeGeneration &&
        generation !== projection.pendingGeneration,
    );
    if (index < 0) break;
    const [evicted] = projection.generationOrder.splice(index, 1);
    if (evicted === undefined) break;
    projection.byGeneration.delete(evicted);
    projection.retiredGenerations.add(evicted);
  }
  while (
    projection.retiredGenerations.size > MAX_RETIRED_GENERATIONS_PER_AGENT
  ) {
    const oldest = projection.retiredGenerations.values().next().value;
    if (oldest === undefined) break;
    projection.retiredGenerations.delete(oldest);
  }
}

/**
 * Feed the native runtime row into the projection after a badge render commits.
 *
 * The runtime's opaque start nonce is the only authority that can promote a
 * frame generation to current. A frame can arrive before this row; the
 * projection holds the first unbound candidate in `pendingGeneration` until
 * native status confirms or retires it. This keeps retention independent of
 * producer clocks and protects the current row from late generations.
 */
export function observeChannelMembershipRuntime(
  agentPubkey: string | null | undefined,
  runtime: MembershipRuntime | null | undefined,
): void {
  if (!agentPubkey?.trim() || !runtime?.pubkey) return;
  if (normalizePubkey(runtime.pubkey) !== normalizePubkey(agentPubkey)) return;
  const projection = ensureProjection(agentPubkey);
  const relayUrl = canonicalRelayUrl(runtime.relayUrl);
  const previousActive = projection.activeGeneration;
  const previousPending = projection.pendingGeneration;
  const previousRelay = projection.relayUrl;

  // A malformed relay cannot establish a native scope. Retired transport is
  // a durable fence for the supplied nonce, so remove that exact generation.
  if (!relayUrl || runtime.transportRetired === true) {
    if (runtime.startNonce && runtime.transportRetired === true) {
      retireGeneration(projection, runtime.startNonce);
    }
    if (runtime.transportRetired === true) {
      projection.activeGeneration = undefined;
    }
    if (
      previousActive !== projection.activeGeneration ||
      previousPending !== projection.pendingGeneration ||
      previousRelay !== projection.relayUrl
    ) {
      notify();
    }
    return;
  }
  projection.relayUrl ??= relayUrl;
  if (projection.relayUrl !== relayUrl) return;

  const startNonce = runtime.startNonce;
  if (!startNonce) return;
  if (runtime.transport?.state !== "connected") return;

  // Native status wins over a recycled replay tombstone: if it names this
  // generation, a later frame for it is allowed to rebuild the row.
  projection.retiredGenerations.delete(startNonce);

  if (projection.activeGeneration !== startNonce) {
    const previousActive = projection.activeGeneration;
    if (previousActive) retireGeneration(projection, previousActive);
    projection.activeGeneration = startNonce;
  }
  // A frame may have arrived before the matching status query. Promote that
  // candidate when native status names it, while leaving any other candidate
  // intact until a later native transition proves it stale. Query rerenders
  // can repeat an older status, and must never delete a newer frame.
  if (projection.pendingGeneration === startNonce) {
    projection.pendingGeneration = undefined;
  }
  trimProjection(projection);
  if (
    previousActive !== projection.activeGeneration ||
    previousPending !== projection.pendingGeneration ||
    previousRelay !== projection.relayUrl
  ) {
    notify();
  }
}

/** Project the harness subscription snapshot, independently of conversation sessions. */
export function applyChannelMembershipObserverFrame(
  agentPubkey: string,
  frame: Pick<ObserverEvent, "kind" | "payload" | "seq">,
): void {
  if (frame.kind !== "channel_membership" || !agentPubkey.trim()) return;
  const payload = frame.payload;
  if (!payload || typeof payload !== "object") return;
  const {
    channel_count: count,
    generation,
    generation_started_at: started,
  } = payload as Record<string, unknown>;
  if (
    (count !== null &&
      (typeof count !== "number" ||
        !Number.isSafeInteger(count) ||
        count < 0)) ||
    typeof generation !== "string" ||
    !generation.trim() ||
    typeof started !== "string" ||
    !Number.isSafeInteger(frame.seq) ||
    frame.seq < 0
  )
    return;
  const normalizedCount = count as number | null;
  const startedAt = Date.parse(started);
  if (!Number.isFinite(startedAt)) return;
  const projection = ensureProjection(agentPubkey);
  const previous = projection.byGeneration.get(generation);
  if (
    previous &&
    (previous.startedAt !== startedAt || frame.seq <= previous.seq)
  )
    return;
  if (!previous) {
    if (projection.retiredGenerations.has(generation)) return;
    const next = {
      generation,
      startedAt,
      seq: frame.seq,
      count: normalizedCount,
    } satisfies Membership;
    projection.generationOrder.push(generation);
    projection.byGeneration.set(generation, next);
    // Until native status identifies the live harness, retain the first
    // unseen generation as a candidate. This is deliberately arrival based;
    // producer timestamps are metadata and cannot rank active processes.
    if (
      projection.activeGeneration !== generation &&
      projection.pendingGeneration === undefined
    ) {
      projection.pendingGeneration = generation;
    }
    trimProjection(projection);
  } else {
    projection.byGeneration.set(generation, {
      generation,
      startedAt,
      seq: frame.seq,
      count: normalizedCount,
    });
  }
  const retained = projection.byGeneration.get(generation);
  if (
    retained &&
    (!previous ||
      stateForMembership(previous) !== stateForCount(normalizedCount))
  ) {
    // A new runtime generation must publish even when it carries the same
    // count: a reader may have been unknown until its matching status arrived.
    notify();
  }
}

export function getChannelMembershipState(
  agentPubkey: string | null | undefined,
  runtime: MembershipRuntime | null | undefined,
): ChannelMembershipState {
  if (!agentPubkey?.trim()) return "unknown";
  if (
    !runtime?.startNonce ||
    !runtime.pubkey ||
    !canonicalRelayUrl(runtime.relayUrl) ||
    normalizePubkey(runtime.pubkey) !== normalizePubkey(agentPubkey) ||
    runtime.transport?.state !== "connected" ||
    runtime.transportRetired === true
  ) {
    return "unknown";
  }
  const projection = membershipByAgent.get(normalizePubkey(agentPubkey));
  const runtimeRelayUrl = canonicalRelayUrl(runtime.relayUrl);
  if (projection?.relayUrl && projection.relayUrl !== runtimeRelayUrl) {
    return "unknown";
  }
  if (
    projection?.activeGeneration &&
    projection.activeGeneration !== runtime.startNonce
  ) {
    return "unknown";
  }
  const membership = projection?.byGeneration.get(runtime.startNonce);
  if (!membership) {
    return "unknown";
  }
  return stateForMembership(membership);
}

export function hasNoChannelMembership(
  agentPubkey: string | null | undefined,
  runtime: MembershipRuntime | null | undefined,
): boolean {
  return (
    !!agentPubkey && getChannelMembershipState(agentPubkey, runtime) === "zero"
  );
}

export function subscribeChannelMembershipState(
  listener: () => void,
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useChannelMembershipState(
  agentPubkey: string | null | undefined,
  runtime: MembershipRuntime | null | undefined,
): ChannelMembershipState {
  // Register after commit so an aborted render cannot retire a newer native
  // generation. The snapshot reader remains pure; an observer frame that
  // arrives before this effect is retained as the pending candidate.
  React.useLayoutEffect(() => {
    observeChannelMembershipRuntime(agentPubkey, runtime);
  }, [agentPubkey, runtime]);
  return React.useSyncExternalStore(subscribeChannelMembershipState, () =>
    getChannelMembershipState(agentPubkey, runtime),
  );
}

/** Community boundaries discard both projection and harness generation authority. */
export function resetChannelMembershipState(): void {
  membershipByAgent.clear();
  notify();
}

export function deriveNoChannelMembershipBadge(
  signal: boolean,
  status: ManagedAgent["status"],
): boolean {
  return signal && (status === "running" || status === "deployed");
}

export function useNoChannelMembership(
  agentPubkey: string | null | undefined,
  runtime: MembershipRuntime | null | undefined,
): boolean {
  return useChannelMembershipState(agentPubkey, runtime) === "zero";
}
