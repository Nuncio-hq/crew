import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const sourcePath = new URL("./useLiveHomeFeedActions.ts", import.meta.url);

test("durable home hydration uses the reusable retry state machine", async () => {
  const source = await readFile(sourcePath, "utf8");

  assert.match(source, /createHydrationRetryController/);
  assert.match(source, /hydrationRetry\.stop\(\)/);
  assert.doesNotMatch(source, /hydrationRetryTimer/);
});
