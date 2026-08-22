import * as React from "react";

import {
  subscribeActiveAgentTurns,
  walkActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import {
  getAgentTranscript,
  subscribeAgentObserverProjection,
} from "@/features/agents/observerRelayStore";
import {
  getActivityHeadline,
  isMeaningfulItem,
  isSpineItem,
} from "@/features/agents/ui/agentSessionTranscriptPresentation";
import type { TranscriptItem } from "@/features/agents/ui/agentSessionTypes";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";

/** Rotate the visible headline every ~3s (shared across all running rows). */
export const HEADLINE_ROTATION_MS = 3_000;
const MAX_HEADLINES = 5;

const EMPTY_HEADLINES: readonly string[] = Object.freeze([]);
const EMPTY_AGENTS: readonly ConversationAgentActivity[] = Object.freeze([]);

export type ConversationAgentActivity = {
  agentPubkey: string;
  lastActivityAt: number;
  turnIds: readonly string[];
};

export type ConversationActivityHeadlineSelection = {
  agentPubkey: string;
  /** Oldest→newest among the last unique headlines (same order as BotActivityBar). */
  headlines: readonly string[];
  /** Most recent headline; used for reduced-motion + chip tooltip. */
  latest: string | null;
  /** True when more than one agent is live in the conversation. */
  prefixAgentName: boolean;
  agentShortName: string;
};

type HeadlinesCacheEntry = {
  transcript: readonly TranscriptItem[];
  turnKey: string;
  headlines: readonly string[];
};

const headlinesByConversation = new Map<string, HeadlinesCacheEntry>();
const selectionByConversation = new Map<
  string,
  {
    transcript: readonly TranscriptItem[];
    agentPubkey: string;
    prefixAgentName: boolean;
    agentShortName: string;
    value: ConversationActivityHeadlineSelection;
  }
>();

function arraysShallowEqual(
  a: readonly string[],
  b: readonly string[],
): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (!Object.is(a[i], b[i])) return false;
  }
  return true;
}

function turnIdsKey(turnIds: ReadonlySet<string> | readonly string[]): string {
  return [...turnIds].sort().join("\0");
}

function itemMatchesConversation(
  item: TranscriptItem,
  conversationId: string,
  turnIds: ReadonlySet<string>,
): boolean {
  if (item.conversationId != null && item.conversationId !== "") {
    return item.conversationId === conversationId;
  }
  if (item.turnId) return turnIds.has(item.turnId);
  return false;
}

/**
 * Two-tier spine/meaningful headline scan for one conversation — same
 * selection rules as the historic BotActivityBar channel scan, scoped by
 * `conversationId` (with turn-id fallback for items that lack it).
 *
 * Result reference is stable when the derived headline list is unchanged.
 */
export function collectConversationActivityHeadlines(
  transcript: readonly TranscriptItem[],
  conversationId: string | null | undefined,
  turnIds: ReadonlySet<string> = new Set(),
): readonly string[] {
  if (!conversationId) return EMPTY_HEADLINES;

  const turnKey = turnIdsKey(turnIds);
  const prev = headlinesByConversation.get(conversationId);
  if (prev && prev.transcript === transcript && prev.turnKey === turnKey) {
    return prev.headlines;
  }

  const scoped = transcript.filter((item) =>
    itemMatchesConversation(item, conversationId, turnIds),
  );
  const passFilter: (item: TranscriptItem) => boolean = scoped.some(isSpineItem)
    ? isSpineItem
    : isMeaningfulItem;

  const seen = new Set<string>();
  const headlines: string[] = [];
  for (let i = scoped.length - 1; i >= 0; i -= 1) {
    const item = scoped[i];
    if (!item || !passFilter(item)) continue;
    const headline = getActivityHeadline(item);
    if (!headline || seen.has(headline)) continue;
    seen.add(headline);
    headlines.unshift(headline);
    if (headlines.length >= MAX_HEADLINES) break;
  }

  if (prev && arraysShallowEqual(prev.headlines, headlines)) {
    headlinesByConversation.set(conversationId, {
      transcript,
      turnKey,
      headlines: prev.headlines,
    });
    return prev.headlines;
  }

  const frozen =
    headlines.length === 0 ? EMPTY_HEADLINES : Object.freeze(headlines);
  headlinesByConversation.set(conversationId, {
    transcript,
    turnKey,
    headlines: frozen,
  });
  return frozen;
}

/** Pick the agent with the newest `lastActivityAt` (ties → pubkey order). */
export function pickMostRecentlyActiveAgent(
  agents: readonly ConversationAgentActivity[],
): ConversationAgentActivity | null {
  const first = agents[0];
  if (!first) return null;
  let best = first;
  for (let i = 1; i < agents.length; i += 1) {
    const candidate = agents[i];
    if (!candidate) continue;
    if (
      candidate.lastActivityAt > best.lastActivityAt ||
      (candidate.lastActivityAt === best.lastActivityAt &&
        candidate.agentPubkey.localeCompare(best.agentPubkey) < 0)
    ) {
      best = candidate;
    }
  }
  return best;
}

