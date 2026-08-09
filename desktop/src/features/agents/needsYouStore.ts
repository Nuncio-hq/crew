import * as React from "react";

import { deriveAgentConversationId } from "@/features/agents/conversationId";
import { getThreadReference } from "@/features/messages/lib/threading";
import type { RelayEvent } from "@/shared/api/types";
import {
  KIND_APPROVAL_DENY,
  KIND_APPROVAL_GRANT,
  KIND_APPROVAL_REQUEST,
} from "@/shared/constants/kinds";

export const NEEDS_YOU_TTL_MS = 30 * 60 * 1_000;

export type NeedsYouRequest = {
  id: string;
  channelId: string;
  rootEventId: string;
  conversationId: string;
  agentPubkey: string;
  createdAt: number;
  approvalReferences: string[];
  /** Present for durable 46040 requests; omitted for workflow approvals. */
  ownerPubkey?: string;
};

// Entries in `requests` are durable workflow-human approvals (kind 46010),
// not ACP tool permission prompts. Tool permissions stay on the established
// permission/bypass path and never enter Agent Attention's `Needs you` state.

type UserInputNeedsYouRequest = NeedsYouRequest & {
  kind: "user-input";
  ownerPubkey: string;
};

const requests = new Map<string, NeedsYouRequest>();
const pendingApprovalResolutions = new Map<string, RelayEvent>();
const MAX_PENDING_APPROVAL_RESOLUTIONS = 1_000;
const MAX_PENDING_APPROVAL_SETTLEMENT_PASSES = 4;
let approvalProjectionUnavailable = false;
let nextApprovalProjectionGeneration = 0;
let approvalStoreEpoch = 0;
let activeApprovalProjection: {
  generation: number;
  requests: Map<string, NeedsYouRequest>;
  pendingResolutions: Map<string, RelayEvent>;
  resolvedRequestIds: Set<string>;
  overflowed: boolean;
} | null = null;
const userInputRequests = new Map<string, UserInputNeedsYouRequest>();
const resolvedUserInputRequestIds = new Set<string>();
const listeners = new Set<() => void>();
let generation = 0;
const channelCache = new Map<string, NeedsYouRequest[]>();
const conversationCache = new Map<string, NeedsYouRequest[]>();
const channelsCache = new Map<string, NeedsYouRequest[]>();
const EMPTY_REQUESTS: NeedsYouRequest[] = [];
let allCache: NeedsYouRequest[] | null = null;
let allCacheGeneration = -1;
let expiryTimer: ReturnType<typeof globalThis.setTimeout> | null = null;

export function beginExhaustiveApprovalProjection(): number {
  nextApprovalProjectionGeneration += 1;
  const projectionGeneration = nextApprovalProjectionGeneration;
  approvalProjectionUnavailable = true;
  activeApprovalProjection = {
    generation: projectionGeneration,
    requests: new Map(),
    pendingResolutions: new Map(),
    resolvedRequestIds: new Set(),
    overflowed: false,
  };
  notify();
  return projectionGeneration;
}

export function isExhaustiveApprovalProjectionCurrent(
  projectionGeneration: number,
): boolean {
  return activeApprovalProjection?.generation === projectionGeneration;
}

export function endExhaustiveApprovalProjection(
  projectionGeneration: number,
  success: boolean,
): boolean {
  const projection = activeApprovalProjection;
  if (!projection || projection.generation !== projectionGeneration)
    return false;
  const projectionReady = success && !projection.overflowed;
  activeApprovalProjection = null;
  approvalProjectionUnavailable = !projectionReady;
  if (projectionReady) {
    requests.clear();
    for (const [id, request] of projection.requests) requests.set(id, request);
    pendingApprovalResolutions.clear();
    for (const [id, event] of projection.pendingResolutions)
      pendingApprovalResolutions.set(id, event);
    resolvedRequestIds.clear();
    for (const id of projection.resolvedRequestIds) rememberResolved(id);
    scheduleExpiry();
  }
  notify();
  return projectionReady;
}

