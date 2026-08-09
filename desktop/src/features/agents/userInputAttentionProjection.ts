import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import {
  clearUserInputRequests,
  getPendingUserInputRequest,
  ingestUserInputRequest,
  reconcileUserInputRequestAuthority,
  resolveUserInputRequest,
} from "@/features/agents/needsYouStore";
import {
  deriveUserInputRootEventId,
  parseUserInputRequest,
  parseUserInputResolution,
} from "@/features/channels/lib/userInput";
import type { RelayEvent } from "@/shared/api/types";
import {
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_RESOLVED,
} from "@/shared/constants/kinds";
import { normalizePubkey } from "@/shared/lib/pubkey";

function singleTag(event: RelayEvent, name: string): string | null {
  const tags = event.tags.filter((tag) => tag[0] === name);
  if (tags.length !== 1) return null;
  const tag = tags[0];
  const value = tag?.[1];
  return tag?.length === 2 && value && value === value.trim() ? value : null;
}

const LOWER_HEX_64 = /^[0-9a-f]{64}$/;

function hasCanonicalPubkeyTags(event: RelayEvent): boolean {
  return event.tags
    .filter((tag) => tag[0] === "p")
    .every((tag) => tag.length >= 2 && LOWER_HEX_64.test(tag[1] ?? ""));
}

function hasCanonicalThreadTags(event: RelayEvent): boolean {
  return event.tags
    .filter((tag) => tag[0] === "e")
    .every(
      (tag) =>
        tag.length === 4 &&
        LOWER_HEX_64.test(tag[1] ?? "") &&
        (tag[3] === "root" || tag[3] === "reply"),
    );
}

function ownedAgent(pubkey: string, ownedAgentPubkeys: ReadonlySet<string>) {
  return LOWER_HEX_64.test(pubkey) && ownedAgentPubkeys.has(pubkey);
}

function validAnswerContent(content: string): boolean {
  try {
    const value = JSON.parse(content);
    return Boolean(value && typeof value === "object" && !Array.isArray(value));
  } catch {
    return false;
  }
}

function canonicalThread(event: RelayEvent): {
  rootId: string | null;
  parentId: string | null;
} | null {
  const eventTags = event.tags.filter((tag) => tag[0] === "e");
  if (eventTags.length === 0) return { rootId: null, parentId: null };
  if (eventTags.length > 2) return null;
  let rootId: string | null = null;
  let parentId: string | null = null;
  for (const tag of eventTags) {
    const id = tag[1] ?? "";
    if (tag.length !== 4 || !LOWER_HEX_64.test(id)) return null;
    if (tag[3] === "root" && rootId === null) rootId = id;
    else if (tag[3] === "reply" && parentId === null) parentId = id;
    else return null;
  }
  if (!parentId && rootId) parentId = rootId;
  if (!rootId && parentId) rootId = parentId;
  return { rootId, parentId };
}

const MAX_PENDING_USER_INPUT_TRANSITIONS = 1_000;
const MAX_PENDING_TRANSITIONS_PER_REQUEST = 8;
const pendingTransitionsByRequestId = new Map<string, RelayEvent[]>();
let projectionUnavailable = false;
let exhaustiveProjection = false;

function validatesRequestTrigger(
  event: RelayEvent,
  parentEvent: RelayEvent | null | undefined,
  channelId: string,
  agentPubkey: string,
  rootEventId: string,
  ancestryById: ReadonlyMap<string, RelayEvent> | undefined,
): boolean {
  if (
    !LOWER_HEX_64.test(parentEvent?.id ?? "") ||
    !LOWER_HEX_64.test(parentEvent?.pubkey ?? "") ||
    !parentEvent ||
    !hasCanonicalPubkeyTags(parentEvent) ||
    !hasCanonicalThreadTags(parentEvent)
  ) {
    return false;
  }
  const requestThread = canonicalThread(event);
  if (!requestThread) return false;
  const parentId = requestThread.parentId ?? requestThread.rootId;
  if (!parentId || parentEvent.id !== parentId) return false;
  const parentChannels = parentEvent.tags.filter((tag) => tag[0] === "h");
  if (parentChannels.length !== 1 || parentChannels[0]?.[1] !== channelId) {
    return false;
  }
  let targetsAgent = false;
  for (const tag of parentEvent.tags) {
    if (tag[0] !== "p") continue;
    const target = tag[1] ?? "";
    if (!/^[0-9a-f]{64}$/.test(target)) return false;
    targetsAgent ||= target === agentPubkey;
  }
  if (!targetsAgent) return false;

  const visited = new Set<string>();
  let current = parentEvent;
  for (let depth = 0; depth < 256; depth += 1) {
    if (visited.has(current.id)) return false;
    visited.add(current.id);
    if (
      !LOWER_HEX_64.test(current.id) ||
      !LOWER_HEX_64.test(current.pubkey) ||
      !hasCanonicalPubkeyTags(current) ||
      !hasCanonicalThreadTags(current)
    ) {
      return false;
    }
    if (singleTag(current, "h") !== channelId) return false;
    const thread = canonicalThread(current);
    if (!thread) return false;
    if ((thread.rootId ?? current.id) !== rootEventId) return false;
    if (current.id === rootEventId) return thread.parentId === null;
    const ancestorId = thread.parentId ?? thread.rootId;
    if (!ancestorId) return false;
    const ancestor = ancestryById?.get(ancestorId);
    if (!ancestor) return false;
    current = ancestor;
  }
  return false;
}

