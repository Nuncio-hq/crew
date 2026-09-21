import assert from "node:assert/strict";
import test from "node:test";

import {
  canAskPrivately,
  canRetryAsk,
  initialPrivateAskState,
  isAskInFlight,
  privateAskReducer,
  rememberedAgentKey,
} from "./privateAskDev.ts";

const OPEN = { enabled: true, blockedReason: null };
const CLOSED = { enabled: false, blockedReason: null };
const AGENT = "b".repeat(64);

const CITATION = { path: "src/lib.rs", startLine: 1, endLine: 3 };
const MANIFEST = {
  includedPages: [{ slug: "lib", title: "Lib", score: 4 }],
  omittedPages: [{ slug: "extra", title: "Extra", score: 1 }],
  includedSources: [{ path: "src/lib.rs", startLine: 1, endLine: 3 }],
  omittedSources: [],
  sourceGrant: true,
};

function withQuestion(question) {
  return privateAskReducer(initialPrivateAskState, {
    type: "question",
    value: question,
  });
}

function began(state, attemptId = "attempt-1", extra = {}) {
  return privateAskReducer(state, {
    type: "begin",
    attemptId,
    questionId: "question-1",
    followUpOf: null,
    ...extra,
  });
}

function settle(attemptId, status = "answered", over = {}) {
  return {
    type: "settled",
    result: {
      attemptId,
      questionId: "question-1",
      status,
      markdown: over.markdown ?? "the answer",
      refusal: over.refusal ?? null,
      citations: over.citations ?? [],
      sourceRevision: over.sourceRevision ?? "git:rev",
      manifest: over.manifest ?? MANIFEST,
      historyRecorded: over.historyRecorded ?? true,
    },
  };
}

function historyEntry(over = {}) {
  return {
    attemptId: "stored-1",
    questionId: "question-9",
    followUpOf: null,
    status: "answered",
    detail: null,
    question: "stored question",
    markdown: "stored answer",
    citations: [CITATION],
    manifest: MANIFEST,
    sourceRevision: "git:stored",
    agentPubkey: AGENT,
    askedAt: 1000,
    finishedAt: 1001,
    ...over,
  };
}

test("canAskPrivately_requiresTheBackendGateAndAnAgent", () => {
  const state = withQuestion("What does answer do?");
  assert.equal(canAskPrivately(state, OPEN, AGENT), true);
  // A closed backend gate wins regardless of what the renderer holds.
  assert.equal(canAskPrivately(state, CLOSED, AGENT), false);
  // No agent chosen is nothing to address.
  assert.equal(canAskPrivately(state, OPEN, ""), false);
});

test("canAskPrivately_refusesABlankQuestion", () => {
  for (const blank of ["", "   ", "\n", "\t "]) {
    assert.equal(
      canAskPrivately(withQuestion(blank), OPEN, AGENT),
      false,
      blank,
    );
  }
});

test("canAskPrivately_refusesWhileAnAttemptIsInFlight", () => {
  const running = began(withQuestion("q"));
  assert.equal(canAskPrivately(running, OPEN, AGENT), false);
  assert.equal(isAskInFlight(running), true);
});

test("begin_clearsThePreviousOutcomeSoItIsNotReadAsTheNewOne", () => {
  let state = began(withQuestion("first"));
  state = privateAskReducer(
    state,
    settle("attempt-1", "answered", {
      markdown: "first answer",
      citations: [CITATION],
    }),
  );
  state = began(state, "attempt-2");
  assert.equal(state.answer, "");
  assert.deepEqual(state.citations, []);
  assert.equal(state.error, null);
  assert.equal(state.phase, "retrieving");
  assert.equal(state.attemptId, "attempt-2");
});

test("progress_and_chunks_are_fenced_to_the_active_attempt", () => {
  let state = began(withQuestion("q"), "attempt-1");

  // A foreign attempt's events drop — it is not this run's progress.
  state = privateAskReducer(state, {
    type: "phase",
    attemptId: "other",
    phase: "running",
  });
  assert.equal(state.phase, "retrieving");
  state = privateAskReducer(state, {
    type: "chunk",
    attemptId: "other",
    text: "ghost",
  });
  assert.equal(state.answer, "");

  // Its own events move it forward and stream its bytes.
  state = privateAskReducer(state, {
    type: "phase",
    attemptId: "attempt-1",
    phase: "running",
  });
  state = privateAskReducer(state, {
    type: "chunk",
    attemptId: "attempt-1",
    text: "par",
  });
  state = privateAskReducer(state, {
    type: "chunk",
    attemptId: "attempt-1",
    text: "tial",
  });
  assert.equal(state.phase, "running");
  assert.equal(state.answer, "partial");
});

test("a_late_settle_from_a_superseded_attempt_cannot_overwrite_the_pane", () => {
  let state = began(withQuestion("q"), "old-attempt");
  state = privateAskReducer(state, {
    type: "cancelled",
    attemptId: "old-attempt",
  });
  state = began(state, "new-attempt");

  // The cancelled attempt's completion arrives late. The fence drops it: the
  // running attempt is what the pane belongs to.
  state = privateAskReducer(
    state,
    settle("old-attempt", "answered", { markdown: "late ghost" }),
  );
  assert.equal(state.answer, "");
  assert.equal(state.phase, "retrieving");
  assert.equal(state.attemptId, "new-attempt");

  state = privateAskReducer(
    state,
    settle("new-attempt", "answered", { markdown: "the real one" }),
  );
  assert.equal(state.answer, "the real one");
  assert.equal(state.phase, "answered");
});

