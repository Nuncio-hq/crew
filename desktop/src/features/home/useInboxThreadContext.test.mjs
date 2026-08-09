import assert from "node:assert/strict";
import { test } from "node:test";

import { deriveInboxThreadSelectionFromVerifiedEvent } from "./useInboxThreadContext.ts";

function event(overrides = {}) {
  return {
    id: "selected-event",
    pubkey: "a".repeat(64),
    created_at: 1,
    kind: 9,
    tags: [
      ["h", "verified-channel"],
      ["e", "verified-root", "", "root"],
      ["e", "verified-parent", "", "reply"],
    ],
    content: "hello",
    sig: "b".repeat(128),
    ...overrides,
  };
}

test("thread selection derives only from the exact verified event", () => {
  assert.deepEqual(
    deriveInboxThreadSelectionFromVerifiedEvent(event(), "selected-event"),
    {
      selectedChannelId: "verified-channel",
      selectedEventId: "selected-event",
      selectedParentId: "verified-parent",
      selectedThreadRootId: "verified-root",
    },
  );
  assert.equal(
    deriveInboxThreadSelectionFromVerifiedEvent(
      event({ id: "different-event" }),
      "selected-event",
    ),
    null,
  );
  assert.equal(
    deriveInboxThreadSelectionFromVerifiedEvent(
      event({ tags: [["e", "verified-root", "", "root"]] }),
      "selected-event",
    ),
    null,
  );
});
