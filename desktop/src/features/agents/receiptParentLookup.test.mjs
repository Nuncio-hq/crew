import assert from "node:assert/strict";
import test from "node:test";

import { buildReceiptParentFilter } from "./receiptParentLookup.ts";

test("receipt parent lookup is exact-id scoped and accepts every producer kind", () => {
  const filter = buildReceiptParentFilter(["a".repeat(64), "b".repeat(64)]);

  assert.deepEqual(filter, {
    ids: ["a".repeat(64), "b".repeat(64)],
    limit: 2,
  });
  assert.equal("kinds" in filter, false);
});
