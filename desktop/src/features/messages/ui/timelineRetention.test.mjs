import assert from "node:assert/strict";
import test from "node:test";

import { nextRetainedTimelineKeys } from "./timelineRetention.ts";

function keys(count) {
  return Array.from({ length: count }, (_, index) => `message-${index}`);
}

function fixedHeightList(count, scrollOffset) {
  const itemHeight = 100;
  return {
    viewportSize: 1_000,
    scrollOffset,
    scrollSize: count * itemHeight,
    findItemIndex(target) {
      return Math.min(count - 1, Math.floor(target / itemHeight));
    },
  };
}

test("bottom-of-feed retention keeps only the nearby four-viewport window", () => {
  const timelineKeys = keys(140);
  const retained = nextRetainedTimelineKeys(
    timelineKeys,
    new Set(),
    fixedHeightList(140, 13_000),
  );

  assert.equal(retained.size, 40);
  assert.deepEqual(
    [...retained],
    timelineKeys.slice(100),
    "opening a side panel must not reflow the entire paged channel history",
  );
});

test("mid-feed retention keeps the nearby window plus a two-viewport live tail", () => {
  const timelineKeys = keys(200);
  const retained = nextRetainedTimelineKeys(
    timelineKeys,
    new Set(),
    fixedHeightList(200, 10_000),
  );

  assert.equal(retained.size, 91);
  assert.deepEqual([...retained].slice(0, 3), [
    "message-70",
    "message-71",
    "message-72",
  ]);
  assert.deepEqual([...retained].slice(-3), [
    "message-197",
    "message-198",
    "message-199",
  ]);
});

test("eviction hysteresis preserves nearby retained rows and drops distant ones", () => {
  const timelineKeys = keys(200);
  const retained = nextRetainedTimelineKeys(
    timelineKeys,
    new Set(["message-49", "message-55", "message-170"]),
    fixedHeightList(200, 10_000),
  );

  assert.equal(retained.has("message-55"), true);
  assert.equal(retained.has("message-49"), false);
  assert.equal(retained.has("message-170"), false);
});

test("an unchanged retained window preserves its reference", () => {
  const timelineKeys = keys(40);
  const list = fixedHeightList(40, 3_000);
  const previous = nextRetainedTimelineKeys(timelineKeys, new Set(), list);
  const retained = nextRetainedTimelineKeys(timelineKeys, previous, list);

  assert.equal(retained, previous);
});
