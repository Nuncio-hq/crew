import * as React from "react";

import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import { validatesCanonicalEventAncestry } from "@/features/agents/receiptParentLookup";
import { parseAgentReceipt } from "@/features/messages/lib/agentReceipt.mjs";
import { getThreadReference } from "@/features/messages/lib/threading";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_AGENT_RECEIPT, KIND_REACTION } from "@/shared/constants/kinds";
import { normalizePubkey } from "@/shared/lib/pubkey";

export type AgentReceiptSummary = {
  id: string;
  channelId: string;
  conversationId: string;
  rootEventId: string | null;
  parentEventId: string;
  agentPubkey: string;
  sessionId: string;
  turnId: string;
  createdAt: number;
  summary: string;
  verify: string;
  reviewed: boolean;
};

const receiptsById = new Map<string, AgentReceiptSummary>();
const reviewedReceiptIds = new Set<string>();
const MAX_PENDING_RECEIPT_REVIEWS = 1_000;
const pendingReviewEventsByReceiptId = new Map<string, RelayEvent>();
let pendingReviewProjectionUnavailable = false;
let exhaustiveReviewProjection = false;
let nextReviewProjectionOwner = 0;
let exhaustiveReviewProjectionOwner = 0;
let reviewAuthority: {
  currentPubkey: string;
  ownedAgentPubkeys: ReadonlySet<string>;
} = { currentPubkey: "", ownedAgentPubkeys: new Set() };
const listeners = new Set<() => void>();
let generation = 0;
let cachedAll: AgentReceiptSummary[] | null = null;
const cachedByConversation = new Map<string, AgentReceiptSummary | null>();
const EMPTY: AgentReceiptSummary[] = [];

function ownsReviewProjection(owner: number): boolean {
  return owner === exhaustiveReviewProjectionOwner;
}

export function beginExhaustiveAgentReceiptProjection(
  currentPubkey = "",
  ownedAgentPubkeys: ReadonlySet<string> = new Set(),
): number {
  nextReviewProjectionOwner += 1;
  exhaustiveReviewProjectionOwner = nextReviewProjectionOwner;
  pendingReviewEventsByReceiptId.clear();
  pendingReviewProjectionUnavailable = false;
  exhaustiveReviewProjection = true;
  reviewAuthority = {
    currentPubkey: normalizePubkey(currentPubkey),
    ownedAgentPubkeys: new Set([...ownedAgentPubkeys].map(normalizePubkey)),
  };
  return exhaustiveReviewProjectionOwner;
}

export function endExhaustiveAgentReceiptProjection(owner: number): boolean {
  if (!ownsReviewProjection(owner)) return false;
  exhaustiveReviewProjection = false;
  return true;
}

export function isAgentReceiptProjectionUnavailable(): boolean {
  return pendingReviewProjectionUnavailable;
}

export function markAgentReceiptProjectionUnavailable(owner: number): boolean {
  if (!ownsReviewProjection(owner)) return false;
  pendingReviewEventsByReceiptId.clear();
  pendingReviewProjectionUnavailable = true;
  exhaustiveReviewProjection = false;
  receiptsById.clear();
  reviewedReceiptIds.clear();
  notify();
  return true;
}

function notify() {
  generation += 1;
  cachedAll = null;
  cachedByConversation.clear();
  for (const listener of listeners) listener();
}

function tagValue(event: RelayEvent, name: string): string | null {
  const tags = event.tags.filter((tag) => tag[0] === name);
  const value = tags[0]?.[1];
  return tags.length === 1 && value && value === value.trim() ? value : null;
}

function receiptEventId(event: RelayEvent): string | null {
  // NIP-25 reactions target the last valid `e` tag. Keep this identical to
  // relay ingestion, which derives the reaction's channel from that target.
  const eventTags = event.tags.filter(
    (tag) => tag[0] === "e" && /^[0-9a-f]{64}$/.test(tag[1] ?? ""),
  );
  return eventTags.at(-1)?.[1] || null;
}

