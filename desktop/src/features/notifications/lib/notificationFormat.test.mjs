import assert from "node:assert/strict";
import test from "node:test";

import { formatMessageNotification } from "./notificationFormat.ts";
import { senderNameFromSummary } from "./senderName.ts";

test("DM title uses the resolved sender name", () => {
  assert.deepEqual(
    formatMessageNotification({
      source: "dm",
      senderName: "Taylor",
      channelName: "taylor-wes",
      content: "hey there",
    }),
    { title: "Taylor", body: "hey there" },
  );
});

test("DM title falls back to channel name and then generic copy", () => {
  assert.equal(
    formatMessageNotification({
      source: "dm",
      senderName: null,
      channelName: "taylor-wes",
      content: "hi",
    }).title,
    "taylor-wes",
  );
  assert.equal(
    formatMessageNotification({
      source: "dm",
      senderName: " ",
      channelName: " ",
      content: "hi",
    }).title,
    "Direct message",
  );
});

test("thread replies and feed alerts use stable sender-first copy", () => {
  assert.equal(
    formatMessageNotification({
      source: "thread_reply",
      senderName: "Taylor",
      channelName: "ship-room",
      content: "done",
    }).title,
    "Taylor replied in #ship-room",
  );
  assert.equal(
    formatMessageNotification({
      source: "mention",
      senderName: null,
      channelName: "ship-room",
      content: "@you",
    }).title,
    "@Mention in #ship-room",
  );
});

test("approval and needs-action titles retain home-feed conventions", () => {
  assert.deepEqual(
    formatMessageNotification({
      source: "approval",
      senderName: "Taylor",
      channelName: "ops",
      content: "",
    }),
    {
      title: "Taylor requested approval in #ops",
      body: "A workflow is waiting for your approval.",
    },
  );
  assert.deepEqual(
    formatMessageNotification({
      source: "needs_action",
      senderName: null,
      channelName: null,
      content: "",
    }),
    {
      title: "Needs Action",
      body: "Something in Buzz needs your attention.",
    },
  );
});

test("sender labels never fall back to pubkeys", () => {
  assert.equal(
    senderNameFromSummary({
      displayName: " ",
      avatarUrl: null,
      nip05Handle: "taylor@example.com",
      ownerPubkey: null,
    }),
    "taylor@example.com",
  );
  assert.equal(senderNameFromSummary(null), null);
});
