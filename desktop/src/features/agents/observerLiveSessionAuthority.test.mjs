import assert from "node:assert/strict";
import { beforeEach, test } from "node:test";

import {
  observeLiveSessionAuthority,
  resetLiveSessionAuthority,
} from "./observerLiveSessionAuthority.ts";

const AGENT = "a".repeat(64);

function frame(overrides = {}) {
  return {
    seq: 1,
    timestamp: "2026-08-10T00:00:00Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "channel-1",
    conversationId: "conversation-1",
    sessionId: "session-a",
    turnId: "turn-a",
    payload: null,
    ...overrides,
  };
}

beforeEach(resetLiveSessionAuthority);

test("same-conversation producer indexes never retire one another", () => {
  assert.deepEqual(observeLiveSessionAuthority(AGENT, frame(), "current"), {
    accepted: true,
    unavailable: false,
  });
  assert.deepEqual(
    observeLiveSessionAuthority(
      AGENT,
      frame({
        agentIndex: 1,
        sessionId: "session-b",
        turnId: "turn-b",
      }),
      "changed",
    ),
    { accepted: true, unavailable: false },
  );
  assert.deepEqual(
    observeLiveSessionAuthority(
      AGENT,
      frame({ kind: "tool_call", seq: 2 }),
      "current",
    ),
    { accepted: true, unavailable: false },
  );
});
