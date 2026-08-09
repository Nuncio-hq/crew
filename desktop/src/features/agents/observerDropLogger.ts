import type { RelayEvent } from "@/shared/api/types";

const OBSERVER_DROP_LOG_INTERVAL_MS = 10_000;

export type ObserverDropReason =
  | "missing_telemetry_tag"
  | "unknown_agent"
  | "sender_agent_mismatch"
  | "stale_generation"
  | "decrypt_failed";

type ObserverDropLogState = {
  count: number;
  lastLoggedAt: number;
};

const observerDropLogState = new Map<
  ObserverDropReason,
  ObserverDropLogState
>();

export function logObserverDrop(
  reason: ObserverDropReason,
  event: RelayEvent,
  eventGeneration: number,
) {
  const previous = observerDropLogState.get(reason) ?? {
    count: 0,
    lastLoggedAt: 0,
  };
  const count = previous.count + 1;
  const now = Date.now();
  const shouldLog =
    count === 1 ||
    now - previous.lastLoggedAt >= OBSERVER_DROP_LOG_INTERVAL_MS ||
    count % 100 === 0;
  observerDropLogState.set(reason, {
    count,
    lastLoggedAt: shouldLog ? now : previous.lastLoggedAt,
  });
  if (!shouldLog) return;

  const agentTag = event.tags.find((tag) => tag[0] === "agent")?.[1] ?? null;
  console.debug("[observerRelayStore] observer frame dropped", {
    reason,
    count,
    eventId: event.id,
    agentTag,
    senderPubkey: event.pubkey,
    eventGeneration,
  });
}

export function resetObserverDropLogger() {
  observerDropLogState.clear();
}

export function getObserverDropCountsForTest(): Partial<
  Record<ObserverDropReason, number>
> {
  return Object.fromEntries(
    [...observerDropLogState].map(([reason, state]) => [reason, state.count]),
  );
}
