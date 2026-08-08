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
  agentPubkey: string;
  createdAt: number;
  summary: string;
  verify: string;
  reviewed: boolean;
};

const receiptsById = new Map<string, AgentReceiptSummary>();
const reviewedReceiptIds = new Set<string>();
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
    (tag) => tag[0] === "e" && Boolean(tag[1]?.trim()),
  );
  return eventTags.at(-1)?.[1]?.trim() || null;
}

export function subscribeAgentReceipts(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function ingestAgentReceiptEvent(event: RelayEvent): boolean {
  if (event.kind !== KIND_AGENT_RECEIPT) return false;
  const parsed = parseAgentReceipt(event.content);
  const channelId = tagValue(event, "h");
  const rootId = getThreadReference(event.tags).rootId;
  const conversationId = deriveAgentConversationIdOrNull(channelId, rootId);
  if (!parsed || !channelId || !conversationId) return false;

  const prior = receiptsById.get(event.id);
  const receipt: AgentReceiptSummary = {
    id: event.id,
    channelId,
    conversationId,
    agentPubkey: normalizePubkey(event.pubkey),
    createdAt: event.created_at * 1_000,
    summary: parsed.summary,
    verify: parsed.verify,
    reviewed: reviewedReceiptIds.has(event.id),
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
    if (!latest || receipt.createdAt > latest.createdAt) latest = receipt;
  }
  cachedByConversation.set(conversationId, latest);
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

export function resetAgentReceiptStore(): void {
  if (receiptsById.size === 0 && reviewedReceiptIds.size === 0) return;
  receiptsById.clear();
  reviewedReceiptIds.clear();
  notify();
}
