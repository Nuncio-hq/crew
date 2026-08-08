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
const attentionDefaults = {
  connectionState: "open",
  ownedAgentPubkeys: new Set(["agent-1", "agent-2"]),
  receipts: [],
  snoozedUntilByConversation: new Map(),
};

function activeTurn(conversationId, overrides = {}) {
  return {
    conversationId,
    channelId: "channel-a",
    agentPubkeys: ["agent-1"],
    anchorAt: 1_000,
    lastSeenAt: 100_000,
    lastSubstantiveProgressAt: 100_000,
    progressKind: "progress",
    progressLabel: "Running tests",
    ...overrides,
  };
}

test("blocked conversations win over working", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
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

test("read-state acknowledgement does not review a durable receipt", () => {
  const input = {
    ...attentionDefaults,
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
    receipts: [
      {
        id: "receipt-2",
        channelId: "channel-b",
        conversationId: "conversation-2",
        agentPubkey: "agent-2",
        createdAt: 200,
        summary: "Completed successfully",
        verify: "pnpm check passed",
        reviewed: false,
      },
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
    1,
  );
});

test("receipt-only rows retain a direct message and thread target", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [],
    channels,
    inboxItems: [],
    needsYou: [],
    outcomes: [],
    receipts: [
      {
        id: "a".repeat(64),
        channelId: "channel-a",
        conversationId: "receipt-only",
        rootEventId: "b".repeat(64),
        agentPubkey: "agent-1",
        createdAt: 200,
        summary: "Completed successfully",
        verify: "pnpm check passed",
        reviewed: false,
      },
    ],
  });
  assert.deepEqual(getMissionInboxEventTarget(sections.readyToReview[0]), {
    messageId: "a".repeat(64),
    threadRootId: "b".repeat(64),
  });
});

test("non-owned receipts never enter the owner's review queue", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [],
    channels,
    inboxItems: [item("shared-agent", "channel-a", 100)],
    needsYou: [],
    outcomes: [],
    ownedAgentPubkeys: new Set(["agent-1"]),
    receipts: [
      {
        id: "shared-receipt",
        channelId: "channel-a",
        conversationId: "shared-agent",
        agentPubkey: "somebody-elses-agent",
        createdAt: 200,
        summary: "Completed successfully",
        verify: "pnpm check passed",
        reviewed: false,
      },
    ],
  });

  assert.equal(sections.readyToReview.length, 0);
});

test("observer completion alone is not ready to review", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [],
    channels,
    inboxItems: [item("completed", "channel-a", 100)],
    needsYou: [],
    outcomes: [
      [
        "completed",
        {
          outcome: "completed",
          channelId: "channel-a",
          agentPubkey: "agent-1",
          endedAt: 200,
        },
      ],
    ],
  });
  assert.equal(sections.readyToReview.length, 0);
});

test("attention exceptions outrank an unreviewed receipt", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [
      activeTurn("stalled", {
        lastSeenAt: 109_000,
        lastSubstantiveProgressAt: 10_000,
      }),
    ],
    channels,
    inboxItems: [item("stalled", "channel-a", 100)],
    needsYou: [],
    now: 110_000,
    outcomes: [],
    receipts: [
      {
        id: "receipt-stalled",
        channelId: "channel-a",
        conversationId: "stalled",
        agentPubkey: "agent-1",
        createdAt: 109_000,
        summary: "Completed successfully",
        verify: "pnpm check passed",
        reviewed: false,
      },
    ],
  });
  assert.equal(sections.needsYou[0]?.state, "possiblyStalled");
  assert.equal(sections.readyToReview.length, 0);
});

test("a newer active turn suppresses a stale receipt from the prior run", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [
      activeTurn("rerun", {
        anchorAt: 200_000,
        lastSeenAt: 200_000,
        lastSubstantiveProgressAt: 200_000,
      }),
    ],
    channels,
    inboxItems: [item("rerun", "channel-a", 100)],
    needsYou: [],
    now: 201_000,
    outcomes: [],
    receipts: [
      {
        id: "receipt-old-run",
        channelId: "channel-a",
        conversationId: "rerun",
        agentPubkey: "agent-1",
        createdAt: 100_000,
        summary: "Old run completed",
        verify: "old verification",
        reviewed: false,
      },
    ],
  });
  assert.equal(sections.readyToReview.length, 0);
  assert.equal(sections.working[0]?.conversationId, "rerun");
});

test("unavailable observer telemetry never masquerades as stalled", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [
      activeTurn("offline", {
        lastSeenAt: 109_000,
        lastSubstantiveProgressAt: 10_000,
      }),
    ],
    channels,
    connectionState: "error",
    inboxItems: [item("offline", "channel-a", 100)],
    needsYou: [],
    now: 110_000,
    outcomes: [],
  });
  assert.equal(sections.needsYou[0]?.state, "telemetryUnavailable");
});

test("observer failures are scoped to the affected conversation agents", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [
      activeTurn("healthy", {
        agentPubkeys: ["agent-1"],
        lastSeenAt: 109_000,
        lastSubstantiveProgressAt: 109_000,
      }),
      activeTurn("offline", {
        agentPubkeys: ["agent-2"],
        lastSeenAt: 109_000,
        lastSubstantiveProgressAt: 109_000,
      }),
    ],
    channels,
    connectionState: "error",
    connectionStateByAgent: new Map([
      ["agent-1", "open"],
      ["agent-2", "error"],
    ]),
    inboxItems: [
      item("healthy", "channel-a", 100),
      item("offline", "channel-a", 100),
    ],
    needsYou: [],
    now: 110_000,
    outcomes: [],
  });
  assert.equal(sections.working[0]?.conversationId, "healthy");
  assert.equal(
    sections.needsYou.find((row) => row.conversationId === "offline")?.state,
    "telemetryUnavailable",
  );
});

test("needs-you rows order newest request first", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
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
    ...attentionDefaults,
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
    ...attentionDefaults,
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
    ...attentionDefaults,
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
