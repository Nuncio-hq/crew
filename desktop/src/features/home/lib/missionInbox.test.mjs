import assert from "node:assert/strict";
import test from "node:test";

import {
  deriveMissionInboxSections as deriveTrustedMissionInboxSections,
  getMissionInboxEventTarget,
} from "./missionInbox.ts";

function deriveMissionInboxSections(input) {
  return deriveTrustedMissionInboxSections({
    ...input,
    activeTurns: input.activeTurns.map((turn) => ({
      ...turn,
      agentTriggerPairs: turn.agentTriggerPairs?.map((pair) => ({
        ...pair,
        sessionId: pair.sessionId ?? "session",
        turnId: pair.turnId ?? "turn",
      })),
    })),
    outcomes: input.outcomes.map(([conversationId, outcome]) => [
      conversationId,
      outcome.outcome === "completed"
        ? {
            sessionId: "session",
            turnId: "turn",
            ...outcome,
          }
        : outcome,
    ]),
    receipts: input.receipts.map((receipt) => ({
      sessionId: "session",
      turnId: "turn",
      ...receipt,
    })),
  });
}

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
  const turn = {
    conversationId,
    channelId: "channel-a",
    agentPubkeys: ["agent-1"],
    anchorAt: 1_000,
    lastSeenAt: 100_000,
    lastSubstantiveProgressAt: 100_000,
    progressKind: "progress",
    progressLabel: "Running tests",
    triggeringEventIds: ["current-trigger"],
    ...overrides,
  };
  return {
    ...turn,
    agentTriggerPairs:
      turn.agentTriggerPairs ??
      (turn.agentPubkeys.length === 1
        ? turn.triggeringEventIds.map((eventId) => ({
            agentPubkey: turn.agentPubkeys[0],
            eventId,
            sessionId: "session",
            turnId: "turn",
          }))
        : []),
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

test("one completed receipt cannot hide an unrelated active sibling", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    now: 100_100,
    channels,
    inboxItems: [item("conversation-siblings", "channel-a", 100)],
    needsYou: [],
    activeTurns: [
      activeTurn("conversation-siblings", {
        agentPubkeys: ["agent-1", "agent-2"],
        triggeringEventIds: ["trigger-1", "trigger-2"],
        agentTriggerPairs: [
          { agentPubkey: "agent-1", eventId: "trigger-1" },
          { agentPubkey: "agent-2", eventId: "trigger-2" },
        ],
      }),
    ],
    outcomes: [],
    receipts: [
      {
        id: "receipt-1",
        channelId: "channel-a",
        conversationId: "conversation-siblings",
        agentPubkey: "agent-1",
        parentEventId: "trigger-1",
        createdAt: 100_050,
        summary: "Agent one completed",
        verify: "done",
        reviewed: false,
      },
    ],
    acknowledgedConversationIds: new Set(),
  });

  assert.equal(sections.readyToReview.length, 0);
  assert.equal(sections.working.length, 1);
  assert.equal(sections.working[0].conversationId, "conversation-siblings");
});