function notify() {
  generation += 1;
  channelCache.clear();
  conversationCache.clear();
  channelsCache.clear();
  allCache = null;
  allCacheGeneration = -1;
  for (const listener of listeners) listener();
}

function approvalState(projectionGeneration?: number) {
  if (activeApprovalProjection) {
    if (
      projectionGeneration !== undefined &&
      projectionGeneration !== activeApprovalProjection.generation
    )
      return null;
    return {
      requests: activeApprovalProjection.requests,
      pendingResolutions: activeApprovalProjection.pendingResolutions,
      resolvedRequestIds: activeApprovalProjection.resolvedRequestIds,
      staged: true,
    };
  }
  if (projectionGeneration !== undefined) return null;
  return {
    requests,
    pendingResolutions: pendingApprovalResolutions,
    resolvedRequestIds,
    staged: false,
  };
}

export function ingestUserInputRequest(
  input: Omit<
    UserInputNeedsYouRequest,
    "kind" | "approvalReferences" | "ownerPubkey"
  > & { ownerPubkey?: string },
) {
  if (resolvedUserInputRequestIds.has(input.id)) return null;
  const entry: UserInputNeedsYouRequest = {
    ...input,
    ownerPubkey: input.ownerPubkey?.trim().toLowerCase() ?? "",
    kind: "user-input",
    approvalReferences: [],
  };
  const prior = userInputRequests.get(entry.id);
  userInputRequests.set(entry.id, entry);
  scheduleExpiry();
  if (!prior || JSON.stringify(prior) !== JSON.stringify(entry)) notify();
  return entry;
}

export function reconcileUserInputRequestAuthority(
  currentPubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
): boolean {
  const current = currentPubkey.trim().toLowerCase();
  let changed = false;
  for (const [id, request] of userInputRequests) {
    if (
      request.ownerPubkey !== current ||
      !ownedAgentPubkeys.has(request.agentPubkey.trim().toLowerCase())
    ) {
      userInputRequests.delete(id);
      changed = true;
    }
  }
  if (changed) notify();
  return changed;
}

export function resolveUserInputRequest(requestId: string) {
  resolvedUserInputRequestIds.add(requestId);
  // Same LRU bound as approval tombstones: unbounded growth is a leak in
  // long-lived sessions with chatty agents.
  if (resolvedUserInputRequestIds.size > RESOLVED_TOMBSTONE_LIMIT) {
    const oldest = resolvedUserInputRequestIds.values().next().value;
    if (oldest !== undefined) resolvedUserInputRequestIds.delete(oldest);
  }
  if (!userInputRequests.delete(requestId)) return false;
  scheduleExpiry();
  notify();
  return true;
}

export function getPendingUserInputRequest(
  requestId: string,
): NeedsYouRequest | null {
  return userInputRequests.get(requestId) ?? null;
}

function prune(now: number): boolean {
  let changed = false;
  for (const [id, request] of requests) {
    if (now - request.createdAt >= NEEDS_YOU_TTL_MS) {
      requests.delete(id);
      changed = true;
    }
  }
  return changed;
}

function scheduleExpiry() {
  if (expiryTimer !== null) {
    globalThis.clearTimeout(expiryTimer);
    expiryTimer = null;
  }
  const nextExpiry = Math.min(
    ...[...requests.values()].map(
      (request) => request.createdAt + NEEDS_YOU_TTL_MS,
    ),
  );
  if (!Number.isFinite(nextExpiry)) return;
  expiryTimer = globalThis.setTimeout(
    () => {
      expiryTimer = null;
      if (prune(Date.now())) notify();
      scheduleExpiry();
    },
    Math.max(0, nextExpiry - Date.now()),
  );
}

