import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, it, mock } from "node:test";

import {
  CONVERSATION_OUTCOME_TTL_MS,
  getConversationOutcomeEntry,
  resetActiveAgentTurnsStore,
  restoreActiveAgentTurnsForCommunity,
  saveActiveAgentTurnsForCommunity,
  subscribeActiveAgentTurns,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";
import { getRecentOutcomeForConversation } from "./recentConversationOutcomes.ts";

const AGENT =
  "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const AGENT_2 =
  "bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222";

const EPOCH = Date.parse("2026-07-31T00:00:00.000Z");
const PRUNE_INTERVAL_MS = 5_000;

function makeEvent(overrides) {
  return {
    seq: 1,
    timestamp: "2026-07-31T00:00:00.000Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "chan-1",
    conversationId: "conv-1",
    sessionId: "sess-1",
    turnId: "turn-1",
    payload: null,
    ...overrides,
  };
}

describe("conversation outcome ledger", () => {
  beforeEach(() => {
    resetActiveAgentTurnsStore();
  });

  afterEach(() => {
    mock.timers.reset();
  });

  it("records completed on turn_completed", () => {
    mock.timers.enable({ apis: ["Date"], now: EPOCH });
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started" }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    const entry = getConversationOutcomeEntry("conv-1");
    assert.ok(entry);
    assert.equal(entry.outcome, "completed");
    assert.equal(entry.agentPubkey, AGENT);
    assert.equal(entry.channelId, "chan-1");
    assert.equal(entry.endedAt, EPOCH);
  });

  it("records error on turn_error and agent_panic", () => {
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
      makeEvent({
        seq: 2,
        kind: "turn_error",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    assert.equal(getConversationOutcomeEntry("conv-1")?.outcome, "error");

    resetActiveAgentTurnsStore();
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t2" }),
      makeEvent({
        seq: 2,
        kind: "agent_panic",
        turnId: "t2",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    assert.equal(getConversationOutcomeEntry("conv-1")?.outcome, "error");
  });

  it("retains recovery targets when liveness arrives before the delayed start", () => {
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({
        seq: 2,
        kind: "turn_liveness",
        turnId: "t1",
        timestamp: "2026-07-31T00:00:02.000Z",
        startedAt: "2026-07-31T00:00:00.000Z",
      }),
    ]);
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({
        seq: 1,
        kind: "turn_started",
        turnId: "t1",
        payload: { triggeringEventIds: ["msg-1"] },
      }),
      makeEvent({
        seq: 3,
        kind: "turn_error",
        turnId: "t1",
        timestamp: "2026-07-31T00:00:03.000Z",
      }),
    ]);

    assert.deepEqual(getConversationOutcomeEntry("conv-1")?.failedEventIds, [
      "msg-1",
    ]);
  });

  it("clears outcome when a new turn_started arrives for the conversation", () => {
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    assert.ok(getConversationOutcomeEntry("conv-1"));

    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({
        seq: 3,
        kind: "turn_started",
        turnId: "t2",
        timestamp: "2026-07-31T00:02:00.000Z",
      }),
    ]);
    assert.equal(getConversationOutcomeEntry("conv-1"), null);
  });

  it("keeps only the latest terminal event per conversation", () => {
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    syncAgentTurnsFromEvents(AGENT_2, [
      makeEvent({
        seq: 1,
        kind: "turn_started",
        turnId: "t2",
        timestamp: "2026-07-31T00:02:00.000Z",
      }),
      makeEvent({
        seq: 2,
        kind: "turn_error",
        turnId: "t2",
        timestamp: "2026-07-31T00:03:00.000Z",
      }),
    ]);
    const entry = getConversationOutcomeEntry("conv-1");
    assert.equal(entry?.outcome, "error");
    assert.equal(entry?.agentPubkey, AGENT_2);
  });

  it("suppresses recent outcome while any agent turn is still active", () => {
    // Both agents start first — a post-completion turn_started would clear the
    // ledger (by design). Suppression covers the multi-agent overlap case.
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
    ]);
    syncAgentTurnsFromEvents(AGENT_2, [
      makeEvent({
        seq: 1,
        kind: "turn_started",
        turnId: "t2",
        timestamp: "2026-07-31T00:00:30.000Z",
      }),
    ]);
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    assert.ok(getConversationOutcomeEntry("conv-1"));
    assert.equal(getRecentOutcomeForConversation("conv-1"), null);
  });

  it("exposes recent outcome after the last active turn ends", () => {
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    const recent = getRecentOutcomeForConversation("conv-1");
    assert.ok(recent);
    assert.equal(recent.outcome, "completed");
    assert.equal(recent.agentPubkey, AGENT);
  });

  it("returns a stable reference until the generation bumps", () => {
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    const first = getRecentOutcomeForConversation("conv-1");
    const second = getRecentOutcomeForConversation("conv-1");
    assert.equal(first, second);

    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({
        seq: 3,
        kind: "turn_started",
        turnId: "t2",
        timestamp: "2026-07-31T00:02:00.000Z",
      }),
    ]);
    assert.equal(getRecentOutcomeForConversation("conv-1"), null);
  });

  it("prunes outcomes past the TTL via the prune interval", () => {
    mock.timers.enable({ apis: ["setInterval", "Date"], now: EPOCH });
    // Subscribe so the prune interval starts.
    const unsub = subscribeActiveAgentTurns(() => {});

    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    assert.ok(getConversationOutcomeEntry("conv-1"));

    mock.timers.tick(CONVERSATION_OUTCOME_TTL_MS + PRUNE_INTERVAL_MS);
    assert.equal(getConversationOutcomeEntry("conv-1"), null);
    assert.equal(getRecentOutcomeForConversation("conv-1"), null);

    unsub();
  });

  it("save/restore preserves outcomes across community switch", () => {
    syncAgentTurnsFromEvents(AGENT, [
      makeEvent({ seq: 1, kind: "turn_started", turnId: "t1" }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t1",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
    ]);
    const before = getConversationOutcomeEntry("conv-1");
    assert.ok(before);

    saveActiveAgentTurnsForCommunity("ws-a");
    resetActiveAgentTurnsStore();
    assert.equal(getConversationOutcomeEntry("conv-1"), null);

    restoreActiveAgentTurnsForCommunity("ws-a");
    const after = getConversationOutcomeEntry("conv-1");
    assert.deepEqual(after, before);
  });
});
