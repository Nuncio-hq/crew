import * as React from "react";

import { resetPersistentAgentAudienceStore } from "./persistentAgentAudience";

export const KEEP_MENTIONED_AGENTS_PINNED_STORAGE_KEY =
  "buzz.messages.keepMentionedAgentsPinned";
export const DEFAULT_KEEP_MENTIONED_AGENTS_PINNED = false;

const listeners = new Set<() => void>();
let keepMentionedAgentsPinned = readStoredPreference();

export function parseKeepMentionedAgentsPinned(
  value: string | null | undefined,
): boolean {
  if (value === "false") return false;
  if (value === "true") return true;
  return DEFAULT_KEEP_MENTIONED_AGENTS_PINNED;
}

function readStoredPreference(): boolean {
  try {
    const storage = globalThis.localStorage;
    const current = storage?.getItem(KEEP_MENTIONED_AGENTS_PINNED_STORAGE_KEY);
    if (current != null) return parseKeepMentionedAgentsPinned(current);
    // Preserve an explicit Crew preference when moving onto the shared model.
    // Old saved audiences are not imported: recipients stay community-scoped.
    const previous = storage?.getItem("buzz:keep-addressed-agents-active");
    if (previous === "1" || previous === "0") {
      const enabled = previous === "1";
      storage?.setItem(
        KEEP_MENTIONED_AGENTS_PINNED_STORAGE_KEY,
        String(enabled),
      );
      return enabled;
    }
    return DEFAULT_KEEP_MENTIONED_AGENTS_PINNED;
  } catch {
    return DEFAULT_KEEP_MENTIONED_AGENTS_PINNED;
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getKeepMentionedAgentsPinned(): boolean {
  return keepMentionedAgentsPinned;
}

export function setKeepMentionedAgentsPinned(value: boolean): void {
  if (!value) resetPersistentAgentAudienceStore();
  if (value === keepMentionedAgentsPinned) return;
  keepMentionedAgentsPinned = value;
  try {
    globalThis.localStorage?.setItem(
      KEEP_MENTIONED_AGENTS_PINNED_STORAGE_KEY,
      String(value),
    );
  } catch {
    // Persistence is best-effort; the live preference still applies.
  }
  for (const listener of listeners) listener();
}

export function useKeepMentionedAgentsPinned(): boolean {
  return React.useSyncExternalStore(
    subscribe,
    getKeepMentionedAgentsPinned,
    () => DEFAULT_KEEP_MENTIONED_AGENTS_PINNED,
  );
}
