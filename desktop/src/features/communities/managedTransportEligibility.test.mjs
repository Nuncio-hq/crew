import assert from "node:assert/strict";
import test from "node:test";
import {
  createTransportEligibilityQueue,
  completeCommunityRemoval,
} from "./managedTransportEligibility.ts";

function deferred() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("cancel invalidation cannot be overtaken by an already-enqueued rejoin enable", async () => {
  const gate = deferred();
  const calls = [];
  const update = createTransportEligibilityQueue(async (_relay, enabled) => {
    calls.push(enabled ? "enable" : "disable");
    if (enabled) await gate.promise;
  });
  const enable = update("ws://fixture", true);
  const disable = update("ws://fixture", false);
  await Promise.resolve();
  assert.deepEqual(calls, ["enable"]);
  gate.resolve();
  await Promise.all([enable, disable]);
  assert.deepEqual(calls, ["enable", "disable"]);
});

test("failed command propagates but does not permanently block the queue", async () => {
  const calls = [];
  const update = createTransportEligibilityQueue(async (_relay, enabled) => {
    calls.push(enabled);
    if (enabled) throw new Error("native unavailable");
  });
  await assert.rejects(update("ws://fixture", true), /native unavailable/);
  await update("ws://fixture", false);
  assert.deepEqual(calls, [true, false]);
});

test("failed invalidation retains the local community and retry path", async () => {
  const calls = [];
  await assert.rejects(
    completeCommunityRemoval(
      async () => {
        calls.push("left");
        return { status: "left" };
      },
      async () => {
        calls.push("invalidate");
        throw new Error("native unavailable");
      },
      () => {
        calls.push("removed");
      },
    ),
    /native unavailable/,
  );
  assert.deepEqual(calls, ["left", "invalidate"]);
});

test("successful removal is ordered leave then invalidate then local cleanup", async () => {
  const calls = [];
  const result = await completeCommunityRemoval(
    async () => {
      calls.push("left");
      return { status: "already-absent" };
    },
    async () => {
      calls.push("invalidate");
    },
    () => {
      calls.push("removed");
    },
  );
  assert.deepEqual(calls, ["left", "invalidate", "removed"]);
  assert.equal(result.status, "already-absent");
});
