import assert from "node:assert/strict";
import test from "node:test";

import {
  deriveMissionInboxSections,
  getMissionInboxEventTarget,
} from "./missionInbox.ts";

function item(conversationId, channelId, createdAt, overrides = {}) {
  return {
    conversationId,
    channelLabel: overrides.channelLabel ?? channelId,
    id: overrides.id ?? `${conversationId}-event`,
    item: {
      channelId,
      createdAt,
      pubkey: overrides.agentPubkey ?? "agent-1",
      tags: [],
    },
    latestActivityAt: createdAt,
    preview: overrides.preview ?? "live text",
    subject: overrides.subject ?? `Thread ${conversationId}`,
    senderLabel: overrides.senderLabel ?? "Agent One",
    ...overrides,
  };
}

const channels = [
  { id: "channel-a", name: "alpha" },
  { id: "channel-b", name: "beta" },
];

test("blocked conversations win over working", () => {
  const sections = deriveMissionInboxSections({
    channels,
    inboxItems: [item("conversation-1", "channel-a", 100)],
    needsYou: [
      {
        conversationId: "conversation-1",
        channelId: "channel-a",
        agentPubkey: "agent-1",
        createdAt: 200,
        id: "approval-1",
      },
    ],
    activeTurns: [
      {
        conversationId: "conversation-1",
        channelId: "channel-a",
        agentPubkeys: ["agent-1"],
        anchorAt: 300,
      },
    ],
    outcomes: [],
    acknowledgedConversationIds: new Set(),
  });

  assert.equal(sections.needsYou[0].conversationId, "conversation-1");
  assert.equal(sections.working.length, 0);
});

test("read-state acknowledgement removes ready-to-review rows", () => {
  const input = {
    channels,
    inboxItems: [item("conversation-2", "channel-b", 100)],
    needsYou: [],
    activeTurns: [],
    outcomes: [
      [
        "conversation-2",
        {
          outcome: "completed",
          channelId: "channel-b",
          agentPubkey: "agent-2",
          endedAt: 200,
        },
      ],
    ],
  };

  assert.equal(
    deriveMissionInboxSections({
      ...input,
      acknowledgedConversationIds: new Set(),
    }).readyToReview.length,
    1,
  );
  assert.equal(
    deriveMissionInboxSections({
      ...input,
      acknowledgedConversationIds: new Set(["conversation-2"]),
    }).readyToReview.length,
    0,
  );
});

test("needs-you rows order newest request first", () => {
  const sections = deriveMissionInboxSections({
    channels,
    inboxItems: [item("old", "channel-a", 100), item("new", "channel-b", 101)],
    needsYou: [
      {
        conversationId: "old",
        channelId: "channel-a",
        agentPubkey: "a",
        createdAt: 10,
        id: "old-request",
      },
      {
        conversationId: "new",
        channelId: "channel-b",
        agentPubkey: "b",
        createdAt: 20,
        id: "new-request",
      },
    ],
    activeTurns: [],
    outcomes: [],
    acknowledgedConversationIds: new Set(),
  });

  assert.deepEqual(
    sections.needsYou.map((row) => row.conversationId),
    ["new", "old"],
  );
});

test("needs-you combines approval and user-input families by conversation", () => {
  const sections = deriveMissionInboxSections({
    channels,
    inboxItems: [
      item("approval-conversation", "channel-a", 100),
      item("input-conversation", "channel-b", 100),
    ],
    needsYou: [
      {
        conversationId: "approval-conversation",
        channelId: "channel-a",
        agentPubkey: "agent-a",
        createdAt: 200,
        id: "approval-request",
      },
      {
        conversationId: "input-conversation",
        channelId: "channel-b",
        agentPubkey: "agent-b",
        createdAt: 300,
        id: "user-input-request",
      },
    ],
    activeTurns: [],
    outcomes: [],
    acknowledgedConversationIds: new Set(),
  });

  assert.deepEqual(
    sections.needsYou.map((row) => [row.channelId, row.conversationId]),
    [
      ["channel-b", "input-conversation"],
      ["channel-a", "approval-conversation"],
    ],
  );
});

test("same inputs return a reference-stable snapshot", () => {
  const input = {
    channels,
    inboxItems: [item("conversation-3", "channel-a", 100)],
    needsYou: [],
    activeTurns: [],
    outcomes: [],
    acknowledgedConversationIds: new Set(),
  };

  assert.strictEqual(
    deriveMissionInboxSections(input),
    deriveMissionInboxSections(input),
  );
});

test("mission rows use real roots and never promote conversation UUIDs to event ids", () => {
  const root = "a".repeat(64);
  const [needsYouRow] = deriveMissionInboxSections({
    channels,
    inboxItems: [],
    needsYou: [
      {
        conversationId: "conversation-1",
        channelId: "channel-a",
        rootEventId: root,
        agentPubkey: "agent-1",
        createdAt: 200,
        id: "request-1",
      },
    ],
    activeTurns: [],
    outcomes: [],
    acknowledgedConversationIds: new Set(),
  }).needsYou;

  assert.deepEqual(getMissionInboxEventTarget(needsYouRow), {
    messageId: root,
    threadRootId: root,
  });
  assert.equal(
    getMissionInboxEventTarget({ ...needsYouRow, rootEventId: null }),
    null,
  );
});
