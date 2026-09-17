import assert from "node:assert/strict";
import test from "node:test";

import {
  PRIVATE_ASK_HISTORY_LIMIT,
  canAskPrivately,
  initialPrivateAskState,
  privateAskReducer,
} from "./privateAskDev.ts";

const OPEN = { enabled: true, blockedReason: null };
const CLOSED = { enabled: false, blockedReason: null };

const CITATION = { path: "src/lib.rs", startLine: 1, endLine: 3 };

function withQuestion(question) {
  return privateAskReducer(initialPrivateAskState, {
    type: "question",
    value: question,
  });
}

test("canAskPrivately_requiresTheBackendGate", () => {
  const state = withQuestion("What does answer do?");
  assert.equal(canAskPrivately(state, OPEN), true);
  // A closed backend gate wins regardless of what the renderer holds.
  assert.equal(canAskPrivately(state, CLOSED), false);
});

test("canAskPrivately_refusesABlankQuestion", () => {
  for (const blank of ["", "   ", "\n", "\t "]) {
    assert.equal(canAskPrivately(withQuestion(blank), OPEN), false, blank);
  }
});

test("canAskPrivately_refusesWhileARunIsInFlight", () => {
  const running = privateAskReducer(withQuestion("q"), { type: "start" });
  assert.equal(canAskPrivately(running, OPEN), false);
});

test("start_clearsThePreviousAnswerSoItIsNotReadAsTheNewOne", () => {
  let state = withQuestion("first");
  state = privateAskReducer(state, { type: "start" });
  state = privateAskReducer(state, {
    type: "answered",
    attemptId: "a1",
    markdown: "first answer",
    citations: [CITATION],
  });
  state = privateAskReducer(state, { type: "start" });
  assert.equal(state.answer, "");
  assert.deepEqual(state.citations, []);
  assert.equal(state.error, null);
  assert.equal(state.running, true);
  // The finished attempt is still in history.
  assert.equal(state.history.length, 1);
});

test("chunk_accumulatesOnlyWhileARunIsInFlight", () => {
  let state = privateAskReducer(withQuestion("q"), { type: "start" });
  state = privateAskReducer(state, { type: "chunk", value: "par" });
  state = privateAskReducer(state, { type: "chunk", value: "tial" });
  assert.equal(state.answer, "partial");

  // A late chunk after cancellation must not repaint an idle composer.
  const cancelled = privateAskReducer(state, { type: "cancelled" });
  const late = privateAskReducer(cancelled, { type: "chunk", value: "ghost" });
  assert.equal(late.answer, "");
});

test("refused_isRecordedSoAnIntentionalFenceDoesNotLookLikeAHang", () => {
  let state = privateAskReducer(withQuestion("q"), { type: "start" });
  state = privateAskReducer(state, {
    type: "refused",
    attemptId: "a1",
    error: "runtime network egress is not bounded to the model provider",
  });
  assert.equal(state.running, false);
  assert.equal(state.answer, "");
  assert.equal(
    state.error,
    "runtime network egress is not bounded to the model provider",
  );
  assert.equal(state.history.length, 1);
  assert.equal(state.history[0].markdown, null);
  assert.equal(
    state.history[0].error,
    "runtime network egress is not bounded to the model provider",
  );
});

test("cancelled_recordsNothingBecauseTheViewerWithdrewTheQuestion", () => {
  let state = privateAskReducer(withQuestion("q"), { type: "start" });
  state = privateAskReducer(state, { type: "chunk", value: "half" });
  state = privateAskReducer(state, { type: "cancelled" });
  assert.equal(state.running, false);
  assert.equal(state.answer, "");
  assert.equal(state.history.length, 0);
});

test("history_isNewestFirstAndBounded", () => {
  let state = initialPrivateAskState;
  const total = PRIVATE_ASK_HISTORY_LIMIT + 5;
  for (let index = 0; index < total; index += 1) {
    state = privateAskReducer(state, {
      type: "question",
      value: `question ${index}`,
    });
    state = privateAskReducer(state, { type: "start" });
    state = privateAskReducer(state, {
      type: "answered",
      attemptId: `a${index}`,
      markdown: `answer ${index}`,
      citations: [],
    });
  }
  assert.equal(state.history.length, PRIVATE_ASK_HISTORY_LIMIT);
  assert.equal(state.history[0].attemptId, `a${total - 1}`);
  // The oldest entries fell off the end rather than accumulating forever.
  assert.equal(
    state.history.at(-1).attemptId,
    `a${total - PRIVATE_ASK_HISTORY_LIMIT}`,
  );
});

test("answered_keepsItsCitations", () => {
  let state = privateAskReducer(withQuestion("q"), { type: "start" });
  state = privateAskReducer(state, {
    type: "answered",
    attemptId: "a1",
    markdown: "grounded answer",
    citations: [CITATION],
  });
  assert.deepEqual(state.citations, [CITATION]);
  assert.deepEqual(state.history[0].citations, [CITATION]);
});
