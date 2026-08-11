import assert from "node:assert/strict";
import test from "node:test";

import { finalizeEvent } from "nostr-tools/pure";
import {
  isVerifiedRelayEvent,
  relayEventMatchesFilter,
} from "./relayEventVerification.ts";

const channelFilter = {
  kinds: [7],
  "#h": ["channel-a"],
};

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
test("relay event verification rejects malformed event data without throwing", () => {
  const missingSignature = {
    id: "event",
    kind: 1,
    pubkey: "pubkey",
    created_at: 1,
    tags: [],
    content: "missing signature",
  };
  const malformedTags = {
    ...missingSignature,
    sig: "not-a-signature",
    tags: null,
  };

  assert.doesNotThrow(() => isVerifiedRelayEvent(missingSignature));
  assert.doesNotThrow(() => isVerifiedRelayEvent(malformedTags));
  assert.equal(isVerifiedRelayEvent(missingSignature), false);
  assert.equal(isVerifiedRelayEvent(malformedTags), false);
});

test("h-less reactions match channel filters through relay-derived context", () => {
  assert.equal(
    relayEventMatchesFilter(
      {
        id: "reaction",
        pubkey: "author",
        created_at: 1,
        kind: 7,
        tags: [["e", "target"]],
        content: "👍",
        sig: "sig",
      },
      channelFilter,
    ),
    true,
  );
});

test("events with matching h tags match channel filters", () => {
  assert.equal(
    relayEventMatchesFilter(
      {
        id: "message",
        pubkey: "author",
        created_at: 1,
        kind: 9,
        tags: [["h", "channel-a"]],
        content: "message",
        sig: "sig",
      },
      { "#h": ["channel-a"] },
    ),
    true,
  );
});

test("events with mismatching h tags do not match channel filters", () => {
  assert.equal(
    relayEventMatchesFilter(
      {
        id: "message",
        pubkey: "author",
        created_at: 1,
        kind: 9,
        tags: [["h", "channel-b"]],
        content: "message",
        sig: "sig",
      },
      { "#h": ["channel-a"] },
    ),
    false,
  );
});

test("non-h tag filters remain strict for tag-less events", () => {
  assert.equal(
    relayEventMatchesFilter(
      {
        id: "reaction",
        pubkey: "author",
        created_at: 1,
        kind: 7,
        tags: [["e", "target"]],
        content: "👍",
        sig: "sig",
      },
      { "#e": ["other-target"] },
    ),
    false,
  );
});
