import type { ActiveChannelTurnSummary } from "@/features/agents/activeAgentTurnsStore";
import { normalizePubkey } from "@/shared/lib/pubkey";

// Crew extension of activeAgentTurnsStore (issue #286): per-agent liveness
// fan-out and content-stable global snapshots, split out so observer liveness
// frames (token, usage, stdout, raw ACP traffic) no longer wake every global
// active-turns subscriber per streamed token.

// Per-agent liveness listeners: woken by observer frames for live turns
// that change no turn membership. Global listeners deliberately stay asleep
// for those frames.
const livenessListenersByAgent = new Map<string, Set<() => void>>();

/** Bumps on liveness frames too — see getActiveTurnsDataVersion. */
let activeTurnsDataVersion = 0;

// Last returned global snapshots, kept across generation bumps so a rebuild
// with structurally-equal content can return the prior reference and
// useSyncExternalStore consumers skip the re-render.
let previousChannelTurnSummaries: ActiveChannelTurnSummary[] | null = null;
const previousAgentsByConversation = new Map<string, string[]>();

/**
 * Subscribe to liveness frames for one agent's live turns (token, usage,
 * stdout, raw ACP traffic). These frames never wake the global
 * `subscribeActiveAgentTurns` listeners; surfaces that display per-frame
 * activity (activity bounds, transcript panels) subscribe here for the
 * agents they render, alongside the global subscription for membership
 * changes.
 */
export function subscribeAgentLiveness(
  agentPubkey: string,
  listener: () => void,
): () => void {
  const key = normalizePubkey(agentPubkey);
  let agentListeners = livenessListenersByAgent.get(key);
  if (!agentListeners) {
    agentListeners = new Set();
    livenessListenersByAgent.set(key, agentListeners);
  }
  agentListeners.add(listener);
  return () => {
    agentListeners.delete(listener);
    if (agentListeners.size === 0) {
      livenessListenersByAgent.delete(key);
    }
  };
}

/** Wake the liveness listeners registered for one normalized agent key. */
export function notifyAgentLivenessListeners(agentKey: string) {
  const agentListeners = livenessListenersByAgent.get(agentKey);
  if (!agentListeners) return;
  for (const listener of agentListeners) {
    listener();
  }
}

/**
 * Data-version counter: advances on membership changes AND on liveness
 * frames for live turns. Projections that snapshot in-place-mutated turn
 * fields (`lastSeenAt`, progress) must key their caches on this so
 * timer-driven re-reads see fresh liveness without a global notification.
 */
export function getActiveTurnsDataVersion(): number {
  return activeTurnsDataVersion;
}

/** Advance the data version (membership changes and liveness frames). */
export function bumpActiveTurnsDataVersion() {
  activeTurnsDataVersion += 1;
}

/**
 * Reuse the prior channel-summary snapshot reference when `next` is
 * structurally equal, so useSyncExternalStore consumers skip the re-render.
 */
export function stableChannelTurnSummaries(
  next: ActiveChannelTurnSummary[],
): ActiveChannelTurnSummary[] {
  const stable =
    previousChannelTurnSummaries &&
    channelTurnSummariesEqual(previousChannelTurnSummaries, next)
      ? previousChannelTurnSummaries
      : next;
  previousChannelTurnSummaries = stable;
  return stable;
}

/**
 * Reuse the prior agents-by-conversation entry reference when `next` has
 * identical content.
 */
export function stableAgentsForConversation(
  conversationId: string,
  next: string[],
): string[] {
  const prior = previousAgentsByConversation.get(conversationId);
  const stable =
    prior &&
    prior.length === next.length &&
    prior.every((pubkey, index) => pubkey === next[index])
      ? prior
      : next;
  previousAgentsByConversation.set(conversationId, stable);
  return stable;
}

/** Drop the retained prior snapshots (store reset / community restore). */
export function resetStableActiveTurnSnapshots() {
  previousChannelTurnSummaries = null;
  previousAgentsByConversation.clear();
}

function channelTurnSummariesEqual(
  a: readonly ActiveChannelTurnSummary[],
  b: readonly ActiveChannelTurnSummary[],
): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const left = a[i];
    const right = b[i];
    if (!left || !right) return false;
    if (
      left.channelId !== right.channelId ||
      left.anchorAt !== right.anchorAt ||
      left.agentCount !== right.agentCount ||
      left.agentPubkeys.length !== right.agentPubkeys.length
    ) {
      return false;
    }
    for (let j = 0; j < left.agentPubkeys.length; j += 1) {
      if (left.agentPubkeys[j] !== right.agentPubkeys[j]) return false;
    }
  }
  return true;
}