function retainPendingTransition(requestId: string, event: RelayEvent): void {
  if (projectionUnavailable) return;
  const candidates = pendingTransitionsByRequestId.get(requestId) ?? [];
  if (candidates.some((candidate) => candidate.id === event.id)) return;
  if (candidates.length >= MAX_PENDING_TRANSITIONS_PER_REQUEST) {
    pendingTransitionsByRequestId.clear();
    projectionUnavailable = true;
    clearUserInputRequests();
    return;
  }
  pendingTransitionsByRequestId.set(requestId, [...candidates, event]);
  if (pendingTransitionsByRequestId.size > MAX_PENDING_USER_INPUT_TRANSITIONS) {
    pendingTransitionsByRequestId.clear();
    projectionUnavailable = true;
    clearUserInputRequests();
  }
}

export function beginExhaustiveUserInputProjection(): void {
  pendingTransitionsByRequestId.clear();
  projectionUnavailable = false;
  exhaustiveProjection = true;
  clearUserInputRequests();
}

export function markUserInputAttentionProjectionUnavailable(): void {
  pendingTransitionsByRequestId.clear();
  projectionUnavailable = true;
  exhaustiveProjection = false;
  clearUserInputRequests();
}

export function endExhaustiveUserInputProjection(): void {
  exhaustiveProjection = false;
}

export function isUserInputAttentionProjectionUnavailable(): boolean {
  return projectionUnavailable;
}

export type AuthorizedUserInputRequest = {
  id: string;
  channelId: string;
  agentPubkey: string;
  rootEventId: string;
  createdAt: number;
};

export function reconcileAuthorizedUserInputRequests(
  currentPubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
): boolean {
  return reconcileUserInputRequestAuthority(
    normalizePubkey(currentPubkey),
    ownedAgentPubkeys,
  );
}

export function validateAuthorizedUserInputRequest(
  event: RelayEvent,
  currentPubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
  parentEvent?: RelayEvent | null,
  ancestryById?: ReadonlyMap<string, RelayEvent>,
): AuthorizedUserInputRequest | null {
  const current = normalizePubkey(currentPubkey);
  const channelId = singleTag(event, "h");
  const request = parseUserInputRequest(event);
  const owner = singleTag(event, "p");
  const rootEventId = deriveUserInputRootEventId(event);
  if (
    !current ||
    !LOWER_HEX_64.test(event.id) ||
    !LOWER_HEX_64.test(event.pubkey) ||
    !hasCanonicalPubkeyTags(event) ||
    !hasCanonicalThreadTags(event) ||
    !channelId ||
    !request ||
    !rootEventId ||
    request.channel_id !== channelId ||
    owner !== current ||
    !ownedAgent(event.pubkey, ownedAgentPubkeys) ||
    !validatesRequestTrigger(
      event,
      parentEvent,
      channelId,
      event.pubkey,
      rootEventId,
      ancestryById,
    )
  ) {
    return null;
  }
  return {
    id: event.id,
    channelId,
    agentPubkey: event.pubkey,
    rootEventId,
    createdAt: event.created_at,
  };
}

