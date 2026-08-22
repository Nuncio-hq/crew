/**
 * Regression guards for submitMessageEdit:
 * - Crew: edit-save must forward both mention diffs (added + removed).
 * - Buzz desktop-v0.5.10: unresolved / late-resolved mention references.
 *
 * Upstream #4522 extracted submitMessageEdit and only wired
 * diffAddedMentionPubkeys. Softening either assert (dropping removed from
 * save, or re-hardcoding undefined in MessageComposer) must fail this file.
 */

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { submitMessageEdit } from "./submitMessageEdit.ts";

const ALICE = "a".repeat(64);
const BOB = "b".repeat(64);
const SELF = "c".repeat(64);
const UNRESOLVED_USER = "b".repeat(64);

function extractMentionPubkeys(content) {
  return [...content.matchAll(/@([a-f0-9]{64})/g)].map((match) => match[1]);
}

function baseOptions(overrides = {}) {
  return {
    clearComposer: () => {},
    content: `hi @${ALICE}`,
    customEmoji: [],
    editTarget: {
      mentionRefs: [],
      unresolvedMentionPubkeys: [],
    },
    editTargetId: "event-edit-1",
    extractMentionPubkeys,
    getMentionRefs: () => [],
    originalContent: `hi @${ALICE} @${BOB}`,
    ownerPubkey: SELF,
    pendingImeta: [],
    queuedAttachments: [],
    restoreComposer: () => {},
    restoreMentionRefs: () => {},
    revalidateMentionPubkeys: async (pubkeys) => [...pubkeys],
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

test("edit save emits unresolved identities as non-notifying mention references", async () => {
  let saved;
  await submitMessageEdit(
    baseOptions({
      content: "hello @Missing User",
      originalContent: "hello @Missing User",
      editTarget: {
        mentionRefs: [],
        unresolvedMentionPubkeys: [UNRESOLVED_USER],
      },
      editTargetId: "event-id",
      extractMentionPubkeys: () => [],
      ownerPubkey: ALICE,
      save: async (
        content,
        tags,
        mentionPubkeys,
        removedMentionPubkeys,
        suppressLinkPreviews,
        eventId,
      ) => {
        saved = {
          content,
          tags,
          mentionPubkeys,
          removedMentionPubkeys,
          suppressLinkPreviews,
          eventId,
        };
      },
    }),
  );

  assert.deepEqual(saved, {
    content: "hello @Missing User",
    tags: [["mention", UNRESOLVED_USER]],
    mentionPubkeys: [],
    removedMentionPubkeys: [],
    suppressLinkPreviews: false,
    eventId: "event-id",
  });
});

test("edit save uses edit-target refs that resolve after edit-open", async () => {
  let saved;
  const resolvedRef = {
    displayName: "Missing User",
    isAgent: false,
    pubkey: UNRESOLVED_USER,
  };
  await submitMessageEdit(
    baseOptions({
      content: "hello @Missing User",
      originalContent: "hello @Missing User",
      editTarget: {
        mentionRefs: [resolvedRef],
        unresolvedMentionPubkeys: [],
      },
      editTargetId: "event-id",
      extractMentionPubkeys: () => [],
      ownerPubkey: ALICE,
      save: async (
        content,
        tags,
        mentionPubkeys,
        removedMentionPubkeys,
        suppressLinkPreviews,
        eventId,
      ) => {
        saved = {
          content,
          tags,
          mentionPubkeys,
          removedMentionPubkeys,
          suppressLinkPreviews,
          eventId,
        };
      },
    }),
  );

  assert.deepEqual(saved, {
    content: "hello @Missing User",
    tags: [["mention", UNRESOLVED_USER]],
    mentionPubkeys: [],
    removedMentionPubkeys: [],
    suppressLinkPreviews: false,
    eventId: "event-id",
  });
});

test("edit save revalidates added mentions immediately before save", async () => {
  const agent = "c".repeat(64);
  const calls = [];
  await submitMessageEdit({
    ...baseOptions(async (_content, _tags, mentionPubkeys) => {
      calls.push(["save", mentionPubkeys]);
    }),
    content: "hello @Agent",
    originalContent: "hello",
    extractMentionPubkeys: (content) =>
      content.includes("@Agent") ? [agent] : [],
    revalidateMentionPubkeys: async (pubkeys) => {
      calls.push(["revalidate", pubkeys]);
      return [];
    },
  });

  assert.deepEqual(calls, [
    ["revalidate", [agent]],
    ["save", []],
  ]);
});

test("edit upload pause revalidates revoked mentions only after upload completes", async () => {
  const agent = "d".repeat(64);
  const calls = [];
  let completeUpload;
  await submitMessageEdit({
    ...baseOptions(async (_content, _tags, mentionPubkeys) => {
      calls.push(["save", mentionPubkeys]);
    }),
    content: "hello @Agent",
    originalContent: "hello",
    extractMentionPubkeys: (content) =>
      content.includes("@Agent") ? [agent] : [],
    queuedAttachments: [
      {
        file: new File(["image"], "image.png", { type: "image/png" }),
        id: 1,
        spoilered: false,
      },
    ],
    enqueueUpload: ({ onComplete }) => {
      completeUpload = () => onComplete([], new AbortController().signal);
      return {};
    },
    revalidateMentionPubkeys: async (pubkeys) => {
      calls.push(["revalidate", pubkeys]);
      return [];
    },
  });

  assert.deepEqual(calls, []);
  await completeUpload();
  assert.deepEqual(calls, [
    ["revalidate", [agent]],
    ["save", []],
  ]);
});
