import * as React from "react";

import type { NeedsYouRequest } from "@/features/agents/needsYouStore";
import {
  getNeedsYouForAll,
  subscribeNeedsYou,
} from "@/features/agents/needsYouStore";
import {
  getActiveTurnsByConversation,
  getActiveTurnsGeneration,
  subscribeActiveAgentTurns,
  type ActiveConversationTurnSummary,
} from "@/features/agents/activeAgentTurnsStore";
import {
  walkConversationOutcomes,
  type ConversationOutcomeEntry,
} from "@/features/agents/conversationOutcomeLedger";
import type { InboxItem } from "@/features/home/lib/inbox";
import type { Channel } from "@/shared/api/types";
import { getThreadReference } from "@/features/messages/lib/threading";

export type MissionInboxState = "needsYou" | "readyToReview" | "working";

export type MissionInboxRow = {
  conversationId: string;
  channelId: string;
  threadTitle: string;
  agentPubkey: string;
  state: MissionInboxState;
  phaseOrHeadline: string;
  age: number;
  inboxItem: InboxItem | null;
  rootEventId: string | null;
};

export type MissionInboxSections = {
  needsYou: MissionInboxRow[];
  readyToReview: MissionInboxRow[];
  working: MissionInboxRow[];
};

type MissionInboxInput = {
  channels: readonly Pick<Channel, "id" | "name">[];
  inboxItems: readonly InboxItem[];
  needsYou: readonly NeedsYouRequest[];
  activeTurns: readonly ActiveConversationTurnSummary[];
  outcomes: readonly (readonly [string, ConversationOutcomeEntry])[];
  acknowledgedConversationIds: ReadonlySet<string>;
  now?: number;
};

const EMPTY_SECTIONS: MissionInboxSections = {
  needsYou: [],
  readyToReview: [],
  working: [],
};
let lastKey = "";
let lastSections = EMPTY_SECTIONS;
let outcomeCacheGeneration = -1;
let outcomeCache: [string, ConversationOutcomeEntry][] = [];

function latestRequest(requests: readonly NeedsYouRequest[]) {
  return requests.reduce<NeedsYouRequest | null>(
    (latest, request) =>
      latest === null || request.createdAt > latest.createdAt
        ? request
        : latest,
    null,
  );
}

function rowFor({
  conversationId,
  channelId,
  state,
  agentPubkey,
  age,
  phaseOrHeadline,
  inboxItem,
  rootEventId,
  channelName,
}: {
  conversationId: string;
  channelId: string;
  state: MissionInboxState;
  agentPubkey: string;
  age: number;
  phaseOrHeadline: string;
  inboxItem: InboxItem | null;
  rootEventId: string | null;
  channelName: string;
}): MissionInboxRow {
  return {
    age,
    agentPubkey,
    channelId,
    conversationId,
    inboxItem,
    rootEventId,
    phaseOrHeadline,
    state,
    threadTitle:
      inboxItem?.subject ||
      (channelName
        ? `Conversation in #${channelName}`
        : `Conversation ${conversationId.slice(0, 8)}`),
  };
}