export function ingestApprovalRequest(
  input: Omit<NeedsYouRequest, "conversationId" | "approvalReferences"> & {
    approvalReferences?: string[];
    conversationId?: string;
  },
  projectionGeneration?: number,
) {
  const state = approvalState(projectionGeneration);
  if (!state) return null;
  // A feed page fetched before a live grant landed must not resurrect an
  // already-resolved request.
  if (state.resolvedRequestIds.has(input.id)) return null;
  const conversationId =
    input.conversationId ??
    deriveAgentConversationId(input.channelId, input.rootEventId);
  const entry = {
    ...input,
    conversationId,
    approvalReferences: input.approvalReferences ?? [],
  };
  const prior = state.requests.get(entry.id);
  state.requests.set(entry.id, entry);
  if (!state.staged) {
    scheduleExpiry();
    if (!prior || JSON.stringify(prior) !== JSON.stringify(entry)) notify();
  }
  for (const resolution of state.pendingResolutions.values()) {
    void resolveApprovalRequestEvent(resolution, projectionGeneration);
  }
  return entry;
}

function requestFields(
  id: string,
  channelId: string,
  tags: string[][],
  agentPubkey: string,
  createdAt: number,
  projectionGeneration?: number,
) {
  const thread = getThreadReference(tags);
  // Approval requests often carry only an unmarked `e` tag (or none at all):
  // fall back to the last `e` tag, then to the request event id, so root-only
  // requests still ingest. Scoped here on purpose — the NIP-10 reply-marker
  // semantics of getThreadReference stay untouched for message threading.
  const fallbackEventTag = [...tags]
    .reverse()
    .find((tag) => tag[0] === "e" && tag[1])?.[1];
  const rootEventId =
    thread.rootId ?? thread.parentId ?? fallbackEventTag ?? id;
  const approvalReferences = tags
    .filter((tag) => ["d", "e", "t"].includes(tag[0]) && tag[1])
    .map((tag) => tag[1]);
  try {
    return ingestApprovalRequest(
      {
        id,
        channelId,
        rootEventId,
        agentPubkey,
        createdAt,
        approvalReferences,
      },
      projectionGeneration,
    );
  } catch {
    return null;
  }
}

export function ingestApprovalRequestEvent(
  event: RelayEvent,
  projectionGeneration?: number,
) {
  if (event.kind !== KIND_APPROVAL_REQUEST) return null;
  if (!approvalState(projectionGeneration)) return null;
  const channelId = event.tags.find((tag) => tag[0] === "h")?.[1];
  return channelId
    ? requestFields(
        event.id,
        channelId,
        event.tags,
        event.pubkey,
        event.created_at * 1_000,
        projectionGeneration,
      )
    : null;
}

export function resolveApprovalRequest(
  requestId: string,
  projectionGeneration?: number,
) {
  const state = approvalState(projectionGeneration);
  if (!state) return false;
  rememberResolved(requestId, state.resolvedRequestIds);
  if (!state.requests.delete(requestId)) return false;
  if (!state.staged) {
    scheduleExpiry();
    notify();
  }
  return true;
}

function findRequestByReferences(
  references: string[],
  projectionGeneration?: number,
) {
  const state = approvalState(projectionGeneration);
  if (!state) return null;
  return [...state.requests.values()].find(
    (candidate) =>
      references.includes(candidate.id) ||
      references.some((reference) =>
        candidate.approvalReferences.includes(reference),
      ),
  );
}

async function sha256Hex(input: string): Promise<string | null> {
  try {
    const digest = await globalThis.crypto.subtle.digest(
      "SHA-256",
      new TextEncoder().encode(input),
    );
    return [...new Uint8Array(digest)]
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");
  } catch {
    return null;
  }
}