export function validateAuthorizedUserInputTransition(
  event: RelayEvent,
  request: AuthorizedUserInputRequest,
  currentPubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
): boolean {
  if (
    !LOWER_HEX_64.test(event.id) ||
    !LOWER_HEX_64.test(event.pubkey) ||
    !hasCanonicalPubkeyTags(event) ||
    event.created_at < request.createdAt ||
    singleTag(event, "e") !== request.id ||
    singleTag(event, "h") !== request.channelId ||
    !ownedAgent(request.agentPubkey, ownedAgentPubkeys)
  ) {
    return false;
  }
  if (event.kind === KIND_AGENT_USER_INPUT_ANSWER) {
    return (
      singleTag(event, "p") === request.agentPubkey &&
      event.pubkey === normalizePubkey(currentPubkey) &&
      validAnswerContent(event.content)
    );
  }
  if (event.kind === KIND_AGENT_USER_INPUT_RESOLVED) {
    const resolved = parseUserInputResolution(event);
    return (
      event.pubkey === request.agentPubkey &&
      singleTag(event, "p") === normalizePubkey(currentPubkey) &&
      resolved?.request_event_id === request.id
    );
  }
  return false;
}

/** Apply only user-input facts whose signed signer→owner→request chain is valid. */
export function projectAuthorizedUserInputEvent(
  event: RelayEvent,
  fallbackChannelId: string,
  currentPubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
  parentEvent?: RelayEvent | null,
  ancestryById?: ReadonlyMap<string, RelayEvent>,
): boolean {
  if (projectionUnavailable) return false;
  const current = normalizePubkey(currentPubkey);
  const channelId = singleTag(event, "h");
  if (
    !current ||
    !channelId ||
    (fallbackChannelId && fallbackChannelId !== channelId)
  ) {
    return false;
  }

  if (event.kind === KIND_AGENT_USER_INPUT_REQUESTED) {
    const request = validateAuthorizedUserInputRequest(
      event,
      current,
      ownedAgentPubkeys,
      parentEvent,
      ancestryById,
    );
    if (!request || request.channelId !== channelId) return false;
    const conversationId = deriveAgentConversationIdOrNull(
      request.channelId,
      request.rootEventId,
    );
    if (!conversationId) return false;
    const ingested = ingestUserInputRequest({
      id: request.id,
      channelId: request.channelId,
      rootEventId: request.rootEventId,
      conversationId,
      agentPubkey: request.agentPubkey,
      ownerPubkey: current,
      createdAt: event.created_at * 1_000,
    });
    const pending = pendingTransitionsByRequestId.get(request.id);
    const authorizedTerminal = pending?.find(
      (transition) =>
        transition.kind === KIND_AGENT_USER_INPUT_RESOLVED &&
        validateAuthorizedUserInputTransition(
          transition,
          request,
          current,
          ownedAgentPubkeys,
        ),
    );
    if (exhaustiveProjection) pendingTransitionsByRequestId.delete(request.id);
    else if (authorizedTerminal)
      pendingTransitionsByRequestId.set(request.id, [authorizedTerminal]);
    if (authorizedTerminal) {
      resolveUserInputRequest(request.id);
    }
    return Boolean(ingested);
  }

  const requestId = singleTag(event, "e");
  const request = requestId ? getPendingUserInputRequest(requestId) : null;
  if (!requestId) return false;
  if (!request) {
    if (
      event.kind === KIND_AGENT_USER_INPUT_RESOLVED &&
      ownedAgent(event.pubkey, ownedAgentPubkeys) &&
      singleTag(event, "p") === current &&
      parseUserInputResolution(event)?.request_event_id === requestId
    ) {
      if (!exhaustiveProjection) retainPendingTransition(requestId, event);
    }
    return false;
  }
  if (request.channelId !== channelId) return false;
  if (
    !validateAuthorizedUserInputTransition(
      event,
      {
        id: request.id,
        channelId: request.channelId,
        agentPubkey: request.agentPubkey,
        rootEventId: request.rootEventId,
        createdAt: Math.floor(request.createdAt / 1_000),
      },
      current,
      ownedAgentPubkeys,
    )
  ) {
    return false;
  }
  // An owner answer (46041) means input was sent, but the requesting agent may
  // still fail before consuming it. Only the producer's terminal 46042 closes
  // the authoritative Needs You lifecycle.
  if (event.kind === KIND_AGENT_USER_INPUT_RESOLVED) {
    resolveUserInputRequest(requestId);
    if (!exhaustiveProjection) retainPendingTransition(requestId, event);
  }
  return true;
}

export function resetUserInputAttentionProjection(): void {
  pendingTransitionsByRequestId.clear();
  projectionUnavailable = false;
  exhaustiveProjection = false;
}

export function _testPendingUserInputTransitionCount(): number {
  return pendingTransitionsByRequestId.size;
}
