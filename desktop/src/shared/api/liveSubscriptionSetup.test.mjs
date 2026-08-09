import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import { establishLiveSubscription } from "./liveSubscriptionSetup.ts";

const originalWindow = globalThis.window;

afterEach(() => {
  globalThis.window = originalWindow;
});

function installTimerHarness() {
  let callback;
  const cleared = [];
  globalThis.window = {
    setTimeout(fn) {
      callback = fn;
      return 41;
    },
    clearTimeout(id) {
      cleared.push(id);
    },
  };
  return {
    fire: () => callback?.(),
    cleared,
  };
}

function setupInput(overrides = {}) {
  return {
    subscriptions: new Map(),
    subId: "live-1",
    filter: { kinds: [9] },
    onEvent: () => {},
    recoveryFloorCreatedAt: 1,
    sendRequest: async () => {},
    closeSubscription: async () => {},
    ...overrides,
  };
}

test("live setup timeout fails closed, removes aliases, and requests CLOSE", async () => {
  const timer = installTimerHarness();
  const closed = [];
  const input = setupInput({
    closeSubscription: async (subId) => {
      closed.push(subId);
    },
  });

  const setup = establishLiveSubscription(input);
  await Promise.resolve();
  timer.fire();
  await assert.rejects(setup, /readiness timed out/);
  await Promise.resolve();

  assert.equal(input.subscriptions.size, 0);
  assert.deepEqual(closed, ["live-1"]);
  assert.deepEqual(timer.cleared, [41]);
});

test("setup deadline also bounds a send request that never settles", async () => {
  const timer = installTimerHarness();
  const input = setupInput({
    sendRequest: () => new Promise(() => {}),
  });

  const setup = establishLiveSubscription(input);
  timer.fire();
  await assert.rejects(
    Promise.race([
      setup,
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error("setup remained blocked")), 50),
      ),
    ]),
    /readiness timed out/,
  );
  assert.equal(input.subscriptions.size, 0);
});

test("a request that settles after timeout is closed again after its late send", async () => {
  const timer = installTimerHarness();
  const closed = [];
  let finishSend;
  const input = setupInput({
    sendRequest: () =>
      new Promise((resolve) => {
        finishSend = resolve;
      }),
    closeSubscription: async (subId) => {
      closed.push(subId);
    },
  });

  const setup = establishLiveSubscription(input);
  timer.fire();
  await assert.rejects(setup, /readiness timed out/);
  assert.deepEqual(closed, []);
  finishSend();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(closed, ["live-1"]);
});

test("stale setup timeout closes only its original wire and preserves a recovery replacement", async () => {
  const timer = installTimerHarness();
  const closed = [];
  const input = setupInput({
    closeSubscription: async (subId) => {
      closed.push(subId);
    },
  });

  const setup = establishLiveSubscription(input);
  await Promise.resolve();
  const subscription = input.subscriptions.get("live-1");
  subscription.currentSubId = "live-1:recovery:1";
  input.subscriptions.set(subscription.currentSubId, subscription);
  timer.fire();
  await assert.rejects(setup, /readiness timed out/);
  await Promise.resolve();

  assert.deepEqual(closed, ["live-1"]);
  assert.equal(input.subscriptions.size, 1);
  assert.equal(input.subscriptions.get("live-1:recovery:1"), subscription);
});

test("replacement EOSE readiness clears the deadline without closing", async () => {
  const timer = installTimerHarness();
  const closed = [];
  const input = setupInput({
    closeSubscription: async (subId) => {
      closed.push(subId);
    },
  });

  const setup = establishLiveSubscription(input);
  await Promise.resolve();
  const subscription = input.subscriptions.get("live-1");
  assert.equal(subscription?.mode, "live");
  subscription.resolveReady();
  await setup;

  assert.deepEqual(closed, []);
  assert.deepEqual(timer.cleared, [41]);
});
