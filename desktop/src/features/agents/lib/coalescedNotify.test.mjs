import assert from "node:assert/strict";
import { afterEach, describe, it } from "node:test";

import {
  coalesceNotifier,
  createCoalescedHub,
  flushPendingNotificationsForTest,
  mergeUpdatesByAgentPubkey,
} from "./coalescedNotify.ts";

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

  return {
    flushRaf() {
      const queued = rafQueue.splice(0);
      for (const entry of queued) entry.callback(0);
    },
  };
}

afterEach(() => {
  flushPendingNotificationsForTest();
  restoreDocument();
});

describe("coalesceNotifier", () => {
  it("absorbs same-tick schedules and flushes at most once per animation frame", async () => {
    installDocument({ hidden: false });
    let calls = 0;
    const notifier = coalesceNotifier(() => {
      calls += 1;
    });

    for (let i = 0; i < 10; i++) notifier.schedule();
    assert.equal(calls, 0, "the listener sweep must wait for the frame cap");

    await Promise.resolve();
    assert.equal(calls, 0, "the microtask only arms requestAnimationFrame");

    notifier.flush();
    assert.equal(calls, 1);
  });

  it("flushes synchronously when document.hidden is true", () => {
    installDocument({ hidden: true });
    let calls = 0;
    const notifier = coalesceNotifier(() => {
      calls += 1;
    });

    notifier.schedule();
    notifier.schedule();
    assert.equal(calls, 2, "hidden documents flush each schedule immediately");
  });

  it("flushes synchronously in test mode when document is missing", () => {
    restoreDocument();
    delete globalThis.document;
    let calls = 0;
    const notifier = coalesceNotifier(() => {
      calls += 1;
    });

    notifier.schedule();
    notifier.schedule();
    assert.equal(calls, 2, "node tests without a document flush immediately");
  });

  it("flushPendingNotificationsForTest drains a pending visible-document sweep", async () => {
    installDocument({ hidden: false });
    let calls = 0;
    const notifier = coalesceNotifier(() => {
      calls += 1;
    });

    notifier.schedule();
    await Promise.resolve();
    assert.equal(calls, 0);
    flushPendingNotificationsForTest();
    assert.equal(calls, 1);
  });
});

describe("createCoalescedHub", () => {
  it("keeps mutations out of the sweep: listeners see the merged payload once", () => {
    installDocument({ hidden: false });
    const hub = createCoalescedHub({
      merge: mergeUpdatesByAgentPubkey,
    });
    const payloads = [];
    hub.subscribe((update) => {
      payloads.push(update);
    });

    hub.notify({ agentPubkey: "a".repeat(64), events: [{ seq: 1 }] });
    hub.notify({ agentPubkey: "a".repeat(64), events: [{ seq: 2 }] });
    assert.equal(payloads.length, 0);

    hub.flush();
    assert.equal(payloads.length, 1);
    assert.deepEqual(
      payloads[0].events.map((event) => event.seq),
      [1, 2],
    );
  });

  it("connection-state (generic) plus an agent update in one tick is one sweep", () => {
    installDocument({ hidden: false });
    const hub = createCoalescedHub({
      merge: mergeUpdatesByAgentPubkey,
    });
    let calls = 0;
    let last;
    hub.subscribe((update) => {
      calls += 1;
      last = update;
    });

    hub.notify();
    hub.notify({ agentPubkey: "b".repeat(64), events: [{ seq: 7 }] });
    hub.flush();

    assert.equal(calls, 1);
    assert.equal(last.agentPubkey, "b".repeat(64));
    assert.equal(last.events[0].seq, 7);
  });
});
