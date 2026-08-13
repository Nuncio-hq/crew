import * as React from "react";

export type ThreadForgeHubSubject =
  | {
      kind: "pr";
      owner: string;
      name: string;
      number: number;
      repositoryPath?: string | null;
      worktreePath?: string | null;
      branch?: string | null;
      channelId: string | null;
      rootEventId: string | null;
      source: "thread" | "url";
    }
  | {
      kind: "empty";
      owner?: string | null;
      name?: string | null;
      repositoryPath: string;
      worktreePath?: string | null;
      branch: string;
      baseRef?: string | null;
      channelId: string | null;
      rootEventId: string;
    };

let subject: ThreadForgeHubSubject | null = null;
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

export function getThreadForgeHubSubject(): ThreadForgeHubSubject | null {
  return subject;
}

export function setThreadForgeHubSubject(
  next: ThreadForgeHubSubject | null,
): void {
  subject = next;
  notify();
}

export function resetThreadForgeHubSubject(): void {
  subject = null;
  notify();
}

export function useThreadForgeHubSubject(): ThreadForgeHubSubject | null {
  return React.useSyncExternalStore(subscribe, getThreadForgeHubSubject);
}
