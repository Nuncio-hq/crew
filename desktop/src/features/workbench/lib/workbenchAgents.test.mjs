import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { collectThreadAgents } from "./workbenchAgents.ts";

const HERMES = "aa".repeat(32);
const CODEX = "bb".repeat(32);
const OWNER = "cc".repeat(32);

describe("collectThreadAgents", () => {
  it("orders agents by first appearance in the thread, including p-tags", () => {
    const agents = collectThreadAgents({
      knownAgentPubkeys: new Set([HERMES, CODEX]),
      managedAgents: [
        { name: "Hermes", pubkey: HERMES },
        { name: "Codex", pubkey: CODEX },
      ],
      messages: [
        {
          id: "1".repeat(64),
          createdAt: 1,
          author: "owner",
          time: "",
          body: "kickoff",
          depth: 0,
          pubkey: OWNER,
          tags: [
            ["p", HERMES],
            ["p", CODEX],
          ],
        },
        {
          id: "2".repeat(64),
          createdAt: 2,
          author: "Codex",
          time: "",
          body: "done",
          depth: 1,
          pubkey: CODEX,
          tags: [["p", CODEX]],
        },
      ],
    });
    assert.deepEqual(
      agents.map((agent) => agent.name),
      ["Hermes", "Codex"],
    );
  });
});
