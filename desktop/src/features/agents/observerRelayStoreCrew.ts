import { normalizePubkey } from "@/shared/lib/pubkey";

import { prepareAgentSessionObservation } from "./activeAgentSessionGeneration";
import { observeLiveSessionAuthority } from "./observerLiveSessionAuthority";
import { dispatchControlResult } from "./controlResultDispatch";
import { ingestObserverFrameForEditAsUndo } from "./dispatchedEventIds";
import { ingestProjectThreadWorkspaceEvent } from "./projectThreadWorkspaceStore";
import { applySessionAgingObserverPayload } from "./sessionAgingObserverEffects";
import type { ObserverEvent } from "./ui/agentSessionTypes";

export type LiveObservationFilterResult = {
  acceptedEvents: ObserverEvent[];
  authorityUnavailable: boolean;
};

/** Crew session-generation + live authority gate before batch append. */
export function filterLiveObserverEventsForCrew(
  agentPubkey: string,
  events: readonly ObserverEvent[],
): LiveObservationFilterResult {
  const acceptedEvents: ObserverEvent[] = [];
  let authorityUnavailable = false;

  for (const parsed of events) {
    const sessionObservation = prepareAgentSessionObservation(
      normalizePubkey(agentPubkey),
      parsed,
    );
    if (sessionObservation === "retired") {
      continue;
    }
    const authority = observeLiveSessionAuthority(
      agentPubkey,
      parsed,
      sessionObservation,
    );
    if (!authority.accepted) {
      if (authority.unavailable) {
        authorityUnavailable = true;
      }
      continue;
    }
    acceptedEvents.push(parsed);
  }

  return { acceptedEvents, authorityUnavailable };
}

/** Crew workspace + edit-as-undo side effects after a retained batch append. */
export function applyCrewAppendBatchSideEffects(
  agentKey: string,
  sortedAdded: readonly ObserverEvent[],
  onTurnStarted: () => void,
): void {
  for (const event of sortedAdded) {
    ingestProjectThreadWorkspaceEvent(agentKey, event);
    ingestObserverFrameForEditAsUndo(event);
  }
  if (sortedAdded.some((event) => event.kind === "turn_started")) {
    onTurnStarted();
  }
}

/** Crew-only live-frame dispatch (session aging, control results). */
export function applyCrewLiveFrameSideEffects(
  agentPubkey: string,
  parsed: ObserverEvent,
): void {
  if (parsed.kind === "session_aging") {
    applySessionAgingObserverPayload(agentPubkey, parsed.payload);
  } else if (parsed.kind === "control_result") {
    dispatchControlResult(agentPubkey, parsed.payload);
  }
}

export function applyCrewE2EInjectSideEffects(
  agentPubkey: string,
  events: readonly ObserverEvent[],
): void {
  for (const event of events) {
    applyCrewLiveFrameSideEffects(agentPubkey, event);
  }
}
