import assert from "node:assert/strict";
import test from "node:test";

import {
  diffAddedMentionPubkeys,
  diffRemovedMentionPubkeys,
} from "./threading.ts";

const ALICE = "a".repeat(64);
const BOB = "b".repeat(64);
const SELF = "c".repeat(64);

test("returns mentions the edit newly adds", () => {
  // Original mentioned Alice; edit adds Bob.
  assert.deepEqual(diffAddedMentionPubkeys([ALICE], [ALICE, BOB], SELF), [BOB]);
});

test("typo-fix edit with unchanged mentions re-wakes nobody", () => {
  assert.deepEqual(diffAddedMentionPubkeys([ALICE], [ALICE], SELF), []);
});

test("adding the first mention to a previously unmentioned body", () => {
  assert.deepEqual(diffAddedMentionPubkeys([], [ALICE], SELF), [ALICE]);
});

test("removing a mention adds nothing", () => {
  assert.deepEqual(diffAddedMentionPubkeys([ALICE, BOB], [ALICE], SELF), []);
});

test("case-only difference is not treated as newly added", () => {
  // Original stored uppercase, edit resolves lowercase (or vice versa).
  assert.deepEqual(
    diffAddedMentionPubkeys([ALICE.toUpperCase()], [ALICE], SELF),
    [],
  );
});

test("self-mention added in the edit is scrubbed, never notified", () => {
  assert.deepEqual(diffAddedMentionPubkeys([ALICE], [ALICE, SELF], SELF), []);
});

test("duplicate added mention collapses to one", () => {
  assert.deepEqual(diffAddedMentionPubkeys([], [BOB, BOB, BOB], SELF), [BOB]);
});

test("re-adding a removed mention counts as newly added", () => {
  // Original had no Bob (he was removed in a prior state); this edit adds him.
  assert.deepEqual(diffAddedMentionPubkeys([ALICE], [ALICE, BOB], SELF), [BOB]);
});

test("returns mentions the edit removes", () => {
  assert.deepEqual(
    diffRemovedMentionPubkeys([ALICE, BOB], [ALICE], SELF),
    [BOB],
  );
});

test("typo-fix edit with unchanged mentions removes nobody", () => {
  assert.deepEqual(diffRemovedMentionPubkeys([ALICE], [ALICE], SELF), []);
});

test("removed-set is the complement of the added-set over the same inputs", () => {
  const original = [ALICE];
  const edited = [BOB];
  const added = diffAddedMentionPubkeys(original, edited, SELF);
  const removed = diffRemovedMentionPubkeys(original, edited, SELF);
  assert.deepEqual(added, [BOB]);
  assert.deepEqual(removed, [ALICE]);
  // Self never appears in either set.
  assert.ok(!added.includes(SELF));
  assert.ok(!removed.includes(SELF));
});

test("self-pubkey never appears in the removed set", () => {
  assert.deepEqual(diffRemovedMentionPubkeys([ALICE, SELF], [ALICE], SELF), []);
});

test("case-only difference is not treated as removed", () => {
  assert.deepEqual(
    diffRemovedMentionPubkeys([ALICE.toUpperCase()], [ALICE], SELF),
    [],
  );
});
