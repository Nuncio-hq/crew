import type { ObserverEvent } from "@/features/agents/ui/agentSessionTypes";

const MAX_RETIRED_SESSIONS = 128;

const liveSessionByAgentChannel = new Map<string, string>();
const retiredSessionsByAgentChannel = new Map<string, Set<string>>();

export type AgentSessionObservation = "current" | "changed" | "retired";

export type AgentSessionGenerationSnapshot = {
  live: Map<string, string>;
  retired: Map<string, Set<string>>;
};

function sessionKey(agentKey: string, event: ObserverEvent): string {
  return `${agentKey}\u0000${event.channelId ?? "null-channel"}`;
}

export function observeAgentSession(
  agentKey: string,
  event: ObserverEvent,
): AgentSessionObservation {
  if (event.replayed || !event.sessionId) return "current";
  const key = sessionKey(agentKey, event);
  const current = liveSessionByAgentChannel.get(key);
  if (!current) {
    liveSessionByAgentChannel.set(key, event.sessionId);
    return "current";
  }
  if (current === event.sessionId) return "current";

  const retired = retiredSessionsByAgentChannel.get(key);
  if (retired?.has(event.sessionId)) return "retired";
  const nextRetired = retired ?? new Set<string>();
  nextRetired.add(current);
  if (nextRetired.size > MAX_RETIRED_SESSIONS) {
    const oldest = nextRetired.values().next().value;
    if (oldest !== undefined) nextRetired.delete(oldest);
  }
  retiredSessionsByAgentChannel.set(key, nextRetired);
  liveSessionByAgentChannel.set(key, event.sessionId);
  return "changed";
}

export function resetAgentSessionGenerations(): void {
  liveSessionByAgentChannel.clear();
  retiredSessionsByAgentChannel.clear();
}

export function snapshotAgentSessionGenerations(): AgentSessionGenerationSnapshot {
  return {
    live: new Map(liveSessionByAgentChannel),
    retired: new Map(
      [...retiredSessionsByAgentChannel].map(([key, sessions]) => [
        key,
        new Set(sessions),
      ]),
    ),
  };
}

export function restoreAgentSessionGenerations(
  snapshot: AgentSessionGenerationSnapshot,
): void {
  resetAgentSessionGenerations();
  for (const [key, sessionId] of snapshot.live) {
    liveSessionByAgentChannel.set(key, sessionId);
  }
  for (const [key, sessions] of snapshot.retired) {
    retiredSessionsByAgentChannel.set(key, new Set(sessions));
  }
}
