import type { ObserverEvent } from "@/features/agents/ui/agentSessionTypes";

const MAX_RETIRED_SESSIONS = 128;

const liveSessionByAgentChannel = new Map<string, string>();
const retiredSessionsByAgentChannel = new Map<string, Set<string>>();
let preparedObservationByEvent = new WeakMap<
  ObserverEvent,
  AgentSessionObservation
>();

export type AgentSessionObservation = "current" | "changed" | "retired";

export type AgentSessionGenerationSnapshot = {
  live: Map<string, string>;
  retired: Map<string, Set<string>>;
};

function sessionKey(agentKey: string, event: ObserverEvent): string {
  const producerIndex =
    typeof event.agentIndex === "number" ? event.agentIndex : "null-agent";
  const conversation =
    event.conversationId ?? event.turnId ?? "null-conversation";
  return [
    agentKey,
    event.channelId ?? "null-channel",
    producerIndex,
    conversation,
  ].join("\u0000");
}

export function observeAgentSession(
  agentKey: string,
  event: ObserverEvent,
): AgentSessionObservation {
  const prepared = preparedObservationByEvent.get(event);
  if (prepared) return prepared;
  return commitAgentSessionObservation(agentKey, event);
}

function commitAgentSessionObservation(
  agentKey: string,
  event: ObserverEvent,
): AgentSessionObservation {
  if (!event.sessionId) return "current";
  const key = sessionKey(agentKey, event);
  const current = liveSessionByAgentChannel.get(key);
  const retired = retiredSessionsByAgentChannel.get(key);
  if (retired?.has(event.sessionId)) return "retired";
  if (event.replayed) {
    return current && current !== event.sessionId ? "retired" : "current";
  }
  if (!current) {
    liveSessionByAgentChannel.set(key, event.sessionId);
    return "current";
  }
  if (current === event.sessionId) return "current";

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

/**
 * Commit session authority before observer side effects and preserve the same
 * decision for downstream active-turn projection of this exact frame.
 */
export function prepareAgentSessionObservation(
  agentKey: string,
  event: ObserverEvent,
): AgentSessionObservation {
  const prior = preparedObservationByEvent.get(event);
  if (prior) {
    const key = sessionKey(agentKey, event);
    if (
      key &&
      retiredSessionsByAgentChannel.get(key)?.has(event.sessionId ?? "")
    ) {
      preparedObservationByEvent.set(event, "retired");
      return "retired";
    }
    return prior;
  }
  const observation = commitAgentSessionObservation(agentKey, event);
  preparedObservationByEvent.set(event, observation);
  return observation;
}

export function resetAgentSessionGenerations(): void {
  liveSessionByAgentChannel.clear();
  retiredSessionsByAgentChannel.clear();
  preparedObservationByEvent = new WeakMap();
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
