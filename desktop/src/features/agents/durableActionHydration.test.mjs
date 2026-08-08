import assert from "node:assert/strict";
import test from "node:test";

import {
  enumerateDurableActionEvents,
  mergeDurableActionEvents,
} from "./durableActionHydration.ts";

function event(id, createdAt) {
  return {
    id,
    pubkey: "a".repeat(64),
    created_at: createdAt,
    kind: 46040,
    tags: [],
    content: "{}",
    sig: "b".repeat(128),
  };
}

test("exhaustively pages durable actions and drains timestamp boundaries", async () => {
  const all = [
    event("newest", 30),
    event("boundary-a", 20),
    event("boundary-b", 20),
    event("oldest", 10),
  ];
  const filters = [];
  const result = await enumerateDurableActionEvents(
    async (filter) => {
      filters.push(filter);
      return all
        .filter(
          (candidate) =>
            (filter.since === undefined ||
              candidate.created_at >= filter.since) &&
            (filter.until === undefined ||
              candidate.created_at <= filter.until),
        )
        .sort(
          (left, right) =>
            right.created_at - left.created_at ||
            left.id.localeCompare(right.id),
        )
        .slice(0, filter.limit);
    },
    { kinds: [46040, 46041, 46042], "#h": ["channel"] },
    3,
  );

  assert.deepEqual(
    result.map((candidate) => candidate.id).sort(),
    all.map((candidate) => candidate.id).sort(),
  );
  assert.ok(
    filters.some((filter) => filter.since === 20 && filter.until === 20),
  );
  assert.ok(filters.some((filter) => filter.until === 19));
});

test("fails closed when one timestamp bucket exceeds the relay page limit", async () => {
  const all = [event("a", 20), event("b", 20)];
  await assert.rejects(
    enumerateDurableActionEvents(
      async (filter) =>
        all
          .filter(
            (candidate) =>
              (filter.since === undefined ||
                candidate.created_at >= filter.since) &&
              (filter.until === undefined ||
                candidate.created_at <= filter.until),
          )
          .slice(0, filter.limit),
      { kinds: [46043], "#h": ["channel"] },
      2,
    ),
    /timestamp bucket/,
  );
});

test("merges live events buffered during hydration before projecting transitions", () => {
  const request = { ...event("request", 10), kind: 46040 };
  const receipt = { ...event("receipt", 11), kind: 46043 };
  // Cross-host clocks may place a transition before its request. Authority
  // reconstruction still has to establish every request before transitions.
  const answer = { ...event("answer", 9), kind: 46041 };
  const review = { ...event("review", 13), kind: 7 };

  assert.deepEqual(
    mergeDurableActionEvents(
      [request],
      [receipt],
      [],
      [answer, receipt, review],
    ),
    {
      userInputEvents: [request, answer],
      receiptEvents: [receipt],
      reviewEvents: [review],
    },
  );
});
