import assert from "node:assert/strict";
import test from "node:test";

import { getOverrides, OVERRIDES_KEY, setOverride } from "./store.ts";

function installStorage(value) {
  const values = new Map([[OVERRIDES_KEY, JSON.stringify(value)]]);
  const writes = [];
  globalThis.window = {
    localStorage: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, next) => {
        writes.push([key, String(next)]);
        values.set(key, String(next));
      },
    },
  };
  return { values, writes };
}

test("getOverrides drops unknown feature IDs without writing to storage", () => {
  const { values, writes } = installStorage({
    pulse: true,
    removedFeature: false,
  });

  assert.deepEqual(getOverrides(), { pulse: true });
  assert.deepEqual(writes, []);
  assert.equal(
    values.get(OVERRIDES_KEY),
    JSON.stringify({ pulse: true, removedFeature: false }),
  );
});

test("setOverride persists filtered overrides", () => {
  const { values } = installStorage({
    pulse: true,
    removedFeature: false,
  });

  setOverride("forum", true);

  assert.equal(
    values.get(OVERRIDES_KEY),
    JSON.stringify({ pulse: true, forum: true }),
  );
});

test("thread-scoped ACP session preference persists without changing existing experiments", () => {
  const { values } = installStorage({ pulse: true, forum: false });

  setOverride("threadScopedAcpSessions", true);

  assert.equal(
    values.get(OVERRIDES_KEY),
    JSON.stringify({
      pulse: true,
      forum: false,
      threadScopedAcpSessions: true,
    }),
  );
});

test("getOverrides drops non-boolean values", () => {
  installStorage({ pulse: "yes", forum: false });

  assert.deepEqual(getOverrides(), { forum: false });
});
