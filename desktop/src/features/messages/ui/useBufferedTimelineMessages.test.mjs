import assert from "node:assert/strict";
import test from "node:test";

import { selectBufferedTimelineMessages } from "./useBufferedTimelineMessages.ts";

const rows = (...ids) => ids.map((id) => ({ id }));

test("freezes live arrivals after the semantic tail while scrolled up", () => {
  assert.deepEqual(
    selectBufferedTimelineMessages({
      frozenMessageIds: ["a", "b", "c"],
      isAtBottom: false,
      messages: rows("a", "b", "c", "d", "e"),
    }).map(({ id }) => id),
    ["a", "b", "c"],
  );
});

test("admits older-history prepends without exposing buffered arrivals", () => {
  assert.deepEqual(
    selectBufferedTimelineMessages({
      frozenMessageIds: ["a", "b", "c"],
      isAtBottom: false,
      messages: rows("older-a", "older-b", "a", "b", "c", "d"),
    }).map(({ id }) => id),
    ["older-a", "older-b", "a", "b", "c"],
  );
});

test("releases the full logical dataset at bottom", () => {
  const messages = rows("a", "b", "c", "d");
  assert.deepEqual(
    selectBufferedTimelineMessages({
      frozenMessageIds: ["a", "b"],
      isAtBottom: true,
      messages,
    }),
    messages,
  );
});

test("accepts an authoritative replacement when its old tail disappeared", () => {
  const messages = rows("x", "y");
  assert.deepEqual(
    selectBufferedTimelineMessages({
      frozenMessageIds: ["old-tail"],
      isAtBottom: false,
      messages,
    }),
    messages,
  );
});

test("admits older pages that sort between a frozen live-overlay row and the page head (#154)", () => {
  // Reconnect keeps a pre-disconnect live-overlay event (`seen`) that is older
  // than the refreshed head page (`h1`…`h3`). Freezing at bottom captures
  // [seen, h1, h2, h3]. Paging older then inserts interstitial rows between
  // `seen` and `h1`. Those rows must render while the reader is scrolled up.
  assert.deepEqual(
    selectBufferedTimelineMessages({
      frozenMessageIds: ["seen", "h1", "h2", "h3"],
      isAtBottom: false,
      messages: rows(
        "seen",
        "older-a",
        "older-b",
        "h1",
        "h2",
        "h3",
        "live-new",
      ),
    }).map(({ id }) => id),
    ["seen", "older-a", "older-b", "h1", "h2", "h3"],
  );
});