export async function resolveApprovalRequestEvent(
  event: RelayEvent,
  projectionGeneration?: number,
) {
  if (event.kind !== KIND_APPROVAL_GRANT && event.kind !== KIND_APPROVAL_DENY) {
    return false;
  }
  const references = event.tags
    .filter((tag) => ["d", "e", "t"].includes(tag[0]) && tag[1])
    .map((tag) => tag[1]);
  const operationGeneration =
    projectionGeneration ?? activeApprovalProjection?.generation;
  const operationStoreEpoch = approvalStoreEpoch;
  const operationStartedGlobally = operationGeneration === undefined;
  const operationState = () =>
    operationStoreEpoch !== approvalStoreEpoch
      ? null
      : operationStartedGlobally
        ? activeApprovalProjection === null
          ? approvalState()
          : null
        : approvalState(operationGeneration);
  let state = operationState();
  if (!state) return false;
  const direct = findRequestByReferences(references, operationGeneration);
  if (direct) {
    state.pendingResolutions.delete(event.id);
    return resolveApprovalRequest(direct.id, operationGeneration);
  }
  // Desktop grants/denies carry the RAW approval token in a `t` tag
  // (src-tauri events.rs build_approval_grant), while request references
  // use sha256(token) (buzz-sdk build_workflow_approval d-tag contract).
  // Hash each reference and retry so raw-token resolutions still clear.
  const hashed = (await Promise.all(references.map(sha256Hex))).filter(
    (reference): reference is string => reference !== null,
  );
  state = operationState();
  if (!state) return false;
  const viaHash = findRequestByReferences(hashed, operationGeneration);
  if (viaHash) {
    state.pendingResolutions.delete(event.id);
    return resolveApprovalRequest(viaHash.id, operationGeneration);
  }
  state.pendingResolutions.set(event.id, event);
  if (state.pendingResolutions.size > MAX_PENDING_APPROVAL_RESOLUTIONS) {
    if (activeApprovalProjection && state.staged)
      activeApprovalProjection.overflowed = true;
    approvalProjectionUnavailable = true;
    state.pendingResolutions.clear();
    state.requests.clear();
    notify();
  }
  return false;
}

export async function settlePendingApprovalResolutions(
  projectionGeneration: number,
): Promise<boolean> {
  const processed = new Set<string>();
  let passes = 0;
  while (true) {
    const state = approvalState(projectionGeneration);
    if (!state?.staged) return false;
    const pending = [...state.pendingResolutions.values()].filter(
      (event) => !processed.has(event.id),
    );
    if (pending.length === 0) {
      // Drain validation continuations already queued by live events. A later
      // relay task runs after this hydration commits and therefore targets the
      // committed projection instead of the staging generation.
      await Promise.resolve();
      const current = approvalState(projectionGeneration);
      if (!current?.staged) return false;
      if (
        [...current.pendingResolutions.keys()].every((id) => processed.has(id))
      )
        return true;
      continue;
    }
    passes += 1;
    if (passes > MAX_PENDING_APPROVAL_SETTLEMENT_PASSES) {
      if (activeApprovalProjection?.generation === projectionGeneration)
        activeApprovalProjection.overflowed = true;
      return false;
    }
    for (const event of pending) processed.add(event.id);
    await Promise.all(
      pending.map((event) =>
        resolveApprovalRequestEvent(event, projectionGeneration),
      ),
    );
  }
}

// Tombstones prevent delayed verified request replay from resurrecting an
// approval already resolved by a verified terminal event.
const resolvedRequestIds = new Set<string>();
const RESOLVED_TOMBSTONE_LIMIT = 512;

function rememberResolved(
  requestId: string,
  tombstones: Set<string> = resolvedRequestIds,
) {
  tombstones.add(requestId);
  if (tombstones.size > RESOLVED_TOMBSTONE_LIMIT) {
    const oldest = tombstones.values().next().value;
    if (oldest !== undefined) tombstones.delete(oldest);
  }
}

export function getNeedsYouForConversation(
  conversationId: string | null | undefined,
  now = Date.now(),
): NeedsYouRequest[] {
  if (!conversationId) return EMPTY_REQUESTS;
  if (prune(now)) {
    channelCache.clear();
    conversationCache.clear();
    channelsCache.clear();
    allCache = null;
    allCacheGeneration = -1;
    scheduleExpiry();
  }
  const cached = conversationCache.get(conversationId);
  if (cached) return cached;
  const approvalRequests = approvalProjectionUnavailable
    ? []
    : requests.values();
  const result = [...approvalRequests, ...userInputRequests.values()]
    .filter((request) => request.conversationId === conversationId)
    .sort((a, b) => a.createdAt - b.createdAt);
  conversationCache.set(conversationId, result);
  return result;
}

