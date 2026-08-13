import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  agentDisplayName,
  cycleComposerTarget,
  defaultComposerTarget,
  lastInteractingAgentPubkey,
  mentionPubkeysForTarget,
  resolveSendTarget,
  retainComposerTarget,
} from "./workbenchComposerTarget.ts";

const agents = [
  { name: "Hermes", pubkey: "aa".repeat(32) },
  { name: "Codex", pubkey: "bb".repeat(32) },
  { name: "Claude", pubkey: "cc".repeat(32) },
];

describe("workbenchComposerTarget", () => {
  it("defaults to the last interacting agent when that agent is in the thread", () => {
    const last = lastInteractingAgentPubkey(
      [
        { createdAt: 1, pubkey: agents[0].pubkey },
        { createdAt: 3, pubkey: agents[1].pubkey },
        { createdAt: 2, pubkey: "dd".repeat(32) },
      ],
      new Set(agents.map((agent) => agent.pubkey)),
    );
    assert.equal(last, agents[1].pubkey);
    assert.equal(defaultComposerTarget(agents, last), agents[1].pubkey);
  });

  it("falls back to the first agent when last-interacting is unknown", () => {
    assert.equal(
      defaultComposerTarget(agents, "ff".repeat(32)),
      agents[0].pubkey,
    );
    assert.equal(defaultComposerTarget([], null), null);
  });

  it("retains a Tab choice when the agent list refreshes", () => {
    assert.equal(
      retainComposerTarget(agents, agents[0].pubkey, agents[1].pubkey),
      agents[0].pubkey,
    );
    assert.equal(
      retainComposerTarget(agents, null, agents[1].pubkey),
      agents[1].pubkey,
    );
    assert.equal(
      retainComposerTarget(agents, "ff".repeat(32), agents[1].pubkey),
      agents[1].pubkey,
    );
  });

  it("Tab cycles agents and wraps", () => {
    assert.equal(
      cycleComposerTarget(agents, agents[0].pubkey),
      agents[1].pubkey,
    );
    assert.equal(
      cycleComposerTarget(agents, agents[2].pubkey),
      agents[0].pubkey,
    );
    assert.equal(cycleComposerTarget(agents, null), agents[0].pubkey);
  });

  it("lets @ mentions override the chip for that send", () => {
    assert.equal(
      resolveSendTarget({
        agentPubkeys: agents.map((agent) => agent.pubkey),
        chipPubkey: agents[0].pubkey,
        mentionPubkeys: [agents[2].pubkey],
      }),
      agents[2].pubkey,
    );
    assert.equal(
      resolveSendTarget({
        agentPubkeys: agents.map((agent) => agent.pubkey),
        chipPubkey: agents[0].pubkey,
        mentionPubkeys: ["ee".repeat(32)],
      }),
      agents[0].pubkey,
    );
  });

  it("ensures the target is in mentionPubkeys without duplicating", () => {
    assert.deepEqual(
      mentionPubkeysForTarget([agents[1].pubkey], agents[0].pubkey),
      [agents[0].pubkey, agents[1].pubkey],
    );
    assert.deepEqual(
      mentionPubkeysForTarget(
        [agents[0].pubkey.toUpperCase()],
        agents[0].pubkey,
      ),
      [agents[0].pubkey],
    );
  });

  it("names the Stop/Steer label from the chip target", () => {
    assert.equal(agentDisplayName(agents, agents[1].pubkey), "Codex");
    assert.equal(agentDisplayName(agents, null), "agent");
  });
});
