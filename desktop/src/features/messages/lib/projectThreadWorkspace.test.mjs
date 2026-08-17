import assert from "node:assert/strict";
import test from "node:test";

import {
  buildProjectThreadAgentSteps,
  collectProjectThreadAgentMentions,
  isMissingFolderWorkspaceError,
  parseCrewRepoAddress,
  parseProjectThreadContext,
  projectThreadRootAudiencePubkeys,
  projectThreadStickyBarOwnsAgentSignal,
} from "./projectThreadWorkspace.ts";

const PROJECT_BODY =
  "[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew>\n\n@agent fix";

test("Sticky bar owns the agent signal only when it will actually render", () => {
  // Bar renders (context + at least one agent step) → composer drops its copy.
  assert.equal(projectThreadStickyBarOwnsAgentSignal(PROJECT_BODY, 1), true);
});

test("Project thread without resolved mentions leaves the signal to the composer", () => {
  // The bar needs steps to render, and steps come only from mentions. With
  // none, the bar is invisible — so it must NOT claim the signal, or a working
  // agent would have no indicator anywhere. Reachable while the known-agent
  // set is still loading, and for threads the viewer did not author.
  assert.equal(projectThreadStickyBarOwnsAgentSignal(PROJECT_BODY, 0), false);
});

test("Plain threads never hand the agent signal to the sticky bar", () => {
  assert.equal(
    projectThreadStickyBarOwnsAgentSignal("just a message", 2),
    false,
  );
  assert.equal(projectThreadStickyBarOwnsAgentSignal(null, 2), false);
  assert.equal(projectThreadStickyBarOwnsAgentSignal(undefined, 0), false);
});

test("Project context parses workspace binding params", () => {
  assert.deepEqual(
    parseProjectThreadContext(
      "[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew&ws=main>\n\n@agent",
    ),
    {
      localPath: "/tmp/crew",
      repoAddress: "Nuncio-hq/crew",
      ws: "main",
      branch: null,
      base: null,
      mode: "git",
    },
  );
  assert.deepEqual(
    parseProjectThreadContext(
      "[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew&ws=branch:release>\n\n@agent",
    ),
    {
      localPath: "/tmp/crew",
      repoAddress: "Nuncio-hq/crew",
      ws: "branch",
      branch: "release",
      base: null,
      mode: "git",
    },
  );
  const content =
    "[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew>\n\n@agent fix";
  assert.deepEqual(parseProjectThreadContext(content), {
    localPath: "/tmp/crew",
    repoAddress: "Nuncio-hq/crew",
    ws: "new",
    branch: null,
    base: null,
    mode: "git",
  });
  assert.deepEqual(
    parseProjectThreadContext(
      "[ctx]: <buzz://project-workspace?repo=Nuncio-hq%2Fcrew&path=%2Ftmp%2Fcrew&mode=folder>\n\n@agent",
    ),
    {
      localPath: "/tmp/crew",
      repoAddress: "Nuncio-hq/crew",
      ws: "new",
      branch: null,
      base: null,
      mode: "folder",
    },
  );
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

test("Crew repo addresses parse owner and dtag for relink", () => {
  assert.deepEqual(
    parseCrewRepoAddress(
      "30617:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:crew",
    ),
    {
      owner: "a".repeat(64),
      dtag: "crew",
    },
  );
  assert.equal(parseCrewRepoAddress("Nuncio-hq/crew"), null);
});

test("missing-folder workspace errors are the recover path, not a generic setup failure", () => {
  assert.equal(
    isMissingFolderWorkspaceError({
      status: "error",
      agentPubkey: "agent-a",
      conversationId: "conversation-a",
      message: "The Project folder is gone. Pick a workspace again.",
      reason: "missing-folder",
      rootEventId: "a".repeat(64),
    }),
    true,
  );
  assert.equal(
    isMissingFolderWorkspaceError({
      status: "error",
      agentPubkey: "agent-a",
      conversationId: "conversation-a",
      message: "branch already checked out",
      rootEventId: "a".repeat(64),
    }),
    false,
  );
  assert.equal(isMissingFolderWorkspaceError({ status: "pending" }), false);
});

test("composer audience remains limited to agents mentioned at the root", () => {
  assert.deepEqual(
    projectThreadRootAudiencePubkeys([
      { pubkey: "agent-a", source: "root" },
      { pubkey: "agent-b", source: "reply" },
      { pubkey: "agent-c", source: "root" },
    ]),
    ["agent-a", "agent-c"],
  );
});
