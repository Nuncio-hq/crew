import assert from "node:assert/strict";
import test from "node:test";

import { shouldHoldMissingThreadHead } from "./useThreadTargetSync.ts";

test("holds a missing head while its matching route target resolves", () => {
  assert.equal(
    shouldHoldMissingThreadHead({
      isRouteTargetResolving: true,
      openThreadHeadId: "thread-head",
      routeThreadTargetId: "thread-head",
    }),
    true,
  );
});

test("does not hold a missing head after route resolution settles", () => {
  assert.equal(
    shouldHoldMissingThreadHead({
      isRouteTargetResolving: false,
      openThreadHeadId: "thread-head",
      routeThreadTargetId: "thread-head",
    }),
    false,
  );
});

test("does not hold an unrelated missing head during route resolution", () => {
  assert.equal(
    shouldHoldMissingThreadHead({
      isRouteTargetResolving: true,
      openThreadHeadId: "other-head",
      routeThreadTargetId: "thread-head",
    }),
    false,
  );
});
