import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import {
  getAgentObserverSnapshot,
  injectObserverEventsForE2E,
  resetAgentObserverStore,
  shouldResetObserverLiveContacts,
} from "./observerRelayStore.ts";

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
