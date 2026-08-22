import assert from "node:assert/strict";
import test from "node:test";

let pendingActivations = [];
let hangWindowInvokes = false;

const tauriInternals = {
  invoke(command) {
    if (command === "take_pending_activations") {
      const drained = pendingActivations;
      pendingActivations = [];
      return Promise.resolve(drained);
    }
    if (hangWindowInvokes && command.startsWith("plugin:window|")) {
      return new Promise(() => {});
    }
    if (command === "plugin:event|listen") {
      return Promise.resolve(1);
    }
    return Promise.resolve(undefined);
  },
  transformCallback() {
    return 0;
  },
  metadata: { currentWindow: { label: "main" } },
};

const testWindow = new EventTarget();
testWindow.__TAURI_INTERNALS__ = tauriInternals;
testWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
function StubNotification() {}
StubNotification.permission = "granted";
testWindow.Notification = StubNotification;
globalThis.window = testWindow;
globalThis.document = new EventTarget();
globalThis.isTauri = true;
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: { platform: "MacIntel", userAgent: "buzz-test" },
});

const { revealDesktopAppWindow, listenForDesktopNotificationActions } =
  await import("./desktop.ts");

test("reveal resolves via timeout when a window invoke hangs", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  hangWindowInvokes = true;
  let settled = false;
  const reveal = revealDesktopAppWindow().then(() => {
    settled = true;
  });

  await Promise.resolve();
  await Promise.resolve();
  assert.equal(settled, false);
  t.mock.timers.tick(1_500);
  await reveal;
  assert.equal(settled, true);
  hangWindowInvokes = false;
});

test("focus re-drains pending macOS activations", async () => {
  const received = [];
  const dispose = await listenForDesktopNotificationActions((target) => {
    received.push(target);
  });

  pendingActivations = [
    { channelId: "channel-1", eventId: "event-1", kind: 9 },
  ];
  window.dispatchEvent(new Event("focus"));
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(received[0].channelId, "channel-1");
  dispose();
  pendingActivations = [];
});

test("visibilitychange re-drains pending macOS activations", async () => {
  const received = [];
  const dispose = await listenForDesktopNotificationActions((target) => {
    received.push(target);
  });

  pendingActivations = [
    { channelId: "channel-2", eventId: "event-2", kind: 9 },
  ];
  document.dispatchEvent(new Event("visibilitychange"));
  await new Promise((resolve) => setImmediate(resolve));

  assert.equal(received[0].channelId, "channel-2");
  dispose();
  pendingActivations = [];
});
