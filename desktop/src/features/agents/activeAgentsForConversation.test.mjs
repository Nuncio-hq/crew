import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  getActiveAgentsForConversation,
  getActiveTurnsByConversation,
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";

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
