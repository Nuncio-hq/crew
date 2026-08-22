import assert from "node:assert/strict";
import test from "node:test";

import {
  activateDesktopNotificationTarget,
  createDesktopNotificationActivationQueue,
  deriveShellRoute,
  markAllReadSources,
  shouldBounceForChannelNotification,
} from "./AppShell.helpers.ts";

const CHANNEL = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const ROOT = "1".repeat(64);

test("shouldBounceForChannelNotification_allowsTopLevelChannelMessages", () => {
  assert.equal(shouldBounceForChannelNotification([["h", "channel"]]), true);
});

test("shouldBounceForChannelNotification_suppressesThreadReplies", () => {
  assert.equal(
    shouldBounceForChannelNotification([
      ["h", "channel"],
      ["e", "root", "", "reply"],
    ]),
    false,
  );
});

test("shouldBounceForChannelNotification_allowsBroadcastReplies", () => {
  assert.equal(
    shouldBounceForChannelNotification([
      ["h", "channel"],
      ["e", "root", "", "reply"],
      ["broadcast", "1"],
    ]),
    true,
  );
});

test("markAllReadSources clears Inbox overrides and active thread activity", () => {
  const calls = [];

  markAllReadSources({
    activeChannelId: "active-channel",
    channelActivityItems: [
      { channelId: "another-channel", createdAt: 100 },
      { channelId: "active-channel", createdAt: 200 },
      { channelId: "active-channel", createdAt: 300 },
    ],
    unreadFeedItemIds: new Set(["first-inbox-item", "second-inbox-item"]),
    undoUnreadFeedItem: (itemId) => calls.push(`inbox:${itemId}`),
    markAllChannelReadMarkers: () => calls.push("channels"),
    markActiveChannelRead: (channelId, createdAt) =>
      calls.push(`active:${channelId}:${createdAt}`),
  });

  assert.deepEqual(calls, [
    "inbox:first-inbox-item",
    "inbox:second-inbox-item",
    "channels",
    "active:active-channel:300",
  ]);
});

test("deriveShellRoute does not treat /workbench as a picker place (#219)", () => {
  assert.deepEqual(deriveShellRoute("/workbench"), {
    selectedChannelId: null,
    selectedView: "home",
  });
  assert.deepEqual(deriveShellRoute(`/workbench/${CHANNEL}/${ROOT}`), {
    selectedChannelId: CHANNEL,
    selectedView: "channel",
  });
});

test("markAllReadSources skips the active marker without projected activity", () => {
  const calls = [];

  markAllReadSources({
    activeChannelId: "active-channel",
    channelActivityItems: [],
    unreadFeedItemIds: new Set(),
    undoUnreadFeedItem: () => calls.push("inbox"),
    markAllChannelReadMarkers: () => calls.push("channels"),
    markActiveChannelRead: () => calls.push("active"),
  });

  assert.deepEqual(calls, ["channels"]);
});

test("notification activation queue preserves click order", async () => {
  const calls = [];
  const resolvers = new Map();
  const queue = createDesktopNotificationActivationQueue((target) => {
    calls.push(`start:${target.channelId}`);
    return new Promise((resolve) => {
      resolvers.set(target.channelId, () => {
        calls.push(`finish:${target.channelId}`);
        resolve();
      });
    });
  });

  queue.enqueue({ channelId: "first", eventId: null, kind: null });
  queue.enqueue({ channelId: "second", eventId: null, kind: null });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls, ["start:first"]);

  resolvers.get("first")();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls, ["start:first", "finish:first", "start:second"]);
  resolvers.get("second")();
});

test("notification activation starts routing before a hung reveal", async () => {
  const calls = [];
  let resolveNavigation;
  const activation = activateDesktopNotificationTarget(
    { channelId: "channel", eventId: "event", kind: 9 },
    {
      goChannel: async () => calls.push("channel"),
      goHome: async () => calls.push("home"),
      openSearchHit: (_hit, behavior) => {
        calls.push(`message:${String(behavior?.force)}`);
        return new Promise((resolve) => {
          resolveNavigation = resolve;
        });
      },
      revealWindow: () => new Promise(() => {}),
    },
  );

  assert.deepEqual(calls, ["message:true"]);
  resolveNavigation();
  await activation;
});

test("notification activation queue drops pending targets after cancellation", async () => {
  const calls = [];
  let resolveFirst;
  const queue = createDesktopNotificationActivationQueue((target) => {
    calls.push(target.channelId);
    if (target.channelId === "first") {
      return new Promise((resolve) => {
        resolveFirst = resolve;
      });
    }
    return Promise.resolve();
  });

  queue.enqueue({ channelId: "first", eventId: null, kind: null });
  queue.enqueue({ channelId: "second", eventId: null, kind: null });
  await new Promise((resolve) => setImmediate(resolve));
  queue.cancel();
  resolveFirst();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls, ["first"]);
});

test("notification activation queue reports failures and continues", async () => {
  const calls = [];
  const errors = [];
  const queue = createDesktopNotificationActivationQueue(
    async (target) => {
      calls.push(target.channelId);
      if (target.channelId === "first") throw new Error("navigation failed");
    },
    (error) => errors.push(error),
  );

  queue.enqueue({ channelId: "first", eventId: null, kind: null });
  queue.enqueue({ channelId: "second", eventId: null, kind: null });
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls, ["first", "second"]);
  assert.equal(errors.length, 1);
  assert.match(errors[0].message, /navigation failed/);
});

test("notification activation without a channel opens home", async () => {
  const calls = [];
  await activateDesktopNotificationTarget(
    { channelId: null, eventId: "event", kind: 9 },
    {
      goChannel: async () => calls.push("channel"),
      goHome: async () => calls.push("home"),
      openSearchHit: async () => calls.push("message"),
      revealWindow: async () => calls.push("reveal"),
    },
  );

  assert.deepEqual(calls, ["home", "reveal"]);
});
