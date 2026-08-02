import assert from "node:assert/strict";
import test from "node:test";

import {
  conversationHasStoppableWork,
  deriveEditAsUndoUiState,
  filterPendingToKnownAgents,
  getPendingAgentPubkeysForConversation,
  getPendingAgentRequestsForConversation,
  ingestObserverFrameForEditAsUndo,
  PENDING_AGENT_REQUEST_MAX_AGE_MS,
  prunePendingAgentRequests,
  recordMessageEditApplied,
  recordPendingAgentRequest,
  resetDispatchedEventIdsStore,
  getMessageEditAppliedResult,
} from "./dispatchedEventIds.ts";

const EVENT = "a".repeat(64);
const OTHER = "b".repeat(64);
const AGENT = "c".repeat(64);
const HUMAN = "d".repeat(64);
const CHANNEL = "11111111-1111-1111-1111-111111111111";
const CONVERSATION = "22222222-2222-2222-2222-222222222222";

test.beforeEach(() => {
  resetDispatchedEventIdsStore();
});

test("absent from turn_started → queued when message mentions an agent", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: null,
    }),
    "queued",
  );
});

test("present in turn_started → dispatched", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set([EVENT]),
      editResult: null,
    }),
    "dispatched",
  );
});

test("applied:false overrides a locally-queued derivation", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: { outcome: "patched", applied: false },
    }),
    "too-late",
  );
});

test("applied dropped → withdrawn", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: { outcome: "dropped", applied: true },
    }),
    "withdrawn",
  );
});

test("human-to-human messages never get undo framing", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: false,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: null,
    }),
    null,
  );
});

test("ingest message_edit_applied records outcome", () => {
  ingestObserverFrameForEditAsUndo({
    kind: "message_edit_applied",
    payload: {
      targetEventId: EVENT,
      outcome: "dropped",
      applied: true,
    },
  });
  assert.deepEqual(getMessageEditAppliedResult(EVENT), {
    outcome: "dropped",
    applied: true,
  });
  assert.equal(getMessageEditAppliedResult(OTHER), null);
});

test("record ignores non-hex ids", () => {
  recordMessageEditApplied("not-an-id", "patched", true);
  assert.equal(getMessageEditAppliedResult("not-an-id"), null);
});

test("queued request with no turn reports stoppable work", () => {
  const sentAt = Date.now();
  recordPendingAgentRequest({
    eventId: EVENT,
    channelId: CHANNEL,
    conversationId: CONVERSATION,
    agentPubkeys: [AGENT],
    sentAt,
  });
  assert.deepEqual(getPendingAgentPubkeysForConversation(CONVERSATION), [
    AGENT,
  ]);
  assert.equal(
    conversationHasStoppableWork({
      hasRunningTurn: false,
      pendingRequests: [
        {
          eventId: EVENT,
          channelId: CHANNEL,
          conversationId: CONVERSATION,
          agentPubkeys: [AGENT],
          sentAt,
        },
      ],
    }),
    true,
  );
});

test("running turn alone is stoppable even with empty pending", () => {
  assert.equal(
    conversationHasStoppableWork({
      hasRunningTurn: true,
      pendingRequests: [],
    }),
    true,
  );
});

test("pending clears once the event is in turn_started", () => {
  recordPendingAgentRequest({
    eventId: EVENT,
    channelId: CHANNEL,
    conversationId: CONVERSATION,
    agentPubkeys: [AGENT],
  });
  prunePendingAgentRequests(new Set([EVENT]));
  assert.deepEqual(getPendingAgentPubkeysForConversation(CONVERSATION), []);
});

test("pending pubkey list is reference-stable across unrelated edits", () => {
  recordPendingAgentRequest({
    eventId: EVENT,
    channelId: CHANNEL,
    conversationId: CONVERSATION,
    agentPubkeys: [AGENT],
  });
  const first = getPendingAgentPubkeysForConversation(CONVERSATION);
  const second = getPendingAgentPubkeysForConversation(CONVERSATION);
  assert.equal(first, second);
  recordMessageEditApplied(OTHER, "patched", true);
  const third = getPendingAgentPubkeysForConversation(CONVERSATION);
  assert.equal(first, third);
});

test("human mention never surfaces as stoppable even when known agents are populated", () => {
  // Identity source is useKnownAgentPubkeys (community agents), not observer
  // liveness — a full known set must still drop humans.
  const known = new Set([AGENT, "e".repeat(64)]);
  assert.deepEqual(filterPendingToKnownAgents([HUMAN], known), []);
  assert.deepEqual(filterPendingToKnownAgents([HUMAN, AGENT], known), [AGENT]);
});

test("empty known-agent set surfaces no pending stoppable agents", () => {
  assert.deepEqual(filterPendingToKnownAgents([HUMAN, AGENT], new Set()), []);
});

test("stale pending disappears from selectors without turn_started", () => {
  // Agent is known but never gets turn_started — age prune drops Stop.
  const sentAt = Date.now() - PENDING_AGENT_REQUEST_MAX_AGE_MS - 1;
  recordPendingAgentRequest({
    eventId: EVENT,
    channelId: CHANNEL,
    conversationId: CONVERSATION,
    agentPubkeys: [AGENT],
    sentAt,
  });
  assert.deepEqual(getPendingAgentRequestsForConversation(CONVERSATION), []);
  assert.deepEqual(getPendingAgentPubkeysForConversation(CONVERSATION), []);
  assert.deepEqual(
    filterPendingToKnownAgents(
      getPendingAgentPubkeysForConversation(CONVERSATION),
      new Set([AGENT]),
    ),
    [],
  );
});
