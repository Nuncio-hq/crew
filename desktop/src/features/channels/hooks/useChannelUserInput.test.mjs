import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { describe, it } from "node:test";

const sourcePath = new URL("./useChannelUserInput.ts", import.meta.url);

describe("useChannelUserInput durable reconstruction wiring", () => {
  it("uses a live-only overlap plus exhaustive durable history without count eviction", async () => {
    const source = await readFile(sourcePath, "utf8");

    assert.match(source, /enumerateDurableActionEvents/);
    assert.match(source, /createHydrationRetryController/);
    assert.match(source, /hydrationRetry\.stop\(\)/);
    assert.match(source, /buildChannelUserInputFilter\(channelId,\s*0\)/);
    assert.doesNotMatch(source, /RETAINED_EVENTS/);
    assert.doesNotMatch(source, /\.slice\(0,\s*\d+\)/);
  });
});
