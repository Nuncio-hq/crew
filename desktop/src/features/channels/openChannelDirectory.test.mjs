import assert from "node:assert/strict";
import test from "node:test";

import { isChannelReferenceOpenable } from "./openChannelDirectory.ts";

test("isChannelReferenceOpenable: members and open channels only", () => {
  const base = {
    id: "channel-id",
    name: "general",
    channelType: "stream",
    visibility: "private",
    description: "",
    topic: null,
    purpose: null,
    memberCount: 1,
    memberPubkeys: [],
    lastMessageAt: null,
    archivedAt: null,
    participants: [],
    participantPubkeys: [],
    isMember: false,
    ttlSeconds: null,
    ttlDeadline: null,
  };

  assert.equal(isChannelReferenceOpenable(undefined), false);
  assert.equal(isChannelReferenceOpenable(base), false);
  assert.equal(isChannelReferenceOpenable({ ...base, isMember: true }), true);
  assert.equal(
    isChannelReferenceOpenable({ ...base, visibility: "open" }),
    true,
  );
});
