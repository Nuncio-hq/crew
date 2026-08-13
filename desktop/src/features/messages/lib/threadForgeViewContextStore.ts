import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";

export type ThreadForgeViewContext = {
  channelId: string | null;
  rootEventId: string | null;
  messages: TimelineMessage[];
};

let context: ThreadForgeViewContext | null = null;
const listeners = new Set<() => void>();

function notify(): void {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getThreadForgeViewContext(): ThreadForgeViewContext | null {
  return context;
}

export function setThreadForgeViewContext(
  next: ThreadForgeViewContext | null,
): void {
  context = next;
  notify();
}

export function resetThreadForgeViewContext(): void {
  context = null;
  notify();
}

export function useThreadForgeViewContext(): ThreadForgeViewContext | null {
  return React.useSyncExternalStore(subscribe, getThreadForgeViewContext);
}
