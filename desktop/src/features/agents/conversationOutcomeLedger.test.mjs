import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  clearConversationOutcomeLedger,
  getConversationOutcomeEntry,
  recordConversationOutcome,
} from "./conversationOutcomeLedger.ts";

beforeEach(() => {
  clearConversationOutcomeLedger();
});

function outcome(overrides = {}) {
  return {
    outcome: "completed",
    agentPubkey: "agent-b",
    channelId: "channel",
    endedAt: 3_000,
    terminalAt: 3_000,
    terminalOrderKey: "agent-b|session|turn|0000000003",
    triggeringEventIds: ["trigger-b"],
    ...overrides,
  };
}

test("a delayed older terminal cannot roll a conversation outcome backward", () => {
  recordConversationOutcome("conversation", outcome());
  recordConversationOutcome(
    "conversation",
    outcome({
      agentPubkey: "agent-a",
      endedAt: 4_000,
      terminalAt: 2_000,
      terminalOrderKey: "agent-a|session|turn|0000000009",
      triggeringEventIds: ["trigger-a"],
    }),
  );

  assert.equal(
    getConversationOutcomeEntry("conversation")?.agentPubkey,
    "agent-b",
  );
  assert.deepEqual(
    getConversationOutcomeEntry("conversation")?.triggeringEventIds,
    ["trigger-b"],
  );
});

test("equal terminal timestamps use a stable producer key, not arrival order", () => {
  recordConversationOutcome(
    "conversation",
    outcome({ terminalOrderKey: "z-producer" }),
  );
  recordConversationOutcome(
    "conversation",
    outcome({ agentPubkey: "agent-a", terminalOrderKey: "a-producer" }),
  );

  assert.equal(
    getConversationOutcomeEntry("conversation")?.terminalOrderKey,
    "z-producer",
  );
});
