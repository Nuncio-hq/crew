import * as React from "react";

export const AGENT_ATTENTION_SNOOZE_MS = 10 * 60_000;

const snoozedUntilByConversation = new Map<string, number>();
const listeners = new Set<() => void>();
let generation = 0;

function notify() {
  generation += 1;
  for (const listener of listeners) listener();
}

export function subscribeAgentAttentionSnoozes(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getAgentAttentionSnoozedUntil(
  conversationId: string | null | undefined,
): number {
  if (!conversationId) return 0;
  return snoozedUntilByConversation.get(conversationId) ?? 0;
}

export function getAgentAttentionSnoozeGeneration(): number {
  return generation;
}

export function snoozeAgentAttention(
  conversationId: string,
  now: number = Date.now(),
  durationMs: number = AGENT_ATTENTION_SNOOZE_MS,
): number {
  const until = now + durationMs;
  snoozedUntilByConversation.set(conversationId, until);
  notify();
  return until;
}

export function clearAgentAttentionSnooze(conversationId: string): void {
  if (!snoozedUntilByConversation.delete(conversationId)) return;
  notify();
}

export function useAgentAttentionSnoozedUntil(
  conversationId: string | null | undefined,
): number {
  const getSnapshot = React.useCallback(
    () => getAgentAttentionSnoozedUntil(conversationId),
    [conversationId],
  );
  return React.useSyncExternalStore(
    subscribeAgentAttentionSnoozes,
    getSnapshot,
    getSnapshot,
  );
}

export function resetAgentAttentionSnoozes(): void {
  if (snoozedUntilByConversation.size === 0) return;
  snoozedUntilByConversation.clear();
  notify();
}