const liveAgentsCache = new Map<
  string,
  { value: readonly ConversationAgentActivity[] }
>();

/**
 * Live agents in one conversation with per-agent activity + turn ids.
 * Reference-stable while the live turn contents are unchanged. The active-turn
 * store updates `lastSeenAt` in place for ordinary observer activity, so
 * this deliberately walks live turns on every snapshot rather than trusting
 * the store generation alone.
 */
export function getLiveAgentsForConversation(
  conversationId: string | null | undefined,
): readonly ConversationAgentActivity[] {
  if (!conversationId) return EMPTY_AGENTS;

  const byAgent = new Map<
    string,
    { lastActivityAt: number; turnIds: string[] }
  >();
  walkActiveAgentTurns((agentKey, turn) => {
    if (turn.conversationId !== conversationId) return;
    const prior = byAgent.get(agentKey);
    if (!prior) {
      byAgent.set(agentKey, {
        lastActivityAt: turn.lastSeenAt,
        turnIds: [turn.turnId],
      });
      return;
    }
    prior.turnIds.push(turn.turnId);
    if (turn.lastSeenAt > prior.lastActivityAt) {
      prior.lastActivityAt = turn.lastSeenAt;
    }
  });

  const nextValue =
    byAgent.size === 0
      ? EMPTY_AGENTS
      : [...byAgent.entries()]
          .map(([agentPubkey, entry]) => ({
            agentPubkey,
            lastActivityAt: entry.lastActivityAt,
            turnIds: entry.turnIds.sort(),
          }))
          .sort((a, b) => a.agentPubkey.localeCompare(b.agentPubkey));

  const cached = liveAgentsCache.get(conversationId);
  if (cached && liveAgentsEqual(cached.value, nextValue)) return cached.value;
  liveAgentsCache.set(conversationId, { value: nextValue });
  return nextValue;
}

function liveAgentsEqual(
  a: readonly ConversationAgentActivity[],
  b: readonly ConversationAgentActivity[],
): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const left = a[i];
    const right = b[i];
    if (!left || !right) return false;
    if (
      left.agentPubkey !== right.agentPubkey ||
      left.lastActivityAt !== right.lastActivityAt ||
      left.turnIds.length !== right.turnIds.length
    ) {
      return false;
    }
    for (let j = 0; j < left.turnIds.length; j += 1) {
      if (left.turnIds[j] !== right.turnIds[j]) return false;
    }
  }
  return true;
}

export function shortAgentDisplayName(
  pubkey: string,
  profiles: UserProfileLookup | undefined,
): string {
  const profile = profiles?.[normalizePubkey(pubkey)];
  const full = profile?.displayName ?? profile?.name ?? truncatePubkey(pubkey);
  const first = full.trim().split(/\s+/)[0];
  return first && first.length > 0 ? first : full;
}

/**
 * Pure selection for the thread-row headline: most-recent agent, conversation
 * spine scan, optional short-name prefix when multiple agents are live.
 */
export function selectConversationActivityHeadline(
  transcript: readonly TranscriptItem[],
  conversationId: string | null | undefined,
  agents: readonly ConversationAgentActivity[],
  profiles: UserProfileLookup | undefined,
): ConversationActivityHeadlineSelection | null {
  if (!conversationId) return null;
  const mostRecent = pickMostRecentlyActiveAgent(agents);
  if (!mostRecent) return null;

  const turnIds = new Set(mostRecent.turnIds);
  // Also accept any other live turn ids in this conversation so items tagged
  // only by conversationId still match when the chosen agent authored them.
  for (const agent of agents) {
    for (const turnId of agent.turnIds) turnIds.add(turnId);
  }

  const headlines = collectConversationActivityHeadlines(
    transcript,
    conversationId,
    turnIds,
  );
  const prefixAgentName = agents.length > 1;
  const agentShortName = shortAgentDisplayName(
    mostRecent.agentPubkey,
    profiles,
  );
  const displayHeadlines =
    prefixAgentName && headlines.length > 0
      ? Object.freeze(headlines.map((h) => `${agentShortName} · ${h}`))
      : headlines;
  const latest =
    displayHeadlines.length > 0
      ? (displayHeadlines[displayHeadlines.length - 1] ?? null)
      : null;

  const prev = selectionByConversation.get(conversationId);
  if (
    prev &&
    prev.transcript === transcript &&
    prev.agentPubkey === mostRecent.agentPubkey &&
    prev.prefixAgentName === prefixAgentName &&
    prev.agentShortName === agentShortName &&
    arraysShallowEqual(prev.value.headlines, displayHeadlines)
  ) {
    return prev.value;
  }

  const value: ConversationActivityHeadlineSelection = {
    agentPubkey: mostRecent.agentPubkey,
    headlines: displayHeadlines,
    latest,
    prefixAgentName,
    agentShortName,
  };
  selectionByConversation.set(conversationId, {
    transcript,
    agentPubkey: mostRecent.agentPubkey,
    prefixAgentName,
    agentShortName,
    value,
  });
  return value;
}

