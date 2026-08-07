import * as React from "react";

import {
  CONVERSATION_OUTCOME_TTL_MS,
  getActiveTurnsGeneration,
  subscribeActiveAgentTurns,
  walkActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import {
  getNeedsYouForChannel,
  getNeedsYouGeneration,
  subscribeNeedsYou,
} from "@/features/agents/needsYouStore";
import {
  walkConversationOutcomes,
  type ConversationOutcomeEntry,
} from "@/features/agents/conversationOutcomeLedger";
import { normalizePubkey } from "@/shared/lib/pubkey";

export type ChannelAgentPresenceState =
  | "needs-you"
  | "working"
  | "done-recent"
  | "idle";

export type ChannelAgentRosterEntry = {
  agentPubkey: string;
};

export type ChannelAgentPresence = {
  agentPubkey: string;
  state: ChannelAgentPresenceState;
  conversationId: string | null;
  since: number | null;
};

type CacheEntry = {
  generation: string;
  rosterSignature: string;
  now: number;
  value: ChannelAgentPresence[];
};

const cache = new Map<string, CacheEntry>();

function earliestWorkingByAgent(channelId: string) {
  const working = new Map<string, { conversationId: string; since: number }>();
  walkActiveAgentTurns((agentPubkey, turn, offset) => {
    if (turn.channelId !== channelId) return;
    const since = turn.startedAt + offset;
    const prior = working.get(agentPubkey);
    if (!prior || since < prior.since) {
      working.set(agentPubkey, { conversationId: turn.conversationId, since });
    }
  });
  return working;
}

function latestDoneByAgent(channelId: string, now: number) {
  const done = new Map<
    string,
    ConversationOutcomeEntry & { conversationId: string }
  >();
  walkConversationOutcomes((conversationId, entry) => {
    if (
      entry.channelId !== channelId ||
      now - entry.endedAt > CONVERSATION_OUTCOME_TTL_MS
    ) {
      return;
    }
    const prior = done.get(entry.agentPubkey);
    if (!prior || entry.endedAt > prior.endedAt) {
      done.set(entry.agentPubkey, { ...entry, conversationId });
    }
  });
  return done;
}

function samePresence(
  left: readonly ChannelAgentPresence[],
  right: readonly ChannelAgentPresence[],
) {
  return (
    left.length === right.length &&
    left.every((entry, index) => {
      const other = right[index];
      return (
        entry.agentPubkey === other.agentPubkey &&
        entry.state === other.state &&
        entry.conversationId === other.conversationId &&
        entry.since === other.since
      );
    })
  );
}

/**
 * Derive the channel header's per-agent presence from the existing lifecycle
 * stores. The roster is supplied by the channel-members seam; this selector
 * only decides each member's phase and never creates another source of truth.
 */
export function deriveChannelAgentPresence(
  channelId: string | null | undefined,
  roster: readonly ChannelAgentRosterEntry[] = [],
  now = Date.now(),
): ChannelAgentPresence[] {
  if (!channelId) return [];

  const normalizedRoster = [
    ...new Set(roster.map((entry) => normalizePubkey(entry.agentPubkey))),
  ].sort();
  const rosterSignature = normalizedRoster.join(",");
  const generation = `${getActiveTurnsGeneration()}:${getNeedsYouGeneration()}`;
  const prior = cache.get(channelId);
  if (
    prior?.generation === generation &&
    prior.rosterSignature === rosterSignature &&
    prior.now === now
  ) {
    return prior.value;
  }

  const needsYouByAgent = new Map<
    string,
    { conversationId: string; since: number }
  >();
  for (const request of getNeedsYouForChannel(channelId, now)) {
    const agentPubkey = normalizePubkey(request.agentPubkey);
    const existing = needsYouByAgent.get(agentPubkey);
    if (!existing || request.createdAt < existing.since) {
      needsYouByAgent.set(agentPubkey, {
        conversationId: request.conversationId,
        since: request.createdAt,
      });
    }
  }
  const workingByAgent = earliestWorkingByAgent(channelId);
  const doneByAgent = latestDoneByAgent(channelId, now);

  const next = normalizedRoster.map((agentPubkey) => {
    const needsYou = needsYouByAgent.get(agentPubkey);
    if (needsYou) {
      return {
        agentPubkey,
        state: "needs-you" as const,
        conversationId: needsYou.conversationId,
        since: needsYou.since,
      };
    }
    const working = workingByAgent.get(agentPubkey);
    if (working) {
      return {
        agentPubkey,
        state: "working" as const,
        conversationId: working.conversationId,
        since: working.since,
      };
    }
    const done = doneByAgent.get(agentPubkey);
    if (done) {
      return {
        agentPubkey,
        state: "done-recent" as const,
        conversationId: done.conversationId,
        since: done.endedAt,
      };
    }
    return {
      agentPubkey,
      state: "idle" as const,
      conversationId: null,
      since: null,
    };
  });

  const value = prior && samePresence(prior.value, next) ? prior.value : next;
  cache.set(channelId, { generation, rosterSignature, now, value });
  return value;
}

export function useChannelAgentPresence(
  channelId: string | null | undefined,
  roster: readonly ChannelAgentRosterEntry[],
  now = Date.now(),
): ChannelAgentPresence[] {
  const stableRoster = React.useMemo(() => roster, [roster]);
  const getSnapshot = React.useCallback(
    () => deriveChannelAgentPresence(channelId, stableRoster, now),
    [channelId, now, stableRoster],
  );
  return React.useSyncExternalStore(
    (listener) => {
      const unsubscribeTurns = subscribeActiveAgentTurns(listener);
      const unsubscribeNeedsYou = subscribeNeedsYou(listener);
      return () => {
        unsubscribeTurns();
        unsubscribeNeedsYou();
      };
    },
    getSnapshot,
    getSnapshot,
  );
}

export function resetChannelAgentPresenceCache() {
  cache.clear();
}

export function getChannelAgentPresenceStoreGeneration() {
  return `${getActiveTurnsGeneration()}:${getNeedsYouGeneration()}`;
}
