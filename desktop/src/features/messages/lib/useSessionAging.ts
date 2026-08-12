import { useSyncExternalStore } from "react";

import {
  getSessionAgingSnapshot,
  subscribeSessionAging,
  type SessionAgingEntry,
} from "./sessionAgingStore";

export function useSessionAgingForConversation(
  conversationId: string | null | undefined,
): SessionAgingEntry[] {
  const map = useSyncExternalStore(
    subscribeSessionAging,
    getSessionAgingSnapshot,
    getSessionAgingSnapshot,
  );
  if (!conversationId) {
    return [];
  }
  const out: SessionAgingEntry[] = [];
  for (const entry of map.values()) {
    if (
      entry.conversationId === conversationId ||
      entry.channelId === conversationId
    ) {
      out.push(entry);
    }
  }
  return out;
}

/** Deduped aging rows for any of the given conversation / channel ids. */
export function useSessionAgingForConversations(
  conversationIds: readonly (string | null | undefined)[],
): SessionAgingEntry[] {
  const map = useSyncExternalStore(
    subscribeSessionAging,
    getSessionAgingSnapshot,
    getSessionAgingSnapshot,
  );
  const seen = new Set<string>();
  const out: SessionAgingEntry[] = [];
  for (const id of conversationIds) {
    if (!id) continue;
    for (const entry of map.values()) {
      if (entry.conversationId !== id && entry.channelId !== id) {
        continue;
      }
      const key = `${entry.agentPubkey}:${entry.conversationId}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(entry);
    }
  }
  return out;
}
