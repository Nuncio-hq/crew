import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  deriveAgentAttention,
  progressFromObserverEvent,
} from "./agentAttention.ts";

const NOW = 1_000_000;

function event(kind, payload = null) {
  return {
    seq: 1,
    timestamp: new Date(NOW).toISOString(),
    kind,
    agentIndex: 0,
    channelId: "channel-1",
    conversationId: "conversation-1",
    sessionId: "session-1",
    turnId: "turn-1",
    payload,
  };
}

function turn(overrides = {}) {
  return {
    agentPubkey: "agent-1",
    anchorAt: NOW - 120_000,
    lastSeenAt: NOW - 5_000,
    lastSubstantiveProgressAt: NOW - 5_000,
    progressKind: "progress",
    progressLabel: "Plan updated",
    ...overrides,
  };
}

describe("substantive progress classification", () => {
  it("does not count liveness, token chunks, or usage updates as progress", () => {
    assert.equal(progressFromObserverEvent(event("turn_liveness")), null);
    assert.equal(
      progressFromObserverEvent(
        event("acp_read", {
          method: "session/update",
          params: {
            update: {
              sessionUpdate: "agent_message_chunk",
              content: { type: "text", text: "still streaming" },
            },
          },
        }),
      ),
      null,
    );
    assert.equal(
      progressFromObserverEvent(
        event("acp_read", {
          method: "session/update",
          params: {
            update: { sessionUpdate: "usage_update", used: 42, size: 100 },
          },
        }),
      ),
      null,
    );
  });

  it("counts turn start, tool phase changes, retry, and plan changes", () => {
    assert.deepEqual(progressFromObserverEvent(event("turn_started")), {
      fingerprint: "turn_started",
      kind: "progress",
      label: "Turn started",
    });
    assert.deepEqual(
      progressFromObserverEvent(
        event("acp_read", {
          method: "session/update",
          params: {
            update: {
              sessionUpdate: "tool_call",
              toolCallId: "tool-1",
              title: "pnpm check",
              status: "executing",
            },
          },
        }),
      ),
      {
        fingerprint: "tool:tool-1:executing",
        kind: "progress",
        label: "Running pnpm check",
      },
    );
    assert.deepEqual(
      progressFromObserverEvent(
        event("acp_read", {
          method: "session/update",
          params: {
            update: {
              sessionUpdate: "tool_call_update",
              toolCallId: "tool-1",
              title: "pnpm check",
              status: "completed",
            },
          },
        }),
      ),
      {
        fingerprint: "tool:tool-1:completed",
        kind: "progress",
        label: "Completed pnpm check",
      },
    );
    assert.deepEqual(
      progressFromObserverEvent(
        event("turn_retrying", { attempt: 2, maxAttempts: 4 }),
      ),
      {
        fingerprint: "retry:2:4",
        kind: "progress",
        label: "Retrying 2/4",
      },
    );
    assert.deepEqual(
      progressFromObserverEvent(
        event("acp_read", {
          method: "session/update",
          params: {
            update: {
              sessionUpdate: "plan",
              entries: [{ content: "Run tests", status: "in_progress" }],
            },
          },
        }),
      ),
      {
        fingerprint:
          'plan:{"entries":[{"content":"Run tests","status":"in_progress"}],"sessionUpdate":"plan"}',
        kind: "progress",
        label: "Plan updated",
      },
    );
  });
});

describe("agent attention projection", () => {
  it("prioritizes needs-you and terminal failures", () => {
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: true,
        now: NOW,
        outcome: "error",
        receipt: null,
        turns: [turn()],
      }).state,
      "needs-you",
    );
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: false,
        now: NOW,
        outcome: "error",
        receipt: null,
        turns: [turn()],
      }).state,
      "failed",
    );
  });

  it("does not mislabel unavailable telemetry as a stall", () => {
    const projection = deriveAgentAttention({
      connectionState: "error",
      needsYou: false,
      now: NOW,
      outcome: null,
      receipt: null,
      turns: [
        turn({
          lastSeenAt: NOW - 180_000,
          lastSubstantiveProgressAt: NOW - 180_000,
        }),
      ],
    });
    assert.equal(projection.state, "telemetry-unavailable");
  });

  it("does not let a locally pruned lost-contact outcome mask telemetry failure", () => {
    const projection = deriveAgentAttention({
      connectionState: "error",
      needsYou: false,
      now: NOW,
      outcome: "lost-contact",
      receipt: null,
      turns: [],
    });
    assert.equal(projection.state, "telemetry-unavailable");
  });

  it("distinguishes lost contact from alive-without-progress", () => {
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: false,
        now: NOW,
        outcome: null,
        receipt: null,
        turns: [
          turn({
            lastSeenAt: NOW - 31_000,
            lastSubstantiveProgressAt: NOW - 31_000,
          }),
        ],
      }).state,
      "lost-contact",
    );
    const stalled = deriveAgentAttention({
      connectionState: "open",
      needsYou: false,
      now: NOW,
      outcome: null,
      receipt: null,
      turns: [
        turn({
          lastSeenAt: NOW - 5_000,
          lastSubstantiveProgressAt: NOW - 91_000,
        }),
      ],
    });
    assert.equal(stalled.state, "possibly-stalled");
    assert.equal(stalled.lastVerifiedLabel, "Plan updated");
  });

  it("keeps a known wait calm for its bounded grace period", () => {
    const waiting = deriveAgentAttention({
      connectionState: "open",
      needsYou: false,
      now: NOW,
      outcome: null,
      receipt: null,
      turns: [
        turn({
          lastSubstantiveProgressAt: NOW - 120_000,
          progressKind: "known-wait",
          progressLabel: "Running pnpm check",
        }),
      ],
    });
    assert.equal(waiting.state, "known-wait");

    const overdue = deriveAgentAttention({
      connectionState: "open",
      needsYou: false,
      now: NOW,
      outcome: null,
      receipt: null,
      turns: [
        turn({
          lastSubstantiveProgressAt: NOW - 301_000,
          progressKind: "known-wait",
          progressLabel: "Running pnpm check",
        }),
      ],
    });
    assert.equal(overdue.state, "possibly-stalled");
  });

  it("derives review state only from an unreviewed durable receipt", () => {
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: false,
        now: NOW,
        outcome: "completed",
        receipt: null,
        turns: [],
      }).state,
      "idle",
    );
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: false,
        now: NOW,
        outcome: "completed",
        receipt: { createdAt: NOW - 1_000, reviewed: false },
        turns: [],
      }).state,
      "ready-to-review",
    );
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: false,
        now: NOW,
        outcome: "completed",
        receipt: { createdAt: NOW - 1_000, reviewed: true },
        turns: [],
      }).state,
      "done",
    );
  });

  it("prioritizes an unreviewed receipt over calm working state", () => {
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: false,
        now: NOW,
        outcome: null,
        receipt: { createdAt: NOW - 1_000, reviewed: false },
        turns: [turn()],
      }).state,
      "ready-to-review",
    );
  });

  it("suppresses a stalled warning until the user-selected wait expires", () => {
    assert.equal(
      deriveAgentAttention({
        connectionState: "open",
        needsYou: false,
        now: NOW,
        outcome: null,
        receipt: null,
        snoozedUntil: NOW + 60_000,
        turns: [
          turn({
            lastSubstantiveProgressAt: NOW - 91_000,
          }),
        ],
      }).state,
      "working",
    );
  });
});
