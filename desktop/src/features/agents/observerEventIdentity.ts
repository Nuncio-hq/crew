import type { ObserverEvent } from "@/features/agents/ui/agentSessionTypes";

const OBSERVER_BATCH_KIND = "batch";

export function observerEventIdentity(event: ObserverEvent) {
  return [
    event.sourceEventId ?? "no-source",
    event.agentIndex ?? "no-agent",
    event.channelId ?? "no-channel",
    event.conversationId ?? "no-conversation",
    event.sessionId ?? "no-session",
    event.turnId ?? "no-turn",
    event.kind,
    event.seq,
    event.timestamp,
  ].join("\u0000");
}

export function unwrapObserverBatch(parsed: ObserverEvent): ObserverEvent[] {
  if (parsed.kind !== OBSERVER_BATCH_KIND) return [parsed];
  const payload = parsed.payload as { events?: unknown } | null;
  const events = Array.isArray(payload?.events)
    ? (payload.events as ObserverEvent[])
    : null;
  return events && events.length > 0 ? events : [parsed];
}
