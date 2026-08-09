import * as React from "react";

import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
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
  createdAt: number;
  summary: string;
  verify: string;
  reviewed: boolean;
};

const receiptsById = new Map<string, AgentReceiptSummary>();
const reviewedReceiptIds = new Set<string>();
const pendingReviewOwnershipByReceiptId = new Map<
  string,
  ReadonlySet<string>
>();
const MAX_PENDING_RECEIPT_REVIEWS = 512;
const listeners = new Set<() => void>();
let generation = 0;
let cachedAll: AgentReceiptSummary[] | null = null;
const cachedByConversation = new Map<string, AgentReceiptSummary | null>();
const EMPTY: AgentReceiptSummary[] = [];

function notify() {
  generation += 1;
  cachedAll = null;
  cachedByConversation.clear();
  for (const listener of listeners) listener();
}

function tagValue(event: RelayEvent, name: string): string | null {
  return event.tags.find((tag) => tag[0] === name)?.[1]?.trim() || null;
}

function receiptEventId(event: RelayEvent): string | null {
  // NIP-25 reactions target the last valid `e` tag. Keep this identical to
  // relay ingestion, which derives the reaction's channel from that target.
  const eventTags = event.tags.filter(
    (tag) => tag[0] === "e" && /^[0-9a-f]{64}$/.test(tag[1]?.trim() ?? ""),
  );
  return eventTags.at(-1)?.[1]?.trim() || null;
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
  const channelId = channelTags[0]?.[1]?.trim() ?? "";
  let parentId: string | null = null;
  let rootId: string | null = null;
  for (const tag of eventTags) {
    const eventId = tag[1]?.trim() ?? "";
    if (!/^[0-9a-f]{64}$/.test(eventId)) return null;
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
): boolean {
  const thread = receiptThreadIds(event);
  if (!thread || !parentEvent || parentEvent.id !== thread.parentId)
    return false;
  const parentChannelTags = parentEvent.tags.filter((tag) => tag[0] === "h");
  if (
    parentChannelTags.length !== 1 ||
    parentChannelTags[0]?.[1]?.trim() !== thread.channelId
  ) {
    return false;
  }
  const parentRoot =
    getThreadReference(parentEvent.tags).rootId ?? parentEvent.id;
  if (parentRoot !== thread.rootId) return false;

  const parentTargets = parentEvent.tags.filter((tag) => tag[0] === "p");
  if (
    parentTargets.length === 0 ||
    parentTargets.some((tag) => !/^[0-9a-f]{64}$/.test(tag[1]?.trim() ?? ""))
  ) {
    return false;
  }
  return parentTargets.some(
    (tag) => normalizePubkey(tag[1] ?? "") === normalizePubkey(event.pubkey),
  );
}

export function subscribeAgentReceipts(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function ingestAgentReceiptEvent(
  event: RelayEvent,
  parentEvent: RelayEvent | null | undefined,
): boolean {
  if (event.kind !== KIND_AGENT_RECEIPT) return false;
  if (!validateAgentReceiptThreadRelationship(event, parentEvent)) return false;
  const thread = receiptThreadIds(event);
  const parsed = parseAgentReceipt(event.content);
  const channelId = tagValue(event, "h");
  const rootId = getThreadReference(event.tags).rootId;
  const conversationId = deriveAgentConversationIdOrNull(channelId, rootId);
  if (!thread || !parsed || !channelId || !conversationId) return false;

  const prior = receiptsById.get(event.id);
  const pendingReviewOwnership = pendingReviewOwnershipByReceiptId.get(
    event.id,
  );
  const reviewed =
    reviewedReceiptIds.has(event.id) ||
    pendingReviewOwnership?.has(normalizePubkey(event.pubkey)) === true;
  pendingReviewOwnershipByReceiptId.delete(event.id);
  if (reviewed) reviewedReceiptIds.add(event.id);
  const receipt: AgentReceiptSummary = {
    id: event.id,
    channelId,
    conversationId,
    rootEventId: rootId,
    parentEventId: thread.parentId,
    agentPubkey: normalizePubkey(event.pubkey),
    createdAt: event.created_at * 1_000,
    summary: parsed.summary,
    verify: parsed.verify,
    reviewed,
  };
  if (
    prior &&
    prior.reviewed === receipt.reviewed &&
    prior.createdAt === receipt.createdAt &&
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
): boolean {
  if (
    event.kind !== KIND_REACTION ||
    event.content.trim() !== "✅" ||
    normalizePubkey(event.pubkey) !== normalizePubkey(currentPubkey)
  ) {
    return false;
  }
  const receiptId = receiptEventId(event);
  const receipt = receiptId ? receiptsById.get(receiptId) : null;
  if (receiptId && !receipt && ownedAgentPubkeys.size > 0) {
    if (
      !pendingReviewOwnershipByReceiptId.has(receiptId) &&
      pendingReviewOwnershipByReceiptId.size >= MAX_PENDING_RECEIPT_REVIEWS
    ) {
      return false;
    }
    pendingReviewOwnershipByReceiptId.set(
      receiptId,
      new Set([...ownedAgentPubkeys].map(normalizePubkey)),
    );
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

export function getAgentReceipts(): AgentReceiptSummary[] {
  if (cachedAll) return cachedAll;
  if (receiptsById.size === 0) return EMPTY;
  cachedAll = [...receiptsById.values()].sort(
    (a, b) => b.createdAt - a.createdAt || a.id.localeCompare(b.id),
  );
  return cachedAll;
}

export function getAgentReceiptsGeneration(): number {
  return generation;
}

export function _testPendingAgentReceiptReviewCount(): number {
  return pendingReviewOwnershipByReceiptId.size;
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

export function resetAgentReceiptStore(): void {
  if (
    receiptsById.size === 0 &&
    reviewedReceiptIds.size === 0 &&
    pendingReviewOwnershipByReceiptId.size === 0
  ) {
    return;
  }
  receiptsById.clear();
  reviewedReceiptIds.clear();
  pendingReviewOwnershipByReceiptId.clear();
  notify();
}