function receiptThreadIds(
  event: RelayEvent,
): { channelId: string; parentId: string; rootId: string } | null {
  const channelTags = event.tags.filter((tag) => tag[0] === "h");
  const eventTags = event.tags.filter((tag) => tag[0] === "e");
  if (
    channelTags.length !== 1 ||
    eventTags.length < 1 ||
    eventTags.length > 2
  ) {
    return null;
  }
  const channelId = channelTags[0]?.[1] ?? "";
  let parentId: string | null = null;
  let rootId: string | null = null;
  for (const tag of eventTags) {
    const eventId = tag[1] ?? "";
    if (tag.length !== 4 || !/^[0-9a-f]{64}$/.test(eventId)) return null;
    if (tag[3] === "reply" && parentId === null) parentId = eventId;
    else if (tag[3] === "root" && rootId === null) rootId = eventId;
    else return null;
  }
  if (!channelId || !parentId) return null;
  return { channelId, parentId, rootId: rootId ?? parentId };
}

/** Independently verify the receipt's signed parent and thread relationship. */
export function validateAgentReceiptThreadRelationship(
  event: RelayEvent,
  parentEvent: RelayEvent | null | undefined,
  causalEvents: ReadonlyMap<string, RelayEvent> = new Map(
    parentEvent ? [[parentEvent.id, parentEvent]] : [],
  ),
): boolean {
  const thread = receiptThreadIds(event);
  if (
    !thread ||
    !parentEvent ||
    !/^[0-9a-f]{64}$/.test(parentEvent.id) ||
    !/^[0-9a-f]{64}$/.test(parentEvent.pubkey) ||
    parentEvent.id !== thread.parentId
  )
    return false;
  const parentChannelTags = parentEvent.tags.filter((tag) => tag[0] === "h");
  if (
    parentChannelTags.length !== 1 ||
    parentChannelTags[0]?.[1] !== thread.channelId
  ) {
    return false;
  }
  if (
    !validatesCanonicalEventAncestry(
      parentEvent,
      thread.rootId,
      thread.channelId,
      causalEvents,
    )
  )
    return false;

  const parentTargets = parentEvent.tags.filter((tag) => tag[0] === "p");
  if (
    parentTargets.length === 0 ||
    parentTargets.some((tag) => !/^[0-9a-f]{64}$/.test(tag[1] ?? ""))
  ) {
    return false;
  }
  return parentTargets.some((tag) => tag[1] === event.pubkey);
}

