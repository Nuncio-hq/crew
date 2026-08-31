import assert from "node:assert/strict";
import test from "node:test";

import { resolveChannelDisplayLabel } from "./channelLabels.ts";

const SELF = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const AGENT =
  "313bfaeb518ecfcbd0dabae05ba29f9133153adacac34628ece0854496961244";

function dm(name) {
  return {
    id: "dm-1",
    name,
    channelType: "dm",
    visibility: "private",
    description: "",
    topic: null,
    purpose: null,
    memberCount: 2,
    memberPubkeys: [SELF, AGENT],
    lastMessageAt: null,
    archivedAt: null,
    participants: [SELF, AGENT],
    participantPubkeys: [SELF, AGENT],
    isMember: true,
    ttlSeconds: null,
    ttlDeadline: null,
  };
}

test("generic DM names resolve to the other participant's profile label", () => {
  assert.equal(
    resolveChannelDisplayLabel(dm("DM"), SELF, {
      [AGENT]: {
        displayName: "Hermes Default",
        name: null,
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: SELF,
      },
    }),
    "Hermes Default",
  );
});

test("a pubkey-named DM also resolves to the participant profile label", () => {
  assert.equal(
    resolveChannelDisplayLabel(dm(AGENT), SELF, {
      [AGENT]: {
        displayName: "Hermes Default",
        name: null,
        avatarUrl: null,
        nip05Handle: null,
        isAgent: true,
        ownerPubkey: SELF,
      },
    }),
    "Hermes Default",
  );
});
