import assert from "node:assert/strict";
import test from "node:test";

import {
  buildEventNotificationTarget,
  buildFeedItemNotificationTarget,
} from "./target.ts";

test("builds a complete target from a relay event", () => {
  assert.deepEqual(
    buildEventNotificationTarget(
      {
        content: "hello",
        created_at: 123,
        id: "event-id",
        kind: 9,
        pubkey: "sender",
        tags: [
          ["h", "channel-id"],
          ["e", "root-id", "", "root"],
          ["e", "parent-id", "", "reply"],
        ],
      },
      { id: "channel-id", name: "ship-room" },
    ),
    {
      channelId: "channel-id",
      channelName: "ship-room",
      content: "hello",
      createdAt: 123,
      eventId: "event-id",
      kind: 9,
      pubkey: "sender",
      threadRootId: "root-id",
    },
  );
});

test("builds a target from a feed item", () => {
  const target = buildFeedItemNotificationTarget({
    id: "feed-event",
    kind: 9,
    pubkey: "sender",
    content: "ping",
    createdAt: 456,
    channelId: "channel-id",
    channelName: "ship-room",
    tags: [
      ["e", "root-id", "", "root"],
      ["e", "parent-id", "", "reply"],
    ],
    category: "mention",
  });
  assert.equal(target.eventId, "feed-event");
  assert.equal(target.threadRootId, "root-id");
});