test("completed multi-trigger outcomes require one receipt per trigger", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    channels,
    inboxItems: [item("conversation-completed-siblings", "channel-a", 100)],
    needsYou: [],
    activeTurns: [],
    outcomes: [
      [
        "conversation-completed-siblings",
        {
          outcome: "completed",
          channelId: "channel-a",
          agentPubkey: "agent-1",
          endedAt: 200,
          triggeringEventIds: ["trigger-1", "trigger-2"],
        },
      ],
    ],
    receipts: [
      {
        id: "receipt-1",
        channelId: "channel-a",
        conversationId: "conversation-completed-siblings",
        agentPubkey: "agent-1",
        parentEventId: "trigger-1",
        createdAt: 201,
        summary: "Only one trigger completed",
        verify: "done",
        reviewed: false,
      },
    ],
    acknowledgedConversationIds: new Set(),
  });

  assert.equal(sections.readyToReview.length, 0);
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
          triggeringEventIds: ["receipt-2-parent"],
        },
      ],
    ],
    receipts: [
      {
        id: "receipt-2",
        channelId: "channel-b",
        conversationId: "conversation-2",
        agentPubkey: "agent-2",
        parentEventId: "receipt-2-parent",
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

test("receipt-only rows resolve navigation from the exact verified event", async () => {
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
  const messageId = "a".repeat(64);
  const threadRootId = "b".repeat(64);
  assert.deepEqual(
    await getMissionInboxEventTarget(
      sections.readyToReview[0],
      async () => ({
        id: messageId,
        tags: [
          ["h", "channel-a"],
          ["e", threadRootId, "", "root"],
          ["e", messageId, "", "reply"],
        ],
      }),
      () => true,
    ),
    {
      channelId: "channel-a",
      messageId,
      parentEventId: messageId,
      threadRootId,
    },
  );
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

test("a completed run never falls back to an older run's receipt", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [],
    channels,
    inboxItems: [item("conversation-2", "channel-b", 100)],
    needsYou: [],
    outcomes: [
      [
        "conversation-2",
        {
          outcome: "completed",
          channelId: "channel-b",
          agentPubkey: "agent-2",
          endedAt: 300,
          triggeringEventIds: ["run-b-trigger"],
        },
      ],
    ],
    receipts: [
      {
        id: "run-a-receipt",
        channelId: "channel-b",
        conversationId: "conversation-2",
        agentPubkey: "agent-2",
        parentEventId: "run-a-trigger",
        createdAt: 200,
        summary: "Run A completed",
        reviewed: false,
      },
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

test("an active turn accepts only a receipt linked to its exact trigger", () => {
  const base = {
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [activeTurn("exact")],
    channels,
    inboxItems: [item("exact", "channel-a", 100)],
    needsYou: [],
    now: 101_000,
    outcomes: [],
  };
  const receipt = {
    id: "receipt-exact",
    channelId: "channel-a",
    conversationId: "exact",
    rootEventId: null,
    agentPubkey: "agent-1",
    createdAt: 100_000,
    summary: "Completed",
    verify: "verified",
    reviewed: false,
  };
  assert.equal(
    deriveMissionInboxSections({
      ...base,
      receipts: [{ ...receipt, parentEventId: "prior-trigger" }],
    }).working[0]?.conversationId,
    "exact",
  );
  assert.equal(
    deriveMissionInboxSections({
      ...base,
      receipts: [{ ...receipt, parentEventId: "current-trigger" }],
    }).readyToReview[0]?.conversationId,
    "exact",
  );
});

test("multi-agent turns never cross-pair one agent with another agent's trigger", () => {
  const turn = activeTurn("paired", {
    agentPubkeys: ["agent-1", "agent-2"],
    triggeringEventIds: ["trigger-1", "trigger-2"],
    agentTriggerPairs: [
      { agentPubkey: "agent-1", eventId: "trigger-1" },
      { agentPubkey: "agent-2", eventId: "trigger-2" },
    ],
  });
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [turn],
    channels,
    inboxItems: [item("paired", "channel-a", 100)],
    needsYou: [],
    now: 101_000,
    outcomes: [],
    receipts: [
      {
        id: "cross-paired",
        channelId: "channel-a",
        conversationId: "paired",
        rootEventId: null,
        parentEventId: "trigger-2",
        agentPubkey: "agent-1",
        createdAt: 100_000,
        summary: "must not match",
        verify: "none",
        reviewed: false,
      },
    ],
  });
  assert.equal(sections.readyToReview.length, 0);
  assert.equal(sections.working[0]?.conversationId, "paired");
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

test("mission rows use real roots and never promote conversation UUIDs to event ids", async () => {
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

  assert.deepEqual(
    await getMissionInboxEventTarget(
      needsYouRow,
      async () => ({ id: root, tags: [["h", "channel-a"]] }),
      () => true,
    ),
    {
      channelId: "channel-a",
      messageId: root,
      parentEventId: root,
      threadRootId: root,
    },
  );
  assert.equal(
    await getMissionInboxEventTarget({ ...needsYouRow, rootEventId: null }),
    null,
  );
});

test("a same-trigger rerun requires the receipt from the exact producer turn", () => {
  const sections = deriveMissionInboxSections({
    ...attentionDefaults,
    acknowledgedConversationIds: new Set(),
    activeTurns: [],
    channels,
    inboxItems: [item("same-trigger-rerun", "channel-a", 100)],
    needsYou: [],
    outcomes: [
      [
        "same-trigger-rerun",
        {
          outcome: "completed",
          channelId: "channel-a",
          agentPubkey: "agent-1",
          sessionId: "session-new",
          turnId: "turn-new",
          endedAt: 300,
          triggeringEventIds: ["same-trigger"],
        },
      ],
    ],
    receipts: [
      {
        id: "receipt-old-turn",
        channelId: "channel-a",
        conversationId: "same-trigger-rerun",
        agentPubkey: "agent-1",
        parentEventId: "same-trigger",
        sessionId: "session-old",
        turnId: "turn-old",
        createdAt: 200,
        summary: "Old turn",
        verify: "old",
        reviewed: false,
      },
    ],
  });

  assert.equal(sections.readyToReview.length, 0);
});
