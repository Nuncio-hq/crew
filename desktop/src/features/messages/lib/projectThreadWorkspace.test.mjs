import assert from "node:assert/strict";
import test from "node:test";

import {
  buildProjectThreadAgentSteps,
  collectProjectThreadAgentMentions,
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
      agentMentions: [
        { pubkey: "agent-a", source: "root" },
        { pubkey: "agent-b", source: "root" },
        { pubkey: "agent-c", source: "reply" },
      ],
      replies,
    }),
    [
      { pubkey: "agent-a", source: "root", status: "done" },
      { pubkey: "agent-b", source: "root", status: "working" },
      { pubkey: "agent-c", source: "reply", status: "queued" },
    ],
  );
});

test("agent mentions keep root priority and append first-seen reply agents", () => {
  const profiles = {
    "agent-a": { displayName: "Alpha", isAgent: true },
    "agent-b": { displayName: "Beta", isAgent: true },
    "agent-c": { displayName: "Gamma", isAgent: true },
  };
  const result = collectProjectThreadAgentMentions({
    knownAgentPubkeys: new Set(["agent-a", "agent-b", "agent-c"]),
    profiles,
    threadHead: {
      body: "@Alpha starts, then @Beta",
      tags: [
        ["p", "agent-a"],
        ["mention", "agent-b"],
      ],
    },
    replies: [
      {
        body: "@Beta continues, then @Gamma",
        tags: [
          ["p", "agent-b"],
          ["p", "agent-c"],
        ],
      },
    ],
  });
  assert.deepEqual(result, [
    { pubkey: "agent-a", source: "root" },
    { pubkey: "agent-b", source: "root" },
    { pubkey: "agent-c", source: "reply" },
  ]);
});

test("an agent introduced only in a reply is retained", () => {
  assert.deepEqual(
    collectProjectThreadAgentMentions({
      knownAgentPubkeys: new Set(["agent-c"]),
      profiles: {
        "agent-c": { displayName: "Gamma", isAgent: true },
      },
      threadHead: { body: "Start", tags: [] },
      replies: [{ body: "Add @Gamma", tags: [["p", "agent-c"]] }],
    }),
    [{ pubkey: "agent-c", source: "reply" }],
  );
});
