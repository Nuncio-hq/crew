import assert from "node:assert/strict";
import test from "node:test";

import {
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_RESOLVED,
} from "@/shared/constants/kinds";
import { getNeedsYouForAll, resetNeedsYouStore } from "./needsYouStore.ts";
import { projectAuthorizedUserInputEvent } from "./userInputAttentionProjection.ts";

const CHANNEL = "11111111-1111-4111-8111-111111111111";
const OWNER = "a".repeat(64);
const AGENT = "b".repeat(64);
const SIBLING = "c".repeat(64);
const STRANGER = "d".repeat(64);
const REQUEST_ID = "e".repeat(64);
const ownedAgents = new Set([AGENT, SIBLING]);

function event({
  id = crypto.randomUUID().replaceAll("-", "").padEnd(64, "0"),
  kind,
  pubkey,
  content,
  tags,
}) {
  return {
    id,
    kind,
    pubkey,
    content,
    tags,
    created_at: 100,
    sig: "",
  };
}

function request(pubkey = AGENT, owner = OWNER) {
  return event({
    id: REQUEST_ID,
    kind: KIND_AGENT_USER_INPUT_REQUESTED,
    pubkey,
    tags: [
      ["h", CHANNEL],
      ["p", owner],
    ],
    content: JSON.stringify({
      request_id: "q0",
      session_id: "session",
      turn_id: "turn",
      channel_id: CHANNEL,
      tool_call_id: null,
      engine: "codex",
      message: "Choose",
      questions: [],
    }),
  });
}

function transition(
  kind,
  pubkey,
  content = { q0: "yes" },
  relation = kind === KIND_AGENT_USER_INPUT_ANSWER ? AGENT : OWNER,
) {
  return event({
    kind,
    pubkey,
    tags: [
      ["h", CHANNEL],
      ["e", REQUEST_ID],
      ["p", relation],
    ],
    content: JSON.stringify(content),
  });
}

test.beforeEach(resetNeedsYouStore);

test("projects only requests from an owned agent addressed to the current owner", () => {
  assert.equal(
    projectAuthorizedUserInputEvent(request(STRANGER), "", OWNER, ownedAgents),
    false,
  );
  assert.equal(
    projectAuthorizedUserInputEvent(
      request(AGENT, STRANGER),
      "",
      OWNER,
      ownedAgents,
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 0);

  assert.equal(
    projectAuthorizedUserInputEvent(request(), "", OWNER, ownedAgents),
    true,
  );
  assert.equal(getNeedsYouForAll().length, 1);
});

test("rejects forged answers and accepts the owner or a verified sibling", () => {
  projectAuthorizedUserInputEvent(request(), "", OWNER, ownedAgents);
  assert.equal(
    projectAuthorizedUserInputEvent(
      transition(KIND_AGENT_USER_INPUT_ANSWER, STRANGER),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 1);

  assert.equal(
    projectAuthorizedUserInputEvent(
      transition(KIND_AGENT_USER_INPUT_ANSWER, SIBLING),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    true,
  );
  assert.equal(getNeedsYouForAll().length, 0);
});

test("accepts resolution only from the requesting agent with matching content", () => {
  projectAuthorizedUserInputEvent(request(), "", OWNER, ownedAgents);
  const forged = transition(KIND_AGENT_USER_INPUT_RESOLVED, STRANGER, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  assert.equal(
    projectAuthorizedUserInputEvent(forged, CHANNEL, OWNER, ownedAgents),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 1);

  const legitimate = transition(KIND_AGENT_USER_INPUT_RESOLVED, AGENT, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  assert.equal(
    projectAuthorizedUserInputEvent(legitimate, CHANNEL, OWNER, ownedAgents),
    true,
  );
  assert.equal(getNeedsYouForAll().length, 0);
});

test("rejects transitions whose relationship target does not match the request", () => {
  projectAuthorizedUserInputEvent(request(), "", OWNER, ownedAgents);
  assert.equal(
    projectAuthorizedUserInputEvent(
      transition(KIND_AGENT_USER_INPUT_ANSWER, OWNER, { q0: "yes" }, OWNER),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 1);

  assert.equal(
    projectAuthorizedUserInputEvent(
      transition(
        KIND_AGENT_USER_INPUT_RESOLVED,
        AGENT,
        { request_event_id: REQUEST_ID, outcome: "cancelled" },
        AGENT,
      ),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 1);
});
