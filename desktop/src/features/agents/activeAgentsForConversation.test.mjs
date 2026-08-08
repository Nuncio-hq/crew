import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  getActiveAgentsForConversation,
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";
import { getActiveTurnsByConversation } from "./activeConversationTurns.ts";
import { getActiveTurnSummariesForConversation } from "./activeConversationAgentTurnSummaries.ts";

function started(conversationId, turnId, overrides = {}) {
  return {
    seq: 1,
    timestamp: "2026-07-31T00:00:00.000Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "channel-a",
    conversationId,
    sessionId: null,
    turnId,
    payload: {},
    ...overrides,
  };
}

beforeEach(resetActiveAgentTurnsStore);

test("conversation snapshot separates parallel threads in one channel", () => {
  syncAgentTurnsFromEvents("agent-a", [started("thread-a", "turn-a")]);
  syncAgentTurnsFromEvents("agent-b", [started("thread-b", "turn-b")]);

  assert.deepEqual(getActiveAgentsForConversation("thread-a"), ["agent-a"]);
  assert.deepEqual(getActiveAgentsForConversation("thread-b"), ["agent-b"]);
});

test("getActiveTurnsByConversation groups multiple agents in one thread", () => {
  syncAgentTurnsFromEvents("agent-a", [started("thread-a", "turn-a")]);
  syncAgentTurnsFromEvents("agent-b", [
    started("thread-a", "turn-b", {
      seq: 2,
      timestamp: "2026-07-31T00:01:00.000Z",
    }),
  ]);
  syncAgentTurnsFromEvents("agent-c", [started("thread-b", "turn-c")]);

  const summaries = getActiveTurnsByConversation();
  assert.deepEqual(
    summaries.map(({ conversationId, agentCount, agentPubkeys }) => ({
      conversationId,
      agentCount,
      agentPubkeys,
    })),
    [
      {
        conversationId: "thread-a",
        agentCount: 2,
        agentPubkeys: ["agent-a", "agent-b"],
      },
      {
        conversationId: "thread-b",
        agentCount: 1,
        agentPubkeys: ["agent-c"],
      },
    ],
  );
});

test("getActiveTurnsByConversation drops a thread when its last turn ends", () => {
  syncAgentTurnsFromEvents("agent-a", [
    started("thread-a", "turn-a"),
    {
      seq: 2,
      timestamp: "2026-07-31T00:02:00.000Z",
      kind: "turn_completed",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
  ]);

  assert.deepEqual(getActiveTurnsByConversation(), []);
});

test("getActiveTurnSummariesForConversation returns per-agent anchors", () => {
  syncAgentTurnsFromEvents("agent-a", [started("thread-a", "turn-a")]);
  syncAgentTurnsFromEvents("agent-b", [
    started("thread-a", "turn-b", {
      seq: 2,
      timestamp: "2026-07-31T00:01:00.000Z",
    }),
  ]);
  syncAgentTurnsFromEvents("agent-c", [started("thread-b", "turn-c")]);

  const summaries = getActiveTurnSummariesForConversation("thread-a");
  assert.equal(summaries.length, 2);
  assert.deepEqual(
    summaries.map((s) => s.agentPubkey),
    ["agent-a", "agent-b"],
  );
  assert.ok(Number.isFinite(summaries[0].anchorAt));
  assert.ok(Number.isFinite(summaries[1].anchorAt));

  const other = getActiveTurnSummariesForConversation("thread-b");
  assert.equal(other.length, 1);
  assert.equal(other[0].agentPubkey, "agent-c");
  assert.equal(typeof other[0].anchorAt, "number");

  assert.deepEqual(getActiveTurnSummariesForConversation("missing"), []);
  assert.deepEqual(getActiveTurnSummariesForConversation(null), []);
});

test("getActiveTurnSummariesForConversation is reference-stable until mutation", () => {
  syncAgentTurnsFromEvents("agent-a", [started("thread-a", "turn-a")]);
  const first = getActiveTurnSummariesForConversation("thread-a");
  const second = getActiveTurnSummariesForConversation("thread-a");
  assert.equal(first, second);

  syncAgentTurnsFromEvents("agent-b", [
    started("thread-a", "turn-b", {
      seq: 2,
      timestamp: "2026-07-31T00:01:00.000Z",
    }),
  ]);
  const third = getActiveTurnSummariesForConversation("thread-a");
  assert.notEqual(first, third);
  assert.equal(third.length, 2);
});

test("getActiveTurnSummariesForConversation cache invalidates on turn end", () => {
  syncAgentTurnsFromEvents("agent-a", [started("thread-a", "turn-a")]);
  const before = getActiveTurnSummariesForConversation("thread-a");
  assert.equal(before.length, 1);

  syncAgentTurnsFromEvents("agent-a", [
    {
      seq: 2,
      timestamp: "2026-07-31T00:02:00.000Z",
      kind: "turn_completed",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
  ]);
  const after = getActiveTurnSummariesForConversation("thread-a");
  assert.notEqual(before, after);
  assert.deepEqual(after, []);
});
