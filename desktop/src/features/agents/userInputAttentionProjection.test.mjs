import assert from "node:assert/strict";
import test from "node:test";

import {
  KIND_AGENT_USER_INPUT_ANSWER,
  KIND_AGENT_USER_INPUT_REQUESTED,
  KIND_AGENT_USER_INPUT_RESOLVED,
} from "@/shared/constants/kinds";
import { getNeedsYouForAll, resetNeedsYouStore } from "./needsYouStore.ts";
import {
  _testPendingUserInputTransitionCount,
  beginExhaustiveUserInputProjection,
  endExhaustiveUserInputProjection,
  markUserInputAttentionProjectionUnavailable,
  projectAuthorizedUserInputEvent,
  reconcileAuthorizedUserInputRequests,
  resetUserInputAttentionProjection,
} from "./userInputAttentionProjection.ts";

const CHANNEL = "11111111-1111-4111-8111-111111111111";
const OWNER = "a".repeat(64);
const AGENT = "b".repeat(64);
const SIBLING = "c".repeat(64);
const STRANGER = "d".repeat(64);
const REQUEST_ID = "e".repeat(64);
const TRIGGER_ID = "f".repeat(64);
const ownedAgents = new Set([AGENT, SIBLING]);
let projectionOwner;

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
      ["e", TRIGGER_ID, "", "reply"],
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

function triggerParent(target = AGENT) {
  return event({
    id: TRIGGER_ID,
    kind: 9,
    pubkey: OWNER,
    tags: [
      ["h", CHANNEL],
      ["p", target],
    ],
    content: "trigger",
  });
}

function project(candidate, fallbackChannelId, currentPubkey, agents) {
  const parent =
    candidate.kind === KIND_AGENT_USER_INPUT_REQUESTED
      ? triggerParent(candidate.pubkey)
      : undefined;
  return projectAuthorizedUserInputEvent(
    candidate,
    fallbackChannelId,
    currentPubkey,
    agents,
    parent,
    parent ? new Map([[parent.id, parent]]) : undefined,
    projectionOwner,
  );
}

test.beforeEach(() => {
  resetNeedsYouStore();
  resetUserInputAttentionProjection();
  projectionOwner = beginExhaustiveUserInputProjection();
  endExhaustiveUserInputProjection(projectionOwner);
});

test("projects only requests from an owned agent addressed to the current owner", () => {
  assert.equal(project(request(STRANGER), "", OWNER, ownedAgents), false);
  assert.equal(
    project(request(AGENT, STRANGER), "", OWNER, ownedAgents),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 0);

  assert.equal(project(request(), "", OWNER, ownedAgents), true);
  assert.equal(getNeedsYouForAll().length, 1);
  assert.equal(getNeedsYouForAll()[0]?.rootEventId, TRIGGER_ID);
});

test("rejects requests without a canonical triggering thread reference", () => {
  const missingTrigger = request();
  missingTrigger.tags = missingTrigger.tags.filter(([name]) => name !== "e");
  assert.equal(project(missingTrigger, CHANNEL, OWNER, ownedAgents), false);

  const malformedMarker = request();
  malformedMarker.tags = malformedMarker.tags.map((tag) =>
    tag[0] === "e" ? ["e", TRIGGER_ID, "", "mention"] : tag,
  );
  assert.equal(project(malformedMarker, CHANNEL, OWNER, ownedAgents), false);
  assert.equal(getNeedsYouForAll().length, 0);
});

