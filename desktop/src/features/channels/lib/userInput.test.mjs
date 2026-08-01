import assert from "node:assert/strict";
import test from "node:test";
import {
  buildSkippedAnswers,
  buildUserInputAnswers,
  derivePendingUserInputs,
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

test("answer payloads preserve all wire shapes", () => {
  assert.equal(
    buildUserInputAnswers({
      q0: "production",
      q1: ["lint", "tests"],
      q2: { selected: "yes", choice_notes: { yes: "because" } },
      q3: null,
    }),
    '{"q0":"production","q1":["lint","tests"],"q2":{"selected":"yes","choice_notes":{"yes":"because"}},"q3":null}',
  );
  assert.deepEqual(buildSkippedAnswers(["q0", "q1"]), { q0: null, q1: null });
});
