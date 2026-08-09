import { deriveAgentConversationIdOrNull } from "@/features/agents/conversationId";
import {
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
  return tags[0]?.[1]?.trim() || null;
}

function ownedAgent(
  pubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
): boolean {
  return ownedAgentPubkeys.has(normalizePubkey(pubkey));
}

function validAnswerContent(content: string): boolean {
  try {
    const value = JSON.parse(content);
    return Boolean(value && typeof value === "object" && !Array.isArray(value));
  } catch {
    return false;
  }
}

export type AuthorizedUserInputRequest = {
  id: string;
  channelId: string;
  agentPubkey: string;
  rootEventId: string;
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
): AuthorizedUserInputRequest | null {
  const current = normalizePubkey(currentPubkey);
  const channelId = singleTag(event, "h");
  const request = parseUserInputRequest(event);
  const owner = singleTag(event, "p");
  const rootEventId = deriveUserInputRootEventId(event);
  if (
    !current ||
    !channelId ||
    !request ||
    !rootEventId ||
    request.channel_id !== channelId ||
    normalizePubkey(owner ?? "") !== current ||
    !ownedAgent(event.pubkey, ownedAgentPubkeys)
  ) {
    return null;
  }
  return {
    id: event.id,
    channelId,
    agentPubkey: normalizePubkey(event.pubkey),
    rootEventId,
  };
}

export function validateAuthorizedUserInputTransition(
  event: RelayEvent,
  request: AuthorizedUserInputRequest,
  currentPubkey: string,
  ownedAgentPubkeys: ReadonlySet<string>,
): boolean {
  if (
    singleTag(event, "e") !== request.id ||
    singleTag(event, "h") !== request.channelId ||
    !ownedAgent(request.agentPubkey, ownedAgentPubkeys)
  ) {
    return false;
  }
  if (event.kind === KIND_AGENT_USER_INPUT_ANSWER) {
    const author = normalizePubkey(event.pubkey);
    return (
      normalizePubkey(singleTag(event, "p") ?? "") === request.agentPubkey &&
      author === normalizePubkey(currentPubkey) &&
      validAnswerContent(event.content)
    );
  }
  if (event.kind === KIND_AGENT_USER_INPUT_RESOLVED) {
    const resolved = parseUserInputResolution(event);
    return (
      normalizePubkey(event.pubkey) === request.agentPubkey &&
      normalizePubkey(singleTag(event, "p") ?? "") ===
        normalizePubkey(currentPubkey) &&
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
): boolean {
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
    );
    if (!request || request.channelId !== channelId) return false;
    const conversationId = deriveAgentConversationIdOrNull(
      request.channelId,
      request.rootEventId,
    );
    if (!conversationId) return false;
    ingestUserInputRequest({
      id: request.id,
      channelId: request.channelId,
      rootEventId: request.rootEventId,
      conversationId,
      agentPubkey: request.agentPubkey,
      ownerPubkey: current,
      createdAt: event.created_at * 1_000,
    });
    return true;
  }

  const requestId = singleTag(event, "e");
  const request = requestId ? getPendingUserInputRequest(requestId) : null;
  if (!requestId || !request || request.channelId !== channelId) return false;
  if (
    !validateAuthorizedUserInputTransition(
      event,
      {
        id: request.id,
        channelId: request.channelId,
        agentPubkey: request.agentPubkey,
        rootEventId: request.rootEventId,
      },
      current,
      ownedAgentPubkeys,
    )
  ) {
    return false;
  }
  resolveUserInputRequest(requestId);
  return true;
}
