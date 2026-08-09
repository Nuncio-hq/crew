import * as React from "react";

import { deriveAgentConversationId } from "@/features/agents/conversationId";
import { getThreadReference } from "@/features/messages/lib/threading";
import type { FeedItem, RelayEvent } from "@/shared/api/types";
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
};

// Entries in `requests` are durable workflow-human approvals (kind 46010),
// not ACP tool permission prompts. Tool permissions stay on the established
// permission/bypass path and never enter Agent Attention's `Needs you` state.

type UserInputNeedsYouRequest = NeedsYouRequest & {
  kind: "user-input";
  ownerPubkey: string;
};

const requests = new Map<string, NeedsYouRequest>();
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

function notify() {
  generation += 1;
  channelCache.clear();
  conversationCache.clear();
  channelsCache.clear();
  allCache = null;
  allCacheGeneration = -1;
  for (const listener of listeners) listener();
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
) {
  // A feed page fetched before a live grant landed must not resurrect an
  // already-resolved request.
  if (resolvedRequestIds.has(input.id)) return null;
  const conversationId =
    input.conversationId ??
    deriveAgentConversationId(input.channelId, input.rootEventId);
  const entry = {
    ...input,
    conversationId,
    approvalReferences: input.approvalReferences ?? [],
  };
  const prior = requests.get(entry.id);
  requests.set(entry.id, entry);
  scheduleExpiry();
  if (!prior || JSON.stringify(prior) !== JSON.stringify(entry)) notify();
  return entry;
}

function requestFields(
  id: string,
  channelId: string,
  tags: string[][],
  agentPubkey: string,
  createdAt: number,
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
    return ingestApprovalRequest({
      id,
      channelId,
      rootEventId,
      agentPubkey,
      createdAt,
      approvalReferences,
    });
  } catch {
    return null;
  }
}

export function ingestApprovalRequestEvent(event: RelayEvent) {
  if (event.kind !== KIND_APPROVAL_REQUEST) return null;
  const channelId = event.tags.find((tag) => tag[0] === "h")?.[1];
  return channelId
    ? requestFields(
        event.id,
        channelId,
        event.tags,
        event.pubkey,
        event.created_at * 1_000,
      )
    : null;
}

export function ingestApprovalRequestFeedItem(item: FeedItem) {
  if (item.kind !== KIND_APPROVAL_REQUEST || !item.channelId) return null;
  return requestFields(
    item.id,
    item.channelId,
    item.tags,
    item.pubkey,
    item.createdAt * 1_000,
  );
}

export function resolveApprovalRequest(requestId: string) {
  rememberResolved(requestId);
  if (!requests.delete(requestId)) return false;
  scheduleExpiry();
  notify();
  return true;
}

function findRequestByReferences(references: string[]) {
  return [...requests.values()].find(
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

export async function resolveApprovalRequestEvent(event: RelayEvent) {
  if (event.kind !== KIND_APPROVAL_GRANT && event.kind !== KIND_APPROVAL_DENY) {
    return false;
  }
  const references = event.tags
    .filter((tag) => ["d", "e", "t"].includes(tag[0]) && tag[1])
    .map((tag) => tag[1]);
  const direct = findRequestByReferences(references);
  if (direct) return resolveApprovalRequest(direct.id);
  // Desktop grants/denies carry the RAW approval token in a `t` tag
  // (src-tauri events.rs build_approval_grant), while request references
  // use sha256(token) (buzz-sdk build_workflow_approval d-tag contract).
  // Hash each reference and retry so raw-token resolutions still clear.
  const hashed = (await Promise.all(references.map(sha256Hex))).filter(
    (reference): reference is string => reference !== null,
  );
  const viaHash = findRequestByReferences(hashed);
  return viaHash ? resolveApprovalRequest(viaHash.id) : false;
}

// Hydration reconcile: `needs_action` is the relay's authoritative pending
// set (kind 46010, buzz-db feed.rs). Requests resolved or expired while the
// app was closed never emit a live grant we can observe, so on each feed
// fetch drop store entries that the fresh snapshot no longer contains.
// A grace window protects requests ingested live moments before the fetch.
// When the page is full (length >= limit) the snapshot may be PARTIAL —
// deletions are skipped then, because absence proves nothing.
const RECONCILE_GRACE_MS = 60_000;

// Tombstones for live-resolved ids: a stale feed page fetched before the
// grant landed must not re-add a request the user already resolved.
const resolvedRequestIds = new Set<string>();
const RESOLVED_TOMBSTONE_LIMIT = 512;

function rememberResolved(requestId: string) {
  resolvedRequestIds.add(requestId);
  if (resolvedRequestIds.size > RESOLVED_TOMBSTONE_LIMIT) {
    const oldest = resolvedRequestIds.values().next().value;
    if (oldest !== undefined) resolvedRequestIds.delete(oldest);
  }
}

export function reconcileNeedsYouFromFeed(
  items: FeedItem[],
  fetchedAt = Date.now(),
  { snapshotComplete = true }: { snapshotComplete?: boolean } = {},
) {
  if (!snapshotComplete) return false;
  const present = new Set(
    items
      .filter((item) => item.kind === KIND_APPROVAL_REQUEST)
      .map((item) => item.id),
  );
  let changed = false;
  for (const [id, request] of requests) {
    if (
      !present.has(id) &&
      request.createdAt < fetchedAt - RECONCILE_GRACE_MS
    ) {
      requests.delete(id);
      changed = true;
    }
  }
  if (changed) {
    scheduleExpiry();
    notify();
  }
  return changed;
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
  const result = [...requests.values(), ...userInputRequests.values()]
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
  const result = [...requests.values(), ...userInputRequests.values()]
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
  allCache = [...requests.values(), ...userInputRequests.values()].sort(
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

export function resetNeedsYouStore() {
  requests.clear();
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