export function subscribeAgentReceipts(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function ingestAgentReceiptEvent(
  event: RelayEvent,
  parentEvent: RelayEvent | null | undefined,
  causalEvents: ReadonlyMap<string, RelayEvent>,
  projectionOwner: number,
): boolean {
  if (!ownsReviewProjection(projectionOwner)) return false;
  if (pendingReviewProjectionUnavailable) return false;
  if (event.kind !== KIND_AGENT_RECEIPT) return false;
  if (!/^[0-9a-f]{64}$/.test(event.id) || !/^[0-9a-f]{64}$/.test(event.pubkey))
    return false;
  if (
    !validateAgentReceiptThreadRelationship(event, parentEvent, causalEvents)
  ) {
    return false;
  }
  const thread = receiptThreadIds(event);
  const parsed = parseAgentReceipt(event.content);
  const channelId = tagValue(event, "h");
  const rootId = getThreadReference(event.tags).rootId;
  const conversationId = deriveAgentConversationIdOrNull(channelId, rootId);
  if (!thread || !parsed?.run || !channelId || !conversationId) return false;

  const prior = receiptsById.get(event.id);
  const pendingReview = pendingReviewEventsByReceiptId.get(event.id);
  const reviewed =
    reviewedReceiptIds.has(event.id) ||
    (pendingReview !== undefined &&
      !pendingReviewProjectionUnavailable &&
      normalizePubkey(pendingReview.pubkey) === reviewAuthority.currentPubkey &&
      reviewAuthority.ownedAgentPubkeys.has(normalizePubkey(event.pubkey)) &&
      receiptEventId(pendingReview) === event.id);
  pendingReviewEventsByReceiptId.delete(event.id);
  if (reviewed) reviewedReceiptIds.add(event.id);
  const receipt: AgentReceiptSummary = {
    id: event.id,
    channelId,
    conversationId,
    rootEventId: rootId,
    parentEventId: thread.parentId,
    agentPubkey: normalizePubkey(event.pubkey),
    sessionId: parsed.run.sessionId,
    turnId: parsed.run.turnId,
    createdAt: event.created_at * 1_000,
    summary: parsed.summary,
    verify: parsed.verify,
    reviewed,
  };
  if (
    prior &&
    prior.reviewed === receipt.reviewed &&
    prior.createdAt === receipt.createdAt &&
    prior.sessionId === receipt.sessionId &&
    prior.turnId === receipt.turnId &&
    prior.summary === receipt.summary &&
    prior.verify === receipt.verify
  ) {
    return false;
  }
  receiptsById.set(event.id, receipt);
  notify();
  return true;
}

export function ingestAgentReceiptReviewEvent(
  event: RelayEvent,
  currentPubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
  projectionOwner: number,
): boolean {
  if (!ownsReviewProjection(projectionOwner)) return false;
  reviewAuthority = {
    currentPubkey: normalizePubkey(currentPubkey),
    ownedAgentPubkeys: new Set([...ownedAgentPubkeys].map(normalizePubkey)),
  };
  if (
    event.kind !== KIND_REACTION ||
    !/^[0-9a-f]{64}$/.test(event.id) ||
    !/^[0-9a-f]{64}$/.test(event.pubkey) ||
    event.content !== "✅" ||
    event.pubkey !== normalizePubkey(currentPubkey)
  ) {
    return false;
  }
  const receiptId = receiptEventId(event);
  const receipt = receiptId ? receiptsById.get(receiptId) : null;
  if (receiptId && !receipt && ownedAgentPubkeys.size > 0) {
    if (exhaustiveReviewProjection) return false;
    if (pendingReviewProjectionUnavailable) return false;
    pendingReviewEventsByReceiptId.set(receiptId, event);
    if (pendingReviewEventsByReceiptId.size > MAX_PENDING_RECEIPT_REVIEWS) {
      markAgentReceiptProjectionUnavailable(projectionOwner);
    }
    return false;
  }
  if (
    !receiptId ||
    !receipt ||
    !ownedAgentPubkeys.has(receipt.agentPubkey) ||
    reviewedReceiptIds.has(receiptId)
  ) {
    return false;
  }
  reviewedReceiptIds.add(receiptId);
  receiptsById.set(receiptId, { ...receipt, reviewed: true });
  notify();
  return true;
}

/** Apply the owner-gated reviewed action after its relay publication succeeds. */
export function markAgentReceiptReviewed(receiptId: string): boolean {
  const receipt = receiptsById.get(receiptId);
  if (!receipt || reviewedReceiptIds.has(receiptId)) return false;
  reviewedReceiptIds.add(receiptId);
  receiptsById.set(receiptId, { ...receipt, reviewed: true });
  notify();
  return true;
}

export function getLatestAgentReceiptForConversation(
  conversationId: string | null | undefined,
): AgentReceiptSummary | null {
  if (!conversationId) return null;
  const cached = cachedByConversation.get(conversationId);
  if (cached !== undefined) return cached;
  let latest: AgentReceiptSummary | null = null;
  for (const receipt of receiptsById.values()) {
    if (receipt.conversationId !== conversationId) continue;
    if (
      !latest ||
      receipt.createdAt > latest.createdAt ||
      (receipt.createdAt === latest.createdAt && receipt.id > latest.id)
    ) {
      latest = receipt;
    }
  }
  cachedByConversation.set(conversationId, latest);
  return latest;
}

export function getLatestOwnedAgentReceiptForConversation(
  conversationId: string | null | undefined,
  ownedAgentPubkeys: ReadonlySet<string>,
): AgentReceiptSummary | null {
  if (!conversationId || ownedAgentPubkeys.size === 0) return null;
  let latest: AgentReceiptSummary | null = null;
  for (const receipt of receiptsById.values()) {
    if (
      receipt.conversationId !== conversationId ||
      !ownedAgentPubkeys.has(receipt.agentPubkey)
    ) {
      continue;
    }
    if (
      !latest ||
      receipt.createdAt > latest.createdAt ||
      (receipt.createdAt === latest.createdAt && receipt.id > latest.id)
    ) {
      latest = receipt;
    }
  }
  return latest;
}

type ActiveReceiptAuthority = {
  agentPubkey: string;
  runs: ReadonlyArray<{
    sessionId: string;
    turnId: string;
    triggeringEventIds: readonly string[];
  }>;
};

export function getLatestOwnedAgentReceiptForActiveTurns(
  conversationId: string | null | undefined,
  ownedAgentPubkeys: ReadonlySet<string>,
  turns: readonly ActiveReceiptAuthority[],
): AgentReceiptSummary | null {
  if (!conversationId) return null;
  if (turns.length === 0) return null;
  const pairs = new Set(
    turns.flatMap((turn) =>
      ownedAgentPubkeys.has(normalizePubkey(turn.agentPubkey))
        ? turn.runs.flatMap((run) =>
            run.triggeringEventIds.map(
              (eventId) =>
                `${normalizePubkey(turn.agentPubkey)}\0${eventId}\0${run.sessionId}\0${run.turnId}`,
            ),
          )
        : [],
    ),
  );
  if (pairs.size === 0) return null;
  const receipts = getAgentReceipts();
  const matching = [...pairs].map((pair) =>
    receipts.find(
      (receipt) =>
        receipt.conversationId === conversationId &&
        ownedAgentPubkeys.has(receipt.agentPubkey) &&
        `${receipt.agentPubkey}\0${receipt.parentEventId}\0${receipt.sessionId}\0${receipt.turnId}` ===
          pair,
    ),
  );
  if (matching.some((receipt) => !receipt)) return null;
  const covered = matching.filter(
    (receipt): receipt is AgentReceiptSummary => receipt !== undefined,
  );
  const latest = covered.reduce((current, receipt) =>
    receipt.createdAt > current.createdAt ||
    (receipt.createdAt === current.createdAt && receipt.id > current.id)
      ? receipt
      : current,
  );
  const allReviewed = covered.every((receipt) => receipt.reviewed);
  return {
    ...latest,
    reviewed: allReviewed,
  };
}

export function getAgentReceipts(): AgentReceiptSummary[] {
  if (cachedAll) return cachedAll;
  if (receiptsById.size === 0) return EMPTY;
  cachedAll = [...receiptsById.values()].sort(
    (a, b) => b.createdAt - a.createdAt || b.id.localeCompare(a.id),
  );
  return cachedAll;
}

export function getAgentReceiptsGeneration(): number {
  return generation;
}

export function _testPendingAgentReceiptReviewCount(): number {
  return pendingReviewEventsByReceiptId.size;
}

export function useLatestAgentReceiptForConversation(
  conversationId: string | null | undefined,
): AgentReceiptSummary | null {
  const getSnapshot = React.useCallback(
    () => getLatestAgentReceiptForConversation(conversationId),
    [conversationId],
  );
  return React.useSyncExternalStore(
    subscribeAgentReceipts,
    getSnapshot,
    getSnapshot,
  );
}

export function useLatestOwnedAgentReceiptForConversation(
  conversationId: string | null | undefined,
  ownedAgentPubkeys: ReadonlySet<string>,
): AgentReceiptSummary | null {
  const getSnapshot = React.useCallback(
    () =>
      getLatestOwnedAgentReceiptForConversation(
        conversationId,
        ownedAgentPubkeys,
      ),
    [conversationId, ownedAgentPubkeys],
  );
  return React.useSyncExternalStore(
    subscribeAgentReceipts,
    getSnapshot,
    getSnapshot,
  );
}

export function useLatestOwnedAgentReceiptForActiveTurns(
  conversationId: string | null | undefined,
  ownedAgentPubkeys: ReadonlySet<string>,
  turns: readonly ActiveReceiptAuthority[],
): AgentReceiptSummary | null {
  const getSnapshot = React.useCallback(
    () =>
      getLatestOwnedAgentReceiptForActiveTurns(
        conversationId,
        ownedAgentPubkeys,
        turns,
      ),
    [conversationId, ownedAgentPubkeys, turns],
  );
  return React.useSyncExternalStore(
    subscribeAgentReceipts,
    getSnapshot,
    getSnapshot,
  );
}

export function resetAgentReceiptStore(): void {
  nextReviewProjectionOwner += 1;
  exhaustiveReviewProjectionOwner = nextReviewProjectionOwner;
  exhaustiveReviewProjection = false;
  if (
    receiptsById.size === 0 &&
    reviewedReceiptIds.size === 0 &&
    pendingReviewEventsByReceiptId.size === 0 &&
    !pendingReviewProjectionUnavailable
  ) {
    return;
  }
  receiptsById.clear();
  reviewedReceiptIds.clear();
  pendingReviewEventsByReceiptId.clear();
  pendingReviewProjectionUnavailable = false;
  reviewAuthority = { currentPubkey: "", ownedAgentPubkeys: new Set() };
  notify();
}
