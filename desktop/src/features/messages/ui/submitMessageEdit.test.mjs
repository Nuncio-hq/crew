/**
 * Regression guard: edit-save must forward both mention diffs.
 *
 * Upstream #4522 extracted submitMessageEdit and only wired
 * diffAddedMentionPubkeys. The save adapter then hardcoded
 * removedMentionPubkeys to `undefined`, so kind:40003 never emitted
 * `p-removed` when an edit un-mentioned someone.
 *
 * Softening either assert (dropping removed from save, or re-hardcoding
 * undefined in MessageComposer) must fail this file.
 */

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { submitMessageEdit } from "./submitMessageEdit.ts";

const ALICE = "a".repeat(64);
const BOB = "b".repeat(64);
const SELF = "c".repeat(64);

function extractMentionPubkeys(content) {
  return [...content.matchAll(/@([a-f0-9]{64})/g)].map((match) => match[1]);
}

function baseOptions(overrides = {}) {
  return {
    clearComposer: () => {},
    content: `hi @${ALICE}`,
    customEmoji: [],
    editTargetId: "event-edit-1",
    extractMentionPubkeys,
    getMentionRefs: () => [],
    originalContent: `hi @${ALICE} @${BOB}`,
    ownerPubkey: SELF,
    pendingImeta: [],
    queuedAttachments: [],
    restoreComposer: () => {},
    restoreMentionRefs: () => {},
    setDeferredUploadPending: () => {},
    setUploadError: () => {},
    shouldRestoreComposer: () => true,
    spoileredAttachmentUrls: new Set(),
    ...overrides,
  };
}

test("edit that drops a mention passes removedMentionPubkeys into save", async () => {
  /** @type {unknown[][]} */
  const saveCalls = [];
  await submitMessageEdit(
    baseOptions({
      save: async (...args) => {
        saveCalls.push(args);
      },
    }),
  );

  assert.equal(saveCalls.length, 1);
  const [
    content,
    mediaTags,
    addedMentionPubkeys,
    removedMentionPubkeys,
    suppressLinkPreviews,
    eventId,
  ] = saveCalls[0];
  assert.equal(content, `hi @${ALICE}`);
  // Edit path coerces missing media/emoji tags to [] (wipe-attachments signal).
  assert.deepEqual(mediaTags, []);
  assert.deepEqual(addedMentionPubkeys, []);
  assert.deepEqual(removedMentionPubkeys, [BOB]);
  assert.equal(suppressLinkPreviews, false);
  assert.equal(eventId, "event-edit-1");
});

test("edit that adds a mention still passes the added set", async () => {
  /** @type {unknown[][]} */
  const saveCalls = [];
  await submitMessageEdit(
    baseOptions({
      originalContent: `hi @${ALICE}`,
      content: `hi @${ALICE} @${BOB}`,
      save: async (...args) => {
        saveCalls.push(args);
      },
    }),
  );

  assert.equal(saveCalls.length, 1);
  const [, , addedMentionPubkeys, removedMentionPubkeys] = saveCalls[0];
  assert.deepEqual(addedMentionPubkeys, [BOB]);
  assert.deepEqual(removedMentionPubkeys, []);
});

test("MessageComposer save adapter forwards removedMentionPubkeys (not undefined)", async () => {
  // Catches the exact sync breakage: positional args shifted so the 4th
  // onEditSave argument was hardcoded undefined while eventId took slot 5.
  const source = await readFile(
    new URL("./MessageComposer.tsx", import.meta.url),
    "utf8",
  );
  const submitEditIndex = source.indexOf("await submitMessageEdit({");
  assert.ok(submitEditIndex >= 0, "submitMessageEdit call site missing");

  const saveAdapterIndex = source.indexOf("save: async (", submitEditIndex);
  assert.ok(saveAdapterIndex >= 0, "save adapter missing");

  const onEditSaveIndex = source.indexOf(
    "onEditSaveRef.current?.(",
    saveAdapterIndex,
  );
  assert.ok(onEditSaveIndex >= 0, "onEditSave call missing");

  const adapterSlice = source.slice(saveAdapterIndex, onEditSaveIndex + 200);
  assert.match(
    adapterSlice,
    /removedMentionPubkeys/,
    "save adapter must accept removedMentionPubkeys",
  );
  assert.equal(
    /onEditSaveRef\.current\?\.\([\s\S]*?\bundefined\b/.test(adapterSlice),
    false,
    "save adapter must not hardcode undefined for removedMentionPubkeys",
  );
  assert.match(
    adapterSlice,
    /onEditSaveRef\.current\?\.\([\s\S]*removedMentionPubkeys/,
    "onEditSave must receive removedMentionPubkeys from the adapter",
  );
});
