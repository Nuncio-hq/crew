import type { ObserverEvent } from "./ui/agentSessionTypes";
import { normalizePubkey } from "@/shared/lib/pubkey";

const MAX_RETIRED_SESSIONS_PER_CONVERSATION = 128;
const currentByAuthority = new Map<string, string>();
const latestDisplayByAgentChannel = new Map<string, string>();
const retiredByAuthority = new Map<string, Set<string>>();
const unavailableAuthorities = new Set<string>();

function displayKey(agentPubkey: string, channelId: string | null): string {
  return `${normalizePubkey(agentPubkey)}:${channelId ?? ""}`;
}

function authorityKey(agentPubkey: string, event: ObserverEvent): string {
  return `${displayKey(agentPubkey, event.channelId)}:${event.conversationId ?? ""}`;
}

export function getLatestAuthorizedLiveSessionId(
  agentPubkey: string | null | undefined,
  channelId: string | null | undefined,
): string | null {
  if (!agentPubkey) return null;
  return (
    latestDisplayByAgentChannel.get(
      displayKey(agentPubkey, channelId ?? null),
    ) ?? null
  );
}

export function observeLiveSessionAuthority(
  agentPubkey: string,
  event: ObserverEvent,
  sessionObservation: "current" | "changed" | "retired",
): { accepted: boolean; unavailable: boolean } {
  if (!event.sessionId || !event.channelId) {
    return { accepted: true, unavailable: false };
  }
  const key = authorityKey(agentPubkey, event);
  if (unavailableAuthorities.has(key)) {
    return { accepted: false, unavailable: true };
  }
  const stored = currentByAuthority.get(key);
  const startsNewAuthority = stored !== event.sessionId;
  const retired = retiredByAuthority.get(key);
  if (retired?.has(event.sessionId)) {
    return { accepted: false, unavailable: false };
  }
  const advances =
    !stored ||
    stored === event.sessionId ||
    sessionObservation === "changed" ||
    event.kind === "turn_started";
  if (!advances) return { accepted: true, unavailable: false };

  if (stored && stored !== event.sessionId) {
    const nextRetired = retired ?? new Set<string>();
    if (
      !nextRetired.has(stored) &&
      nextRetired.size >= MAX_RETIRED_SESSIONS_PER_CONVERSATION
    ) {
      unavailableAuthorities.add(key);
      currentByAuthority.delete(key);
      const currentDisplayKey = displayKey(agentPubkey, event.channelId);
      if (latestDisplayByAgentChannel.get(currentDisplayKey) === stored) {
        latestDisplayByAgentChannel.delete(currentDisplayKey);
      }
      return { accepted: false, unavailable: true };
    }
    nextRetired.add(stored);
    retiredByAuthority.set(key, nextRetired);
  }
  currentByAuthority.set(key, event.sessionId);
  const currentDisplay = latestDisplayByAgentChannel.get(
    displayKey(agentPubkey, event.channelId),
  );
  if (
    !currentDisplay ||
    (event.kind === "turn_started" && startsNewAuthority) ||
    sessionObservation === "changed"
  ) {
    latestDisplayByAgentChannel.set(
      displayKey(agentPubkey, event.channelId),
      event.sessionId,
    );
  }
  return { accepted: true, unavailable: false };
}

export function resetLiveSessionAuthority(): void {
  currentByAuthority.clear();
  latestDisplayByAgentChannel.clear();
  retiredByAuthority.clear();
  unavailableAuthorities.clear();
}
