import assert from "node:assert/strict";
import test from "node:test";

import {
  createHydrationRetryController,
  enumerateDurableActionEvents,
  isPermanentHydrationError,
  mergeDurableActionEvents,
} from "./durableActionHydration.ts";

test("hydration retry controller retries a failed exhaustive load", async () => {
  let attempts = 0;
  const timers = [];
  const errors = [];
  const controller = createHydrationRetryController({
    hydrate: async () => {
      attempts += 1;
      if (attempts === 1) throw new Error("history unavailable");
    },
    onError: (error) => errors.push(error),
    retryDelayMs: 5_000,
    setTimeoutFn: (callback, delayMs) => {
      timers.push({ callback, delayMs });
      return timers.length;
    },
    clearTimeoutFn: () => {},
  });

  await controller.run();
  assert.equal(attempts, 1);
  assert.equal(timers.length, 1);
  assert.equal(timers[0].delayMs, 5_000);
  assert.equal(errors.length, 1);

  timers[0].callback();
  for (let index = 0; index < 3; index++) {
    await Promise.resolve();
  }
  assert.equal(attempts, 2);

  controller.stop();
});

test("hydration retry controller does not loop on permanent policy failure", async () => {
  let attempts = 0;
  const timers = [];
  const controller = createHydrationRetryController({
    hydrate: async () => {
      attempts += 1;
      throw new Error("forbidden: invalid filter policy rejected");
    },
    onError: () => {},
    retryDelayMs: 5_000,
    setTimeoutFn: (callback) => {
      timers.push(callback);
      return timers.length;
    },
    clearTimeoutFn: () => {},
  });

  await controller.run();
  assert.equal(attempts, 1);
  assert.equal(timers.length, 0);
});

test("hydration treats ordinary 4xx as permanent but keeps timeout/rate-limit retryable", () => {
  assert.equal(isPermanentHydrationError(new Error("relay HTTP 403")), true);
  assert.equal(isPermanentHydrationError(new Error("relay HTTP 408")), false);
  assert.equal(isPermanentHydrationError(new Error("relay HTTP 429")), false);
});

test("hydration recognizes every terminal CLOSED policy family", () => {
  for (const reason of [
    "blocked: owner denied",
    "invalid: bad filter",
    "pow: insufficient work",
    "duplicate: subscription",
    "unsupported: filter",
    "terminal error: policy",
  ]) {
    assert.equal(isPermanentHydrationError(new Error(reason)), true, reason);
  }
});

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

test("exhaustively partitions a full timestamp bucket by event-id prefix", async () => {
  const all = [
    event(`${"a0"}${"0".repeat(62)}`, 20),
    event(`${"a1"}${"0".repeat(62)}`, 20),
    event(`${"b0"}${"0".repeat(62)}`, 20),
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
              candidate.created_at <= filter.until) &&
            (filter.ids === undefined ||
              filter.ids.some((prefix) => candidate.id.startsWith(prefix))),
        )
        .slice(0, filter.limit);
    },
    { kinds: [46043], "#h": ["channel"] },
    2,
  );

  assert.deepEqual(
    result.map((candidate) => candidate.id).sort(),
    all.map((candidate) => candidate.id).sort(),
  );
  assert.ok(filters.some((filter) => filter.ids?.includes("a")));
  assert.ok(filters.some((filter) => filter.ids?.includes("a0")));
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
