import assert from "node:assert/strict";
import test from "node:test";

import {
  deriveEditAsUndoUiState,
  ingestObserverFrameForEditAsUndo,
  recordMessageEditApplied,
  resetDispatchedEventIdsStore,
  getMessageEditAppliedResult,
} from "./dispatchedEventIds.ts";

const EVENT = "a".repeat(64);
const OTHER = "b".repeat(64);

test.beforeEach(() => {
  resetDispatchedEventIdsStore();
});

test("absent from turn_started → queued when message mentions an agent", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: null,
    }),
    "queued",
  );
});

test("present in turn_started → dispatched", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set([EVENT]),
      editResult: null,
    }),
    "dispatched",
  );
});

test("applied:false overrides a locally-queued derivation", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: { outcome: "patched", applied: false },
    }),
    "too-late",
  );
});

test("applied dropped → withdrawn", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: true,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: { outcome: "dropped", applied: true },
    }),
    "withdrawn",
  );
});

test("human-to-human messages never get undo framing", () => {
  assert.equal(
    deriveEditAsUndoUiState({
      mentionsAgent: false,
      eventId: EVENT,
      dispatchedIds: new Set(),
      editResult: null,
    }),
    null,
  );
});

test("ingest message_edit_applied records outcome", () => {
  ingestObserverFrameForEditAsUndo({
    kind: "message_edit_applied",
    payload: {
      targetEventId: EVENT,
      outcome: "dropped",
      applied: true,
    },
  });
  assert.deepEqual(getMessageEditAppliedResult(EVENT), {
    outcome: "dropped",
    applied: true,
  });
  assert.equal(getMessageEditAppliedResult(OTHER), null);
});

test("record ignores non-hex ids", () => {
  recordMessageEditApplied("not-an-id", "patched", true);
  assert.equal(getMessageEditAppliedResult("not-an-id"), null);
});
