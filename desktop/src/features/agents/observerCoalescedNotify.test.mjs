import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, it } from "node:test";

import { flushPendingNotificationsForTest } from "./lib/coalescedNotify.ts";
import {
  getActiveTurnsForAgent,
  resetActiveAgentTurnsStore,
  subscribeActiveAgentTurns,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";
import {
  getAgentObserverSnapshot,
  injectObserverEventsForE2E,
  resetAgentObserverStore,
  setObserverConnectionStateForE2E,
  subscribeAgentObserverStore,
} from "./observerRelayStore.ts";

const AGENT =
  "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";

let restoreDocument = () => {};

function installDocument({ hidden }) {
  const previousDocument = globalThis.document;
  const previousRaf = globalThis.requestAnimationFrame;
  const previousCancel = globalThis.cancelAnimationFrame;
  const rafQueue = [];
  let nextId = 1;

  globalThis.document = {
    hidden,
    visibilityState: hidden ? "hidden" : "visible",
    addEventListener() {},
    removeEventListener() {},
  };
  globalThis.requestAnimationFrame = (callback) => {
    const id = nextId++;
    rafQueue.push({ id, callback });
    return id;
  };
  globalThis.cancelAnimationFrame = (id) => {
    const index = rafQueue.findIndex((entry) => entry.id === id);
    if (index !== -1) rafQueue.splice(index, 1);
  };

  restoreDocument = () => {
    if (previousDocument === undefined) delete globalThis.document;
    else globalThis.document = previousDocument;
    if (previousRaf === undefined) delete globalThis.requestAnimationFrame;
    else globalThis.requestAnimationFrame = previousRaf;
    if (previousCancel === undefined) delete globalThis.cancelAnimationFrame;
    else globalThis.cancelAnimationFrame = previousCancel;
    rafQueue.length = 0;
    restoreDocument = () => {};
  };
}

function makeEvent(overrides) {
  return {
    seq: 1,
    timestamp: "2024-01-01T00:00:00Z",
    kind: "turn_started",
    agentIndex: 0,
    channelId: "chan-1",
    sessionId: "sess-1",
    turnId: "turn-1",
    payload: null,
    ...overrides,
  };
}

describe("observer store coalesced notifications (issue #287)", () => {
  beforeEach(() => {
    resetAgentObserverStore();
    resetActiveAgentTurnsStore();
  });

  afterEach(() => {
    flushPendingNotificationsForTest();
    restoreDocument();
    resetAgentObserverStore();
    resetActiveAgentTurnsStore();
  });

  it("appends 10 observer events in one tick and notifies at most once after flush", () => {
    installDocument({ hidden: false });
    let calls = 0;
    const unsubscribe = subscribeAgentObserverStore(() => {
      calls += 1;
    });

    for (let seq = 1; seq <= 10; seq++) {
      injectObserverEventsForE2E(AGENT, [
        makeEvent({
          seq,
          turnId: `turn-${seq}`,
          timestamp: new Date(
            Date.parse("2024-01-01T00:00:00Z") + seq * 1000,
          ).toISOString(),
        }),
      ]);
    }

    assert.equal(
      getAgentObserverSnapshot(AGENT, true).events.length,
      10,
      "mutations stay synchronous — the snapshot already has every event",
    );
    assert.equal(calls, 0, "the listener sweep is deferred until flush");

    flushPendingNotificationsForTest();
    unsubscribe();

    assert.equal(calls, 1, "ten appends in one tick coalesce to one sweep");
    assert.equal(getAgentObserverSnapshot(AGENT, true).events.length, 10);
  });

  it("coalesces a connection-state change and an event append into one notification", () => {
    installDocument({ hidden: false });
    let calls = 0;
    const unsubscribe = subscribeAgentObserverStore(() => {
      calls += 1;
    });

    setObserverConnectionStateForE2E("open");
    injectObserverEventsForE2E(AGENT, [makeEvent({ seq: 1 })]);

    assert.equal(calls, 0);
    flushPendingNotificationsForTest();
    unsubscribe();

    assert.equal(calls, 1);
    const snapshot = getAgentObserverSnapshot(AGENT, true);
    assert.equal(snapshot.connectionState, "open");
    assert.equal(snapshot.events.length, 1);
    assert.equal(snapshot.events[0]?.seq, 1);
  });

  it("flushes observer notifications synchronously when document.hidden", () => {
    installDocument({ hidden: true });
    let calls = 0;
    const unsubscribe = subscribeAgentObserverStore(() => {
      calls += 1;
    });

    injectObserverEventsForE2E(AGENT, [makeEvent({ seq: 1 })]);
    assert.equal(calls, 1, "hidden documents must not wait for rAF");
    assert.equal(getAgentObserverSnapshot(AGENT, true).events.length, 1);
    unsubscribe();
  });
});

describe("active-turns store coalesced notifications (issue #287)", () => {
  beforeEach(() => {
    resetActiveAgentTurnsStore();
  });

  afterEach(() => {
    flushPendingNotificationsForTest();
    restoreDocument();
    resetActiveAgentTurnsStore();
  });

  it("notifies subscribeActiveAgentTurns at most once for 10 same-tick mutations", () => {
    installDocument({ hidden: false });
    let calls = 0;
    const unsubscribe = subscribeActiveAgentTurns(() => {
      calls += 1;
    });

    for (let seq = 1; seq <= 10; seq++) {
      syncAgentTurnsFromEvents(AGENT, [
        makeEvent({
          seq,
          turnId: `t${seq}`,
          channelId: `c${seq}`,
          timestamp: new Date(
            Date.parse("2024-01-01T00:00:00Z") + seq * 1000,
          ).toISOString(),
        }),
      ]);
    }

    assert.equal(
      getActiveTurnsForAgent(AGENT).length,
      10,
      "turn mutations stay synchronous",
    );
    assert.equal(calls, 0, "the listener sweep is deferred until flush");

    flushPendingNotificationsForTest();
    unsubscribe();

    assert.equal(calls, 1);
    assert.equal(getActiveTurnsForAgent(AGENT).length, 10);
  });

  it("flushes turn-store notifications synchronously when document.hidden", () => {
    installDocument({ hidden: true });
    let calls = 0;
    const unsubscribe = subscribeActiveAgentTurns(() => {
      calls += 1;
    });

    syncAgentTurnsFromEvents(AGENT, [makeEvent({ seq: 1 })]);
    assert.equal(calls, 1);
    assert.equal(getActiveTurnsForAgent(AGENT).length, 1);
    unsubscribe();
  });
});