/** Derive the three mission-control sections from the shared agent stores. */
export function deriveMissionInboxSections(
  input: MissionInboxInput,
): MissionInboxSections {
  const now = input.now ?? Date.now();
  const channelIds = new Set(input.channels.map((channel) => channel.id));
  const channelNames = new Map(
    input.channels.map((channel) => [channel.id, channel.name]),
  );
  const itemByConversation = new Map(
    input.inboxItems.map((item) => [item.conversationId, item]),
  );
  const requestsByConversation = new Map<string, NeedsYouRequest[]>();
  for (const request of input.needsYou) {
    if (!channelIds.has(request.channelId)) continue;
    const requests = requestsByConversation.get(request.conversationId) ?? [];
    requests.push(request);
    requestsByConversation.set(request.conversationId, requests);
  }

  const needsYou = [...requestsByConversation.entries()]
    .map(([conversationId, requests]) => {
      const request = latestRequest(requests);
      const item = itemByConversation.get(conversationId) ?? null;
      return rowFor({
        age: now - (request?.createdAt ?? 0),
        agentPubkey: request?.agentPubkey ?? item?.item.pubkey ?? "",
        channelId: request?.channelId ?? item?.item.channelId ?? "",
        channelName:
          channelNames.get(request?.channelId ?? item?.item.channelId ?? "") ??
          "",
        conversationId,
        inboxItem: item,
        rootEventId:
          request?.rootEventId ??
          getThreadReference(item?.item.tags ?? []).rootId ??
          null,
        phaseOrHeadline: item?.preview || "Waiting for your approval",
        state: "needsYou",
      });
    })
    .sort((left, right) => left.age - right.age);

  const blocked = new Set(needsYou.map((row) => row.conversationId));
  const readyToReview: MissionInboxRow[] = [];
  for (const [conversationId, entry] of input.outcomes) {
    if (
      entry.outcome !== "completed" ||
      blocked.has(conversationId) ||
      input.acknowledgedConversationIds.has(conversationId)
    ) {
      continue;
    }
    const item = itemByConversation.get(conversationId) ?? null;
    if (!channelIds.has(entry.channelId)) continue;
    readyToReview.push(
      rowFor({
        age: now - entry.endedAt,
        agentPubkey: entry.agentPubkey || item?.item.pubkey || "",
        channelId: entry.channelId,
        channelName: channelNames.get(entry.channelId) ?? "",
        conversationId,
        inboxItem: item,
        rootEventId: getThreadReference(item?.item.tags ?? []).rootId ?? null,
        phaseOrHeadline: item?.preview || "Completed successfully",
        state: "readyToReview",
      }),
    );
  }
  readyToReview.sort((left, right) => left.age - right.age);

  const working = input.activeTurns
    .filter(
      (turn) =>
        !blocked.has(turn.conversationId) && channelIds.has(turn.channelId),
    )
    .map((turn) => {
      const item = itemByConversation.get(turn.conversationId) ?? null;
      return rowFor({
        age: now - turn.anchorAt,
        agentPubkey: turn.agentPubkeys[0] ?? item?.item.pubkey ?? "",
        channelId: turn.channelId,
        channelName: channelNames.get(turn.channelId) ?? "",
        conversationId: turn.conversationId,
        inboxItem: item,
        rootEventId: getThreadReference(item?.item.tags ?? []).rootId ?? null,
        phaseOrHeadline: item?.preview || "Agent is working",
        state: "working",
      });
    })
    .sort((left, right) => left.age - right.age);

  const key = JSON.stringify({
    acknowledged: [...input.acknowledgedConversationIds].sort(),
    active: input.activeTurns,
    items: input.inboxItems.map((item) => [
      item.conversationId,
      item.id,
      item.latestActivityAt,
    ]),
    needs: input.needsYou.map((request) => [
      request.id,
      request.conversationId,
      request.createdAt,
    ]),
    outcomes: input.outcomes,
    now,
  });
  if (key === lastKey) return lastSections;
  lastKey = key;
  lastSections = { needsYou, readyToReview, working };
  return lastSections;
}

export function getMissionInboxEventTarget(row: MissionInboxRow) {
  const messageId = row.inboxItem?.id ?? row.rootEventId;
  if (!messageId || !/^[0-9a-f]{64}$/i.test(messageId)) return null;
  const itemRootId = row.inboxItem
    ? getThreadReference(row.inboxItem.item.tags).rootId
    : null;
  return { messageId, threadRootId: itemRootId ?? row.rootEventId };
}

export function getMissionInboxOutcomes(): [
  string,
  ConversationOutcomeEntry,
][] {
  const generation = getActiveTurnsGeneration();
  if (generation === outcomeCacheGeneration) return outcomeCache;
  const outcomes: [string, ConversationOutcomeEntry][] = [];
  walkConversationOutcomes((conversationId, entry) =>
    outcomes.push([conversationId, entry]),
  );
  outcomeCacheGeneration = generation;
  outcomeCache = outcomes;
  return outcomeCache;
}

export function useMissionInboxNeedsYou() {
  return React.useSyncExternalStore(
    subscribeNeedsYou,
    getNeedsYouForAll,
    getNeedsYouForAll,
  );
}

export function useMissionInboxOutcomes() {
  return React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getMissionInboxOutcomes,
    getMissionInboxOutcomes,
  );
}

export function useMissionInboxActiveTurns() {
  return React.useSyncExternalStore(
    subscribeActiveAgentTurns,
    getActiveTurnsByConversation,
    getActiveTurnsByConversation,
  );
}

export function resetMissionInboxCache() {
  lastKey = "";
  lastSections = EMPTY_SECTIONS;
  outcomeCacheGeneration = -1;
  outcomeCache = [];
}
