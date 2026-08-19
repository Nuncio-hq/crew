import * as React from "react";

import type { NeedsYouRequest } from "@/features/agents/needsYouStore";
import {
  getNeedsYouForAll,
  subscribeNeedsYou,
} from "@/features/agents/needsYouStore";
import {
  getActiveTurnsGeneration,
  subscribeActiveAgentTurns,
} from "@/features/agents/activeAgentTurnsStore";
import {
  getActiveTurnsByConversation,
  type ActiveConversationTurnSummary,
} from "@/features/agents/activeConversationTurns";
import {
  walkConversationOutcomes,
  type ConversationOutcomeEntry,
} from "@/features/agents/conversationOutcomeLedger";
import type { InboxItem } from "@/features/home/lib/inbox";
import type { Channel } from "@/shared/api/types";
import { getThreadReference } from "@/features/messages/lib/threading";
import { isVerifiedRelayEvent } from "@/shared/api/relayEventVerification";
import { getEventById } from "@/shared/api/tauri";
import {
  deriveAgentAttention,
  type AgentAttentionState,
} from "@/features/agents/agentAttention";
import type { AgentReceiptSummary } from "@/features/agents/agentReceiptStore";
import type { ConnectionState } from "@/features/agents/ui/agentSessionTypes";

export type MissionInboxState =
  | "needsYou"
  | "failed"
  | "lostContact"
  | "telemetryUnavailable"
  | "possiblyStalled"
  | "readyToReview"
  | "working";

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
  messageEventId: string | null;
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
  ownedAgentPubkeys: ReadonlySet<string>;
  receipts: readonly AgentReceiptSummary[];
  connectionState: ConnectionState;
  connectionStateByAgent?: ReadonlyMap<string, ConnectionState>;
  sleepingAgentPubkeys?: ReadonlySet<string>;
  snoozedUntilByConversation: ReadonlyMap<string, number>;
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

const CONNECTION_PRIORITY: Record<ConnectionState, number> = {
  error: 5,
  closed: 4,
  connecting: 3,
  idle: 2,
  open: 1,
};

function connectionStateForAgents(
  input: MissionInboxInput,
  agentPubkeys: readonly string[],
): ConnectionState {
  if (!input.connectionStateByAgent || agentPubkeys.length === 0) {
    return input.connectionState;
  }
  let selected: ConnectionState = "open";
  for (const pubkey of agentPubkeys) {
    const candidate =
      input.connectionStateByAgent.get(pubkey) ?? input.connectionState;
    if (CONNECTION_PRIORITY[candidate] > CONNECTION_PRIORITY[selected]) {
      selected = candidate;
    }
  }
  return selected;
}

