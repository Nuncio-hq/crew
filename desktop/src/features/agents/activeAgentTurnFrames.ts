import {
  applyObserverFrame,
  type ActiveTurn,
} from "@/features/agents/activeAgentTurnModel";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { ObserverEvent } from "./ui/agentSessionTypes";

type ActiveTurnsByAgent = ReadonlyMap<string, ReadonlyMap<string, ActiveTurn>>;

/** Result of applying an observer frame to a retained active turn. */
export type ActiveAgentTurnFrameResult = {
  found: boolean;
  progressChanged: boolean;
};

/** Apply one observer frame to its exact active turn, if retained. */
export function recordActiveAgentTurnFrame(
  activeTurnsByAgent: ActiveTurnsByAgent,
  agentPubkey: string,
  event: ObserverEvent,
  eventObservedAt: (agentKey: string, event: ObserverEvent) => number,
): ActiveAgentTurnFrameResult {
  if (!event.turnId) return { found: false, progressChanged: false };
  const key = normalizePubkey(agentPubkey);
  const turn = activeTurnsByAgent.get(key)?.get(event.turnId);
  if (!turn) return { found: false, progressChanged: false };
  return {
    found: true,
    progressChanged: applyObserverFrame(
      turn,
      event,
      eventObservedAt(key, event),
    ),
  };
}

/**
 * Resolve a turn's session after the harness emits turn_started without one.
 * The callback invalidates the store's derived snapshots after an exact match.
 */
export function backfillActiveAgentTurnSession(
  activeTurnsByAgent: ActiveTurnsByAgent,
  agentPubkey: string,
  event: ObserverEvent,
  invalidate: (agentKey: string) => void,
): boolean {
  if (!event.sessionId || !event.turnId || !event.channelId) return false;
  const key = normalizePubkey(agentPubkey);
  const turn = activeTurnsByAgent.get(key)?.get(event.turnId);
  if (!turn) return false;
  if (
    turn.channelId !== event.channelId ||
    turn.conversationId !== (event.conversationId ?? event.channelId) ||
    turn.sessionId !== null
  ) {
    return false;
  }
  turn.sessionId = event.sessionId;
  invalidate(key);
  return true;
}
