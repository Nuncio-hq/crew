import assert from "node:assert/strict";
import test from "node:test";

import { finalizeEvent } from "nostr-tools/pure";
import { isVerifiedRelayEvent } from "./relayEventVerification.ts";

test("relay event verification rejects tampered relay data", () => {
  const event = finalizeEvent(
    { kind: 1, created_at: 1, tags: [], content: "verified" },
    new Uint8Array(32).fill(1),
  );

  assert.equal(isVerifiedRelayEvent(event), true);
  const tamperedContent = JSON.parse(JSON.stringify(event));
  tamperedContent.content = "tampered";
  const tamperedSignature = JSON.parse(JSON.stringify(event));
  tamperedSignature.sig = "not-a-signature";
  assert.equal(isVerifiedRelayEvent(tamperedContent), false);
  assert.equal(isVerifiedRelayEvent(tamperedSignature), false);
});
