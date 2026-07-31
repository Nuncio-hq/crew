import assert from "node:assert/strict";
import test from "node:test";

import {
  buildProjectThreadAgentSteps,
  parseProjectThreadContext,
} from "./projectThreadWorkspace.ts";

test("Project context parses the hidden workspace URL", () => {
  const content =
    "[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew>\n\n@agent fix";
  assert.deepEqual(parseProjectThreadContext(content), {
    localPath: "/tmp/crew",
    repoAddress: "Nuncio-hq/crew",
  });
  assert.equal(parseProjectThreadContext("ordinary chat"), null);
});

test("workflow status comes from active turns and signed agent replies", () => {
  const replies = [
    {
      id: "reply",
      isAgent: true,
      signerPubkey: "agent-a",
      pubkey: "agent-a",
    },
  ];
  assert.deepEqual(
    buildProjectThreadAgentSteps({
      activeAgentPubkeys: ["agent-b"],
      agentPubkeys: ["agent-a", "agent-b", "agent-c", "agent-a"],
      replies,
    }),
    [
      { pubkey: "agent-a", status: "done" },
      { pubkey: "agent-b", status: "working" },
      { pubkey: "agent-c", status: "queued" },
    ],
  );
});