test("settled_carriesStatusCitationsManifestAndTheRecordFlag", () => {
  let state = began(withQuestion("q"));
  state = privateAskReducer(
    state,
    settle("attempt-1", "answered", {
      markdown: "grounded",
      citations: [CITATION],
    }),
  );
  assert.equal(state.phase, "answered");
  assert.deepEqual(state.citations, [CITATION]);
  assert.deepEqual(state.manifest, MANIFEST);
  assert.equal(state.sourceRevision, "git:rev");
  // An answered attempt is what a follow-up may build on.
  assert.equal(state.followUpOf, "attempt-1");

  state = began(state, "attempt-2", { followUpOf: "attempt-1" });
  state = privateAskReducer(
    state,
    settle("attempt-2", "insufficient", { markdown: "", manifest: MANIFEST }),
  );
  assert.equal(state.phase, "insufficient");
  assert.deepEqual(state.manifest, MANIFEST);
  // Insufficient is terminal for its attempt but does not invent a parent.
  assert.equal(state.followUpOf, "attempt-1");
});

test("a_refusal_is_terminal_and_shown_verbatim", () => {
  let state = began(withQuestion("q"));
  state = privateAskReducer(
    state,
    settle("attempt-1", "refused", {
      markdown: "",
      refusal: "runtime network egress is not bounded",
    }),
  );
  assert.equal(state.phase, "refused");
  assert.equal(state.answer, "");
  assert.equal(state.error, "runtime network egress is not bounded");
  assert.equal(state.attemptId, null);
  assert.equal(canRetryAsk(state), true);
});

test("a_command_that_could_not_run_is_shown_as_a_refusal_of_the_attempt", () => {
  let state = began(withQuestion("q"));
  state = privateAskReducer(state, {
    type: "rejected",
    attemptId: "attempt-1",
    error: "private Ask is not enabled in this build",
  });
  assert.equal(state.phase, "refused");
  assert.equal(state.error, "private Ask is not enabled in this build");
  // Nothing ran, so nothing is claimed about the machine's record.
  assert.equal(state.historyRecorded, true);
});

test("cancel_fences_the_attempt_and_keeps_the_question_for_retry", () => {
  let state = began(withQuestion("q"));
  state = privateAskReducer(state, {
    type: "chunk",
    attemptId: "attempt-1",
    text: "half",
  });
  state = privateAskReducer(state, {
    type: "cancelled",
    attemptId: "attempt-1",
  });
  assert.equal(state.phase, "cancelled");
  assert.equal(state.attemptId, null);
  assert.equal(state.question, "q");
  assert.equal(canRetryAsk(state), true);

  // The cancelled attempt's late chunk cannot repaint an idle composer.
  state = privateAskReducer(state, {
    type: "chunk",
    attemptId: "attempt-1",
    text: "ghost",
  });
  assert.equal(state.answer, "");
});

test("opened_restoresAnEntryTruthfullyAndARunningOneReattaches", () => {
  const entry = historyEntry();
  let state = privateAskReducer(initialPrivateAskState, {
    type: "opened",
    entry,
  });
  assert.equal(state.phase, "answered");
  assert.equal(state.answer, "stored answer");
  assert.equal(state.followUpOf, "stored-1");

  // A record still owned by a live attempt re-attaches its id — Stop names
  // the owned run, and its stream events are honoured again.
  const live = historyEntry({
    attemptId: "live-1",
    status: "running",
    markdown: null,
  });
  state = privateAskReducer(initialPrivateAskState, {
    type: "opened",
    entry: live,
  });
  assert.equal(state.phase, "running");
  assert.equal(state.attemptId, "live-1");
  assert.equal(isAskInFlight(state), true);

  // An interrupted record is honest about it: no auto-retry, no phantom
  // answer.
  const crashed = historyEntry({
    attemptId: "crashed-1",
    status: "interrupted",
    markdown: null,
  });
  state = privateAskReducer(initialPrivateAskState, {
    type: "opened",
    entry: crashed,
  });
  assert.equal(state.phase, "interrupted");
  assert.equal(state.attemptId, null);
});

test("restored_replacesTheWindowListWithTheMachinesOwnRecord", () => {
  // The backend's record is the one that survived a restart, and it is
  // already bounded and pruned there. Merging would resurrect entries its
  // age bound dropped, so the restore replaces rather than merges.
  const restored = privateAskReducer(initialPrivateAskState, {
    type: "restored",
    history: [historyEntry()],
  });
  assert.equal(restored.history.length, 1);
  assert.equal(restored.history[0].attemptId, "stored-1");
});

test("detachFollowUp_startsAFreshQuestionThread", () => {
  let state = began(withQuestion("q"));
  state = privateAskReducer(state, settle("attempt-1", "answered"));
  assert.equal(state.followUpOf, "attempt-1");
  state = privateAskReducer(state, { type: "detachFollowUp" });
  assert.equal(state.followUpOf, null);
});

test("rememberedAgentKey_isScopedToTheRepositoryCoordinate", () => {
  assert.notEqual(rememberedAgentKey("owner:a"), rememberedAgentKey("owner:b"));
  assert.match(rememberedAgentKey("o:d"), /^crew\.private-ask\.agent\./);
});
