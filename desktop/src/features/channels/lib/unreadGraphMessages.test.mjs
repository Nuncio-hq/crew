import assert from "node:assert/strict";
import test from "node:test";

import { mergeUnreadGraphMessages } from "./unreadGraphMessages.ts";
import {
  buildDirectReplyIdsByParentId,
  collectReplyDescendantIds,
} from "./subtreeCreatedAt.ts";

const msg = (id, parentId, createdAt, rootId) => ({
  id,
  parentId: parentId ?? null,
  rootId: rootId ?? parentId ?? null,
  createdAt,
  pubkey: "author",
});

test("returns the timeline array itself when there are no cached replies", () => {
  const timeline = [msg("root", null, 1)];
  assert.equal(mergeUnreadGraphMessages(timeline, []), timeline);
  assert.equal(mergeUnreadGraphMessages(timeline, undefined), timeline);
});

test("returns the timeline array itself when every cached reply is already a row", () => {
  const timeline = [msg("root", null, 1), msg("r1", "root", 2)];
  assert.equal(
    mergeUnreadGraphMessages(timeline, [msg("r1", "root", 2)]),
    timeline,
  );
});

test("counts a reply carried by both sources once", () => {
  const merged = mergeUnreadGraphMessages(
    [msg("root", null, 1), msg("r1", "root", 2)],
    [msg("r1", "root", 2), msg("r2", "root", 3)],
  );
  assert.deepEqual(
    merged.map((message) => message.id),
    ["root", "r1", "r2"],
  );
});

test("merges chronologically so the graph keeps timeline order", () => {
  const merged = mergeUnreadGraphMessages(
    [msg("root", null, 10), msg("live", "root", 40)],
    [msg("older", "root", 20)],
  );
  assert.deepEqual(
    merged.map((message) => message.id),
    ["root", "older", "live"],
  );
});

test("reconnects a subtree severed by a reply the timeline never carried", () => {
  // The intermediate reply exists only in the per-root thread cache, so the
  // parent-adjacency walk over the timeline alone cannot reach the live leaf.
  const timeline = [msg("root", null, 10), msg("leaf", "branch", 40, "root")];
  const cached = [msg("branch", "root", 20, "root")];

  const timelineOnly = collectReplyDescendantIds(
    "root",
    buildDirectReplyIdsByParentId(timeline),
  );
  assert.deepEqual(timelineOnly, []);

  const merged = collectReplyDescendantIds(
    "root",
    buildDirectReplyIdsByParentId(mergeUnreadGraphMessages(timeline, cached)),
  );
  assert.deepEqual(merged.sort(), ["branch", "leaf"]);
});
