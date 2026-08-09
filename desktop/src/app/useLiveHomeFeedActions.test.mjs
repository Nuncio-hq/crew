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

test("durable live overlap is installed before history hydration starts", async () => {
  const source = await readFile(sourcePath, "utf8");
  const hydrateAfterSubscriptions = source.lastIndexOf(
    "void hydrationRetry.run();",
  );
  const startSubscriptions = source.lastIndexOf("void startSubscriptions();");

  assert.ok(hydrateAfterSubscriptions >= 0);
  assert.ok(startSubscriptions > hydrateAfterSubscriptions);
  assert.match(
    source,
    /disposers = nextDisposers;[\s\S]*void hydrationRetry\.run\(\);/,
  );
});
