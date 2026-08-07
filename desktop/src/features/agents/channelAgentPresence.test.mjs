import assert from "node:assert/strict";
import { beforeEach, describe, it } from "node:test";

import { deriveChannelAgentPresence } from "./channelAgentPresence.ts";
import {
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";
import {
  ingestApprovalRequest,
  ingestUserInputRequest,
  resetNeedsYouStore,
} from "./needsYouStore.ts";
import {
  clearConversationOutcomeLedger,
  recordConversationOutcome,
} from "./conversationOutcomeLedger.ts";

const AGENT = "a".repeat(64);
const AGENT_2 = "b".repeat(64);
const NOW = Date.parse("2026-08-07T12:00:00Z");
const roster = (pubkeys) => pubkeys.map((agentPubkey) => ({ agentPubkey }));

function turnEvent(overrides = {}) {
  return {
    seq: 1,
    timestamp: new Date(NOW - 10_000).toISOString(),
    kind: "turn_started",
    agentIndex: 0,
    channelId: "channel-1",
    sessionId: "session-1",
    turnId: "turn-1",
    conversationId: "conversation-1",
    payload: null,
    ...overrides,
  };
}

describe("channelAgentPresence", () => {
  beforeEach(() => {
    resetActiveAgentTurnsStore();
    resetNeedsYouStore();
    clearConversationOutcomeLedger();
  });

  it("resolves needs-you before working for the same agent", () => {
    syncAgentTurnsFromEvents(AGENT, [turnEvent()]);
    ingestApprovalRequest({
      id: "approval-1",
      channelId: "channel-1",
      rootEventId: "root-1",
      conversationId: "conversation-1",
      agentPubkey: AGENT,
      createdAt: NOW - 2_000,
    });

    const [presence] = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT]),
      NOW,
    );
    assert.deepEqual(presence, {
      agentPubkey: AGENT,
      state: "needs-you",
      conversationId: "conversation-1",
      since: NOW - 2_000,
    });
  });

  it("surfaces a 46040 user-input request as needs-you", () => {
    ingestUserInputRequest({
      id: "user-input-1",
      channelId: "channel-1",
      rootEventId: "root-1",
      conversationId: "conversation-1",
      agentPubkey: AGENT,
      createdAt: NOW - 2_000,
    });
    const [presence] = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT]),
      NOW,
    );
    assert.equal(presence.state, "needs-you");
    assert.equal(presence.conversationId, "conversation-1");
  });

  it("expires done-recent entries at the outcome window", () => {
    recordConversationOutcome("conversation-1", {
      outcome: "completed",
      agentPubkey: AGENT,
      channelId: "channel-1",
      endedAt: NOW - 1_000,
    });
    const recent = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT]),
      NOW,
    );
    assert.equal(recent[0].state, "done-recent");

    const expired = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT]),
      NOW + 4 * 60 * 60 * 1_000 + 1,
    );
    assert.equal(expired[0].state, "idle");
    assert.equal(expired[0].conversationId, null);
  });

  it("reuses the result reference when an unrelated store bump changes nothing", () => {
    const agents = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT]),
      NOW,
    );
    syncAgentTurnsFromEvents(AGENT_2, [
      turnEvent({
        seq: 1,
        channelId: "other-channel",
        conversationId: "other-conversation",
      }),
    ]);
    const unchanged = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT]),
      NOW,
    );
    assert.strictEqual(unchanged, agents);
  });

  it("adds and removes agents when the channel roster changes", () => {
    const initial = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT]),
      NOW,
    );
    assert.deepEqual(
      initial.map(({ agentPubkey }) => agentPubkey),
      [AGENT],
    );

    const expanded = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT, AGENT_2]),
      NOW,
    );
    assert.deepEqual(
      expanded.map(({ agentPubkey }) => agentPubkey),
      [AGENT, AGENT_2],
    );

    const reduced = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT_2]),
      NOW,
    );
    assert.deepEqual(
      reduced.map(({ agentPubkey }) => agentPubkey),
      [AGENT_2],
    );
  });

  it("orders done-recent and idle states beyond active states", () => {
    recordConversationOutcome("done-conversation", {
      outcome: "completed",
      agentPubkey: AGENT_2,
      channelId: "channel-1",
      endedAt: NOW - 1_000,
    });
    const entries = deriveChannelAgentPresence(
      "channel-1",
      roster([AGENT, AGENT_2, "c".repeat(64)]),
      NOW,
    );
    assert.deepEqual(
      entries.map(({ state }) => state),
      ["idle", "done-recent", "idle"],
    );
  });
});
