import assert from "node:assert/strict";
import { test } from "node:test";

import {
  commitDiscussionQuery,
  entityDiscussionQuery,
  groupDiscussionChannels,
  repositoryDiscussionQuery,
} from "./discussionChannels.ts";

const OWNER = "a".repeat(64);
const EVENT_ID = "b".repeat(64);
const ALICE = "c".repeat(64);
const BOB = "d".repeat(64);

test("discussion queries use canonical entity and repository tokens", () => {
  assert.equal(entityDiscussionQuery(EVENT_ID), EVENT_ID);
  assert.equal(
    repositoryDiscussionQuery({ owner: OWNER, dtag: "crew" }),
    `${OWNER} crew`,
  );
  const hash = "0123456789abcdef0123456789abcdef01234567";
  assert.equal(
    commitDiscussionQuery({ hash, shortHash: "0123456" }),
    `${hash} OR 0123456`,
  );
});

test("discussion hits group across channels by count and recency", () => {
  assert.deepEqual(
    groupDiscussionChannels([
      {
        channelId: "c1",
        channelName: "general",
        createdAt: 100,
        pubkey: ALICE,
      },
      { channelId: "c2", channelName: "design", createdAt: 300, pubkey: BOB },
      { channelId: "c1", channelName: "general", createdAt: 200, pubkey: BOB },
    ]),
    [
      {
        id: "c1",
        name: "general",
        messageCount: 2,
        lastActivityAt: 200,
        participants: [BOB, ALICE],
      },
      {
        id: "c2",
        name: "design",
        messageCount: 1,
        lastActivityAt: 300,
        participants: [BOB],
      },
    ],
  );
});
