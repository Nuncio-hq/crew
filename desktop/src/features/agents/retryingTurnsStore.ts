/**
 * Tracks automatic harness retries (`turn_retrying` observer frames) so the
 * activity chrome can show "Retrying n/N" during backoff — when no turn is
 * in flight yet.
 */

import { normalizePubkey } from "@/shared/lib/pubkey";

export type RetryingTurn = {
  agentPubkey: string;
  channelId: string;
  conversationId: string;
  attempt: number;
  maxAttempts: number;
  updatedAt: number;
};

const retryingByConversation = new Map<string, RetryingTurn>();
const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) listener();
}

function key(conversationId: string): string {
  return conversationId.toLowerCase();
}

export function recordTurnRetrying(input: {
  agentPubkey: string;
  channelId: string | null | undefined;
  conversationId: string | null | undefined;
  attempt: number;
  maxAttempts: number;
}): void {
  if (!input.conversationId || !input.channelId) return;
  if (!Number.isFinite(input.attempt) || input.attempt < 1) return;
  if (!Number.isFinite(input.maxAttempts) || input.maxAttempts < 1) return;
  retryingByConversation.set(key(input.conversationId), {
    agentPubkey: normalizePubkey(input.agentPubkey),
    channelId: input.channelId,
    conversationId: input.conversationId,
    attempt: input.attempt,
    maxAttempts: input.maxAttempts,
    updatedAt: Date.now(),
  });
  notify();
}

export function clearTurnRetrying(
  conversationId: string | null | undefined,
): void {
  if (!conversationId) return;
  if (retryingByConversation.delete(key(conversationId))) {
    notify();
  }
}

export function getRetryingTurn(
  conversationId: string | null | undefined,
): RetryingTurn | null {
  if (!conversationId) return null;
  return retryingByConversation.get(key(conversationId)) ?? null;
}

export function getRetryingTurnsForChannel(
  channelId: string | null | undefined,
): RetryingTurn[] {
  if (!channelId) return [];
  return [...retryingByConversation.values()].filter(
    (entry) => entry.channelId === channelId,
  );
}

export function subscribeRetryingTurns(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** @internal */
export function _resetRetryingTurnsForTest(): void {
  retryingByConversation.clear();
  notify();
}