/** Test helper — drop module caches between cases. */
export function resetThreadAgentActivityHeadlineCaches() {
  headlinesByConversation.clear();
  selectionByConversation.clear();
  liveAgentsCache.clear();
}

/** Shared ~3s rotation tick — one interval for every mounted running headline. */
let sharedRotationTick = 0;
const sharedRotationListeners = new Set<() => void>();
let sharedRotationInterval: ReturnType<typeof setInterval> | null = null;

function subscribeSharedHeadlineRotation(listener: () => void) {
  sharedRotationListeners.add(listener);
  if (sharedRotationListeners.size === 1) {
    sharedRotationInterval = setInterval(() => {
      sharedRotationTick += 1;
      for (const notify of sharedRotationListeners) notify();
    }, HEADLINE_ROTATION_MS);
  }
  return () => {
    sharedRotationListeners.delete(listener);
    if (sharedRotationListeners.size === 0 && sharedRotationInterval) {
      clearInterval(sharedRotationInterval);
      sharedRotationInterval = null;
    }
  };
}

function getSharedHeadlineRotationSnapshot() {
  return sharedRotationTick;
}

function usePrefersReducedMotion(): boolean {
  const subscribe = React.useCallback((onChange: () => void) => {
    if (typeof window === "undefined" || !window.matchMedia) {
      return () => {};
    }
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handler = () => onChange();
    media.addEventListener("change", handler);
    return () => media.removeEventListener("change", handler);
  }, []);
  const getSnapshot = React.useCallback(() => {
    if (typeof window === "undefined" || !window.matchMedia) return false;
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  }, []);
  return React.useSyncExternalStore(subscribe, getSnapshot, () => false);
}

function useSharedHeadlineRotationWhen(enabled: boolean): number {
  const subscribe = React.useCallback(
    (listener: () => void) => {
      if (!enabled) return () => {};
      return subscribeSharedHeadlineRotation(listener);
    },
    [enabled],
  );
  return React.useSyncExternalStore(
    subscribe,
    getSharedHeadlineRotationSnapshot,
    getSharedHeadlineRotationSnapshot,
  );
}

/**
 * Conversation-scoped rotating headline for a running thread chip.
 * Subscribes to the observer transcript only while `enabled` (running).
 */
export function useThreadAgentActivityHeadline(
  conversationId: string | null | undefined,
  enabled: boolean,
  profiles: UserProfileLookup | undefined,
): ConversationActivityHeadlineSelection | null {
  const agentsSubscribe = React.useCallback(
    (onStoreChange: () => void) => {
      if (!enabled) return () => {};
      return subscribeActiveAgentTurns(onStoreChange);
    },
    [enabled],
  );
  const getAgentsSnapshot = React.useCallback(
    () =>
      enabled ? getLiveAgentsForConversation(conversationId) : EMPTY_AGENTS,
    [conversationId, enabled],
  );
  const agents = React.useSyncExternalStore(
    agentsSubscribe,
    getAgentsSnapshot,
    getAgentsSnapshot,
  );

  const mostRecent = enabled ? pickMostRecentlyActiveAgent(agents) : null;
  const agentPubkey = mostRecent?.agentPubkey ?? null;

  const transcriptSubscribe = React.useCallback(
    (onStoreChange: () => void) => {
      if (!enabled || !agentPubkey) return () => {};
      return subscribeAgentObserverProjection(agentPubkey, onStoreChange);
    },
    [agentPubkey, enabled],
  );
  const getTranscriptSnapshot = React.useCallback(
    () =>
      enabled && agentPubkey
        ? getAgentTranscript(agentPubkey, true)
        : EMPTY_TRANSCRIPT,
    [agentPubkey, enabled],
  );
  const transcript = React.useSyncExternalStore(
    transcriptSubscribe,
    getTranscriptSnapshot,
    getTranscriptSnapshot,
  );

  if (!enabled || !conversationId || agents.length === 0) return null;
  return selectConversationActivityHeadline(
    transcript,
    conversationId,
    agents,
    profiles,
  );
}

const EMPTY_TRANSCRIPT: TranscriptItem[] = [];

export function useRotatingActivityHeadline(
  headlines: readonly string[],
  latest: string | null,
): string | null {
  const reducedMotion = usePrefersReducedMotion();
  const shouldRotate = !reducedMotion && headlines.length > 1;
  const tick = useSharedHeadlineRotationWhen(shouldRotate);

  if (headlines.length === 0) return null;
  if (!shouldRotate) return latest;
  return headlines[tick % headlines.length] ?? latest;
}
