import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const sourcePath = new URL("./relayClientSession.ts", import.meta.url);
const setupPath = new URL("./liveSubscriptionSetup.ts", import.meta.url);

test("live subscription readiness has a bounded setup deadline", async () => {
  const [source, setupSource] = await Promise.all([
    readFile(sourcePath, "utf8"),
    readFile(setupPath, "utf8"),
  ]);
  const subscribeBody = source.match(
    /private async subscribe\([\s\S]*?\n {2}private async sendRaw\(/,
  )?.[0];

  assert.ok(subscribeBody, "private live subscribe implementation is present");
  assert.match(subscribeBody, /establishLiveSubscription/);
  assert.match(setupSource, /LIVE_SUBSCRIPTION_READY_TIMEOUT_MS/);
  assert.match(setupSource, /deleteSubscriptionAliases/);
  assert.match(setupSource, /closeSubscription/);
});
