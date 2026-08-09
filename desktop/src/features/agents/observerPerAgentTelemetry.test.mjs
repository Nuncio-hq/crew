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
