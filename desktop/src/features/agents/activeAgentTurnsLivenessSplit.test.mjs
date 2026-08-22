import assert from "node:assert/strict";
import { describe, it, beforeEach, afterEach, mock } from "node:test";

import { subscribeAgentLiveness } from "./activeAgentTurnsLiveness.ts";
import * as store from "./activeAgentTurnsStore.ts";
import { getActiveTurnsByConversation } from "./activeConversationTurns.ts";

const AGENT_A =
  "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
const AGENT_B =
  "dcba4321dcba4321dcba4321dcba4321dcba4321dcba4321dcba4321dcba4321";

const EPOCH = Date.parse("2024-01-01T00:00:00Z");

function makeEvent(overrides) {
  return {
    seq: 1,
    timestamp: "2024-01-01T00:00:00Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "chan-1",
    sessionId: "sess-1",
    turnId: "turn-1",
    payload: null,
    ...overrides,
  };
}

describe("liveness/membership split (issue #286)", () => {
  /** Unsubscribers registered per test; run even when an assertion throws. */
  let cleanups = [];
  const onCleanup = (unsubscribe) => {
    cleanups.push(unsubscribe);
    return unsubscribe;
  };

  beforeEach(() => {
    cleanups = [];
    // Freeze the clock so clock-offset samples never tighten mid-test (a
    // tightening offset legitimately bumps the generation).
    mock.timers.enable({ apis: ["Date"], now: EPOCH + 1_000 });
    store.resetActiveAgentTurnsStore();
  });

  afterEach(() => {
    for (const cleanup of cleanups) cleanup();
    mock.timers.reset();
  });

  it("liveness-only frames do not bump the generation or wake global listeners; only that agent's liveness listeners fire", () => {
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({ seq: 1, turnId: "t-a", channelId: "c1" }),
    ]);
    store.syncAgentTurnsFromEvents(AGENT_B, [
      makeEvent({ seq: 1, turnId: "t-b", channelId: "c2" }),
    ]);

    let globalNotifies = 0;
    onCleanup(
      store.subscribeActiveAgentTurns(() => {
        globalNotifies += 1;
      }),
    );
    assert.equal(
      typeof subscribeAgentLiveness,
      "function",
      "subscribeAgentLiveness(agentPubkey, listener) must be exposed",
    );
    let agentANotifies = 0;
    let agentBNotifies = 0;
    onCleanup(
      subscribeAgentLiveness(AGENT_A, () => {
        agentANotifies += 1;
      }),
    );
    onCleanup(
      subscribeAgentLiveness(AGENT_B, () => {
        agentBNotifies += 1;
      }),
    );

    const generationBefore = store.getActiveTurnsGeneration();
    const FRAMES = 5;
    for (let i = 0; i < FRAMES; i += 1) {
      store.syncAgentTurnsFromEvents(AGENT_A, [
        makeEvent({
          seq: 2 + i,
          kind: "turn_liveness",
          turnId: "t-a",
          channelId: "c1",
        }),
      ]);
    }

    assert.equal(
      store.getActiveTurnsGeneration(),
      generationBefore,
      "liveness-only frames must not bump the global active-turns generation",
    );
    assert.equal(
      globalNotifies,
      0,
      "liveness-only frames must not wake subscribeActiveAgentTurns listeners",
    );
    assert.equal(
      agentANotifies,
      FRAMES,
      "the streaming agent's liveness listeners must fire per frame",
    );
    assert.equal(
      agentBNotifies,
      0,
      "another agent's liveness listeners must never fire",
    );
  });

  it("a liveness frame leaves getActiveTurnsByChannel() reference-identical", () => {
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({ seq: 1, turnId: "t-a", channelId: "c1" }),
    ]);
    const before = store.getActiveTurnsByChannel();

    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 2,
        kind: "turn_liveness",
        turnId: "t-a",
        channelId: "c1",
      }),
    ]);

    assert.equal(
      store.getActiveTurnsByChannel(),
      before,
      "channel summaries must keep the prior reference across liveness frames",
    );
  });

  it("turn start, terminal end, and resurrection still bump the generation and notify global listeners", () => {
    let globalNotifies = 0;
    onCleanup(
      store.subscribeActiveAgentTurns(() => {
        globalNotifies += 1;
      }),
    );

    let generation = store.getActiveTurnsGeneration();
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({ seq: 1, turnId: "t-a", channelId: "c1" }),
    ]);
    assert.ok(
      store.getActiveTurnsGeneration() > generation,
      "turn start must bump the generation",
    );
    assert.ok(globalNotifies > 0, "turn start must notify global listeners");

    generation = store.getActiveTurnsGeneration();
    const notifiesBeforeEnd = globalNotifies;
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t-a",
        channelId: "c1",
      }),
    ]);
    assert.ok(
      store.getActiveTurnsGeneration() > generation,
      "terminal end must bump the generation",
    );
    assert.ok(
      globalNotifies > notifiesBeforeEnd,
      "terminal end must notify global listeners",
    );
    assert.equal(store.getActiveTurnsForAgent(AGENT_A).length, 0);

    // Resurrection: a liveness frame for an untracked, non-tombstoned turn
    // recreates it — that is a membership change and must stay global.
    generation = store.getActiveTurnsGeneration();
    const notifiesBeforeResurrect = globalNotifies;
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 3,
        kind: "turn_liveness",
        turnId: "t-resurrected",
        channelId: "c3",
        timestamp: "2024-01-01T00:00:01Z",
      }),
    ]);
    assert.equal(store.getActiveTurnsForAgent(AGENT_A).length, 1);
    assert.ok(
      store.getActiveTurnsGeneration() > generation,
      "resurrection must bump the generation",
    );
    assert.ok(
      globalNotifies > notifiesBeforeResurrect,
      "resurrection must notify global listeners",
    );
  });

  it("channel-summary rebuilds reuse the prior reference when content is unchanged, and produce a new one when it changes", () => {
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 1,
        turnId: "t-a",
        channelId: "c1",
        conversationId: "conv-1",
      }),
    ]);
    const before = store.getActiveTurnsByChannel();
    const agentsBefore = store.getActiveAgentsForConversation("conv-1");

    // Membership churn for another agent that nets out to identical content.
    store.syncAgentTurnsFromEvents(AGENT_B, [
      makeEvent({
        seq: 1,
        turnId: "t-b",
        channelId: "c2",
        conversationId: "conv-2",
      }),
      makeEvent({
        seq: 2,
        kind: "turn_completed",
        turnId: "t-b",
        channelId: "c2",
        conversationId: "conv-2",
      }),
    ]);

    assert.equal(
      store.getActiveTurnsByChannel(),
      before,
      "a rebuild with structurally-equal content must return the prior reference",
    );
    assert.equal(
      store.getActiveAgentsForConversation("conv-1"),
      agentsBefore,
      "agents-by-conversation must return the prior reference when unchanged",
    );

    // Content actually changes → a new reference with the new channel.
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 2,
        turnId: "t-a2",
        channelId: "c9",
        conversationId: "conv-1",
        timestamp: "2024-01-01T00:00:01Z",
      }),
    ]);
    const after = store.getActiveTurnsByChannel();
    assert.notEqual(
      after,
      before,
      "changed content must produce a new snapshot reference",
    );
    assert.deepEqual(
      after.map((summary) => summary.channelId),
      ["c1", "c9"],
    );
  });

  it("liveness frames advance the data version so conversation projections re-read fresh lastSeenAt", () => {
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 1,
        turnId: "t-a",
        channelId: "c1",
        conversationId: "conv-1",
      }),
    ]);
    const before = getActiveTurnsByConversation();
    const seenBefore = before[0]?.lastSeenAt;
    const generationBefore = store.getActiveTurnsGeneration();

    mock.timers.tick(5_000);
    store.syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 2,
        kind: "turn_liveness",
        turnId: "t-a",
        channelId: "c1",
        conversationId: "conv-1",
      }),
    ]);

    assert.equal(
      store.getActiveTurnsGeneration(),
      generationBefore,
      "the liveness frame must not bump the global generation",
    );
    const after = getActiveTurnsByConversation();
    assert.ok(
      (after[0]?.lastSeenAt ?? 0) > (seenBefore ?? Number.POSITIVE_INFINITY),
      "a re-read after the liveness frame must see the advanced lastSeenAt",
    );
  });
});