export function getNeedsYouForChannel(
  channelId: string | null | undefined,
  now = Date.now(),
): NeedsYouRequest[] {
  if (!channelId) return EMPTY_REQUESTS;
  if (prune(now)) {
    channelCache.clear();
    conversationCache.clear();
    channelsCache.clear();
    allCache = null;
    allCacheGeneration = -1;
    scheduleExpiry();
  }
  const cached = channelCache.get(channelId);
  if (cached) return cached;
  const approvalRequests = approvalProjectionUnavailable
    ? []
    : requests.values();
  const result = [...approvalRequests, ...userInputRequests.values()]
    .filter((request) => request.channelId === channelId)
    .sort((a, b) => a.createdAt - b.createdAt);
  channelCache.set(channelId, result);
  return result;
}

/** Return every pending request as one reference-stable snapshot. */
export function getNeedsYouForAll(now = Date.now()): NeedsYouRequest[] {
  if (prune(now)) {
    channelCache.clear();
    conversationCache.clear();
    allCache = null;
    allCacheGeneration = -1;
    scheduleExpiry();
  }
  if (allCache && allCacheGeneration === generation) return allCache;
  const approvalRequests = approvalProjectionUnavailable
    ? []
    : requests.values();
  allCache = [...approvalRequests, ...userInputRequests.values()].sort(
    (a, b) => a.createdAt - b.createdAt,
  );
  allCacheGeneration = generation;
  return allCache;
}

export function subscribeNeedsYou(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getNeedsYouGeneration() {
  return generation;
}

export function clearUserInputRequests(channelId?: string): void {
  let changed = false;
  for (const [id, request] of userInputRequests) {
    if (channelId !== undefined && request.channelId !== channelId) continue;
    userInputRequests.delete(id);
    changed = true;
  }
  if (changed) {
    scheduleExpiry();
    notify();
  }
}

export function resetNeedsYouStore() {
  approvalStoreEpoch += 1;
  approvalProjectionUnavailable = false;
  activeApprovalProjection = null;
  requests.clear();
  pendingApprovalResolutions.clear();
  userInputRequests.clear();
  resolvedRequestIds.clear();
  resolvedUserInputRequestIds.clear();
  if (expiryTimer !== null) {
    globalThis.clearTimeout(expiryTimer);
    expiryTimer = null;
  }
  channelCache.clear();
  conversationCache.clear();
  channelsCache.clear();
  allCache = null;
  allCacheGeneration = -1;
  generation += 1;
  for (const listener of listeners) listener();
}

export function useNeedsYouForConversation(
  conversationId: string | null | undefined,
) {
  const getSnapshot = React.useCallback(
    () => getNeedsYouForConversation(conversationId),
    [conversationId],
  );
  return React.useSyncExternalStore(
    subscribeNeedsYou,
    getSnapshot,
    getSnapshot,
  );
}

export function useNeedsYouForChannel(channelId: string | null | undefined) {
  const getSnapshot = React.useCallback(
    () => getNeedsYouForChannel(channelId),
    [channelId],
  );
  return React.useSyncExternalStore(
    subscribeNeedsYou,
    getSnapshot,
    getSnapshot,
  );
}

export function getNeedsYouForChannels(
  channelIds: readonly string[],
): NeedsYouRequest[] {
  const key = channelIds.join("\u0000");
  const cached = channelsCache.get(key);
  if (cached) return cached;
  const result = [
    ...new Map(
      channelIds.flatMap((channelId) =>
        getNeedsYouForChannel(channelId).map((request) => [
          request.id,
          request,
        ]),
      ),
    ).values(),
  ].sort((a, b) => a.createdAt - b.createdAt);
  channelsCache.set(key, result);
  return result;
}

export function useNeedsYouForChannels(channelIds: readonly string[]) {
  const key = channelIds.join("\u0000");
  const getSnapshot = React.useCallback(
    () => getNeedsYouForChannels(key ? key.split("\u0000") : []),
    [key],
  );
  return React.useSyncExternalStore(
    subscribeNeedsYou,
    getSnapshot,
    getSnapshot,
  );
}
