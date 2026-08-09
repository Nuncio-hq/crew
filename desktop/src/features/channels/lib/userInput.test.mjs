import assert from "node:assert/strict";
import test from "node:test";
import {
  buildQuestionAnswer,
  canSubmitUserInput,
  buildSkippedAnswers,
  buildUserInputAnswers,
  derivePendingUserInputs,
  publishUserInputAnswer,
  selectUserInputOption,
  serializeUserInputAnswers,
  setUserInputCustom,
  deriveResolvedUserInputs,
} from "./userInput.ts";

const request = (id, created_at = 10) => ({
  id,
  pubkey: "agent",
  created_at,
  kind: 46040,
  tags: [],
  content: JSON.stringify({
    request_id: id,
    session_id: "session",
    turn_id: "turn",
    channel_id: "channel",
    engine: "claude",
    message: "Choose",
    questions: [
      {
        id: "q0",
        header: "Environment",
        question: "Where?",
        options: [],
      },
    ],
  }),
  sig: "",
});

const answer = (id, pubkey = "owner") => ({
  id: `answer-${id}-${pubkey}`,
  pubkey,
  created_at: 20,
  kind: 46041,
  tags: [
    ["h", "channel"],
    ["e", id],
  ],
  content: JSON.stringify({ q0: "production" }),
  sig: "",
});

test("accepted answer resolves request", () => {
  assert.equal(
    derivePendingUserInputs(
      [request("request-1"), answer("request-1")],
      "owner",
    ).length,
    0,
  );
});

test("foreign-author answer does not resolve request", () => {
  assert.equal(
    derivePendingUserInputs(
      [request("request-1"), answer("request-1", "stranger")],
      "owner",
    ).length,
    1,
  );
});

test("duplicate events are deduplicated", () => {
  assert.equal(
    derivePendingUserInputs(
      [request("request-1"), request("request-1")],
      "owner",
    ).length,
    1,
  );
});

test("optimistic resolution hides the card", () => {
  assert.equal(
    derivePendingUserInputs(
      [request("request-1")],
      "owner",
      new Set(["request-1"]),
    ).length,
    0,
  );
});

test("an unresolved request survives more than 200 newer lifecycle rows", () => {
  const newerRows = Array.from({ length: 201 }, (_, index) => ({
    id: `noise-${index}`,
    pubkey: "agent",
    created_at: 100 + index,
    kind: 46042,
    tags: [["e", `other-${index}`]],
    content: JSON.stringify({
      request_event_id: `other-${index}`,
      outcome: "cancelled",
    }),
    sig: "",
  }));

  assert.deepEqual(
    derivePendingUserInputs(
      [request("old-request", 1), ...newerRows],
      "owner",
    ).map(({ event }) => event.id),
    ["old-request"],
  );
});

test("resolved requests become terminal and stay out of pending questions", () => {
  const requestEvent = request("request-1");
  const resolvedEvent = {
    id: "resolved-1",
    kind: 46042,
    pubkey: "agent",
    content: JSON.stringify({
      request_event_id: "request-1",
      outcome: "cancelled",
    }),
    tags: [
      ["h", "channel"],
      ["e", "request-1"],
    ],
    created_at: 20,
  };
  assert.equal(
    derivePendingUserInputs([requestEvent, resolvedEvent], "owner").length,
    0,
  );
  assert.deepEqual(
    deriveResolvedUserInputs([requestEvent, resolvedEvent]).map(
      ({ resolution }) => resolution,
    ),
    ["cancelled"],
  );
});

test("submit only requires required questions", () => {
  const questions = [
    { id: "q0", required: true },
    { id: "q1", required: false },
  ];
  assert.equal(
    canSubmitUserInput(questions, {
      q0: { selected: ["yes"], custom: "", notes: {} },
    }),
    true,
  );
});

test("submit requires at least one answered question", () => {
  assert.equal(canSubmitUserInput([{ id: "q0", required: false }], {}), false);
});

test("answer payloads preserve all wire shapes", () => {
  assert.equal(
    serializeUserInputAnswers(
      buildUserInputAnswers({
        q0: "production",
        q1: ["lint", "tests"],
        q2: { selected: "yes", choice_notes: { yes: "because" } },
        q3: null,
      }),
    ),
    '{"q0":"production","q1":["lint","tests"],"q2":{"selected":"yes","choice_notes":{"yes":"because"}},"q3":null}',
  );
  assert.deepEqual(buildSkippedAnswers(["q0", "q1"]), { q0: null, q1: null });
});

test("custom text and option selection are mutually exclusive", () => {
  const question = {
    id: "q0",
    header: "Environment",
    question: "Where?",
    options: [{ value: "production", label: "Production", description: "" }],
    allow_custom_answer: true,
  };
  const withCustom = setUserInputCustom(
    { selected: [], custom: "", notes: {} },
    "staging",
  );
  assert.deepEqual(withCustom.selected, []);
  const withOption = selectUserInputOption(question, withCustom, "production");
  assert.deepEqual(withOption, {
    selected: ["production"],
    custom: "",
    notes: {},
  });
});

test("publish failures return an inline-safe error and do not resolve", async () => {
  const answers = { q0: "production" };
  const error = await publishUserInputAnswer(
    async () => {
      throw new Error("relay rejected");
    },
    "channel",
    "request",
    answers,
  );
  assert.equal(error, "relay rejected");
  assert.deepEqual(answers, { q0: "production" });
});

test("multi-select notes are keyed and pruned by selected option", () => {
  const question = {
    id: "q0",
    header: "Checks",
    question: "Which?",
    options: [
      { value: "lint", label: "Lint", description: "" },
      { value: "tests", label: "Tests", description: "" },
    ],
    multi_select: true,
    allow_notes: true,
  };
  let draft = selectUserInputOption(
    question,
    { selected: [], custom: "", notes: {} },
    "lint",
  );
  draft = { ...draft, notes: { lint: "fast", tests: "old" } };
  draft = selectUserInputOption(question, draft, "tests");
  assert.deepEqual(draft, {
    selected: ["lint", "tests"],
    custom: "",
    notes: { lint: "fast", tests: "old" },
  });
  draft = selectUserInputOption(question, draft, "lint");
  assert.deepEqual(draft.notes, { tests: "old" });
  assert.deepEqual(buildQuestionAnswer(question, draft), {
    selected: ["tests"],
    choice_notes: { tests: "old" },
  });
});
