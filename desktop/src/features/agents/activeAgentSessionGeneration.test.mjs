import assert from "node:assert/strict";
import { beforeEach, describe, it } from "node:test";

import {
  observeAgentSession,
  prepareAgentSessionObservation,
  resetAgentSessionGenerations,
} from "./activeAgentSessionGeneration.ts";

function event(overrides = {}) {
  return {
    seq: 1,
    timestamp: "2024-01-01T00:00:00Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "channel-1",
    conversationId: "conversation-1",
    sessionId: "session-1",
    turnId: "turn-1",
    payload: null,
    ...overrides,
  };
}

describe("activeAgentSessionGeneration", () => {
  beforeEach(() => resetAgentSessionGenerations());

  it("preserves a prepared generation decision for downstream turn projection", () => {
    const old = event({ sessionId: "session-old" });
    const current = event({
      seq: 1,
      timestamp: "2024-01-01T00:00:01Z",
      sessionId: "session-current",
    });
    const retired = event({
      seq: 2,
      timestamp: "2024-01-01T00:00:02Z",
      sessionId: "session-old",
    });

    assert.equal(prepareAgentSessionObservation("agent", old), "current");
    assert.equal(prepareAgentSessionObservation("agent", current), "changed");
    assert.equal(
      observeAgentSession("agent", current),
      "changed",
      "the active-turn projection must see the same generation change",
    );
    assert.equal(
      prepareAgentSessionObservation("agent", retired),
      "retired",
      "a retired frame can be rejected before it updates contact state",
    );
  });

  it("rejects replay from a session already proven retired", () => {
    const old = event({ sessionId: "session-old" });
    const current = event({
      seq: 2,
      timestamp: "2024-01-01T00:00:01Z",
      sessionId: "session-current",
    });
    const replayedOld = event({
      seq: 3,
      timestamp: "2024-01-01T00:00:02Z",
      sessionId: "session-old",
      replayed: true,
    });

    assert.equal(observeAgentSession("agent", old), "current");
    assert.equal(observeAgentSession("agent", current), "changed");
    assert.equal(observeAgentSession("agent", replayedOld), "retired");
  });

  it("revalidates a staged frame after a newer session retires it", () => {
    const old = event({ sessionId: "session-old" });
    const prepared = prepareAgentSessionObservation("agent", old);
    assert.equal(prepared, "current");
    assert.equal(
      observeAgentSession(
        "agent",
        event({ seq: 1, sessionId: "session-current" }),
      ),
      "changed",
    );
    assert.equal(prepareAgentSessionObservation("agent", old), "retired");
  });
});