test("rejects a request whose signed trigger does not target the requesting agent", () => {
  assert.equal(
    projectAuthorizedUserInputEvent(
      request(),
      CHANNEL,
      OWNER,
      ownedAgents,
      triggerParent(STRANGER),
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 0);
});

test("rejects malformed sibling target tags on the signed trigger", () => {
  const malformedSibling = triggerParent();
  malformedSibling.tags.push(["p", "not-a-pubkey"]);
  assert.equal(
    projectAuthorizedUserInputEvent(
      request(),
      CHANNEL,
      OWNER,
      ownedAgents,
      malformedSibling,
    ),
    false,
  );
});

test("rejects noncanonical whitespace and uppercase authority tags", () => {
  const paddedChannel = request();
  paddedChannel.tags = paddedChannel.tags.map((tag) =>
    tag[0] === "h" ? ["h", ` ${CHANNEL} `] : tag,
  );
  assert.equal(project(paddedChannel, "", OWNER, ownedAgents), false);

  const uppercaseOwner = request();
  uppercaseOwner.tags = uppercaseOwner.tags.map((tag) =>
    tag[0] === "p" ? ["p", OWNER.toUpperCase()] : tag,
  );
  assert.equal(project(uppercaseOwner, "", OWNER, ownedAgents), false);
});

test("rejects a trigger whose declared root is unrelated to its actual ancestry", () => {
  const unrelatedId = "1".repeat(64);
  const parent = triggerParent();
  parent.tags.push(["e", TRIGGER_ID, "", "root"]);
  parent.tags.push(["e", unrelatedId, "", "reply"]);
  const unrelated = event({
    id: unrelatedId,
    kind: 9,
    pubkey: OWNER,
    tags: [["h", CHANNEL]],
    content: "unrelated root",
  });
  assert.equal(
    projectAuthorizedUserInputEvent(
      request(),
      CHANNEL,
      OWNER,
      ownedAgents,
      parent,
      new Map([
        [parent.id, parent],
        [unrelated.id, unrelated],
      ]),
    ),
    false,
  );
});

test("terminal resolution replayed before its request prevents resurrection", () => {
  const terminal = transition(KIND_AGENT_USER_INPUT_RESOLVED, AGENT, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  assert.equal(project(terminal, CHANNEL, OWNER, ownedAgents), false);
  assert.equal(_testPendingUserInputTransitionCount(), 1);
  assert.equal(project(request(), CHANNEL, OWNER, ownedAgents), true);
  assert.equal(getNeedsYouForAll().length, 0);
  assert.equal(_testPendingUserInputTransitionCount(), 1);
  assert.equal(project(request(), CHANNEL, OWNER, ownedAgents), false);
  assert.equal(getNeedsYouForAll().length, 0);
});

test("a newer sibling terminal cannot shadow the requesting agent terminal", () => {
  const terminal = transition(KIND_AGENT_USER_INPUT_RESOLVED, AGENT, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  const siblingTerminal = transition(KIND_AGENT_USER_INPUT_RESOLVED, SIBLING, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  project(terminal, CHANNEL, OWNER, ownedAgents);
  project(siblingTerminal, CHANNEL, OWNER, ownedAgents);
  assert.equal(_testPendingUserInputTransitionCount(), 1);
  assert.equal(project(request(), CHANNEL, OWNER, ownedAgents), true);
  assert.equal(getNeedsYouForAll().length, 0);
});

test("revalidates projected requests when identity or verified ownership changes", () => {
  project(request(), "", OWNER, ownedAgents);
  assert.equal(getNeedsYouForAll().length, 1);

  assert.equal(
    reconcileAuthorizedUserInputRequests(
      STRANGER,
      new Set([AGENT]),
      projectionOwner,
    ),
    true,
  );
  assert.equal(getNeedsYouForAll().length, 0);

  project(request(), "", OWNER, ownedAgents);
  assert.equal(getNeedsYouForAll().length, 1);
  assert.equal(
    reconcileAuthorizedUserInputRequests(OWNER, new Set(), projectionOwner),
    true,
  );
  assert.equal(getNeedsYouForAll().length, 0);
});

test("rejects a terminal transition older than its request", () => {
  const stale = transition(KIND_AGENT_USER_INPUT_RESOLVED, AGENT, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  stale.created_at = request().created_at - 1;
  project(stale, CHANNEL, OWNER, ownedAgents);
  project(request(), CHANNEL, OWNER, ownedAgents);
  assert.equal(getNeedsYouForAll().length, 1);
});

test("accepts answers only from the intended owner and waits for 46042", () => {
  project(request(), "", OWNER, ownedAgents);
  assert.equal(
    project(
      transition(KIND_AGENT_USER_INPUT_ANSWER, STRANGER),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 1);

  assert.equal(
    project(
      transition(KIND_AGENT_USER_INPUT_ANSWER, SIBLING),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 1);

  assert.equal(
    project(
      transition(KIND_AGENT_USER_INPUT_ANSWER, OWNER),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    true,
  );
  assert.equal(getNeedsYouForAll().length, 1);

  assert.equal(
    project(
      transition(KIND_AGENT_USER_INPUT_RESOLVED, AGENT, {
        request_event_id: REQUEST_ID,
        outcome: "answered",
      }),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    true,
  );
  assert.equal(getNeedsYouForAll().length, 0);
});

test("accepts resolution only from the requesting agent with matching content", () => {
  project(request(), "", OWNER, ownedAgents);
  const forged = transition(KIND_AGENT_USER_INPUT_RESOLVED, STRANGER, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  assert.equal(project(forged, CHANNEL, OWNER, ownedAgents), false);
  assert.equal(getNeedsYouForAll().length, 1);

  const legitimate = transition(KIND_AGENT_USER_INPUT_RESOLVED, AGENT, {
    request_event_id: REQUEST_ID,
    outcome: "cancelled",
  });
  assert.equal(project(legitimate, CHANNEL, OWNER, ownedAgents), true);
  assert.equal(getNeedsYouForAll().length, 0);
});

test("rejects transitions whose relationship target does not match the request", () => {
  project(request(), "", OWNER, ownedAgents);
  assert.equal(
    project(
      transition(KIND_AGENT_USER_INPUT_ANSWER, OWNER, { q0: "yes" }, OWNER),
      CHANNEL,
      OWNER,
      ownedAgents,
    ),
    false,
  );
  assert.equal(getNeedsYouForAll().length, 1);

  assert.equal(
    project(
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

test("stale exhaustive owners cannot mutate a newer user-input projection", () => {
  const older = beginExhaustiveUserInputProjection();
  const newer = beginExhaustiveUserInputProjection();
  assert.equal(endExhaustiveUserInputProjection(older), false);
  assert.equal(markUserInputAttentionProjectionUnavailable(older), false);
  assert.equal(
    reconcileAuthorizedUserInputRequests(OWNER, ownedAgents, older),
    false,
  );
  assert.equal(endExhaustiveUserInputProjection(newer), true);
});
