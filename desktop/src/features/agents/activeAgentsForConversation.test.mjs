import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  getActiveAgentsForConversation,
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";

function started(conversationId, turnId) {
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
  };
}

beforeEach(resetActiveAgentTurnsStore);

test("conversation snapshot separates parallel threads in one channel", () => {
  syncAgentTurnsFromEvents("agent-a", [started("thread-a", "turn-a")]);
  syncAgentTurnsFromEvents("agent-b", [started("thread-b", "turn-b")]);

  assert.deepEqual(getActiveAgentsForConversation("thread-a"), ["agent-a"]);
  assert.deepEqual(getActiveAgentsForConversation("thread-b"), ["agent-b"]);
});