function isSleepingAgent(input: MissionInboxInput, pubkey: string): boolean {
  return input.sleepingAgentPubkeys?.has(pubkey.toLowerCase()) ?? false;
}

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
  messageEventId = null,
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
  messageEventId?: string | null;
  channelName: string;
}): MissionInboxRow {
  return {
    age,
    agentPubkey,
    channelId,
    conversationId,
    inboxItem,
    messageEventId,
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

function missionStateForAttention(
  state: AgentAttentionState,
): MissionInboxState | null {
  switch (state) {
    case "failed":
      return "failed";
    case "lost-contact":
      return "lostContact";
    case "possibly-stalled":
      return "possiblyStalled";
    case "telemetry-unavailable":
      return "telemetryUnavailable";
    default:
      return null;
  }
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
  for (const [conversationId, entry] of input.outcomes) {
    if (
      entry.outcome === "completed" ||
      blocked.has(conversationId) ||
      !channelIds.has(entry.channelId) ||
      isSleepingAgent(input, entry.agentPubkey)
    ) {
      continue;
    }
    const item = itemByConversation.get(conversationId) ?? null;
    const connectionState = connectionStateForAgents(input, [
      entry.agentPubkey,
    ]);
    const telemetryUnavailable =
      entry.outcome === "lost-contact" && connectionState !== "open";
    needsYou.push(
      rowFor({
        age: now - entry.endedAt,
        agentPubkey: entry.agentPubkey || item?.item.pubkey || "",
        channelId: entry.channelId,
        channelName: channelNames.get(entry.channelId) ?? "",
        conversationId,
        inboxItem: item,
        rootEventId: getThreadReference(item?.item.tags ?? []).rootId ?? null,
        phaseOrHeadline:
          entry.outcome === "error"
            ? "Failed — retry from the thread"
            : telemetryUnavailable
              ? "Telemetry unavailable — reconnect observer"
              : "Lost contact — reconnect or retry",
        state:
          entry.outcome === "error"
            ? "failed"
            : telemetryUnavailable
              ? "telemetryUnavailable"
              : "lostContact",
      }),
    );
    blocked.add(conversationId);
  }

  const latestReceiptByConversation = new Map<string, AgentReceiptSummary>();
  const receiptsByConversation = new Map<string, AgentReceiptSummary[]>();
  for (const receipt of input.receipts) {
    if (!input.ownedAgentPubkeys.has(receipt.agentPubkey)) continue;
    const receipts = receiptsByConversation.get(receipt.conversationId) ?? [];
    receipts.push(receipt);
    receiptsByConversation.set(receipt.conversationId, receipts);
    const prior = latestReceiptByConversation.get(receipt.conversationId);
    if (
      !prior ||
      receipt.createdAt > prior.createdAt ||
      (receipt.createdAt === prior.createdAt && receipt.id > prior.id)
    ) {
      latestReceiptByConversation.set(receipt.conversationId, receipt);
    }
  }
  for (const receipts of receiptsByConversation.values()) {
    receipts.sort(
      (left, right) =>
        right.createdAt - left.createdAt || right.id.localeCompare(left.id),
    );
  }
  for (const [conversationId, outcome] of input.outcomes) {
    if (outcome.outcome !== "completed") continue;
    const exactPairs =
      outcome.agentTriggerPairs ??
      (outcome.sessionId && outcome.turnId
        ? (outcome.triggeringEventIds ?? []).map((eventId) => ({
            agentPubkey: outcome.agentPubkey,
            eventId,
            sessionId: outcome.sessionId ?? "",
            turnId: outcome.turnId ?? "",
          }))
        : []);
    const matchingReceipts = (
      receiptsByConversation.get(conversationId) ?? []
    ).filter((candidate) =>
      exactPairs.some(
        (pair) =>
          candidate.agentPubkey === pair.agentPubkey &&
          candidate.parentEventId === pair.eventId &&
          candidate.sessionId === pair.sessionId &&
          candidate.turnId === pair.turnId,
      ),
    );
    const receipt =
      exactPairs.length > 0 &&
      exactPairs.every((pair) =>
        matchingReceipts.some(
          (candidate) =>
            candidate.agentPubkey === pair.agentPubkey &&
            candidate.parentEventId === pair.eventId &&
            candidate.sessionId === pair.sessionId &&
            candidate.turnId === pair.turnId,
        ),
      )
        ? (matchingReceipts[0] ?? null)
        : null;
    if (receipt) {
      latestReceiptByConversation.set(conversationId, receipt);
    } else {
      latestReceiptByConversation.delete(conversationId);
    }
  }
  const calmTurns: MissionInboxInput["activeTurns"][number][] = [];
  for (const turn of input.activeTurns) {
    if (blocked.has(turn.conversationId) || !channelIds.has(turn.channelId)) {
      continue;
    }
    const awakeAgentPubkeys = turn.agentPubkeys.filter(
      (pubkey) => !isSleepingAgent(input, pubkey),
    );
    if (awakeAgentPubkeys.length === 0) continue;
    const awakeAgentPubkeySet = new Set(
      awakeAgentPubkeys.map((pubkey) => pubkey.toLowerCase()),
    );
    const exactPairs = (turn.agentTriggerPairs ?? []).filter((pair) =>
      awakeAgentPubkeySet.has(pair.agentPubkey.toLowerCase()),
    );
    const conversationReceipts =
      receiptsByConversation.get(turn.conversationId) ?? [];
    const matchingReceipts = conversationReceipts.filter((candidate) =>
      exactPairs.some(
        (pair) =>
          pair.agentPubkey === candidate.agentPubkey &&
          pair.eventId === candidate.parentEventId &&
          pair.sessionId === candidate.sessionId &&
          pair.turnId === candidate.turnId,
      ),
    );
    const receipt =
      exactPairs.length > 0 &&
      exactPairs.every((pair) =>
        matchingReceipts.some(
          (candidate) =>
            pair.agentPubkey === candidate.agentPubkey &&
            pair.eventId === candidate.parentEventId &&
            pair.sessionId === candidate.sessionId &&
            pair.turnId === candidate.turnId,
        ),
      )
        ? (matchingReceipts[0] ?? null)
        : null;
    if (receipt) {
      latestReceiptByConversation.set(turn.conversationId, receipt);
    } else {
      latestReceiptByConversation.delete(turn.conversationId);
    }
    const attention = deriveAgentAttention({
      connectionState: connectionStateForAgents(input, awakeAgentPubkeys),
      needsYou: false,
      now,
      outcome: null,
      receipt,
      snoozedUntil:
        input.snoozedUntilByConversation.get(turn.conversationId) ?? 0,
      turns: [
        {
          agentPubkey: awakeAgentPubkeys[0] ?? "",
          anchorAt: turn.anchorAt,
          lastSeenAt: turn.lastSeenAt,
          lastSubstantiveProgressAt: turn.lastSubstantiveProgressAt,
          progressKind: turn.progressKind,
          progressLabel: turn.progressLabel,
        },
      ],
    });
    const exceptionState = missionStateForAttention(attention.state);
    if (!exceptionState) {
      calmTurns.push(turn);
      continue;
    }
    const item = itemByConversation.get(turn.conversationId) ?? null;
    needsYou.push(
      rowFor({
        age:
          now -
          (exceptionState === "lostContact"
            ? turn.lastSeenAt
            : turn.lastSubstantiveProgressAt),
        agentPubkey: awakeAgentPubkeys[0] ?? item?.item.pubkey ?? "",
        channelId: turn.channelId,
        channelName: channelNames.get(turn.channelId) ?? "",
        conversationId: turn.conversationId,
        inboxItem: item,
        rootEventId: getThreadReference(item?.item.tags ?? []).rootId ?? null,
        phaseOrHeadline: attention.lastVerifiedLabel ?? "Agent activity",
        state: exceptionState,
      }),
    );
    blocked.add(turn.conversationId);
  }
  const readyToReview: MissionInboxRow[] = [];
  for (const [conversationId, receipt] of latestReceiptByConversation) {
    if (
      receipt.reviewed ||
      blocked.has(conversationId) ||
      !channelIds.has(receipt.channelId)
    ) {
      continue;
    }
    const item = itemByConversation.get(conversationId) ?? null;
    readyToReview.push(
      rowFor({
        age: now - receipt.createdAt,
        agentPubkey: receipt.agentPubkey || item?.item.pubkey || "",
        channelId: receipt.channelId,
        channelName: channelNames.get(receipt.channelId) ?? "",
        conversationId,
        inboxItem: item,
        messageEventId: receipt.id,
        rootEventId:
          receipt.rootEventId ??
          getThreadReference(item?.item.tags ?? []).rootId ??
          null,
        phaseOrHeadline: receipt.summary || "Ready for review",
        state: "readyToReview",
      }),
    );
    blocked.add(conversationId);
  }
  readyToReview.sort((left, right) => left.age - right.age);

  const working: MissionInboxRow[] = [];
  for (const turn of calmTurns) {
    if (blocked.has(turn.conversationId) || !channelIds.has(turn.channelId)) {
      continue;
    }
    const item = itemByConversation.get(turn.conversationId) ?? null;
    working.push(
      rowFor({
        age: now - turn.lastSubstantiveProgressAt,
        agentPubkey:
          turn.agentPubkeys.find((pubkey) => !isSleepingAgent(input, pubkey)) ??
          item?.item.pubkey ??
          "",
        channelId: turn.channelId,
        channelName: channelNames.get(turn.channelId) ?? "",
        conversationId: turn.conversationId,
        inboxItem: item,
        rootEventId: getThreadReference(item?.item.tags ?? []).rootId ?? null,
        phaseOrHeadline: turn.progressLabel,
        state: "working",
      }),
    );
  }
  needsYou.sort((left, right) => left.age - right.age);
  working.sort((left, right) => left.age - right.age);

  // Without an explicit clock input, ages stay fixed until another input changes.
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
    receipts: input.receipts,
    connectionState: input.connectionState,
    connectionStateByAgent: input.connectionStateByAgent
      ? [...input.connectionStateByAgent]
      : [],
    sleeping: [...(input.sleepingAgentPubkeys ?? [])].sort(),
    snoozed: [...input.snoozedUntilByConversation],
    now: input.now,
  });
  if (key === lastKey) return lastSections;
  lastKey = key;
  lastSections = { needsYou, readyToReview, working };
  return lastSections;
}

