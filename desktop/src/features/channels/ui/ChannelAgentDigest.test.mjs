import assert from "node:assert/strict";
import { beforeEach, describe, it } from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import {
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "@/features/agents/activeAgentTurnsStore.ts";
import { getAgentThreadDigestForChannel } from "@/features/agents/agentThreadDigestForChannel.ts";
import {
  ingestApprovalRequest,
  resetNeedsYouStore,
} from "@/features/agents/needsYouStore.ts";
import {
  ChannelAgentDigest,
  ChannelAgentDigestView,
} from "./ChannelAgentDigest.tsx";

const AGENT_A =
  "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const AGENT_B =
  "bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222";

function makeEvent(overrides) {
  return {
    seq: 1,
    timestamp: "2026-07-31T00:00:00.000Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "chan-1",
    conversationId: "conv-1",
    sessionId: "sess-1",
    turnId: "turn-1",
    payload: null,
    ...overrides,
  };
}

describe("getAgentThreadDigestForChannel", () => {
  beforeEach(() => {
    resetActiveAgentTurnsStore();
    resetNeedsYouStore();
  });

  it("returns null when the channel has no running or recent outcomes", () => {
    assert.equal(getAgentThreadDigestForChannel("chan-empty"), null);
  });

  it("counts threads (conversations), not agents, for running", () => {
    syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 1,
        conversationId: "conv-a",
        turnId: "t-a",
      }),
    ]);
    syncAgentTurnsFromEvents(AGENT_B, [
      makeEvent({
        seq: 1,
        conversationId: "conv-a",
        turnId: "t-b",
        timestamp: "2026-07-31T00:00:30.000Z",
      }),
    ]);
    const digest = getAgentThreadDigestForChannel("chan-1");
    assert.ok(digest);
    assert.equal(digest.running.length, 1);
    assert.equal(digest.running[0].conversationId, "conv-a");
    assert.deepEqual(digest.running[0].agentPubkeys, [AGENT_A, AGENT_B]);
  });

  it("splits running / failed / done across channels and suppresses active", () => {
    // Channel 1: one running, one done, one failed (failed suppressed by sibling run).
    syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 1,
        channelId: "chan-1",
        conversationId: "conv-run",
        turnId: "t-run",
      }),
    ]);
    syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 2,
        channelId: "chan-1",
        conversationId: "conv-done",
        turnId: "t-done-start",
        timestamp: "2026-07-31T00:01:00.000Z",
      }),
      makeEvent({
        seq: 3,
        kind: "turn_completed",
        channelId: "chan-1",
        conversationId: "conv-done",
        turnId: "t-done-start",
        timestamp: "2026-07-31T00:02:00.000Z",
      }),
    ]);
    // Same conversation: A finished, B still running → failed/done suppressed.
    syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({
        seq: 4,
        channelId: "chan-1",
        conversationId: "conv-mixed",
        turnId: "t-mixed-a",
        timestamp: "2026-07-31T00:03:00.000Z",
      }),
      makeEvent({
        seq: 5,
        kind: "turn_error",
        channelId: "chan-1",
        conversationId: "conv-mixed",
        turnId: "t-mixed-a",
        timestamp: "2026-07-31T00:04:00.000Z",
      }),
    ]);
    syncAgentTurnsFromEvents(AGENT_B, [
      makeEvent({
        seq: 1,
        channelId: "chan-1",
        conversationId: "conv-mixed",
        turnId: "t-mixed-b",
        timestamp: "2026-07-31T00:03:30.000Z",
      }),
    ]);

    // Channel 2: one failed (isolated).
    syncAgentTurnsFromEvents(AGENT_B, [
      makeEvent({
        seq: 2,
        channelId: "chan-2",
        conversationId: "conv-fail",
        turnId: "t-fail",
        timestamp: "2026-07-31T00:05:00.000Z",
      }),
      makeEvent({
        seq: 3,
        kind: "turn_error",
        channelId: "chan-2",
        conversationId: "conv-fail",
        turnId: "t-fail",
        timestamp: "2026-07-31T00:06:00.000Z",
      }),
    ]);

    const digest1 = getAgentThreadDigestForChannel("chan-1");
    assert.ok(digest1);
    assert.equal(digest1.running.length, 2); // conv-run + conv-mixed
    assert.equal(digest1.done.length, 1);
    assert.equal(digest1.done[0].conversationId, "conv-done");
    assert.equal(digest1.failed.length, 0); // conv-mixed suppressed

    const digest2 = getAgentThreadDigestForChannel("chan-2");
    assert.ok(digest2);
    assert.equal(digest2.running.length, 0);
    assert.equal(digest2.failed.length, 1);
    assert.equal(digest2.failed[0].conversationId, "conv-fail");
    assert.equal(digest2.done.length, 0);
  });

  it("returns a stable reference until generation bumps", () => {
    syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({ seq: 1, conversationId: "conv-a", turnId: "t1" }),
    ]);
    const first = getAgentThreadDigestForChannel("chan-1");
    const second = getAgentThreadDigestForChannel("chan-1");
    assert.equal(first, second);
  });
  it("counts blocked conversations in the needs-you bucket", () => {
    const root =
      "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    ingestApprovalRequest({
      id: "approval-a",
      channelId: "00112233-4455-6677-8899-aabbccddeeff",
      rootEventId: root,
      agentPubkey: AGENT_A,
      createdAt: Date.now(),
    });
    ingestApprovalRequest({
      id: "approval-b",
      channelId: "00112233-4455-6677-8899-aabbccddeeff",
      rootEventId: root,
      agentPubkey: AGENT_B,
      createdAt: Date.now(),
    });
    const digest = getAgentThreadDigestForChannel(
      "00112233-4455-6677-8899-aabbccddeeff",
    );
    assert.ok(digest);
    assert.equal(digest.needsYou.length, 1);
    assert.deepEqual(digest.needsYou[0].agentPubkeys, [AGENT_A, AGENT_B]);
  });
});

describe("ChannelAgentDigest render", () => {
  beforeEach(() => {
    resetActiveAgentTurnsStore();
    resetNeedsYouStore();
  });

  it("renders nothing when digest is empty", () => {
    const html = renderToStaticMarkup(
      React.createElement(ChannelAgentDigest, {
        channelId: "chan-empty",
        onOpenThread: () => {},
      }),
    );
    assert.equal(html, "");
  });

  it("ChannelAgentDigestView returns null when all buckets empty", () => {
    const html = renderToStaticMarkup(
      React.createElement(ChannelAgentDigestView, {
        digest: { running: [], failed: [], done: [] },
        onOpenThread: () => {},
      }),
    );
    assert.equal(html, "");
  });

  it("renders running pill when the channel has active threads", () => {
    syncAgentTurnsFromEvents(AGENT_A, [
      makeEvent({ seq: 1, conversationId: "conv-a", turnId: "t1" }),
    ]);
    const html = renderToStaticMarkup(
      React.createElement(ChannelAgentDigest, {
        channelId: "chan-1",
        onOpenThread: () => {},
      }),
    );
    assert.match(html, /data-testid="channel-agent-digest"/);
    assert.match(html, /data-testid="channel-agent-digest-pill-running"/);
  });
});
