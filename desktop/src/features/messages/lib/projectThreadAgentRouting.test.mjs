import assert from "node:assert/strict";
import test from "node:test";

import { resolveProjectThreadAgentRouting } from "./projectThreadAgentRouting.ts";

const context =
  "[ctx]: <buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp>";

test("new Project task wakes first agent and keeps later agents as references", () => {
  assert.deepEqual(
    resolveProjectThreadAgentRouting({
      content: `${context}\n\n@planner plan, @builder build, @reviewer review`,
      explicitAgentPubkeys: ["planner", "builder", "reviewer"],
      isThreadReply: false,
      mentionPubkeys: ["human", "planner", "builder", "reviewer"],
    }),
    {
      mentionPubkeys: ["human", "planner"],
      referencePubkeys: ["builder", "reviewer"],
    },
  );
});

test("ordinary channels and thread replies retain all notifying mentions", () => {
  const mentions = ["planner", "builder"];
  assert.deepEqual(
    resolveProjectThreadAgentRouting({
      content: "@planner and @builder",
      explicitAgentPubkeys: mentions,
      isThreadReply: false,
      mentionPubkeys: mentions,
    }),
    { mentionPubkeys: mentions, referencePubkeys: [] },
  );
  assert.deepEqual(
    resolveProjectThreadAgentRouting({
      content: context,
      explicitAgentPubkeys: mentions,
      isThreadReply: true,
      mentionPubkeys: mentions,
    }),
    { mentionPubkeys: mentions, referencePubkeys: [] },
  );
});