export type MissionInboxEventTarget = {
  channelId: string;
  messageId: string;
  parentEventId: string;
  threadRootId: string;
};

export async function getMissionInboxEventTarget(
  row: MissionInboxRow,
  fetchEvent: typeof getEventById = getEventById,
  verifyEvent: typeof isVerifiedRelayEvent = isVerifiedRelayEvent,
): Promise<MissionInboxEventTarget | null> {
  const messageId = row.messageEventId ?? row.inboxItem?.id ?? row.rootEventId;
  if (!messageId || !/^[0-9a-f]{64}$/i.test(messageId)) return null;
  let event: Awaited<ReturnType<typeof getEventById>>;
  try {
    event = await fetchEvent(messageId);
  } catch {
    return null;
  }
  if (!verifyEvent(event) || event.id !== messageId) return null;
  const channelId = event.tags.find((tag) => tag[0] === "h" && tag[1])?.[1];
  if (!channelId) return null;
  const thread = getThreadReference(event.tags);
  const threadRootId = thread.rootId ?? thread.parentId ?? messageId;
  if (row.channelId && row.channelId !== channelId) return null;
  if (row.rootEventId && row.rootEventId !== threadRootId) return null;
  return {
    channelId,
    messageId,
    parentEventId: thread.parentId ?? messageId,
    threadRootId,
  };
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
