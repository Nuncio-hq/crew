import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import {
  _testProcessDecryptedObserverFrame,
  _testProcessLiveObserverEvent,
  _testSetAgentConnectionError,
  getAgentObserverSnapshot,
  getLatestLiveSessionId,
  injectObserverEventsForE2E,
  resetAgentObserverStore,
  setObserverConnectionStateForE2E,
  shouldResetObserverLiveContacts,
} from "./observerRelayStore.ts";
import { resetAgentSessionGenerations } from "./activeAgentSessionGeneration.ts";

const AGENT = "a".repeat(64);

function observerEvent(replayed) {
  return {
    seq: replayed ? 1 : 2,
    timestamp: replayed ? "2024-01-01T00:00:00Z" : "2024-01-01T00:00:01Z",
    kind: "turn_liveness",
    agentIndex: 0,
    channelId: "channel",
    sessionId: "session",
    turnId: "turn",
    payload: null,
    replayed,
  };
}

afterEach(() => {
  resetAgentObserverStore();
  resetAgentSessionGenerations();
});

test("subscription readiness does not certify restored per-agent telemetry", () => {
  injectObserverEventsForE2E(AGENT, [observerEvent(true)]);
  assert.equal(getAgentObserverSnapshot(AGENT).connectionState, "connecting");

  injectObserverEventsForE2E(AGENT, [observerEvent(false)]);
  assert.equal(getAgentObserverSnapshot(AGENT).connectionState, "open");
});

test("open subscription with zero agent events is idle open, not connecting", () => {
  setObserverConnectionStateForE2E("open");
  assert.equal(getAgentObserverSnapshot(AGENT).connectionState, "open");
  assert.equal(getAgentObserverSnapshot(AGENT).events.length, 0);
});

test("a failed connection invalidates contact received before EOSE", () => {
  assert.equal(
    shouldResetObserverLiveContacts("connecting", "connecting"),
    false,
  );
  assert.equal(shouldResetObserverLiveContacts("connecting", "error"), true);
  assert.equal(shouldResetObserverLiveContacts("error", "connecting"), true);
  assert.equal(shouldResetObserverLiveContacts("open", "connecting"), true);
});

test("a retired live frame cannot roll session state backward", () => {
  const base = {
    ...observerEvent(false),
    conversationId: "conversation",
  };
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    seq: 1,
    sessionId: "session-old",
    timestamp: "2024-01-01T00:00:01Z",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    seq: 1,
    sessionId: "session-current",
    timestamp: "2024-01-01T00:00:02Z",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    seq: 2,
    sessionId: "session-old",
    timestamp: "2024-01-01T00:00:03Z",
  });

  assert.equal(getLatestLiveSessionId(AGENT, "channel"), "session-current");
  assert.equal(getAgentObserverSnapshot(AGENT, true).events.length, 2);
});

test("a newly observed live session advances despite producer clock and seq reset", () => {
  const base = {
    ...observerEvent(false),
    conversationId: "conversation",
  };
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    seq: 99,
    sessionId: "session-old",
    timestamp: "2024-01-01T00:10:00Z",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    seq: 1,
    sessionId: "session-current",
    timestamp: "2024-01-01T00:00:00Z",
  });

  assert.equal(getLatestLiveSessionId(AGENT, "channel"), "session-current");
});

test("late frames from another conversation cannot roll channel session authority back", () => {
  const base = { ...observerEvent(false), seq: 1 };
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    kind: "turn_started",
    conversationId: "conversation-old",
    sessionId: "session-old",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    kind: "turn_started",
    conversationId: "conversation-current",
    sessionId: "session-current",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    kind: "turn_started",
    seq: 2,
    conversationId: "conversation-old",
    sessionId: "session-old",
  });
  assert.equal(
    _testProcessLiveObserverEvent(AGENT, {
      ...base,
      kind: "turn_completed",
      seq: 3,
      conversationId: "conversation-old",
      sessionId: "session-old",
    }),
    true,
  );

  assert.equal(getLatestLiveSessionId(AGENT, "channel"), "session-current");
});

test("a sibling producer in another conversation keeps its own session authority", () => {
  const base = { ...observerEvent(false), kind: "turn_started", seq: 1 };
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    conversationId: "conversation-old",
    sessionId: "session-old",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    conversationId: "conversation-current",
    sessionId: "session-current",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    conversationId: "conversation-old",
    sessionId: "session-old-sibling",
  });
  assert.equal(getLatestLiveSessionId(AGENT, "channel"), "session-old-sibling");
});

test("same-sequence same-second frames from concurrent producers remain distinct", () => {
  const base = {
    ...observerEvent(false),
    seq: 1,
    timestamp: "2024-01-01T00:00:01Z",
  };

  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    agentIndex: 0,
    conversationId: "conversation-0",
    sessionId: "session-0",
    turnId: "turn-0",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    agentIndex: 1,
    conversationId: "conversation-1",
    sessionId: "session-1",
    turnId: "turn-1",
  });

  assert.deepEqual(
    getAgentObserverSnapshot(AGENT, true).events.map(
      (event) => event.agentIndex,
    ),
    [0, 1],
  );
});

test("an outer frame with only retired inner events cannot clear telemetry error", () => {
  const base = {
    ...observerEvent(false),
    conversationId: "conversation",
  };
  setObserverConnectionStateForE2E("open");
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    seq: 1,
    sessionId: "session-old",
  });
  _testProcessLiveObserverEvent(AGENT, {
    ...base,
    seq: 2,
    timestamp: "2024-01-01T00:00:02Z",
    sessionId: "session-current",
  });
  _testSetAgentConnectionError(AGENT, "telemetry failed");

  const accepted = _testProcessDecryptedObserverFrame(
    AGENT,
    {
      ...base,
      kind: "batch",
      payload: {
        events: [
          {
            ...base,
            seq: 3,
            timestamp: "2024-01-01T00:00:03Z",
            sessionId: "session-old",
          },
        ],
      },
    },
    { replay: false },
  );

  assert.equal(accepted, false);
  assert.equal(getAgentObserverSnapshot(AGENT).connectionState, "error");
  assert.equal(
    getAgentObserverSnapshot(AGENT).errorMessage,
    "telemetry failed",
  );
  assert.equal(getAgentObserverSnapshot(AGENT).events.length, 2);
});

test("retirement capacity fails closed instead of forgetting old sessions", () => {
  let accepted = true;
  for (let index = 0; index < 130; index += 1) {
    accepted = _testProcessLiveObserverEvent(AGENT, {
      ...observerEvent(false),
      kind: "turn_started",
      conversationId: "conversation",
      sessionId: `session-${index}`,
      seq: index,
    });
  }
  assert.equal(accepted, false);
  assert.equal(getLatestLiveSessionId(AGENT, "channel"), null);
  assert.match(
    getAgentObserverSnapshot(AGENT).errorMessage ?? "",
    /safe recovery bound/,
  );
});
