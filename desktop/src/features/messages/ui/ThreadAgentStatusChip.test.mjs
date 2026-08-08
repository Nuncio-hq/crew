import assert from "node:assert/strict";
import { test } from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import {
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "@/features/agents/activeAgentTurnsStore.ts";
import {
  getNeedsYouForConversation,
  ingestApprovalRequest,
  resetNeedsYouStore,
} from "@/features/agents/needsYouStore.ts";
import {
  buildThreadAgentStatusChipView,
  ThreadAgentStatusChip,
} from "./ThreadAgentStatusChip.tsx";
import { setObserverConnectionStateForE2E } from "@/features/agents/observerRelayStore.ts";

const AGENT_A =
  "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const AGENT_B =
  "bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222";
const AGENT_C =
  "cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333";

const NOW = Date.parse("2026-07-31T00:05:00.000Z");

const PROFILE_A = {
  [AGENT_A]: {
    displayName: "Claude Opus",
    name: "claude",
    avatarUrl: null,
    nip05Handle: null,
    isAgent: true,
    ownerPubkey: null,
  },
};

test("buildThreadAgentStatusChipView returns null with no agents or outcome", () => {
  assert.equal(buildThreadAgentStatusChipView([], null, undefined, NOW), null);
});

test("buildThreadAgentStatusChipView labels a single agent by display name", () => {
  const view = buildThreadAgentStatusChipView(
    [{ agentPubkey: AGENT_A, anchorAt: NOW - 45_000 }],
    null,
    {
      [AGENT_A]: {
        displayName: "Claude",
        name: "claude",
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: null,
      },
    },
    NOW,
  );
  assert.ok(view);
  assert.equal(view.state, "running");
  assert.equal(view.label, "Claude");
  assert.equal(view.elapsedLabel, "45s");
  assert.equal(view.displayAgents.length, 1);
  assert.match(view.title, /Claude working · 45s/);
});

test("buildThreadAgentStatusChipView aggregates multiple agents", () => {
  const view = buildThreadAgentStatusChipView(
    [
      { agentPubkey: AGENT_A, anchorAt: NOW - 90_000 },
      { agentPubkey: AGENT_B, anchorAt: NOW - 30_000 },
      { agentPubkey: AGENT_C, anchorAt: NOW - 10_000 },
    ],
    null,
    {
      [AGENT_A]: {
        displayName: "Claude",
        name: null,
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: null,
      },
      [AGENT_B]: {
        displayName: "Codex",
        name: null,
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: null,
      },
      [AGENT_C]: {
        displayName: "Gemini",
        name: null,
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: null,
      },
    },
    NOW,
  );
  assert.ok(view);
  assert.equal(view.state, "running");
  assert.equal(view.label, "3 agents");
  assert.equal(view.elapsedLabel, "1m 30s");
  assert.equal(view.displayAgents.length, 2);
  assert.equal(view.displayAgents[0].displayName, "Claude");
  assert.equal(view.displayAgents[1].displayName, "Codex");
});

test("buildThreadAgentStatusChipView prefers running over failed/done", () => {
  const view = buildThreadAgentStatusChipView(
    [{ agentPubkey: AGENT_A, anchorAt: NOW - 10_000 }],
    {
      outcome: "error",
      agentPubkey: AGENT_B,
      endedAt: NOW - 60_000,
      channelId: "chan-1",
    },
    PROFILE_A,
    NOW,
  );
  assert.ok(view);
  assert.equal(view.state, "running");
});

test("buildThreadAgentStatusChipView prioritizes needs-you over running", () => {
  resetNeedsYouStore();
  ingestApprovalRequest({
    id: "approval-1",
    channelId: "00112233-4455-6677-8899-aabbccddeeff",
    rootEventId:
      "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    conversationId: "thread-a",
    agentPubkey: AGENT_A,
    createdAt: NOW - 9 * 60_000,
  });
  const view = buildThreadAgentStatusChipView(
    [{ agentPubkey: AGENT_A, anchorAt: NOW - 10_000 }],
    null,
    PROFILE_A,
    NOW,
    getNeedsYouForConversation("thread-a", NOW),
  );
  assert.ok(view);
  assert.equal(view.state, "needs-you");
  assert.equal(view.elapsedLabel, "9m 0s");
  assert.match(view.title, /Claude Opus is waiting for your approval · 9m 0s/);
});

test("observer completion alone does not claim durable Done", () => {
  const view = buildThreadAgentStatusChipView(
    [],
    {
      outcome: "completed",
      agentPubkey: AGENT_A,
      endedAt: NOW - 12 * 60_000,
      channelId: "chan-1",
    },
    PROFILE_A,
    NOW,
  );
  assert.equal(view, null);
});

test("receipt authority drives Ready to review and reviewed Done", () => {
  const receipt = {
    id: "d".repeat(64),
    channelId: "chan-1",
    conversationId: "thread-a",
    rootEventId: "e".repeat(64),
    agentPubkey: AGENT_A,
    createdAt: NOW - 12 * 60_000,
    summary: "Completed",
    verify: "pnpm check",
    reviewed: false,
  };
  const ready = buildThreadAgentStatusChipView(
    [],
    null,
    PROFILE_A,
    NOW,
    [],
    receipt,
  );
  assert.equal(ready?.state, "ready-to-review");
  assert.equal(ready?.label, "Ready to review");
  const done = buildThreadAgentStatusChipView([], null, PROFILE_A, NOW, [], {
    ...receipt,
    reviewed: true,
  });
  assert.equal(done?.state, "done");
});

test("buildThreadAgentStatusChipView builds failed view model", () => {
  const view = buildThreadAgentStatusChipView(
    [],
    {
      outcome: "error",
      agentPubkey: AGENT_A,
      endedAt: NOW - 45_000,
      channelId: "chan-1",
    },
    PROFILE_A,
    NOW,
  );
  assert.ok(view);
  assert.equal(view.state, "failed");
  assert.equal(view.label, "Failed");
  assert.equal(view.elapsedLabel, "45s ago");
  assert.equal(view.title, "Claude Opus failed 45s ago");
});

test("ThreadAgentStatusChip renders nothing when conversation has no agents", () => {
  resetActiveAgentTurnsStore();
  const html = renderToStaticMarkup(
    React.createElement(ThreadAgentStatusChip, {
      conversationId: "thread-empty",
    }),
  );
  assert.equal(html, "");
});

test("ThreadAgentStatusChip renders chip for a single active agent", () => {
  resetActiveAgentTurnsStore();
  setObserverConnectionStateForE2E("open");
  syncAgentTurnsFromEvents(AGENT_A, [
    {
      seq: 1,
      timestamp: "2026-07-31T00:00:00.000Z",
      kind: "turn_started",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
  ]);

  const html = renderToStaticMarkup(
    React.createElement(ThreadAgentStatusChip, {
      conversationId: "thread-a",
      profiles: {
        [AGENT_A]: {
          displayName: "Claude",
          name: null,
          avatarUrl: null,
          nip05Handle: null,
          isAgent: true,
          ownerPubkey: null,
        },
      },
    }),
  );
  assert.match(html, /data-testid="thread-agent-status-chip"/);
  assert.match(html, /data-state="running"/);
  assert.match(html, /Claude/);
});

test("ThreadAgentStatusChip renders N agents label for multiple", () => {
  resetActiveAgentTurnsStore();
  syncAgentTurnsFromEvents(AGENT_A, [
    {
      seq: 1,
      timestamp: "2026-07-31T00:00:00.000Z",
      kind: "turn_started",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
  ]);
  syncAgentTurnsFromEvents(AGENT_B, [
    {
      seq: 1,
      timestamp: "2026-07-31T00:01:00.000Z",
      kind: "turn_started",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-b",
      payload: {},
    },
  ]);

  const html = renderToStaticMarkup(
    React.createElement(ThreadAgentStatusChip, {
      conversationId: "thread-a",
    }),
  );
  assert.match(html, /data-testid="thread-agent-status-chip"/);
  assert.match(html, /data-state="running"/);
  assert.match(html, /2 agents/);
});

test("ThreadAgentStatusChip does not claim Done from observer completion alone", () => {
  resetActiveAgentTurnsStore();
  syncAgentTurnsFromEvents(AGENT_A, [
    {
      seq: 1,
      timestamp: "2026-07-31T00:00:00.000Z",
      kind: "turn_started",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
    {
      seq: 2,
      timestamp: "2026-07-31T00:01:00.000Z",
      kind: "turn_completed",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
  ]);

  const html = renderToStaticMarkup(
    React.createElement(ThreadAgentStatusChip, {
      conversationId: "thread-a",
      profiles: PROFILE_A,
    }),
  );
  assert.equal(html, "");
});

test("ThreadAgentStatusChip renders failed after turn_error", () => {
  resetActiveAgentTurnsStore();
  syncAgentTurnsFromEvents(AGENT_A, [
    {
      seq: 1,
      timestamp: "2026-07-31T00:00:00.000Z",
      kind: "turn_started",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
    {
      seq: 2,
      timestamp: "2026-07-31T00:01:00.000Z",
      kind: "turn_error",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
  ]);

  const html = renderToStaticMarkup(
    React.createElement(ThreadAgentStatusChip, {
      conversationId: "thread-a",
      profiles: PROFILE_A,
    }),
  );
  assert.match(html, /data-state="failed"/);
  assert.match(html, /Failed/);
});

test("ThreadAgentStatusChip stays running when a sibling agent is still active", () => {
  resetActiveAgentTurnsStore();
  syncAgentTurnsFromEvents(AGENT_A, [
    {
      seq: 1,
      timestamp: "2026-07-31T00:00:00.000Z",
      kind: "turn_started",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
    {
      seq: 2,
      timestamp: "2026-07-31T00:01:00.000Z",
      kind: "turn_completed",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-a",
      payload: {},
    },
  ]);
  syncAgentTurnsFromEvents(AGENT_B, [
    {
      seq: 1,
      timestamp: "2026-07-31T00:00:30.000Z",
      kind: "turn_started",
      agentIndex: 0,
      channelId: "channel-a",
      conversationId: "thread-a",
      sessionId: null,
      turnId: "turn-b",
      payload: {},
    },
  ]);

  const html = renderToStaticMarkup(
    React.createElement(ThreadAgentStatusChip, {
      conversationId: "thread-a",
    }),
  );
  assert.match(html, /data-state="running"/);
  assert.doesNotMatch(html, /data-state="done"/);
});
