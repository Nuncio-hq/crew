import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  buildSignedConversationOutcome,
  clearConversationOutcomeLedger,
  getConversationOutcomeEntry,
  recordConversationOutcome,
  conversationOutcomeTerminalOrderKey,
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

test("authoritative terminal evidence replaces newer inferred lost-contact", () => {
  recordConversationOutcome(
    "conversation",
    outcome({
      outcome: "lost-contact",
      endedAt: 5_000,
      terminalAt: 5_000,
      terminalOrderKey: "local-prune",
    }),
  );
  recordConversationOutcome(
    "conversation",
    outcome({
      outcome: "completed",
      endedAt: 6_000,
      terminalAt: 3_000,
      terminalOrderKey: "signed-terminal",
    }),
  );

  assert.equal(
    getConversationOutcomeEntry("conversation")?.outcome,
    "completed",
  );
});

test("same-session producer sequence orders sibling terminals before agent identity", () => {
  const event = (seq, agentIndex) => ({
    kind: "turn_completed",
    seq,
    sessionId: "shared-session",
    agentIndex,
    timestamp: "1970-01-01T00:00:03.000Z",
  });
  const earlierKey = conversationOutcomeTerminalOrderKey(
    "f".repeat(64),
    event(10, "z-agent"),
    "turn",
  );
  const laterKey = conversationOutcomeTerminalOrderKey(
    "0".repeat(64),
    event(11, "a-agent"),
    "turn",
  );

  assert.ok(laterKey > earlierKey);
});

test("same-session terminal sequence overrides a producer clock rollback", () => {
  const producer = {
    agentPubkey: "agent",
    terminalAgentIndex: 0,
    terminalSessionId: "session",
    terminalTurnId: "turn",
  };
  assert.equal(
    recordConversationOutcome(
      "conversation",
      outcome({
        ...producer,
        terminalAt: 2_000,
        terminalOrderKey: "completed",
        terminalSeq: 41,
      }),
    ),
    true,
  );
  assert.equal(
    recordConversationOutcome(
      "conversation",
      outcome({
        ...producer,
        outcome: "error",
        terminalAt: 1_000,
        terminalOrderKey: "error",
        terminalSeq: 42,
      }),
    ),
    true,
  );
  assert.equal(getConversationOutcomeEntry("conversation")?.outcome, "error");
});

test("a newer run replaces only its matching agent-trigger obligation", () => {
  const pair = (agentPubkey, eventId, sessionId, turnId) => ({
    agentPubkey,
    eventId,
    sessionId,
    turnId,
  });
  recordConversationOutcome(
    "conversation",
    outcome({
      agentTriggerPairs: [
        pair("agent-a", "trigger-a", "session-a1", "turn-a1"),
        pair("agent-b", "trigger-b", "session-b", "turn-b"),
      ],
      terminalAt: 1_000,
      terminalOrderKey: "first",
    }),
  );
  recordConversationOutcome(
    "conversation",
    outcome({
      agentPubkey: "agent-a",
      triggeringEventIds: ["trigger-a"],
      agentTriggerPairs: [
        pair("agent-a", "trigger-a", "session-a2", "turn-a2"),
      ],
      terminalAt: 2_000,
      terminalOrderKey: "second",
    }),
  );

  assert.deepEqual(
    getConversationOutcomeEntry("conversation")?.agentTriggerPairs,
    [
      pair("agent-a", "trigger-a", "session-a2", "turn-a2"),
      pair("agent-b", "trigger-b", "session-b", "turn-b"),
    ],
  );
});

test("a sibling completion cannot erase another sibling failure", () => {
  recordConversationOutcome(
    "conversation",
    outcome({
      outcome: "error",
      agentPubkey: "agent-a",
      triggeringEventIds: ["trigger-a"],
      failedEventIds: ["trigger-a"],
      terminalAt: 1_000,
      terminalOrderKey: "failure-a",
    }),
  );
  recordConversationOutcome(
    "conversation",
    outcome({
      agentPubkey: "agent-b",
      triggeringEventIds: ["trigger-b"],
      agentTriggerPairs: [
        {
          agentPubkey: "agent-b",
          eventId: "trigger-b",
          sessionId: "session-b",
          turnId: "turn-b",
        },
      ],
      terminalAt: 2_000,
      terminalOrderKey: "completed-b",
    }),
  );
  const entry = getConversationOutcomeEntry("conversation");
  assert.equal(entry?.outcome, "error");
  assert.deepEqual(entry?.failedEventIds, ["trigger-a"]);
  assert.equal(entry?.agentTriggerPairs?.[0]?.eventId, "trigger-b");
});

test("a delayed sibling failure cannot be rejected by a newer sibling completion", () => {
  recordConversationOutcome(
    "conversation",
    outcome({
      agentPubkey: "agent-b",
      triggeringEventIds: ["trigger-b"],
      agentTriggerPairs: [
        {
          agentPubkey: "agent-b",
          eventId: "trigger-b",
          sessionId: "session-b",
          turnId: "turn-b",
        },
      ],
      terminalAt: 2_000,
      terminalOrderKey: "completed-b",
    }),
  );
  assert.equal(
    recordConversationOutcome(
      "conversation",
      outcome({
        outcome: "error",
        agentPubkey: "agent-a",
        triggeringEventIds: ["trigger-a"],
        failedEventIds: ["trigger-a"],
        terminalAt: 1_000,
        terminalOrderKey: "failure-a",
      }),
    ),
    true,
  );
  const entry = getConversationOutcomeEntry("conversation");
  assert.equal(entry?.outcome, "error");
  assert.deepEqual(entry?.failedEventIds, ["trigger-a"]);
  assert.equal(entry?.agentTriggerPairs?.[0]?.eventId, "trigger-b");
});

test("a sessionless completion frame fails closed", () => {
  const entry = buildSignedConversationOutcome({
    agentKey: "agent-a",
    event: {
      seq: 2,
      timestamp: "2026-08-10T00:00:00.000Z",
      kind: "turn_completed",
      agentIndex: 0,
      channelId: "channel",
      conversationId: "conversation",
      sessionId: null,
      turnId: "turn-a",
      payload: { triggeringEventIds: ["trigger-a"] },
      replayed: false,
    },
    resolvedTurnId: "turn-a",
    sessionId: null,
    channelId: "channel",
    endedAt: 1_000,
    terminalAt: 1_000,
    triggeringEventIds: ["trigger-a"],
  });
  assert.equal(entry.outcome, "error");
  assert.equal(entry.agentTriggerPairs?.length ?? 0, 0);
});
